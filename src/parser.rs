// Deliberately avoid `?` operator throughout this module for compile-time
// performance: explicit match/if-let prevents the compiler from generating
// From::from conversion and drop-glue machinery at every call site.
#![allow(clippy::question_mark)]
#![allow(unsafe_code)]

#[cfg(test)]
#[path = "./parser_tests.rs"]
mod tests;

use crate::{
    Span,
    arena::Arena,
    error::{Error, ErrorKind},
    table::{InnerTable, Table},
    time::DateTime,
    value::{self, Item, Key},
};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::{char, task::Context};

const MAX_RECURSION_DEPTH: i16 = 256;
// When a method returns Err(ParseError), the full error details have already
// been written into Parser::error_kind / Parser::error_span.
#[derive(Copy, Clone)]
struct ParseError;

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
const INDEXED_TABLE_THRESHOLD: usize = 6;

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
    fn new(key: &'de str, first_key_span: u32) -> Self {
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
        self.as_str().hash(state);
        self.first_key_span.hash(state);
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

    // Error context -- populated just before returning ParseError
    error_span: Span,
    error_kind: Option<ErrorKind>,

    // Global key-index for O(1) lookups in large tables.
    // Maps (table-discriminator, key-name) → entry index in the table.
    index: foldhash::HashMap<KeyRef<'de>, usize>,
}

#[allow(unsafe_code)]
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
            // initialize to about ~ 8 KB
            index: foldhash::HashMap::with_capacity_and_hasher(
                256,
                foldhash::fast::RandomState::default(),
            ),
        }
    }

    /// Get a `&str` slice from the underlying bytes.
    /// SAFETY: `self.bytes` is always valid UTF-8, and callers must ensure
    /// `start..end` falls on UTF-8 char boundaries.
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

    #[cold]
    fn set_duplicate_key_error(&mut self, first: Span, second: Span, key: &str) -> ParseError {
        self.error_span = second;
        self.error_kind = Some(ErrorKind::DuplicateKey {
            key: key.into(),
            first,
        });
        ParseError
    }
    #[cold]
    fn set_error(&mut self, start: usize, end: Option<usize>, kind: ErrorKind) -> ParseError {
        self.error_span = Span::new(start as u32, end.unwrap_or(start + 1) as u32);
        self.error_kind = Some(kind);
        ParseError
    }

    fn take_error(&mut self) -> Error {
        let kind = self
            .error_kind
            .take()
            .expect("take_error called without error");
        let span = self.error_span;

        // Black Magic Optimization:
        // Removing the following introduces 8% performance
        // regression across the board.
        std::hint::black_box(&self.bytes.iter().enumerate().next());

        Error { kind, span }
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
    fn expected_error(&mut self, b: u8) -> ParseError {
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

    fn expect_byte(&mut self, b: u8) -> Result<(), ParseError> {
        if self.peek_byte() == Some(b) {
            self.cursor += 1;
            Ok(())
        } else {
            return Err(self.expected_error(b));
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

    fn eat_newline_or_eof(&mut self) -> Result<(), ParseError> {
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
                        expected: "newline",
                        found: found_desc,
                    },
                ))
            }
        }
    }

    fn eat_comment(&mut self) -> Result<bool, ParseError> {
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
    fn scan_token_desc_and_end(&self) -> (&'static str, usize) {
        let Some(b) = self.peek_byte() else {
            return ("eof", self.bytes.len());
        };
        match b {
            b'\n' => ("a newline", self.cursor + 1),
            b'\r' => ("a carriage return", self.cursor + 1),
            b' ' | b'\t' => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len()
                    && (self.bytes[end] == b' ' || self.bytes[end] == b'\t')
                {
                    end += 1;
                }
                ("whitespace", end)
            }
            b'#' => ("a comment", self.cursor + 1),
            b'=' => ("an equals", self.cursor + 1),
            b'.' => ("a period", self.cursor + 1),
            b',' => ("a comma", self.cursor + 1),
            b':' => ("a colon", self.cursor + 1),
            b'+' => ("a plus", self.cursor + 1),
            b'{' => ("a left brace", self.cursor + 1),
            b'}' => ("a right brace", self.cursor + 1),
            b'[' => ("a left bracket", self.cursor + 1),
            b']' => ("a right bracket", self.cursor + 1),
            b'\'' | b'"' => ("a string", self.cursor + 1),
            _ if is_keylike_byte(b) => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len() && is_keylike_byte(self.bytes[end]) {
                    end += 1;
                }
                ("an identifier", end)
            }
            _ => ("a character", self.cursor + 1),
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

    fn read_table_key(&mut self) -> Result<Key<'de>, ParseError> {
        let Some(b) = self.peek_byte() else {
            return Err(self.set_error(
                self.bytes.len(),
                None,
                ErrorKind::Wanted {
                    expected: "a table key",
                    found: "eof",
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
                        expected: "a table key",
                        found: found_desc,
                    },
                ))
            }
        }
    }

    /// Read a basic (double-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `"`.
    fn read_string(&mut self, start: usize, delim: u8) -> Result<(Key<'de>, bool), ParseError> {
        let mut multiline = false;
        if self.eat_byte(delim) {
            if self.eat_byte(delim) {
                multiline = true;
            } else {
                return Ok((
                    Key {
                        name: "",
                        span: Span::new(start as u32, (start + 1) as u32),
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
    ) -> Result<(Key<'de>, bool), ParseError> {
        let mut flush_from = content_start;
        let mut scratch: Option<crate::arena::Scratch<'de>> = None;
        loop {
            self.skip_string_plain(delim);

            let i = self.cursor;
            let Some(&b) = self.bytes.get(i) else {
                return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
            };
            self.cursor = i + 1;

            match b {
                b'\r' => {
                    if self.eat_byte(b'\n') {
                        if !multiline {
                            return Err(self.set_error(
                                i,
                                None,
                                ErrorKind::InvalidCharInString('\n'),
                            ));
                        }
                    } else {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\r')));
                    }
                }
                b'\n' => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
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

                        let maybe_nl = self.bytes[start + 3];
                        let start_off = if maybe_nl == b'\n' {
                            4
                        } else if maybe_nl == b'\r' {
                            5
                        } else {
                            3
                        };

                        (
                            Span::new((start + start_off) as u32, (self.cursor - 3) as u32),
                            i + extra,
                        )
                    } else {
                        (Span::new((start + 1) as u32, (self.cursor - 1) as u32), i)
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
    ) -> Result<(), ParseError> {
        let i = self.cursor;
        let Some(&b) = self.bytes.get(i) else {
            return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
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
        string_start: usize,
        escape_start: usize,
    ) -> Result<char, ParseError> {
        let mut val: u32 = 0;
        for _ in 0..n {
            let Some(&byte) = self.bytes.get(self.cursor) else {
                return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
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
    fn number(
        &mut self,
        start: u32,
        end: u32,
        s: &'de str,
        sign: u8,
    ) -> Result<Item<'de>, ParseError> {
        let bytes = s.as_bytes();

        // Base-prefixed integers (0x, 0o, 0b).
        // TOML forbids signs on these, so only match when first byte is '0'.
        if sign == 2
            && let [b'0', format, rest @ ..] = s.as_bytes()
        {
            match format {
                b'x' => return self.integer_hex(rest, Span::new(start, end)),
                b'o' => return self.integer_octal(rest, Span::new(start, end)),
                b'b' => return self.integer_binary(rest, Span::new(start, end)),
                _ => {}
            }
        }

        if self.eat_byte(b'.') {
            let at = self.cursor;
            return match self.peek_byte() {
                Some(b) if is_keylike_byte(b) => {
                    let after = self.read_keylike();
                    match self.float(start, end, s, Some(after), sign) {
                        Ok(f) => Ok(Item::float(f, Span::new(start, self.cursor as u32))),
                        Err(e) => Err(e),
                    }
                }
                _ => Err(self.set_error(at, Some(end as usize), ErrorKind::InvalidNumber)),
            };
        }

        if sign == 2 {
            let head = &self.bytes[start as usize..];
            if let Some((consumed, moment)) = DateTime::munch(head) {
                self.cursor = start as usize + consumed;
                return Ok(Item::moment(moment, Span::new(start, self.cursor as u32)));
            }
        }

        if let Ok(v) = self.integer_decimal(bytes, Span::new(start, end), sign) {
            return Ok(v);
        }

        if bytes.iter().any(|&b| b == b'e' || b == b'E') {
            return match self.float(start, end, s, None, sign) {
                Ok(f) => Ok(Item::float(f, Span::new(start, self.cursor as u32))),
                Err(e) => Err(e),
            };
        }

        Err(ParseError)
    }

    fn integer_decimal(
        &mut self,
        bytes: &'de [u8],
        span: Span,
        sign: u8,
    ) -> Result<Item<'de>, ParseError> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        let mut leading_zero = false;
        let negative = sign == 0;
        'error: {
            for &b in bytes {
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        break 'error;
                    }
                    prev_underscore = true;
                    continue;
                }
                if !b.is_ascii_digit() {
                    break 'error;
                }
                if leading_zero {
                    break 'error;
                }
                if !has_digit && b == b'0' {
                    leading_zero = true;
                }
                has_digit = true;
                prev_underscore = false;
                let digit = (b - b'0') as u64;
                acc = match acc.checked_mul(10).and_then(|a| a.checked_add(digit)) {
                    Some(v) => v,
                    None => break 'error,
                };
            }

            if !has_digit || prev_underscore {
                break 'error;
            }

            let max = if negative {
                (i64::MAX as u64) + 1
            } else {
                i64::MAX as u64
            };
            if acc > max {
                break 'error;
            }

            let val = if negative {
                (acc as i64).wrapping_neg()
            } else {
                acc as i64
            };
            return Ok(Item::integer(val, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn integer_hex(&mut self, bytes: &'de [u8], span: Span) -> Result<Item<'de>, ParseError> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        'error: {
            if bytes.is_empty() {
                break 'error;
            }

            for &b in bytes {
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        break 'error;
                    }
                    prev_underscore = true;
                    continue;
                }
                let digit = HEX[b as usize];
                if digit < 0 {
                    break 'error;
                }
                has_digit = true;
                prev_underscore = false;
                if acc >> 60 != 0 {
                    break 'error;
                }
                acc = (acc << 4) | digit as u64;
            }

            if !has_digit || prev_underscore {
                break 'error;
            }

            if acc > i64::MAX as u64 {
                break 'error;
            }
            return Ok(Item::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn integer_octal(&mut self, bytes: &'de [u8], span: Span) -> Result<Item<'de>, ParseError> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        'error: {
            if bytes.is_empty() {
                break 'error;
            }

            for &b in bytes {
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        break 'error;
                    }
                    prev_underscore = true;
                    continue;
                }
                if !b.is_ascii_digit() || b > b'7' {
                    break 'error;
                }
                has_digit = true;
                prev_underscore = false;
                if acc >> 61 != 0 {
                    break 'error;
                }
                acc = (acc << 3) | (b - b'0') as u64;
            }

            if !has_digit || prev_underscore {
                break 'error;
            }

            if acc > i64::MAX as u64 {
                break 'error;
            }
            return Ok(Item::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn integer_binary(&mut self, bytes: &'de [u8], span: Span) -> Result<Item<'de>, ParseError> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        'error: {
            if bytes.is_empty() {
                break 'error;
            }

            for &b in bytes {
                if b == b'_' {
                    if !has_digit || prev_underscore {
                        break 'error;
                    }
                    prev_underscore = true;
                    continue;
                }
                if b != b'0' && b != b'1' {
                    break 'error;
                }
                has_digit = true;
                prev_underscore = false;
                if acc >> 63 != 0 {
                    break 'error;
                }
                acc = (acc << 1) | (b - b'0') as u64;
            }

            if !has_digit || prev_underscore {
                break 'error;
            }

            if acc > i64::MAX as u64 {
                break 'error;
            }
            return Ok(Item::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn float(
        &mut self,
        start: u32,
        end: u32,
        s: &'de str,
        after_decimal: Option<&'de str>,
        sign: u8,
    ) -> Result<f64, ParseError> {
        let s_start = start as usize;
        let s_end = end as usize;

        // TOML forbids leading zeros in the integer part (e.g. 00.5, -01.0).
        if let [b'0', b'0'..=b'9' | b'_', ..] = s.as_bytes() {
            return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
        }

        // Safety: no other Scratch or arena.alloc() is active during float parsing.
        let mut scratch = unsafe { self.arena.scratch() };

        if sign == 0 {
            scratch.push(b'-');
        }
        if !scratch.push_strip_underscores(s.as_bytes()) {
            return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
        }

        let mut last = s;

        if let Some(after) = after_decimal {
            if !matches!(after.as_bytes().first(), Some(b'0'..=b'9')) {
                return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
            }
            scratch.push(b'.');
            if !scratch.push_strip_underscores(after.as_bytes()) {
                return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
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
                        return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
                    }
                }
                _ => {
                    return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
                }
            }
        }

        // Scratch is not committed — arena pointer stays unchanged, space is
        // reused by subsequent allocations.
        // SAFETY: scratch contains only ASCII digits, signs, dots, and 'e'/'E'
        // copied from validated input via push_strip_underscores.
        let n: f64 = match unsafe { std::str::from_utf8_unchecked(scratch.as_bytes()) }.parse() {
            Ok(n) => n,
            Err(_) => {
                return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
            }
        };
        if n.is_finite() {
            Ok(n)
        } else {
            Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber))
        }
    }

    fn value(&mut self, depth_remaining: i16) -> Result<Item<'de>, ParseError> {
        let at = self.cursor;
        let Some(byte) = self.peek_byte() else {
            return Err(self.set_error(self.bytes.len(), None, ErrorKind::UnexpectedEof));
        };
        let sign = match byte {
            b'"' | b'\'' => {
                self.cursor += 1;
                return match self.read_string(self.cursor - 1, byte) {
                    Ok((key, _)) => Ok(Item::string(key.name, key.span)),
                    Err(e) => Err(e),
                };
            }
            b'{' => {
                let start = self.cursor as u32;
                self.cursor += 1;
                let mut table = crate::table::InnerTable::new();
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
                let mut arr = value::Array::new();
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
                            expected: "the literal `true`",
                            found: "something else",
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
                            expected: "the literal `false`",
                            found: "something else",
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
                return Ok(Item::float(
                    if sign != 0 {
                        f64::INFINITY
                    } else {
                        f64::NEG_INFINITY
                    },
                    Span::new(at as u32, end),
                ));
            }
            "nan" => {
                return Ok(Item::float(
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
        } else {
            Err(self.set_error(at, Some(self.cursor), ErrorKind::InvalidNumber))
        }
    }

    fn inline_table_contents(
        &mut self,
        out: &mut crate::table::InnerTable<'de>,
        depth_remaining: i16,
    ) -> Result<(), ParseError> {
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
            let mut table_ref: &mut crate::table::InnerTable<'de> = &mut *out;
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
            if let Err(e) = self.expect_byte(b',') {
                return Err(e);
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
        out: &mut value::Array<'de>,
        depth_remaining: i16,
    ) -> Result<(), ParseError> {
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
        self.expect_byte(b']')
    }

    #[inline(always)]
    fn eat_inline_table_whitespace(&mut self) -> Result<(), ParseError> {
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
    fn eat_intermediate(&mut self) -> Result<(), ParseError> {
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
    ) -> Result<&'t mut InnerTable<'de>, ParseError> {
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
    ) -> Result<&'b mut Table<'de>, ParseError> {
        let table = &mut st.value;

        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, existing) = &mut table.entries_mut()[idx];
            let existing_span = existing_key.span;

            // Note: I would use safey accessor heres but that would cause issues
            // with NLL limitiations.
            if existing.is_table() {
                if existing.is_frozen() {
                    return Err(self.set_duplicate_key_error(existing_span, key.span, key.name));
                }
                // SAFETY: is_table() verified by the preceding check.
                unsafe { Ok(existing.as_table_mut_unchecked()) }
            } else if existing.is_aot() {
                // unwrap is safe since we just check it's an array of tables and thus a array.
                let arr = existing.as_array_mut().unwrap();
                // unwrap is safe as array's of tables always have atleast one value by construction
                let last = arr.last_mut().unwrap();
                if !last.is_table() {
                    return Err(self.set_duplicate_key_error(existing_span, key.span, key.name));
                }
                // SAFETY: last.is_table() verified by the preceding check.
                unsafe { Ok(last.as_table_mut_unchecked()) }
            } else {
                Err(self.set_duplicate_key_error(existing_span, key.span, key.name))
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
    ) -> &'t mut value::Item<'de> {
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
    ) -> Result<Ctx<'b, 'de>, ParseError> {
        let table = &mut st.value;

        if let Some(idx) = self.indexed_find(table, key.name) {
            let (existing_key, existing) = &mut table.entries_mut()[idx];
            let first_key_span = existing_key.span;

            if !existing.is_table() || existing.is_frozen() {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.name));
            }
            if existing.has_header_bit() {
                return Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::DuplicateTable {
                        name: String::from(key.name),
                        first: existing.span(),
                    },
                ));
            }
            if existing.has_dotted_bit() {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.name));
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
    ) -> Result<Ctx<'b, 'de>, ParseError> {
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
                Err(self.set_duplicate_key_error(first_key_span, key.span, key.name))
            }
        } else {
            let entry_span = Span::new(header_start, header_end);
            let first_entry = Item::table_header(InnerTable::new(), entry_span);
            let array_span = Span::new(header_start, header_end);
            let array_val = Item::array_aot(
                value::Array::with_single(first_entry, self.arena),
                array_span,
            );
            let inserted = self.insert_value_known_to_be_unique(table, key, array_val);
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
    ) -> Result<(), ParseError> {
        if table.len() < INDEXED_TABLE_THRESHOLD {
            for (existing_key, _) in table.entries() {
                if existing_key.as_str() == key.name {
                    return Err(self.set_duplicate_key_error(
                        existing_key.span,
                        key.span,
                        key.name,
                    ));
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
                Err(self.set_duplicate_key_error(existing_key.span, key.span, key.name))
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
        // NOTE: I would return a refernce to actual entry here, however this
        // runs into all sorts of NLL limitations.
        if table.len() > INDEXED_TABLE_THRESHOLD {
            // SAFETY: len > INDEXED_TABLE_THRESHOLD (> 6), so the table is non-empty.
            let first_key_span = unsafe { table.first_key_span_start_unchecked() };
            self.index.get(&KeyRef::new(name, first_key_span)).copied()
        } else {
            table.find_index(name)
        }
    }

    fn parse_document(&mut self, root_st: &mut Table<'de>) -> Result<(), ParseError> {
        let mut ctx = Ctx {
            table: root_st,
            array_end_span: None,
        };

        loop {
            self.eat_whitespace();
            match self.eat_comment() {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => return Err(e),
            }
            if self.eat_newline() {
                continue;
            }

            match self.peek_byte() {
                None => break,
                Some(b'[') => {
                    ctx = match self.process_table_header(root_st) {
                        Ok(c) => c,
                        Err(e) => return Err(e),
                    };
                }
                Some(b'\r') => {
                    return Err(self.set_error(self.cursor, None, ErrorKind::Unexpected('\r')));
                }
                Some(_) => {
                    if let Err(e) = self.process_key_value(&mut ctx) {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    fn process_table_header<'b>(
        &mut self,
        root_st: &'b mut Table<'de>,
    ) -> Result<Ctx<'b, 'de>, ParseError> {
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

        if is_array {
            self.navigate_header_array_final(current, key, header_start, header_end)
        } else {
            self.navigate_header_table_final(current, key, header_start, header_end)
        }
    }

    fn process_key_value(&mut self, ctx: &mut Ctx<'_, 'de>) -> Result<(), ParseError> {
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

        let start = ctx.table.span_start();
        ctx.table.set_span_start(start.min(line_start));
        ctx.table.extend_span_end(line_end);

        if let Some(end_flag) = &mut ctx.array_end_span {
            let old = **end_flag;
            let current = old >> value::FLAG_SHIFT;
            **end_flag = (current.max(line_end) << value::FLAG_SHIFT) | (old & value::FLAG_MASK);
        }

        Ok(())
    }
}

/// Parses a TOML string into a [`Table`].
///
/// The returned table borrows from both the input string and the [`Arena`],
/// so both must outlive the table. The arena is used to store escape sequences;
/// plain strings borrow directly from the input.
pub fn parse<'de>(s: &'de str, arena: &'de Arena) -> Result<Table<'de>, Error> {
    // Tag bits use the low 3 bits of start_and_tag, limiting span.start to
    // 29 bits (512 MiB). The flag state uses the low 3 bits of end_and_flag,
    // limiting span.end to 29 bits (512 MiB).
    const MAX_SIZE: usize = (1u32 << 29) as usize;

    if s.len() >= MAX_SIZE {
        return Err(Error {
            kind: ErrorKind::FileTooLarge,
            span: Span::new(0, 0),
        });
    }

    let mut root_st = Table::new(Span::new(0, s.len() as u32));
    let mut parser = Parser::new(s, arena);
    match parser.parse_document(&mut root_st) {
        Ok(()) => {}
        Err(_) => return Err(parser.take_error()),
    }
    // Note that root is about the drop (but doesn't implement drop), so we can take
    // ownership of this table.
    // todo don't do this
    Ok(root_st)
}

#[inline]
fn is_keylike_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn byte_describe(b: u8) -> &'static str {
    match b {
        b'\n' => "a newline",
        b' ' | b'\t' => "whitespace",
        b'=' => "an equals",
        b'.' => "a period",
        b',' => "a comma",
        b':' => "a colon",
        b'+' => "a plus",
        b'{' => "a left brace",
        b'}' => "a right brace",
        b'[' => "a left bracket",
        b']' => "a right bracket",
        b'\'' | b'"' => "a string",
        _ if is_keylike_byte(b) => "an identifier",
        _ => "a character",
    }
}
