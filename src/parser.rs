// Deliberately avoid `?` operator throughout this module for compile-time
// performance: explicit match/if-let prevents the compiler from generating
// From::from conversion and drop-glue machinery at every call site.

#[cfg(test)]
#[path = "./parser_tests.rs"]
mod tests;

#[cfg(feature = "from-toml")]
use crate::de::TableHelper;
use crate::{
    Failed, MaybeItem, Span,
    arena::Arena,
    error::{Error, ErrorKind, PathComponent},
    item::{
        self, Item, Key,
        table::{InnerTable, Table},
    },
    time::DateTime,
};
use std::char;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

const MAX_RECURSION_DEPTH: i16 = 256;

struct Ctx<'b, 'de> {
    /// The current table context — a `Table` view into a table `Value`.
    /// Gives direct mutable access to both the span fields and the `Table` payload.
    table: &'b mut Table<'de>,
    /// If this table is an entry in an array-of-tables, a disjoint borrow of
    /// the parent array Value'arena `end_and_flag` field so its span can be
    /// extended alongside the entry.
    array_end_span: Option<&'b mut u32>,
}

/// Tables with at least this many entries use the hash index for lookups.
/// Note: Looking purely at parsing benchmarks you might be inclined to raise
///  this value higher, however the same index is then used during deserialization
///  where the loss of initializing the index is recouped.
pub const INDEXED_TABLE_THRESHOLD: usize = 6;

const fn build_hex_table() -> [i8; 256] {
    let mut table = [-1i8; 256];
    let mut ch = 0usize;
    while ch < 256 {
        table[ch] = match ch as u8 {
            b'0'..=b'9' => (ch as u8 - b'0') as i8,
            b'A'..=b'F' => (ch as u8 - b'A' + 10) as i8,
            b'a'..=b'f' => (ch as u8 - b'a' + 10) as i8,
            _ => -1,
        };
        ch += 1;
    }
    table
}

static HEX: [i8; 256] = build_hex_table();

/// Hash-map key that identifies a (table, key-name) pair without owning the
/// string data.  The raw `key_ptr`/`len` point into either the input buffer
/// or the arena; both are stable for the lifetime of the parse.
/// `first_key_span` is the `span.start()` of the **first** key ever inserted
/// into the table and serves as a cheap, collision-free table discriminator.
pub(crate) struct KeyRef<'de> {
    key_ptr: NonNull<u8>,
    len: u32,
    first_key_span: u32,
    marker: std::marker::PhantomData<&'de str>,
}

impl<'de> KeyRef<'de> {
    #[inline]
    pub(crate) fn new(key: &'de str, first_key_span: u32) -> Self {
        KeyRef {
            // SAFETY: str::as_ptr() is guaranteed non-null.
            key_ptr: unsafe { NonNull::new_unchecked(key.as_ptr() as *mut u8) },
            len: key.len() as u32,
            first_key_span,
            marker: std::marker::PhantomData,
        }
    }
}

impl<'de> KeyRef<'de> {
    #[inline]
    fn as_str(&self) -> &'de str {
        // SAFETY: key_ptr and len were captured from a valid &'de str in new().
        // The PhantomData<&'de str> ensures the borrow is live.
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                self.key_ptr.as_ptr(),
                self.len as usize,
            ))
        }
    }
}

impl<'de> Hash for KeyRef<'de> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.first_key_span.hash(state);
        // Note: KeyRef is meant only beused inside the Index where it's
        // the KeyRef is entirety of the Hash Input so we don't have to
        // worry about prefix freedom.
        self.as_str().hash(state);
    }
}

impl<'de> PartialEq for KeyRef<'de> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.first_key_span == other.first_key_span && self.as_str() == other.as_str()
    }
}

impl<'de> Eq for KeyRef<'de> {}

struct Parser<'de> {
    /// Raw bytes of the input. Always valid UTF-8 (derived from `&str`).
    bytes: &'de [u8],
    cursor: usize,
    arena: &'de Arena,

    // Error context -- populated just before returning Failed
    error_span: Span,
    error_kind: Option<ErrorKind<'static>>,

    // TOML path tracking for error context (zero-cost on happy path)
    path: [PathComponent<'de>; 16],
    path_len: u8,

    // Global key-index for O(1) lookups in large tables.
    // Maps (table-discriminator, key-name) → entry index in the table.
    index: foldhash::HashMap<KeyRef<'de>, usize>,

    // Recovery mode: when true, parse errors are accumulated instead of
    // immediately returned, and parsing continues from the next line.
    recovering: bool,
    errors: Vec<Error>,
}

impl<'de> Parser<'de> {
    fn new(input: &'de str, arena: &'de Arena) -> Self {
        let bytes = input.as_bytes();
        // Skip UTF-8 BOM (U+FEFF = EF BB BF) if present at the start.
        let cursor = if bytes.starts_with(b"\xef\xbb\xbf") {
            3
        } else {
            0
        };
        Parser {
            bytes,
            cursor,
            arena,
            error_span: Span::new(0, 0),
            error_kind: None,
            path: [PathComponent::Index(0); 16],
            path_len: 0,
            // initialize to about ~ 8 KB
            index: foldhash::HashMap::with_capacity_and_hasher(
                256,
                foldhash::fast::RandomState::default(),
            ),
            recovering: false,
            errors: Vec::new(),
        }
    }

    /// Get a `&str` slice from the underlying bytes.
    ///
    /// # Safety
    ///
    /// - `start <= end <= self.bytes.len()`.
    /// - `start` and `end` must lie on UTF-8 character boundaries within
    ///   `self.bytes` (which is always valid UTF-8 because it was derived
    ///   from a `&str`).
    #[inline]
    unsafe fn str_slice(&self, start: usize, end: usize) -> &'de str {
        #[cfg(not(debug_assertions))]
        unsafe {
            std::str::from_utf8_unchecked(&self.bytes[start..end])
        }
        #[cfg(debug_assertions)]
        match std::str::from_utf8(&self.bytes[start..end]) {
            Ok(value) => value,
            Err(err) => panic!(
                "Invalid UTF-8 slice: bytes[{}..{}] is not valid UTF-8: {}",
                start, end, err
            ),
        }
    }

    #[inline]
    fn push_path(&mut self, component: PathComponent<'de>) {
        let len = self.path_len as usize;
        if len < self.path.len() {
            self.path[len] = component;
        }
        self.path_len = self.path_len.saturating_add(1);
    }

    #[cold]
    fn build_error_path(&self) -> crate::error::MaybeTomlPath {
        let depth = (self.path_len as usize).min(self.path.len());
        crate::error::MaybeTomlPath::from_components(&self.path[..depth])
    }

    #[cold]
    fn set_duplicate_key_error(&mut self, first: Span, second: Span) -> Failed {
        self.error_span = second;
        self.error_kind = Some(ErrorKind::DuplicateKey { first });
        Failed
    }

    #[cold]
    fn set_error(&mut self, start: usize, end: Option<usize>, kind: ErrorKind<'static>) -> Failed {
        self.error_span = Span::new(start as u32, end.unwrap_or(start + 1) as u32);
        self.error_kind = Some(kind);
        Failed
    }

    fn take_error(&mut self) -> Error {
        let kind = self
            .error_kind
            .take()
            .expect("take_error called without error");
        let span = self.error_span;
        let path = self.build_error_path();

        // Black Magic Optimization:
        // Removing the following introduces 8% performance
        // regression across the board.
        std::hint::black_box(&self.bytes.iter().enumerate().next());

        Error::new_with_path(kind, span, path)
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }

    #[inline]
    fn peek_byte_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.cursor + offset).copied()
    }

    #[inline]
    fn eat_byte(&mut self, b: u8) -> bool {
        if self.peek_byte() == Some(b) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    #[cold]
    fn expected_error(&mut self, b: u8) -> Failed {
        let start = self.cursor;
        let (found_desc, end) = self.scan_token_desc_and_end();
        self.set_error(
            start,
            Some(end),
            ErrorKind::Wanted {
                expected: byte_describe(b),
                found: found_desc,
            },
        )
    }

    fn expect_byte(&mut self, b: u8) -> Result<(), Failed> {
        if self.peek_byte() == Some(b) {
            self.cursor += 1;
            Ok(())
        } else {
            Err(self.expected_error(b))
        }
    }

    fn eat_whitespace(&mut self) {
        while let Some(b) = self.peek_byte() {
            if b == b' ' || b == b'\t' {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    fn eat_whitespace_to(&mut self) -> Option<u8> {
        while let Some(b) = self.peek_byte() {
            if b == b' ' || b == b'\t' {
                self.cursor += 1;
            } else {
                return Some(b);
            }
        }
        None
    }

    fn eat_newline_or_eof(&mut self) -> Result<(), Failed> {
        match self.peek_byte() {
            None => Ok(()),
            Some(b'\n') => {
                self.cursor += 1;
                Ok(())
            }
            Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                self.cursor += 2;
                Ok(())
            }
            _ => {
                let start = self.cursor;
                let (found_desc, end) = self.scan_token_desc_and_end();
                Err(self.set_error(
                    start,
                    Some(end),
                    ErrorKind::Wanted {
                        expected: &"newline",
                        found: found_desc,
                    },
                ))
            }
        }
    }

    fn eat_comment(&mut self) -> Result<bool, Failed> {
        if !self.eat_byte(b'#') {
            return Ok(false);
        }
        while let Some(0x09 | 0x20..=0x7E | 0x80..) = self.peek_byte() {
            self.cursor += 1;
        }
        self.eat_newline_or_eof().map(|()| true)
    }

    fn eat_newline(&mut self) -> bool {
        match self.peek_byte() {
            Some(b'\n') => {
                self.cursor += 1;
                true
            }
            Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                self.cursor += 2;
                true
            }
            _ => false,
        }
    }

    /// Scan forward from the current position to determine the description
    /// and end position of the "token" at the cursor. This provides compatible
    /// error spans with the old tokenizer.
    fn scan_token_desc_and_end(&self) -> (&'static &'static str, usize) {
        let Some(b) = self.peek_byte() else {
            return (&"eof", self.bytes.len());
        };
        match b {
            b'\n' => (&"a newline", self.cursor + 1),
            b'\r' => (&"a carriage return", self.cursor + 1),
            b' ' | b'\t' => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len()
                    && (self.bytes[end] == b' ' || self.bytes[end] == b'\t')
                {
                    end += 1;
                }
                (&"whitespace", end)
            }
            b'#' => (&"a comment", self.cursor + 1),
            b'=' => (&"an equals", self.cursor + 1),
            b'.' => (&"a period", self.cursor + 1),
            b',' => (&"a comma", self.cursor + 1),
            b':' => (&"a colon", self.cursor + 1),
            b'+' => (&"a plus", self.cursor + 1),
            b'{' => (&"a left brace", self.cursor + 1),
            b'}' => (&"a right brace", self.cursor + 1),
            b'[' => (&"a left bracket", self.cursor + 1),
            b']' => (&"a right bracket", self.cursor + 1),
            b'\'' | b'"' => (&"a string", self.cursor + 1),
            _ if is_keylike_byte(b) => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len() && is_keylike_byte(self.bytes[end]) {
                    end += 1;
                }
                (&"an identifier", end)
            }
            _ => (&"a character", self.cursor + 1),
        }
    }

    fn read_keylike(&mut self) -> &'de str {
        let start = self.cursor;
        while let Some(b) = self.peek_byte() {
            if !is_keylike_byte(b) {
                break;
            }
            self.cursor += 1;
        }
        // SAFETY: keylike bytes are ASCII, always valid UTF-8 boundaries
        unsafe { self.str_slice(start, self.cursor) }
    }

    fn read_table_key(&mut self) -> Result<Key<'de>, Failed> {
        let Some(b) = self.peek_byte() else {
            return Err(self.set_error(
                self.bytes.len(),
                None,
                ErrorKind::Wanted {
                    expected: &"a table key",
                    found: &"eof",
                },
            ));
        };
        match b {
            b'"' => {
                let start = self.cursor;
                self.cursor += 1;
                let (key, multiline) = match self.read_string(start, b'"') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(key.span.end as usize),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(key)
            }
            b'\'' => {
                let start = self.cursor;
                self.cursor += 1;
                let (key, multiline) = match self.read_string(start, b'\'') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(key.span.end as usize),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(key)
            }
            b if is_keylike_byte(b) => {
                let start = self.cursor;
                let name = self.read_keylike();
                let span = Span::new(start as u32, self.cursor as u32);
                Ok(Key { name, span })
            }
            _ => {
                let start = self.cursor;
                let (found_desc, end) = self.scan_token_desc_and_end();
                Err(self.set_error(
                    start,
                    Some(end),
                    ErrorKind::Wanted {
                        expected: &"a table key",
                        found: found_desc,
                    },
                ))
            }
        }
    }

    /// Read a basic (double-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `"`.
    fn read_string(&mut self, start: usize, delim: u8) -> Result<(Key<'de>, bool), Failed> {
        let mut multiline = false;
        if self.eat_byte(delim) {
            if self.eat_byte(delim) {
                multiline = true;
            } else {
                return Ok((
                    Key {
                        name: "",
                        span: Span::new(start as u32, self.cursor as u32),
                    },
                    false,
                ));
            }
        }

        let mut content_start = self.cursor;
        if multiline {
            match self.peek_byte() {
                Some(b'\n') => {
                    self.cursor += 1;
                    content_start = self.cursor;
                }
                Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                    self.cursor += 2;
                    content_start = self.cursor;
                }
                _ => {}
            }
        }

        self.read_string_loop(start, content_start, multiline, delim)
    }

    /// Advance `self.cursor` past bytes that do not require special handling
    /// inside a string.  Uses SWAR (SIMD-Within-A-Register) to scan 8 bytes
    /// at a time.
    ///
    /// Stops at the first byte that is:
    ///   * a control character (< 0x20) — tab (0x09) is a benign false positive
    ///   * DEL (0x7F)
    ///   * the string delimiter (`"` or `'`)
    ///   * a backslash (`\`) — benign false positive for literal strings
    ///   * past the end of input
    fn skip_string_plain(&mut self, delim: u8) {
        // Quick bail-out for EOF or an immediately-interesting byte.
        // Avoids SWAR setup cost for consecutive specials (e.g. \n\n).
        let Some(&b) = self.bytes.get(self.cursor) else {
            return;
        };

        if b == delim || b == b'\\' || b == 0x7F || (b < 0x20 && b != 0x09) {
            return;
        }
        self.cursor += 1;

        let base = self.cursor;
        let rest = &self.bytes[base..];

        type Chunk = u64;
        const STEP: usize = std::mem::size_of::<Chunk>();
        const ONE: Chunk = Chunk::MAX / 255; // 0x0101_0101_0101_0101
        const HIGH: Chunk = ONE << 7; // 0x8080_8080_8080_8080

        let fill_delim = ONE * Chunk::from(delim);
        let fill_bslash = ONE * Chunk::from(b'\\');
        let fill_del = ONE * 0x7F;

        let chunks = rest.chunks_exact(STEP);
        let remainder_len = chunks.remainder().len();

        for (i, chunk) in chunks.enumerate() {
            let v = Chunk::from_le_bytes(chunk.try_into().unwrap());

            let has_ctrl = v.wrapping_sub(ONE * 0x20) & !v;
            let eq_delim = (v ^ fill_delim).wrapping_sub(ONE) & !(v ^ fill_delim);
            let eq_bslash = (v ^ fill_bslash).wrapping_sub(ONE) & !(v ^ fill_bslash);
            let eq_del = (v ^ fill_del).wrapping_sub(ONE) & !(v ^ fill_del);

            let masked = (has_ctrl | eq_delim | eq_bslash | eq_del) & HIGH;
            if masked != 0 {
                self.cursor = base + i * STEP + masked.trailing_zeros() as usize / 8;
                return;
            }
        }

        self.cursor = self.bytes.len() - remainder_len;
        self.skip_string_plain_slow(delim);
    }

    #[cold]
    #[inline(never)]
    fn skip_string_plain_slow(&mut self, delim: u8) {
        while let Some(&b) = self.bytes.get(self.cursor) {
            if b == delim || b == b'\\' || b == 0x7F || (b < 0x20 && b != 0x09) {
                return;
            }
            self.cursor += 1;
        }
    }

    fn read_string_loop(
        &mut self,
        start: usize,
        content_start: usize,
        multiline: bool,
        delim: u8,
    ) -> Result<(Key<'de>, bool), Failed> {
        let mut flush_from = content_start;
        let mut scratch: Option<crate::arena::Scratch<'de>> = None;
        loop {
            self.skip_string_plain(delim);

            let i = self.cursor;
            let Some(&b) = self.bytes.get(i) else {
                return Err(self.set_error(
                    i,
                    Some(i),
                    ErrorKind::UnterminatedString(delim as char),
                ));
            };
            self.cursor = i + 1;

            match b {
                b'\r' => {
                    if self.eat_byte(b'\n') {
                        if !multiline {
                            return Err(self.set_error(
                                i,
                                Some(i),
                                ErrorKind::UnterminatedString(delim as char),
                            ));
                        }
                    } else {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\r')));
                    }
                }
                b'\n' => {
                    if !multiline {
                        return Err(self.set_error(
                            i,
                            Some(i),
                            ErrorKind::UnterminatedString(delim as char),
                        ));
                    }
                }
                d if d == delim => {
                    let (span, end) = if multiline {
                        if !self.eat_byte(delim) {
                            continue;
                        }
                        if !self.eat_byte(delim) {
                            continue;
                        }
                        let mut extra = 0usize;
                        if self.eat_byte(delim) {
                            extra += 1;
                        }
                        if self.eat_byte(delim) {
                            extra += 1;
                        }

                        (Span::new(start as u32, self.cursor as u32), i + extra)
                    } else {
                        (Span::new(start as u32, self.cursor as u32), i)
                    };

                    let name = if let Some(mut s) = scratch {
                        s.extend(&self.bytes[flush_from..end]);
                        let committed = s.commit();
                        // Safety: scratch contents are valid UTF-8 (built from
                        // validated input and well-formed escape sequences).
                        unsafe { std::str::from_utf8_unchecked(committed) }
                    } else {
                        // Safety: content_start..end is validated UTF-8.
                        unsafe { self.str_slice(content_start, end) }
                    };
                    return Ok((Key { name, span }, multiline));
                }
                b'\\' if delim == b'"' => {
                    let arena = self.arena;
                    // SAFETY: the closure only runs when scratch is None, so no
                    // other Scratch or arena.alloc() call is active.
                    let s = scratch.get_or_insert_with(|| unsafe { arena.scratch() });
                    s.extend(&self.bytes[flush_from..i]);
                    if let Err(e) = self.read_basic_escape(s, start, multiline) {
                        return Err(e);
                    }
                    flush_from = self.cursor;
                }
                // Tab or backslash-in-literal-string: benign false positives
                // from the SWAR scan.
                0x09 | 0x20..=0x7E | 0x80.. => {}
                _ => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(b as char)));
                }
            }
        }
    }

    fn read_basic_escape(
        &mut self,
        scratch: &mut crate::arena::Scratch<'_>,
        string_start: usize,
        multi: bool,
    ) -> Result<(), Failed> {
        let i = self.cursor;
        let Some(&b) = self.bytes.get(i) else {
            return Err(self.set_error(i, Some(i), ErrorKind::UnterminatedString('"')));
        };
        self.cursor = i + 1;
        let chr: char = 'char: {
            let byte: u8 = 'byte: {
                match b {
                    b'"' => break 'byte b'"',
                    b'\\' => break 'byte b'\\',
                    b'b' => break 'byte 0x08,
                    b'f' => break 'byte 0x0C,
                    b'n' => break 'byte b'\n',
                    b'r' => break 'byte b'\r',
                    b't' => break 'byte b'\t',
                    b'e' => break 'byte 0x1B,
                    b'u' => match self.read_hex(4, string_start, i) {
                        Ok(ch) => break 'char ch,
                        Err(e) => return Err(e),
                    },
                    b'U' => match self.read_hex(8, string_start, i) {
                        Ok(ch) => break 'char ch,
                        Err(e) => return Err(e),
                    },
                    b'x' => match self.read_hex(2, string_start, i) {
                        Ok(ch) => break 'char ch,
                        Err(e) => return Err(e),
                    },
                    b' ' | b'\t' | b'\n' | b'\r' if multi => {
                        // CRLF folding: \r\n counts as \n
                        let c = if b == b'\r' && self.peek_byte() == Some(b'\n') {
                            self.cursor += 1;
                            '\n'
                        } else if b == b'\r' {
                            return Err(self.set_error(
                                i,
                                None,
                                ErrorKind::InvalidCharInString('\r'),
                            ));
                        } else {
                            b as char
                        };
                        if c != '\n' {
                            loop {
                                match self.peek_byte() {
                                    Some(b' ' | b'\t') => {
                                        self.cursor += 1;
                                    }
                                    Some(b'\n') => {
                                        self.cursor += 1;
                                        break;
                                    }
                                    Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                                        self.cursor += 2;
                                        break;
                                    }
                                    _ => {
                                        return Err(self.set_error(
                                            i,
                                            None,
                                            ErrorKind::InvalidEscape(c),
                                        ));
                                    }
                                }
                            }
                        }
                        loop {
                            match self.peek_byte() {
                                Some(b' ' | b'\t' | b'\n') => {
                                    self.cursor += 1;
                                }
                                Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                                    self.cursor += 2;
                                }
                                _ => break,
                            }
                        }
                    }
                    _ => {
                        self.cursor -= 1;
                        return Err(self.set_error(
                            self.cursor,
                            None,
                            ErrorKind::InvalidEscape(self.next_char_for_error()),
                        ));
                    }
                }
                return Ok(());
            };

            scratch.push(byte);
            return Ok(());
        };
        let mut buf = [0u8; 4];
        let len = chr.encode_utf8(&mut buf).len();
        scratch.extend(&buf[..len]);
        Ok(())
    }

    fn read_hex(
        &mut self,
        n: usize,
        _string_start: usize,
        escape_start: usize,
    ) -> Result<char, Failed> {
        let mut val: u32 = 0;
        for _ in 0..n {
            let Some(&byte) = self.bytes.get(self.cursor) else {
                return Err(self.set_error(
                    self.cursor,
                    Some(self.cursor),
                    ErrorKind::UnterminatedString('"'),
                ));
            };
            let digit = HEX[byte as usize];
            if digit >= 0 {
                val = (val << 4) | digit as u32;
                self.cursor += 1;
            } else {
                return Err(self.set_error(
                    self.cursor,
                    None,
                    ErrorKind::InvalidHexEscape(self.next_char_for_error()),
                ));
            }
        }
        match char::from_u32(val) {
            Some(ch) => Ok(ch),
            None => Err(self.set_error(
                escape_start,
                Some(escape_start + n),
                ErrorKind::InvalidEscapeValue(val),
            )),
        }
    }

    fn next_char_for_error(&self) -> char {
        // Safety: The input was valid UTF-8 via a &str
        let text = unsafe { std::str::from_utf8_unchecked(self.bytes) };
        if let Some(value) = text.get(self.cursor..) {
            value.chars().next().unwrap_or(char::REPLACEMENT_CHARACTER)
        } else {
            char::REPLACEMENT_CHARACTER
        }
    }
    fn number(&mut self, start: u32, end: u32, s: &'de str, sign: u8) -> Result<Item<'de>, Failed> {
        let bytes = s.as_bytes();

        // Base-prefixed integers (0x, 0o, 0b).
        // TOML forbids signs on these, so only match when first byte is '0'.
        if sign == 2
            && let [b'0', format, rest @ ..] = s.as_bytes()
        {
            match format {
                b'x' => return self.integer_prefixed(rest, Span::new(start, end), 4),
                b'o' => return self.integer_prefixed(rest, Span::new(start, end), 3),
                b'b' => return self.integer_prefixed(rest, Span::new(start, end), 1),
                _ => {}
            }
        }

        if self.eat_byte(b'.') {
            let at = self.cursor;
            return match self.peek_byte() {
                Some(b) if is_keylike_byte(b) => {
                    let after = self.read_keylike();
                    match self.float(start, end, s, Some(after), sign) {
                        Ok(f) => Ok(Item::float_spanned(f, Span::new(start, self.cursor as u32))),
                        Err(e) => Err(e),
                    }
                }
                _ => Err(self.set_error(
                    at,
                    Some(end as usize),
                    ErrorKind::InvalidFloat("nothing after decimal point"),
                )),
            };
        }

        if sign == 2 {
            let head = &self.bytes[start as usize..];
            match DateTime::munch(head) {
                Ok((consumed, moment)) => {
                    self.cursor = start as usize + consumed;
                    return Ok(Item::moment(moment, Span::new(start, self.cursor as u32)));
                }
                Err(reason) if !reason.is_empty() => {
                    let rest = &self.bytes[start as usize..];
                    let mut consumed = 0;
                    while consumed < rest.len()
                        && !matches!(
                            rest[consumed],
                            b' ' | b'\t' | b'\n' | b'\r' | b'#' | b',' | b']' | b'}'
                        )
                    {
                        consumed += 1;
                    }
                    self.cursor = start as usize + consumed;
                    return Err(self.set_error(
                        start as usize,
                        Some(self.cursor),
                        ErrorKind::InvalidDateTime(reason),
                    ));
                }
                Err(_) => {}
            }
        }

        if sign != 2
            && let [b'0', b'x' | b'o' | b'b', ..] = bytes
        {
            return Err(self.set_error(
                start as usize,
                Some(end as usize),
                ErrorKind::InvalidInteger("signs are not allowed on prefixed integers"),
            ));
        }

        if let Ok(v) = self.integer_decimal(bytes, Span::new(start, end), sign) {
            return Ok(v);
        }

        if bytes.iter().any(|&b| b == b'e' || b == b'E') {
            return match self.float(start, end, s, None, sign) {
                Ok(f) => Ok(Item::float_spanned(f, Span::new(start, self.cursor as u32))),
                Err(e) => Err(e),
            };
        }

        Err(Failed)
    }

    fn integer_decimal(
        &mut self,
        bytes: &'de [u8],
        span: Span,
        sign: u8,
    ) -> Result<Item<'de>, Failed> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        let mut leading_zero = false;
        let negative = sign == 0;
        let sign_len = if sign != 2 { 1u32 } else { 0u32 };
        let mut error_span = span;
        let reason = 'error: {
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        let pos = span.start + sign_len + i as u32;
                        error_span = Span::new(pos, pos + 1);
                        break 'error "underscores must be between two digits";
                    }
                    prev_underscore = true;
                    i += 1;
                    continue;
                }
                if !b.is_ascii_digit() {
                    let pos = span.start + sign_len + i as u32;
                    error_span = Span::new(pos, pos + 1);
                    break 'error "contains non-digit character";
                }
                if leading_zero {
                    break 'error "leading zeros are not allowed";
                }
                if !has_digit && b == b'0' {
                    leading_zero = true;
                }
                has_digit = true;
                prev_underscore = false;
                let digit = (b - b'0') as u64;
                acc = match acc.checked_mul(10).and_then(|a| a.checked_add(digit)) {
                    Some(v) => v,
                    None => break 'error "integer overflow",
                };
                i += 1;
            }

            if !has_digit {
                break 'error "expected at least one digit";
            }
            if prev_underscore {
                let pos = span.start + sign_len + bytes.len() as u32 - 1;
                error_span = Span::new(pos, pos + 1);
                break 'error "underscores must be between two digits";
            }

            let max = if negative {
                (i64::MAX as u64) + 1
            } else {
                i64::MAX as u64
            };
            if acc > max {
                break 'error "integer overflow";
            }

            let val = if negative {
                (acc as i64).wrapping_neg()
            } else {
                acc as i64
            };
            return Ok(Item::integer_spanned(val, span));
        };
        self.error_span = error_span;
        self.error_kind = Some(ErrorKind::InvalidInteger(reason));
        Err(Failed)
    }

    #[inline(never)]
    fn integer_prefixed(
        &mut self,
        bytes: &'de [u8],
        span: Span,
        shift: u32,
    ) -> Result<Item<'de>, Failed> {
        let max_digit = (1i8 << shift) - 1;
        let invalid_msg = match shift {
            4 => "invalid digit for hexadecimal",
            3 => "invalid digit for octal",
            _ => "invalid digit for binary",
        };
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        let mut error_span = span;
        let reason = 'error: {
            if bytes.is_empty() {
                break 'error "no digits after prefix";
            }

            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        let pos = span.start + 2 + i as u32;
                        error_span = Span::new(pos, pos + 1);
                        break 'error "underscores must be between two digits";
                    }
                    prev_underscore = true;
                    i += 1;
                    continue;
                }
                let digit = HEX[b as usize];
                if digit < 0 || digit > max_digit {
                    let pos = span.start + 2 + i as u32;
                    error_span = Span::new(pos, pos + 1);
                    break 'error invalid_msg;
                }
                has_digit = true;
                prev_underscore = false;
                if acc >> (64 - shift) != 0 {
                    break 'error "integer overflow";
                }
                acc = (acc << shift) | digit as u64;
                i += 1;
            }

            if !has_digit {
                break 'error "no digits after prefix";
            }
            if prev_underscore {
                let pos = span.start + 2 + bytes.len() as u32 - 1;
                error_span = Span::new(pos, pos + 1);
                break 'error "underscores must be between two digits";
            }

            if acc > i64::MAX as u64 {
                break 'error "integer overflow";
            }
            return Ok(Item::integer_spanned(acc as i64, span));
        };
        self.error_span = error_span;
        self.error_kind = Some(ErrorKind::InvalidInteger(reason));
        Err(Failed)
    }

    fn float(
        &mut self,
        start: u32,
        end: u32,
        s: &'de str,
        after_decimal: Option<&'de str>,
        sign: u8,
    ) -> Result<f64, Failed> {
        let s_start = start as usize;
        let s_end = end as usize;

        // TOML forbids leading zeros in the integer part (e.g. 00.5, -01.0).
        if let [b'0', b'0'..=b'9' | b'_', ..] = s.as_bytes() {
            return Err(self.set_error(
                s_start,
                Some(s_end),
                ErrorKind::InvalidFloat("leading zeros are not allowed"),
            ));
        }

        // Safety: no other Scratch or arena.alloc() is active during float parsing.
        let mut scratch = unsafe { self.arena.scratch() };

        if sign == 0 {
            scratch.push(b'-');
        }
        if !scratch.push_strip_underscores(s.as_bytes()) {
            return Err(self.set_error(
                s_start,
                Some(s_end),
                ErrorKind::InvalidFloat("underscores must be between two digits"),
            ));
        }

        let mut last = s;

        if let Some(after) = after_decimal {
            if !matches!(after.as_bytes().first(), Some(b'0'..=b'9')) {
                return Err(self.set_error(
                    s_start,
                    Some(s_end),
                    ErrorKind::InvalidFloat("nothing after decimal point"),
                ));
            }
            scratch.push(b'.');
            if !scratch.push_strip_underscores(after.as_bytes()) {
                return Err(self.set_error(
                    s_start,
                    Some(s_end),
                    ErrorKind::InvalidFloat("underscores must be between two digits"),
                ));
            }
            last = after;
        }

        // When the last keylike token ends with e/E, the '+' and exponent
        // digits are separate tokens in the stream ('-' IS keylike so
        // e.g. "1e-5" stays in one token and needs no special handling).
        if matches!(last.as_bytes().last(), Some(b'e' | b'E')) {
            self.eat_byte(b'+');
            match self.peek_byte() {
                Some(b) if is_keylike_byte(b) && b != b'-' => {
                    let next = self.read_keylike();
                    if !scratch.push_strip_underscores(next.as_bytes()) {
                        return Err(self.set_error(
                            s_start,
                            Some(s_end),
                            ErrorKind::InvalidFloat("exponent requires at least one digit"),
                        ));
                    }
                }
                _ => {
                    return Err(self.set_error(
                        s_start,
                        Some(s_end),
                        ErrorKind::InvalidFloat("exponent requires at least one digit"),
                    ));
                }
            }
        }

        // Scratch is not committed — arena pointer stays unchanged, space is
        // reused by subsequent allocations.
        // SAFETY: scratch contains only ASCII digits, signs, dots, and 'e'/'E'
        // copied from validated input via push_strip_underscores.
        let n: f64 = match unsafe { std::str::from_utf8_unchecked(scratch.as_bytes()) }.parse() {
            Ok(n) => n,
            // std's float parse error is always just "invalid float literal"
            Err(_) => {
                return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidFloat("")));
            }
        };
        if n.is_finite() {
            Ok(n)
        } else {
            Err(self.set_error(
                s_start,
                Some(s_end),
                ErrorKind::InvalidFloat("float overflow"),
            ))
        }
    }

    fn value(&mut self, depth_remaining: i16) -> Result<Item<'de>, Failed> {
        let at = self.cursor;
        let Some(byte) = self.peek_byte() else {
            return Err(self.set_error(self.bytes.len(), None, ErrorKind::UnexpectedEof));
        };
        let sign = match byte {
            b'"' | b'\'' => {
                self.cursor += 1;
                return match self.read_string(self.cursor - 1, byte) {
                    Ok((key, _)) => Ok(Item::string_spanned(key.name, key.span)),
                    Err(e) => Err(e),
                };
            }
            b'{' => {
                let start = self.cursor as u32;
                self.cursor += 1;
                let mut table = crate::item::table::InnerTable::new();
                if let Err(err) = self.inline_table_contents(&mut table, depth_remaining - 1) {
                    return Err(err);
                }
                return Ok(Item::table_frozen(
                    table,
                    Span::new(start, self.cursor as u32),
                ));
            }
            b'[' => {
                let start = self.cursor as u32;
                self.cursor += 1;
                let mut arr = crate::item::array::InternalArray::new();
                if let Err(err) = self.array_contents(&mut arr, depth_remaining - 1) {
                    return Err(err);
                };
                return Ok(Item::array(arr, Span::new(start, self.cursor as u32)));
            }
            b't' => {
                return if self.bytes[self.cursor..].starts_with(b"true") {
                    self.cursor += 4;
                    Ok(Item::boolean(
                        true,
                        Span::new(at as u32, self.cursor as u32),
                    ))
                } else {
                    Err(self.set_error(
                        at,
                        Some(self.cursor),
                        ErrorKind::Wanted {
                            expected: &"the literal `true`",
                            found: &"something else",
                        },
                    ))
                };
            }
            b'f' => {
                self.cursor += 1;
                return if self.bytes[self.cursor..].starts_with(b"alse") {
                    self.cursor += 4;
                    Ok(Item::boolean(
                        false,
                        Span::new(at as u32, self.cursor as u32),
                    ))
                } else {
                    Err(self.set_error(
                        at,
                        Some(self.cursor),
                        ErrorKind::Wanted {
                            expected: &"the literal `false`",
                            found: &"something else",
                        },
                    ))
                };
            }
            b'-' => {
                self.cursor += 1;
                0
            }
            b'+' => {
                self.cursor += 1;
                1
            }
            _ => 2,
        };

        let key = self.read_keylike();

        let end = self.cursor as u32;
        match key {
            "inf" => {
                return Ok(Item::float_spanned(
                    if sign != 0 {
                        f64::INFINITY
                    } else {
                        f64::NEG_INFINITY
                    },
                    Span::new(at as u32, end),
                ));
            }
            "nan" => {
                return Ok(Item::float_spanned(
                    if sign != 0 {
                        f64::NAN.copysign(1.0)
                    } else {
                        f64::NAN.copysign(-1.0)
                    },
                    Span::new(at as u32, end),
                ));
            }
            _ => (),
        }

        if let [b'0'..=b'9', ..] = key.as_bytes() {
            self.number(at as u32, end, key, sign)
        } else if byte == b'\r' {
            Err(self.set_error(at, None, ErrorKind::Unexpected('\r')))
        } else if sign != 2 {
            Err(self.set_error(
                at,
                Some(self.cursor),
                ErrorKind::InvalidInteger("expected digit after sign"),
            ))
        } else if key.is_empty() {
            Err(self.set_error(at, None, ErrorKind::Unexpected(self.next_char_for_error())))
        } else {
            Err(self.set_error(at, Some(self.cursor), ErrorKind::UnquotedString))
        }
    }

    fn inline_table_contents(
        &mut self,
        out: &mut crate::item::table::InnerTable<'de>,
        depth_remaining: i16,
    ) -> Result<(), Failed> {
        if depth_remaining < 0 {
            return Err(self.set_error(
                self.cursor,
                None,
                ErrorKind::OutOfRange("Max recursion depth exceeded"),
            ));
        }
        if let Err(e) = self.eat_inline_table_whitespace() {
            return Err(e);
        }
        if self.eat_byte(b'}') {
            return Ok(());
        }
        loop {
            let mut table_ref: &mut crate::item::table::InnerTable<'de> = &mut *out;
            let mut key = match self.read_table_key() {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            self.eat_whitespace();
            while self.eat_byte(b'.') {
                self.eat_whitespace();
                table_ref = match self.navigate_dotted_key(table_ref, key) {
                    Ok(t) => t,
                    Err(e) => return Err(e),
                };
                key = match self.read_table_key() {
                    Ok(k) => k,
                    Err(e) => return Err(e),
                };
                self.eat_whitespace();
            }
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            if let Err(e) = self.expect_byte(b'=') {
                return Err(e);
            }
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            {
                let val = match self.value(depth_remaining) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if let Err(e) = self.insert_value(table_ref, key, val) {
                    return Err(e);
                }
            }

            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            if self.eat_byte(b'}') {
                return Ok(());
            }
            if !self.eat_byte(b',') {
                let start = self.cursor;
                if self.peek_byte().is_none() {
                    return Err(self.set_error(start, None, ErrorKind::UnclosedInlineTable));
                }
                let (_found_desc, end) = self.scan_token_desc_and_end();
                return Err(self.set_error(start, Some(end), ErrorKind::MissingInlineTableComma));
            }
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            if self.eat_byte(b'}') {
                return Ok(());
            }
        }
    }

    fn array_contents(
        &mut self,
        out: &mut crate::item::array::InternalArray<'de>,
        depth_remaining: i16,
    ) -> Result<(), Failed> {
        if depth_remaining < 0 {
            return Err(self.set_error(
                self.cursor,
                None,
                ErrorKind::OutOfRange("Max recursion depth exceeded"),
            ));
        }
        loop {
            if let Err(e) = self.eat_intermediate() {
                return Err(e);
            }
            if self.eat_byte(b']') {
                return Ok(());
            }
            match self.value(depth_remaining) {
                Ok(value) => out.push(value, self.arena),
                Err(e) => return Err(e),
            };
            if let Err(e) = self.eat_intermediate() {
                return Err(e);
            }
            if !self.eat_byte(b',') {
                break;
            }
        }
        if let Err(e) = self.eat_intermediate() {
            return Err(e);
        }
        if self.eat_byte(b']') {
            return Ok(());
        }
        let start = self.cursor;
        if self.peek_byte().is_none() {
            return Err(self.set_error(start, None, ErrorKind::UnclosedArray));
        }
        let (_found_desc, end) = self.scan_token_desc_and_end();
        Err(self.set_error(start, Some(end), ErrorKind::MissingArrayComma))
    }

    #[inline(always)]
    fn eat_inline_table_whitespace(&mut self) -> Result<(), Failed> {
        loop {
            match self.peek_byte() {
                Some(b' ' | b'\t' | b'\n') => self.cursor += 1,
                Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => self.cursor += 2,
                Some(b'#') => match self.eat_comment() {
                    Ok(_) => {}
                    Err(e) => return Err(e),
                },
                _ => return Ok(()),
            }
        }
    }

    #[inline(always)]
    fn eat_intermediate(&mut self) -> Result<(), Failed> {
        loop {
            match self.peek_byte() {
                Some(b' ' | b'\t' | b'\n') => self.cursor += 1,
                Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => self.cursor += 2,
                Some(b'#') => match self.eat_comment() {
                    Ok(_) => {}
                    Err(e) => return Err(e),
                },
                _ => return Ok(()),
            }
        }
    }

    /// Navigate into an existing or new table for a dotted-key intermediate
    /// segment. Checks frozen and header bits.
    /// New tables are created with the `DOTTED` tag.
    fn navigate_dotted_key<'t>(
        &mut self,
        table: &'t mut InnerTable<'de>,
        key: Key<'de>,
    ) -> Result<&'t mut InnerTable<'de>, Failed> {
        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, value) = &mut table.entries_mut()[idx];
            let ok = value.is_table() && !value.is_frozen() && !value.has_header_bit();

            if !ok {
                return Err(self.set_error(
                    key.span.start as usize,
                    Some(key.span.end as usize),
                    ErrorKind::DottedKeyInvalidType {
                        first: existing_key.span,
                    },
                ));
            }
            // Promote IMPLICIT -> DOTTED: an implicit table created by a section
            // header intermediate (e.g. `b` in `[a.b.c]`) is now being touched
            // by a dotted key in the body (e.g. `b.x = 1` inside `[a]`).
            if value.is_implicit_table() {
                // SAFETY: is_table() verified by the guard above.
                let t = unsafe { value.as_table_mut_unchecked() };
                t.set_dotted_flag();
                t.set_span_start(key.span.start);
                t.set_span_end(key.span.end);
            }
            // SAFETY: is_table() verified by the guard above.
            unsafe { Ok(value.as_inner_table_mut_unchecked()) }
        } else {
            let span = key.span;
            let inserted = self.insert_value_known_to_be_unique(
                table,
                key,
                Item::table_dotted(InnerTable::new(), span),
            );
            // SAFETY: Item::table_dotted() produces a table-tagged item.
            unsafe { Ok(inserted.as_inner_table_mut_unchecked()) }
        }
    }

    /// Navigate an intermediate segment of a table header (e.g. `a` in `[a.b.c]`).
    /// Creates implicit tables (no flag bits) if not found.
    /// Handles arrays-of-tables by navigating into the last element.
    ///
    /// Returns a `Table` view of the table navigated into.
    fn navigate_header_intermediate<'b>(
        &mut self,
        st: &'b mut Table<'de>,
        key: Key<'de>,
    ) -> Result<&'b mut Table<'de>, Failed> {
        let table = &mut st.value;

        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, existing) = &mut table.entries_mut()[idx];
            let existing_span = existing_key.span;

            // Note: I would use safey accessor heres but that would cause issues
            // with NLL limitations.
            if existing.is_table() {
                if existing.is_frozen() {
                    return Err(self.set_duplicate_key_error(existing_span, key.span));
                }
                // SAFETY: is_table() verified by the preceding check.
                unsafe { Ok(existing.as_table_mut_unchecked()) }
            } else if existing.is_aot() {
                // unwrap is safe since we just check it's an array of tables and thus a array.
                let arr = existing.as_array_mut().unwrap();
                self.push_path(PathComponent::Index(arr.len() - 1));
                // unwrap is safe as array's of tables always have atleast one value by construction
                let last = arr.last_mut().unwrap();
                if !last.is_table() {
                    return Err(self.set_duplicate_key_error(existing_span, key.span));
                }
                // SAFETY: last.is_table() verified by the preceding check.
                unsafe { Ok(last.as_table_mut_unchecked()) }
            } else {
                Err(self.set_duplicate_key_error(existing_span, key.span))
            }
        } else {
            let span = key.span;
            let inserted = self.insert_value_known_to_be_unique(
                table,
                key,
                Item::table(InnerTable::new(), span),
            );
            // SAFETY: Item::table() produces a table-tagged item.
            unsafe { Ok(inserted.as_table_mut_unchecked()) }
        }
    }
    fn insert_value_known_to_be_unique<'t>(
        &mut self,
        table: &'t mut InnerTable<'de>,
        key: Key<'de>,
        item: Item<'de>,
    ) -> &'t mut item::Item<'de> {
        let len = table.len();
        if len >= INDEXED_TABLE_THRESHOLD {
            // SAFETY: len >= INDEXED_TABLE_THRESHOLD (>= 6), so the table is non-empty.
            let table_id = unsafe { table.first_key_span_start_unchecked() };
            if len == INDEXED_TABLE_THRESHOLD {
                for (i, (key, _)) in table.entries().iter().enumerate() {
                    self.index.insert(KeyRef::new(key.as_str(), table_id), i);
                }
            }
            self.index.insert(KeyRef::new(key.as_str(), table_id), len);
        }
        &mut table.insert(key, item, self.arena).1
    }

    /// Handle the final segment of a standard table header `[a.b.c]`.
    ///
    /// Returns the [`Ctx`] for the table that subsequent key-value pairs
    /// should be inserted into.
    fn navigate_header_table_final<'b>(
        &mut self,
        st: &'b mut Table<'de>,
        key: Key<'de>,
        header_start: u32,
        header_end: u32,
    ) -> Result<Ctx<'b, 'de>, Failed> {
        let table = &mut st.value;

        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, existing) = &mut table.entries_mut()[idx];
            let first_key_span = existing_key.span;

            if !existing.is_table() || existing.is_frozen() {
                return Err(self.set_duplicate_key_error(first_key_span, key.span));
            }
            if existing.has_header_bit() {
                return Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::DuplicateTable {
                        name: key.span,
                        first: existing.span_unchecked(),
                    },
                ));
            }
            if existing.has_dotted_bit() {
                return Err(self.set_duplicate_key_error(first_key_span, key.span));
            }
            // SAFETY: is_table() verified by the preceding checks.
            let table = unsafe { existing.as_table_mut_unchecked() };
            table.set_header_flag();
            table.set_span_start(header_start);
            table.set_span_end(header_end);
            Ok(Ctx {
                table,
                array_end_span: None,
            })
        } else {
            let inserted = self.insert_value_known_to_be_unique(
                table,
                key,
                Item::table_header(InnerTable::new(), Span::new(header_start, header_end)),
            );
            Ok(Ctx {
                // SAFETY: Item::table_header() produces a table-tagged item.
                table: unsafe { inserted.as_table_mut_unchecked() },
                array_end_span: None,
            })
        }
    }

    /// Handle the final segment of an array-of-tables header `[[a.b.c]]`.
    ///
    /// Returns the [`Ctx`] for the new table entry that subsequent key-value
    /// pairs should be inserted into.
    fn navigate_header_array_final<'b>(
        &mut self,
        st: &'b mut Table<'de>,
        key: Key<'de>,
        header_start: u32,
        header_end: u32,
    ) -> Result<Ctx<'b, 'de>, Failed> {
        let table = &mut st.value;

        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, existing) = &mut table.entries_mut()[idx];
            let first_key_span = existing_key.span;

            if existing.is_aot() {
                // SAFETY: is_aot verified by the preceding check, which implies is_array().
                let (end_flag, arr) = unsafe { existing.split_array_end_flag() };
                let entry_span = Span::new(header_start, header_end);
                arr.push(
                    Item::table_header(InnerTable::new(), entry_span),
                    self.arena,
                );
                self.push_path(PathComponent::Index(arr.len() - 1));
                let entry = arr.last_mut().unwrap();
                Ok(Ctx {
                    // SAFETY: Item::table_header() produces a table-tagged item.
                    table: unsafe { entry.as_table_mut_unchecked() },
                    array_end_span: Some(end_flag),
                })
            } else if existing.is_table() {
                Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::RedefineAsArray,
                ))
            } else {
                Err(self.set_duplicate_key_error(first_key_span, key.span))
            }
        } else {
            let entry_span = Span::new(header_start, header_end);
            let first_entry = Item::table_header(InnerTable::new(), entry_span);
            let array_span = Span::new(header_start, header_end);
            let array_val = Item::array_aot(
                crate::item::array::InternalArray::with_single(first_entry, self.arena),
                array_span,
            );
            let inserted = self.insert_value_known_to_be_unique(table, key, array_val);
            self.push_path(PathComponent::Index(0));
            // SAFETY: Item::array_aot() produces an array-tagged item.
            let (end_flag, arr) = unsafe { inserted.split_array_end_flag() };
            let entry = arr.last_mut().unwrap();
            Ok(Ctx {
                // SAFETY: Item::table_header() (used in with_single) produces a table-tagged item.
                table: unsafe { entry.as_table_mut_unchecked() },
                array_end_span: Some(end_flag),
            })
        }
    }

    /// Insert a value into a table, checking for duplicates.
    fn insert_value(
        &mut self,
        table: &mut InnerTable<'de>,
        key: Key<'de>,
        item: Item<'de>,
    ) -> Result<(), Failed> {
        if table.len() < INDEXED_TABLE_THRESHOLD {
            for (existing_key, _) in table.entries() {
                if existing_key.as_str() == key.name {
                    return Err(self.set_duplicate_key_error(existing_key.span, key.span));
                }
            }
            table.insert(key, item, self.arena);
            return Ok(());
        }
        // SAFETY: len >= INDEXED_TABLE_THRESHOLD (>= 6), so the table is non-empty.
        let table_id = unsafe { table.first_key_span_start_unchecked() };

        // Note: if find a duplicate we bail out, terminating the parsing with an error.
        // Even if we did end up re-inserting no issues would come of it.
        if table.len() == INDEXED_TABLE_THRESHOLD {
            for (i, (key, _)) in table.entries().iter().enumerate() {
                // Wish I could use insert_unique here but that would require
                // pulling in hashbrown :(
                self.index.insert(KeyRef::new(key.as_str(), table_id), i);
            }
        }

        match self.index.entry(KeyRef::new(key.as_str(), table_id)) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                let idx = *occupied_entry.get();
                let (existing_key, _) = &table.entries()[idx];
                Err(self.set_duplicate_key_error(existing_key.span, key.span))
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(table.len());
                table.insert(key, item, self.arena);
                Ok(())
            }
        }
    }

    /// Look up a key name in a table, returning its entry index.
    /// Uses the hash index for tables at or above the threshold, otherwise
    /// falls back to a linear scan.
    fn indexed_find(&self, table: &InnerTable<'de>, name: &str) -> Option<usize> {
        // NOTE: I would return a reference to actual entry here, however this
        // runs into all sorts of NLL limitations.
        if table.len() > INDEXED_TABLE_THRESHOLD {
            // SAFETY: len > INDEXED_TABLE_THRESHOLD (> 6), so the table is non-empty.
            let first_key_span = unsafe { table.first_key_span_start_unchecked() };
            self.index.get(&KeyRef::new(name, first_key_span)).copied()
        } else {
            table.find_index(name)
        }
    }

    fn skip_recovery_string(&mut self) {
        let delim = self.bytes[self.cursor];
        self.cursor += 1;
        let multiline = self.peek_byte() == Some(delim) && self.peek_byte_at(1) == Some(delim);
        if multiline {
            self.cursor += 2;
            loop {
                match self.peek_byte() {
                    None => return,
                    Some(b)
                        if b == delim
                            && self.peek_byte_at(1) == Some(delim)
                            && self.peek_byte_at(2) == Some(delim) =>
                    {
                        self.cursor += 3;
                        while self.peek_byte() == Some(delim) {
                            self.cursor += 1;
                        }
                        return;
                    }
                    Some(b'\\') if delim == b'"' => self.cursor += 2,
                    _ => self.cursor += 1,
                }
            }
        }
        loop {
            match self.peek_byte() {
                None | Some(b'\n') => return,
                Some(b) if b == delim => {
                    self.cursor += 1;
                    return;
                }
                Some(b'\\') if delim == b'"' => self.cursor += 2,
                _ => self.cursor += 1,
            }
        }
    }

    fn at_statement_start(&self) -> bool {
        matches!(self.peek_byte(), None | Some(b'[') | Some(b'#'))
            || matches!(self.peek_byte(), Some(b) if is_keylike_byte(b) || b == b'"' || b == b'\'')
    }

    fn skip_to_next_statement(&mut self) {
        loop {
            match self.peek_byte() {
                None => return,
                Some(b'\n') => {
                    self.cursor += 1;
                    let saved = self.cursor;
                    while matches!(self.peek_byte(), Some(b' ' | b'\t')) {
                        self.cursor += 1;
                    }
                    if self.at_statement_start() {
                        self.cursor = saved;
                        return;
                    }
                    self.cursor = saved;
                }
                Some(b'"' | b'\'') => self.skip_recovery_string(),
                Some(b'#') => {
                    self.cursor += 1;
                    while let Some(b) = self.peek_byte() {
                        if b == b'\n' {
                            break;
                        }
                        self.cursor += 1;
                    }
                }
                _ => self.cursor += 1,
            }
        }
    }

    const MAX_RECOVER_ERRORS: usize = 25;

    fn try_recover(&mut self) -> bool {
        if !self.recovering {
            return false;
        }
        let error = self.take_error();
        self.errors.push(error);
        self.path_len = 0;
        let at_line_start = self.cursor == 0 || self.bytes.get(self.cursor - 1) == Some(&b'\n');
        if at_line_start && self.at_statement_start() {
            return self.errors.len() < Self::MAX_RECOVER_ERRORS;
        }
        let _before = self.cursor;
        self.skip_to_next_statement();
        debug_assert!(
            self.cursor > _before || self.cursor >= self.bytes.len(),
            "skip_to_next_statement did not advance cursor from {_before}",
        );
        self.errors.len() < Self::MAX_RECOVER_ERRORS
    }

    fn parse_document(&mut self, root_st: &mut Table<'de>) -> Result<(), Failed> {
        let mut ctx = Ctx {
            table: root_st,
            array_end_span: None,
        };

        #[cfg(debug_assertions)]
        let mut _prev_loop_cursor = usize::MAX;

        loop {
            #[cfg(debug_assertions)]
            if self.recovering {
                debug_assert!(
                    self.cursor != _prev_loop_cursor || self.peek_byte().is_none(),
                    "parse_document recovery loop stalled at cursor {}",
                    self.cursor,
                );
                _prev_loop_cursor = self.cursor;
            }

            self.eat_whitespace();
            match self.eat_comment() {
                Ok(true) => continue,
                Ok(false) => {}
                Err(_) => {
                    if !self.try_recover() {
                        return Err(Failed);
                    }
                    continue;
                }
            }
            if self.eat_newline() {
                continue;
            }

            match self.peek_byte() {
                None => break,
                Some(b'[') => {
                    ctx = match self.process_table_header(root_st) {
                        Ok(c) => c,
                        Err(_) => {
                            if !self.try_recover() {
                                return Err(Failed);
                            }
                            Ctx {
                                table: root_st,
                                array_end_span: None,
                            }
                        }
                    };
                }
                Some(b'\r') => {
                    self.set_error(self.cursor, None, ErrorKind::Unexpected('\r'));
                    if !self.try_recover() {
                        return Err(Failed);
                    }
                    continue;
                }
                Some(_) => {
                    if let Err(_) = self.process_key_value(&mut ctx) {
                        if !self.try_recover() {
                            return Err(Failed);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn process_table_header<'b>(
        &mut self,
        root_st: &'b mut Table<'de>,
    ) -> Result<Ctx<'b, 'de>, Failed> {
        self.path_len = 0;
        let header_start = self.cursor as u32;
        if let Err(e) = self.expect_byte(b'[') {
            return Err(e);
        }
        let is_array = self.eat_byte(b'[');

        let mut current = root_st;

        self.eat_whitespace();
        let mut key = match self.read_table_key() {
            Ok(k) => k,
            Err(e) => return Err(e),
        };
        loop {
            if self.eat_whitespace_to() == Some(b'.') {
                self.cursor += 1;
                self.eat_whitespace();
                self.push_path(PathComponent::Key(key));
                current = match self.navigate_header_intermediate(current, key) {
                    Ok(p) => p,
                    Err(e) => return Err(e),
                };
                key = match self.read_table_key() {
                    Ok(k) => k,
                    Err(e) => return Err(e),
                };
            } else {
                break;
            }
        }
        if let Err(e) = self.expect_byte(b']') {
            return Err(e);
        }
        if is_array && let Err(e) = self.expect_byte(b']') {
            return Err(e);
        }

        self.eat_whitespace();
        match self.eat_comment() {
            Ok(true) => {}
            Ok(false) => {
                if let Err(e) = self.eat_newline_or_eof() {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }
        let header_end = self.cursor as u32;

        self.push_path(PathComponent::Key(key));
        if is_array {
            self.navigate_header_array_final(current, key, header_start, header_end)
        } else {
            self.navigate_header_table_final(current, key, header_start, header_end)
        }
    }

    fn process_key_value(&mut self, ctx: &mut Ctx<'_, 'de>) -> Result<(), Failed> {
        let saved_path_len = self.path_len;
        let line_start = self.cursor as u32;
        // Borrow the Table payload from the Table. NLL drops this
        // borrow at its last use (the insert_value call), freeing ctx.st
        // for the span updates that follow.
        let mut table_ref: &mut InnerTable<'de> = &mut ctx.table.value;

        let mut key = match self.read_table_key() {
            Ok(k) => k,
            Err(e) => return Err(e),
        };
        self.eat_whitespace();

        while self.eat_byte(b'.') {
            self.eat_whitespace();
            self.push_path(PathComponent::Key(key));
            table_ref = match self.navigate_dotted_key(table_ref, key) {
                Ok(t) => t,
                Err(e) => return Err(e),
            };
            key = match self.read_table_key() {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            self.eat_whitespace();
        }

        self.push_path(PathComponent::Key(key));

        if let Err(e) = self.expect_byte(b'=') {
            return Err(e);
        }
        self.eat_whitespace();
        let val = match self.value(MAX_RECURSION_DEPTH) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let line_end = self.cursor as u32;

        self.eat_whitespace();
        match self.eat_comment() {
            Ok(true) => {}
            Ok(false) => {
                if let Err(e) = self.eat_newline_or_eof() {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }

        if let Err(e) = self.insert_value(table_ref, key, val) {
            return Err(e);
        }

        self.path_len = saved_path_len;

        let start = ctx.table.span_start();
        ctx.table.set_span_start(start.min(line_start));
        ctx.table.extend_span_end(line_end);

        if let Some(end_flag) = &mut ctx.array_end_span {
            let old = **end_flag;
            let current = old >> item::FLAG_SHIFT;
            **end_flag = (current.max(line_end) << item::FLAG_SHIFT) | (old & item::FLAG_MASK);
        }

        Ok(())
    }
}

/// The result of parsing a TOML document.
///
/// A `Document` wraps the parsed [`Table`] tree and, when the `from-toml`
/// feature is enabled, a [`Context`](crate::Context) that accumulates errors.
///
/// Access values via index operators (`doc["key"]`) which return
/// [`MaybeItem`](crate::MaybeItem), or use [`helper`](Self::helper) /
/// [`to`](Self::to) for typed conversion.
///
/// # Examples
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let doc = toml_spanner::parse("name = 'world'", &arena).unwrap();
/// assert_eq!(doc["name"].as_str(), Some("world"));
/// ```
pub struct Document<'de> {
    pub(crate) table: Table<'de>,
    #[cfg(feature = "from-toml")]
    pub ctx: crate::de::Context<'de>,
}

impl<'de> Document<'de> {
    /// Consumes the document and returns the underlying [`Table`].
    pub fn into_table(self) -> Table<'de> {
        self.table
    }

    /// Converts the root table into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        self.table.into_item()
    }

    /// Returns a shared reference to the root table.
    pub fn table(&self) -> &Table<'de> {
        &self.table
    }

    /// Returns disjoint borrows of the [`Context`](crate::Context) and the
    /// root [`Table`].
    ///
    /// This is useful when you need to pass the context into
    /// [`TableHelper::new`](crate::TableHelper::new) while still holding
    /// a reference to the table.
    #[cfg(feature = "from-toml")]
    pub fn split(&mut self) -> (&mut crate::de::Context<'de>, &Table<'de>) {
        (&mut self.ctx, &self.table)
    }

    /// Returns the parser's hash index for O(1) key lookups in large tables.
    ///
    /// Used internally by [`reproject`](crate::reproject).
    #[cfg(feature = "to-toml")]
    pub(crate) fn table_index(&self) -> &crate::item::table::TableIndex<'de> {
        // `to-toml` implies `from-toml`, so ctx is always available here.
        &self.ctx.index
    }

    /// Detects the indent style from parsed item spans.
    ///
    /// Finds the first array element or inline table entry on its own
    /// line and measures the preceding whitespace.
    #[cfg(feature = "to-toml")]
    pub(crate) fn detect_indent(&self) -> crate::emit::Indent {
        let src = self.ctx.source().as_bytes();
        if let Some(indent) = detect_indent_in_table(&self.table, src) {
            return indent;
        }
        crate::emit::Indent::default()
    }
}

#[cfg(feature = "from-toml")]
impl<'de> Document<'de> {
    /// Creates a [`TableHelper`] for the root table.
    ///
    /// This is the typical entry point for typed extraction. Extract fields
    /// with [`TableHelper::required`](crate::TableHelper::required) and
    /// [`TableHelper::optional`](crate::TableHelper::optional), then call
    /// [`TableHelper::expect_empty`](crate::TableHelper::expect_empty) to
    /// reject unknown keys.
    pub fn helper<'ctx>(&'ctx mut self) -> TableHelper<'ctx, 'ctx, 'de> {
        TableHelper::new(&mut self.ctx, &self.table)
    }

    /// Converts the root table into a typed value `T` via [`FromToml`](crate::FromToml).
    ///
    /// # Errors
    ///
    /// Returns [`FromTomlError`](crate::FromTomlError) containing all
    /// accumulated errors.
    pub fn to<T>(&mut self) -> Result<T, crate::de::FromTomlError>
    where
        T: crate::de::FromToml<'de>,
    {
        let result = T::from_toml(&mut self.ctx, self.table.as_item());
        crate::de::compute_paths(&self.table, &mut self.ctx.errors);
        match result {
            Ok(v) if self.ctx.errors.is_empty() => Ok(v),
            _ => Err(crate::de::FromTomlError {
                errors: std::mem::take(&mut self.ctx.errors),
            }),
        }
    }

    /// Returns the accumulated errors.
    pub fn errors(&self) -> &[Error] {
        &self.ctx.errors
    }

    /// Returns `true` if any errors have been recorded.
    pub fn has_errors(&self) -> bool {
        !self.ctx.errors.is_empty()
    }
}

impl<'de> std::ops::Index<&str> for Document<'de> {
    type Output = MaybeItem<'de>;

    fn index(&self, key: &str) -> &Self::Output {
        &self.table[key]
    }
}

impl std::fmt::Debug for Document<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.table.fmt(f)
    }
}

/// Parses a TOML document and returns a [`Document`] containing the parsed tree.
///
/// Both `s` and `arena` must outlive the returned [`Document`] because parsed
/// values borrow directly from the input string and allocate escaped strings
/// into the arena.
///
/// # Errors
///
/// Returns an [`Error`] on the first syntax error encountered.
///
/// # Examples
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let doc = toml_spanner::parse("key = 'value'", &arena).unwrap();
/// assert_eq!(doc["key"].as_str(), Some("value"));
/// ```
#[inline(never)]
pub fn parse<'de>(document: &'de str, arena: &'de Arena) -> Result<Document<'de>, Error> {
    // Tag bits use the low 3 bits of start_and_tag, limiting span.start to
    // 28 bits (256 MiB). The flag state uses the low 3 bits of end_and_flag,
    // and bit 31 is the variant discriminator, limiting span.end to 28 bits
    // (256 MiB).
    const MAX_SIZE: usize = (1u32 << 28) as usize;

    if document.len() >= MAX_SIZE {
        return Err(Error::new(ErrorKind::FileTooLarge, Span::new(0, 0)));
    }

    let mut root_st = Table::new_spanned(Span::new(0, document.len() as u32));
    let mut parser = Parser::new(document, arena);
    match parser.parse_document(&mut root_st) {
        Ok(()) => {}
        Err(_) => return Err(parser.take_error()),
    }
    // Note that root is about the drop (but doesn't implement drop), so we can take
    // ownership of this table.
    // todo don't do this
    Ok(Document {
        table: root_st,
        #[cfg(feature = "from-toml")]
        ctx: crate::de::Context {
            errors: Vec::new(),
            index: parser.index,
            arena,
            source: document,
        },
    })
}

/// Parses a TOML document in recovery mode, accumulating errors instead of
/// stopping on the first one.
///
/// Unlike [`parse`], this function always returns a [`Document`] (never
/// `Err`). Syntax errors are collected into the document's
/// [`Context::errors`](crate::Context) alongside any later deserialization
/// errors. Valid portions of the input are still parsed into the tree.
///
/// Recovery is line-based: when a statement fails to parse, the parser
/// skips to the next line and continues. At most 25 errors are collected
/// before parsing stops.
///
/// # Examples
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let mut doc = toml_spanner::parse_recoverable("key = 'value'\nbad =\n", &arena);
/// assert_eq!(doc["key"].as_str(), Some("value"));
/// assert!(!doc.errors().is_empty());
/// ```
#[cfg(feature = "from-toml")]
pub fn parse_recoverable<'de>(document: &'de str, arena: &'de Arena) -> Document<'de> {
    const MAX_SIZE: usize = (1u32 << 28) as usize;
    let mut parser = Parser::new(document, arena);
    parser.recovering = true;

    if document.len() >= MAX_SIZE {
        parser
            .errors
            .push(Error::new(ErrorKind::FileTooLarge, Span::new(0, 0)));
        return Document {
            table: Table::new_spanned(Span::new(0, 0)),
            ctx: crate::de::Context {
                errors: parser.errors,
                index: parser.index,
                arena,
                source: document,
            },
        };
    }

    let mut root_st = Table::new_spanned(Span::new(0, document.len() as u32));
    let failed = parser.parse_document(&mut root_st).is_err();

    if failed {
        if let Some(kind) = parser.error_kind.take() {
            parser.errors.push(Error::new_with_path(
                kind,
                parser.error_span,
                parser.build_error_path(),
            ));
        }
    }

    Document {
        table: root_st,
        ctx: crate::de::Context {
            errors: parser.errors,
            index: parser.index,
            arena,
            source: document,
        },
    }
}

#[inline]
fn is_keylike_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn byte_describe(b: u8) -> &'static &'static str {
    match b {
        b'\n' => &"a newline",
        b' ' | b'\t' => &"whitespace",
        b'=' => &"an equals",
        b'.' => &"a period",
        b',' => &"a comma",
        b':' => &"a colon",
        b'+' => &"a plus",
        b'{' => &"a left brace",
        b'}' => &"a right brace",
        b'[' => &"a left bracket",
        b']' => &"a right bracket",
        b'\'' | b'"' => &"a string",
        _ if is_keylike_byte(b) => &"an identifier",
        _ => &"a character",
    }
}

#[cfg(feature = "to-toml")]
fn detect_indent_in_table(table: &Table<'_>, src: &[u8]) -> Option<crate::emit::Indent> {
    use crate::item::{ArrayStyle, TableStyle, Value};
    for (_, item) in table {
        match item.value() {
            Value::Array(arr) => {
                if arr.style() == ArrayStyle::Inline {
                    for elem in arr {
                        let span = elem.span();
                        if !span.is_empty() {
                            if let Some(indent) = indent_from_span(src, span.start as usize) {
                                return Some(indent);
                            }
                        }
                    }
                }
                for elem in arr {
                    if let Some(sub) = elem.as_table() {
                        if let Some(indent) = detect_indent_in_table(sub, src) {
                            return Some(indent);
                        }
                    }
                }
            }
            Value::Table(sub) => {
                if sub.style() == TableStyle::Inline {
                    for (key, _) in sub {
                        if !key.span.is_empty() {
                            if let Some(indent) = indent_from_span(src, key.span.start as usize) {
                                return Some(indent);
                            }
                        }
                    }
                }
                if let Some(indent) = detect_indent_in_table(sub, src) {
                    return Some(indent);
                }
            }
            _ => (),
        }
    }
    None
}

#[cfg(feature = "to-toml")]
fn indent_from_span(src: &[u8], pos: usize) -> Option<crate::emit::Indent> {
    let mut i = pos;
    if i >= src.len() {
        return None;
    }
    while i > 0 {
        i -= 1;
        match src[i] {
            b' ' => continue,
            b'\t' => return Some(crate::emit::Indent::Tab),
            b'\n' => {
                let spaces = (pos - i - 1) as u8;
                if spaces > 0 {
                    return Some(crate::emit::Indent::Spaces(if spaces > 8 {
                        8
                    } else {
                        spaces as u8
                    }));
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}
