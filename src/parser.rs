// Deliberately avoid `?` operator throughout this module for compile-time
// performance: explicit match/if-let prevents the compiler from generating
// From::from conversion and drop-glue machinery at every call site.
#![allow(clippy::question_mark)]
#![allow(unsafe_code)]

use crate::{
    Span,
    error::{Error, ErrorKind},
    str::Str,
    value::{self, Key, SpannedTable, Value},
};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::{char, collections::HashMap};

// When a method returns Err(ParseError), the full error details have already
// been written into Parser::error_kind / Parser::error_span.
#[derive(Copy, Clone)]
struct ParseError;

struct Ctx<'b, 'a> {
    /// The current table context — a `SpannedTable` view into a table `Value`.
    /// Gives direct mutable access to both the span fields and the `Table` payload.
    st: &'b mut SpannedTable<'a>,
    /// If this table is an entry in an array-of-tables, a disjoint borrow of
    /// the parent array Value's `end_and_flag` field so its span can be
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
/// (borrowed `Str`) or the heap allocation of an owned `Str`; both are stable
/// for the lifetime of the parse.  `first_key_span` is the `span.start()` of
/// the **first** key ever inserted into the table and serves as a cheap,
/// collision-free table discriminator.
struct KeyIndex {
    key_ptr: NonNull<u8>,
    len: u32,
    first_key_span: u32,
}

impl KeyIndex {
    #[inline]
    unsafe fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                self.key_ptr.as_ptr(),
                self.len as usize,
            ))
        }
    }
}

impl Hash for KeyIndex {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { self.as_str() }.hash(state);
        self.first_key_span.hash(state);
    }
}

impl PartialEq for KeyIndex {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.first_key_span == other.first_key_span && unsafe { self.as_str() == other.as_str() }
    }
}

impl Eq for KeyIndex {}

struct Parser<'a> {
    /// Raw bytes of the input. Always valid UTF-8 (derived from `&str`).
    bytes: &'a [u8],
    cursor: usize,

    // Error context -- populated just before returning ParseError
    error_span: Span,
    error_kind: Option<ErrorKind>,

    // Global key-index for O(1) lookups in large tables.
    // Maps (table-discriminator, key-name) → entry index in the table.
    table_index: foldhash::HashMap<KeyIndex, usize>,
}

// A string with span, that might borrowed from the input or scratch buffer
// High bit set on u64: (from buffer, (needs allocation to convert to Str<'a> or Key<'a>))
// No high bit set on u64: (from scratch, already owned Str<'a> or Key<'a>)
#[derive(Clone, Copy)]
struct RawStr<'de, 's> {
    ptr: NonNull<u8>,
    len_and_tag: u64,
    pub span: Span,
    _marker: std::marker::PhantomData<(&'de str, &'s [u8])>,
}

impl<'de, 's> RawStr<'de, 's> {
    pub fn len(&self) -> usize {
        (self.len_and_tag & 0x7fff_ffff_ffff_ffff) as usize
    }

    /// SAFETY: `scratch` must contain valid UTF-8.
    pub unsafe fn from_scratch(scratch: &'s [u8], span: Span) -> RawStr<'de, 's> {
        unsafe {
            RawStr {
                ptr: NonNull::new_unchecked(scratch.as_ptr() as *mut u8),
                len_and_tag: scratch.len() as u64,
                span,
                _marker: std::marker::PhantomData,
            }
        }
    }

    pub fn from_input(input: &'de str, span: Span) -> RawStr<'de, 's> {
        unsafe {
            RawStr {
                ptr: NonNull::new_unchecked(input.as_ptr() as *mut u8),
                len_and_tag: input.len() as u64 | 0x8000_0000_0000_0000,
                span,
                _marker: std::marker::PhantomData,
            }
        }
    }

    #[inline]
    fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr.as_ptr(), self.len()))
        }
    }
}
impl<'de, 's> From<RawStr<'de, 's>> for Key<'de> {
    fn from(raw: RawStr<'de, 's>) -> Self {
        Key {
            span: raw.span,
            name: raw.into(),
        }
    }
}
impl<'de, 's> From<RawStr<'de, 's>> for Str<'de> {
    fn from(raw: RawStr<'de, 's>) -> Self {
        let len = raw.len();
        let s = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(raw.ptr.as_ptr(), len))
        };
        if (raw.len_and_tag as i64) < 0 {
            // From input buffer: borrow directly
            Str::from(s)
        } else {
            // From scratch buffer: allocate owned copy
            let boxed: Box<str> = s.into();
            Str::from(boxed)
        }
    }
}

#[allow(unsafe_code)]
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            bytes: input.as_bytes(),
            cursor: 0,
            error_span: Span::new(0, 0),
            error_kind: None,
            table_index: HashMap::default(),
        }
    }

    /// Get a `&str` slice from the underlying bytes.
    /// SAFETY: `self.bytes` is always valid UTF-8, and callers must ensure
    /// `start..end` falls on UTF-8 char boundaries.
    #[inline]
    unsafe fn str_slice(&self, start: usize, end: usize) -> &'a str {
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
        let line_info = Some(self.to_linecol(span.start() as usize));
        Error {
            kind,
            span,
            line_info,
        }
    }

    fn to_linecol(&self, offset: usize) -> (usize, usize) {
        let mut line_start = 0;
        let mut line_num = 0;
        for (i, &b) in self.bytes.iter().enumerate() {
            if i >= offset {
                return (line_num, offset - line_start);
            }
            if b == b'\n' {
                line_num += 1;
                line_start = i + 1;
            }
        }
        (line_num, offset - line_start)
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
    fn advance(&mut self) {
        self.cursor += 1;
    }

    #[inline]
    fn eat_byte(&mut self, b: u8) -> bool {
        if self.peek_byte() == Some(b) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_byte_spanned(&mut self, b: u8) -> Option<Span> {
        if self.peek_byte() == Some(b) {
            let start = self.cursor;
            self.advance();
            Some(Span::new(start as u32, self.cursor as u32))
        } else {
            None
        }
    }

    fn expect_byte(&mut self, b: u8) -> Result<(), ParseError> {
        if self.eat_byte(b) {
            Ok(())
        } else {
            let start = self.cursor;
            let (found_desc, end) = self.scan_token_desc_and_end();
            Err(self.set_error(
                start,
                Some(end),
                ErrorKind::Wanted {
                    expected: byte_describe(b),
                    found: found_desc,
                },
            ))
        }
    }

    fn expect_byte_spanned(&mut self, b: u8) -> Result<Span, ParseError> {
        if let Some(span) = self.eat_byte_spanned(b) {
            Ok(span)
        } else {
            let start = self.cursor;
            let (found_desc, end) = self.scan_token_desc_and_end();
            Err(self.set_error(
                start,
                Some(end),
                ErrorKind::Wanted {
                    expected: byte_describe(b),
                    found: found_desc,
                },
            ))
        }
    }

    fn eat_whitespace(&mut self) {
        while let Some(b) = self.peek_byte() {
            if b == b' ' || b == b'\t' {
                self.advance();
            } else {
                break;
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

    fn eat_newline_or_eof(&mut self) -> Result<(), ParseError> {
        match self.peek_byte() {
            None => Ok(()),
            Some(b'\n') => {
                self.advance();
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

    fn eat_newline(&mut self) -> bool {
        match self.peek_byte() {
            Some(b'\n') => {
                self.advance();
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
        match self.peek_byte() {
            None => ("eof", self.bytes.len()),
            Some(b'\n' | b'\r') => ("a newline", self.cursor + 1),
            Some(b' ' | b'\t') => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len()
                    && (self.bytes[end] == b' ' || self.bytes[end] == b'\t')
                {
                    end += 1;
                }
                ("whitespace", end)
            }
            Some(b'#') => ("a comment", self.cursor + 1),
            Some(b'=') => ("an equals", self.cursor + 1),
            Some(b'.') => ("a period", self.cursor + 1),
            Some(b',') => ("a comma", self.cursor + 1),
            Some(b':') => ("a colon", self.cursor + 1),
            Some(b'+') => ("a plus", self.cursor + 1),
            Some(b'{') => ("a left brace", self.cursor + 1),
            Some(b'}') => ("a right brace", self.cursor + 1),
            Some(b'[') => ("a left bracket", self.cursor + 1),
            Some(b']') => ("a right bracket", self.cursor + 1),
            Some(b'\'' | b'"') => ("a string", self.cursor + 1),
            Some(b) if is_keylike_byte(b) => {
                let mut end = self.cursor + 1;
                while end < self.bytes.len() && is_keylike_byte(self.bytes[end]) {
                    end += 1;
                }
                ("an identifier", end)
            }
            Some(_) => ("a character", self.cursor + 1),
        }
    }

    fn read_keylike(&mut self) -> &'a str {
        let start = self.cursor;
        while let Some(b) = self.peek_byte() {
            if !is_keylike_byte(b) {
                break;
            }
            self.advance();
        }
        // SAFETY: keylike bytes are ASCII, always valid UTF-8 boundaries
        unsafe { self.str_slice(start, self.cursor) }
    }

    fn read_table_key<'s>(
        &mut self,
        scratch: &'s mut Vec<u8>,
    ) -> Result<RawStr<'a, 's>, ParseError> {
        match self.peek_byte() {
            Some(b'"') => {
                let start = self.cursor;
                self.advance();
                let (key, multiline) = match self.read_string(scratch, start, b'"') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(start + key.len()),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(key)
            }
            Some(b'\'') => {
                let start = self.cursor;
                self.advance();
                let (key, multiline) = match self.read_string(scratch, start, b'\'') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(start + key.len()),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(key)
            }
            Some(b) if is_keylike_byte(b) => {
                let start = self.cursor;
                let k = self.read_keylike();
                let span = Span::new(start as u32, self.cursor as u32);
                Ok(RawStr::from_input(k, span))
            }
            Some(_) => {
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
            None => Err(self.set_error(
                self.bytes.len(),
                None,
                ErrorKind::Wanted {
                    expected: "a table key",
                    found: "eof",
                },
            )),
        }
    }

    /// Read a basic (double-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `"`.
    fn read_string<'s>(
        &mut self,
        scratch: &'s mut Vec<u8>,
        start: usize,
        delim: u8,
    ) -> Result<(RawStr<'a, 's>, bool), ParseError> {
        let mut multiline = false;
        if self.eat_byte(delim) {
            if self.eat_byte(delim) {
                multiline = true;
            } else {
                return Ok((
                    RawStr::from_input("", Span::new(start as u32, (start + 1) as u32)),
                    false,
                ));
            }
        }

        let mut content_start = self.cursor;
        if multiline {
            match self.peek_byte() {
                Some(b'\n') => {
                    self.advance();
                    content_start = self.cursor;
                }
                Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                    self.cursor += 2;
                    content_start = self.cursor;
                }
                _ => {}
            }
        }

        self.read_string_loop(scratch, start, content_start, multiline, delim)
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

    fn read_string_loop<'s>(
        &mut self,
        scratch: &'s mut Vec<u8>,
        start: usize,
        content_start: usize,
        multiline: bool,
        delim: u8,
    ) -> Result<(RawStr<'a, 's>, bool), ParseError> {
        let mut flush_from = content_start;
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
                    if multiline {
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

                        let span = Span::new((start + start_off) as u32, (self.cursor - 3) as u32);
                        if flush_from != content_start {
                            scratch.extend_from_slice(&self.bytes[flush_from..i + extra]);
                            unsafe {
                                return Ok((RawStr::from_scratch(&*scratch, span), true));
                            }
                        } else {
                            unsafe {
                                return Ok((
                                    RawStr::from_input(
                                        self.str_slice(content_start, i + extra),
                                        span,
                                    ),
                                    true,
                                ));
                            }
                        };
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    if flush_from != content_start {
                        scratch.extend_from_slice(&self.bytes[flush_from..i]);
                        unsafe {
                            return Ok((RawStr::from_scratch(&*scratch, span), false));
                        }
                    } else {
                        unsafe {
                            return Ok((
                                RawStr::from_input(self.str_slice(content_start, i), span),
                                false,
                            ));
                        }
                    };
                }
                b'\\' if delim == b'"' => {
                    if flush_from == content_start {
                        scratch.clear();
                    }
                    scratch.extend_from_slice(&self.bytes[flush_from..i]);
                    if let Err(e) = self.read_basic_escape(scratch, start, multiline) {
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
        scratch: &mut Vec<u8>,
        string_start: usize,
        multi: bool,
    ) -> Result<(), ParseError> {
        let i = self.cursor;
        let Some(&b) = self.bytes.get(i) else {
            return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
        };
        self.cursor = i + 1;

        match b {
            b'"' => scratch.push(b'"'),
            b'\\' => scratch.push(b'\\'),
            b'b' => scratch.push(0x08),
            b'f' => scratch.push(0x0C),
            b'n' => scratch.push(b'\n'),
            b'r' => scratch.push(b'\r'),
            b't' => scratch.push(b'\t'),
            b'e' => scratch.push(0x1B),
            b'u' => {
                let ch = self.read_hex(4, string_start, i);
                match ch {
                    Ok(ch) => {
                        let mut buf = [0u8; 4];
                        let len = ch.encode_utf8(&mut buf).len();
                        scratch.extend_from_slice(&buf[..len]);
                    }
                    Err(e) => return Err(e),
                }
            }
            b'U' => {
                let ch = self.read_hex(8, string_start, i);
                match ch {
                    Ok(ch) => {
                        let mut buf = [0u8; 4];
                        let len = ch.encode_utf8(&mut buf).len();
                        scratch.extend_from_slice(&buf[..len]);
                    }
                    Err(e) => return Err(e),
                }
            }
            b'x' => {
                let ch = self.read_hex(2, string_start, i);
                match ch {
                    Ok(ch) => {
                        let mut buf = [0u8; 4];
                        let len = ch.encode_utf8(&mut buf).len();
                        scratch.extend_from_slice(&buf[..len]);
                    }
                    Err(e) => return Err(e),
                }
            }
            b' ' | b'\t' | b'\n' | b'\r' if multi => {
                // CRLF folding: \r\n counts as \n
                let c = if b == b'\r' && self.peek_byte() == Some(b'\n') {
                    self.advance();
                    '\n'
                } else {
                    b as char
                };
                if c != '\n' {
                    loop {
                        match self.peek_byte() {
                            Some(b' ' | b'\t') => {
                                self.advance();
                            }
                            Some(b'\n') => {
                                self.advance();
                                break;
                            }
                            Some(b'\r') if self.peek_byte_at(1) == Some(b'\n') => {
                                self.cursor += 2;
                                break;
                            }
                            _ => return Err(self.set_error(i, None, ErrorKind::InvalidEscape(c))),
                        }
                    }
                }
                loop {
                    match self.peek_byte() {
                        Some(b' ' | b'\t' | b'\n') => {
                            self.advance();
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
        Ok(())
    }

    fn next_char_for_error(&self) -> char {
        // Safety: The input was valid UTF-8 via a &str
        let text = unsafe { std::str::from_utf8_unchecked(&self.bytes) };
        if let Some(value) = text.get(self.cursor..) {
            value.chars().next().unwrap_or(char::REPLACEMENT_CHARACTER)
        } else {
            char::REPLACEMENT_CHARACTER
        }
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

    fn number(
        &mut self,
        scratch: &mut Vec<u8>,
        start: u32,
        end: u32,
        s: &'a str,
    ) -> Result<Value<'a>, ParseError> {
        let bytes = s.as_bytes();

        // Base-prefixed integers (0x, 0o, 0b).
        // TOML forbids signs on these, so only match when first byte is '0'.
        if let [b'0', format, rest @ ..] = s.as_bytes() {
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
                    match self.float(scratch, start, end, s, Some(after)) {
                        Ok(f) => Ok(Value::float(f, Span::new(start, self.cursor as u32))),
                        Err(e) => Err(e),
                    }
                }
                _ => Err(self.set_error(at, Some(end as usize), ErrorKind::InvalidNumber)),
            };
        }

        // Special float literals (inf, nan and signed variants).
        // Guard behind first-significant-byte check to skip string
        // comparisons for the common digit-only case.
        let off = usize::from(bytes.first() == Some(&b'-'));
        if let Some(&b'i' | &b'n') = bytes.get(off) {
            return match s {
                "inf" => Ok(Value::float(f64::INFINITY, Span::new(start, end))),
                "-inf" => Ok(Value::float(f64::NEG_INFINITY, Span::new(start, end))),
                "nan" => Ok(Value::float(f64::NAN.copysign(1.0), Span::new(start, end))),
                "-nan" => Ok(Value::float(f64::NAN.copysign(-1.0), Span::new(start, end))),
                _ => Err(self.set_error(
                    start as usize,
                    Some(end as usize),
                    ErrorKind::InvalidNumber,
                )),
            };
        }

        if let Ok(v) = self.integer_decimal(bytes, Span::new(start, end)) {
            return Ok(v);
        }

        if bytes.iter().any(|&b| b == b'e' || b == b'E') {
            return match self.float(scratch, start, end, s, None) {
                Ok(f) => Ok(Value::float(f, Span::new(start, self.cursor as u32))),
                Err(e) => Err(e),
            };
        }

        Err(ParseError)
    }

    fn number_leading_plus(
        &mut self,
        scratch: &mut Vec<u8>,
        plus_start: u32,
    ) -> Result<Value<'a>, ParseError> {
        match self.peek_byte() {
            Some(b) if is_keylike_byte(b) => {
                let s = self.read_keylike();
                let end = self.cursor as u32;
                self.number(scratch, plus_start, end, s)
            }
            _ => Err(self.set_error(
                plus_start as usize,
                Some(self.cursor),
                ErrorKind::InvalidNumber,
            )),
        }
    }

    fn integer_decimal(&mut self, bytes: &'a [u8], span: Span) -> Result<Value<'a>, ParseError> {
        let mut acc: u64 = 0;
        let mut prev_underscore = false;
        let mut has_digit = false;
        let mut leading_zero = false;
        'error: {
            let (negative, digits) = match bytes.first() {
                Some(&b'+') => (false, &bytes[1..]),
                Some(&b'-') => (true, &bytes[1..]),
                _ => (false, bytes),
            };

            if digits.is_empty() {
                break 'error;
            }

            for &b in digits {
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
            return Ok(Value::integer(val, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn integer_hex(&mut self, bytes: &'a [u8], span: Span) -> Result<Value<'a>, ParseError> {
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
            return Ok(Value::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        return Err(ParseError);
    }

    fn integer_octal(&mut self, bytes: &'a [u8], span: Span) -> Result<Value<'a>, ParseError> {
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
            return Ok(Value::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn integer_binary(&mut self, bytes: &'a [u8], span: Span) -> Result<Value<'a>, ParseError> {
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
            return Ok(Value::integer(acc as i64, span));
        }
        self.error_span = span;
        self.error_kind = Some(ErrorKind::InvalidNumber);
        Err(ParseError)
    }

    fn float(
        &mut self,
        scratch: &mut Vec<u8>,
        start: u32,
        end: u32,
        s: &'a str,
        after_decimal: Option<&'a str>,
    ) -> Result<f64, ParseError> {
        let s_start = start as usize;
        let s_end = end as usize;

        // TOML forbids leading zeros in the integer part (e.g. 00.5, -01.0).
        let unsigned = if s.as_bytes().first() == Some(&b'-') {
            &s[1..]
        } else {
            s
        };
        if let [b'0', b'0'..=b'9' | b'_', ..] = unsigned.as_bytes() {
            return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
        }

        scratch.clear();

        for &b in s.as_bytes() {
            if b != b'_' {
                scratch.push(b);
            }
        }

        let mut last = s;

        if let Some(after) = after_decimal {
            if !matches!(after.as_bytes().first(), Some(b'0'..=b'9')) {
                return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
            }
            scratch.push(b'.');
            for &b in after.as_bytes() {
                if b != b'_' {
                    scratch.push(b);
                }
            }
            last = after;
        }

        // When the last keylike token ends with e/E, the '+' and exponent
        // digits are separate tokens in the stream ('-' IS keylike so
        // e.g. "1e-5" stays in one token and needs no special handling).
        if matches!(last.as_bytes().last(), Some(b'e' | b'E')) {
            self.eat_byte(b'+');
            match self.peek_byte() {
                Some(b) if is_keylike_byte(b) => {
                    let next = self.read_keylike();
                    for &b in next.as_bytes() {
                        if b != b'_' {
                            scratch.push(b);
                        }
                    }
                }
                _ => {
                    return Err(self.set_error(s_start, Some(s_end), ErrorKind::InvalidNumber));
                }
            }
        }

        let n: f64 = match unsafe { std::str::from_utf8_unchecked(scratch) }.parse() {
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

    fn value(&mut self, scratch: &mut Vec<u8>) -> Result<Value<'a>, ParseError> {
        let at = self.cursor;
        let Some(byte) = self.peek_byte() else {
            return Err(self.set_error(self.bytes.len(), None, ErrorKind::UnexpectedEof));
        };
        match byte {
            b'"' => {
                self.advance();
                let (raw, _multiline) = match self.read_string(scratch, self.cursor - 1, b'"') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::string(raw.into(), raw.span))
            }
            b'\'' => {
                self.advance();
                let (raw, _multiline) = match self.read_string(scratch, self.cursor - 1, b'\'') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::string(raw.into(), raw.span))
            }
            b'{' => {
                let start = self.cursor as u32;
                self.advance();
                let mut table = value::Table::new();
                let end_span = match self.inline_table_contents(scratch, &mut table) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::table_frozen(table, Span::new(start, end_span.end())))
            }
            b'[' => {
                let start = self.cursor as u32;
                self.advance();
                let mut arr = value::Array::new();
                let end_span = match self.array_contents(scratch, &mut arr) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::array(arr, Span::new(start, end_span.end())))
            }
            b'+' => {
                let start = self.cursor as u32;
                self.advance();
                self.number_leading_plus(scratch, start)
            }
            b if is_keylike_byte(b) => {
                let start = self.cursor as u32;
                let key = self.read_keylike();
                let end = self.cursor as u32;
                let span = Span::new(start, end);

                match key {
                    "true" => Ok(Value::boolean(true, span)),
                    "false" => Ok(Value::boolean(false, span)),
                    "inf" | "nan" => self.number(scratch, start, end, key),
                    _ => {
                        let first_char = key.chars().next().expect("key should not be empty");
                        match first_char {
                            '-' | '0'..='9' => self.number(scratch, start, end, key),
                            _ => Err(self.set_error(
                                at,
                                Some(end as usize),
                                ErrorKind::UnquotedString,
                            )),
                        }
                    }
                }
            }
            _ => {
                let (found_desc, end) = self.scan_token_desc_and_end();
                Err(self.set_error(
                    at,
                    Some(end),
                    ErrorKind::Wanted {
                        expected: "a value",
                        found: found_desc,
                    },
                ))
            }
        }
    }

    fn inline_table_contents(
        &mut self,
        scratch: &mut Vec<u8>,
        out: &mut value::Table<'a>,
    ) -> Result<Span, ParseError> {
        if let Err(e) = self.eat_inline_table_whitespace() {
            return Err(e);
        }
        if let Some(span) = self.eat_byte_spanned(b'}') {
            return Ok(span);
        }
        loop {
            let mut table_ref: &mut value::Table<'a> = &mut *out;
            let mut key = match self.read_table_key(scratch) {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            while self.eat_byte(b'.') {
                if let Err(e) = self.eat_inline_table_whitespace() {
                    return Err(e);
                }
                table_ref = match self.navigate_dotted_key(table_ref, key) {
                    Ok(t) => t,
                    Err(e) => return Err(e),
                };
                key = match self.read_table_key(scratch) {
                    Ok(k) => k,
                    Err(e) => return Err(e),
                };
                if let Err(e) = self.eat_inline_table_whitespace() {
                    return Err(e);
                }
            }
            if let Err(e) = self.expect_byte(b'=') {
                return Err(e);
            }
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            {
                let key: Key<'a> = key.into();
                let val = match self.value(scratch) {
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
            if let Some(span) = self.eat_byte_spanned(b'}') {
                return Ok(span);
            }
            if let Err(e) = self.expect_byte(b',') {
                return Err(e);
            }
            if let Err(e) = self.eat_inline_table_whitespace() {
                return Err(e);
            }
            if let Some(span) = self.eat_byte_spanned(b'}') {
                return Ok(span);
            }
        }
    }

    fn array_contents(
        &mut self,
        scratch: &mut Vec<u8>,
        out: &mut value::Array<'a>,
    ) -> Result<Span, ParseError> {
        loop {
            if let Err(e) = self.eat_intermediate() {
                return Err(e);
            }
            if let Some(span) = self.eat_byte_spanned(b']') {
                return Ok(span);
            }
            let val = match self.value(scratch) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            out.push(val);
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
        self.expect_byte_spanned(b']')
    }

    fn eat_inline_table_whitespace(&mut self) -> Result<(), ParseError> {
        loop {
            self.eat_whitespace();
            if self.eat_newline() {
                continue;
            }
            match self.eat_comment() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn eat_intermediate(&mut self) -> Result<(), ParseError> {
        loop {
            self.eat_whitespace();
            if self.eat_newline() {
                continue;
            }
            match self.eat_comment() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Navigate into an existing or new table for a dotted-key intermediate
    /// segment. Checks frozen and header bits.
    /// New tables are created with the `DOTTED` tag.
    fn navigate_dotted_key<'t>(
        &mut self,
        table: &'t mut value::Table<'a>,
        key: RawStr<'a, '_>,
    ) -> Result<&'t mut value::Table<'a>, ParseError> {
        if let Some(idx) = self.indexed_find(table, key.as_str()) {
            // Safety: Indexed find guarantee index exists.
            let (existing_key, value) = unsafe { table.get_mut_unchecked(idx) };
            let ok = value.is_table() && !value.is_frozen() && !value.has_header_bit();

            if !ok {
                return Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DottedKeyInvalidType {
                        first: existing_key.span,
                    },
                ));
            }
            // Safety: check above ensures value is table
            unsafe { Ok(value.as_table_mut_unchecked()) }
        } else {
            let inserted = self.insert_into_table(
                table,
                key,
                Value::table_dotted(value::Table::new(), key.span),
            );
            unsafe { Ok(inserted.as_table_mut_unchecked()) }
        }
    }

    /// Navigate an intermediate segment of a table header (e.g. `a` in `[a.b.c]`).
    /// Creates implicit tables (no flag bits) if not found.
    /// Handles arrays-of-tables by navigating into the last element.
    ///
    /// Returns a `SpannedTable` view of the table navigated into.
    fn navigate_header_intermediate<'b>(
        &mut self,
        st: &'b mut SpannedTable<'a>,
        key: RawStr<'a, '_>,
    ) -> Result<&'b mut SpannedTable<'a>, ParseError> {
        let table = &mut *st.value;

        if let Some(idx) = self.indexed_find(table, key.as_str()) {
            let (existing_key, existing_val) = table.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_table = existing_val.is_table();
            let is_array = existing_val.is_array();
            let is_frozen = existing_val.is_frozen();
            let is_aot = existing_val.is_aot();

            if is_table {
                if is_frozen {
                    return Err(self.set_duplicate_key_error(
                        first_key_span,
                        key.span,
                        key.as_str(),
                    ));
                }
                let existing = table.get_mut_at(idx);
                unsafe { Ok(existing.as_spanned_table_mut_unchecked()) }
            } else if is_array && is_aot {
                let existing = table.get_mut_at(idx);
                let arr = existing.as_array_mut().unwrap();
                let last = arr.last_mut().unwrap();
                if !last.is_table() {
                    return Err(self.set_duplicate_key_error(
                        first_key_span,
                        key.span,
                        key.as_str(),
                    ));
                }
                unsafe { Ok(last.as_spanned_table_mut_unchecked()) }
            } else {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.as_str()));
            }
        } else {
            let inserted =
                self.insert_into_table(table, key, Value::table(value::Table::new(), key.span));
            // let new_val = Value::table(value::Table::new(), key.span);
            // table.insert(key.into(), new_val);
            // self.index_after_insert(table);
            // let last_idx = table.len() - 1;
            // let inserted = table.get_mut_at(last_idx);
            unsafe { Ok(inserted.as_spanned_table_mut_unchecked()) }
        }
    }
    fn insert_into_table<'t>(
        &mut self,
        table: &'t mut value::Table<'a>,
        key: RawStr<'a, '_>,
        value: Value<'a>,
    ) -> &'t mut value::Value<'a> {
        table.insert(key.into(), value);
        self.index_after_insert(table);
        let last_idx = table.len() - 1;
        table.get_mut_at(last_idx)
    }

    /// Handle the final segment of a standard table header `[a.b.c]`.
    ///
    /// Returns the [`Ctx`] for the table that subsequent key-value pairs
    /// should be inserted into.
    fn navigate_header_table_final<'b>(
        &mut self,
        st: &'b mut SpannedTable<'a>,
        key: RawStr<'a, '_>,
        header_start: u32,
        header_end: u32,
    ) -> Result<Ctx<'b, 'a>, ParseError> {
        let table = &mut *st.value;

        if let Some(idx) = self.indexed_find(table, key.as_str()) {
            let (existing_key, existing_val) = table.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_table = existing_val.is_table();
            let is_frozen = existing_val.is_frozen();
            let has_header = existing_val.has_header_bit();
            let has_dotted = existing_val.has_dotted_bit();
            let val_span = existing_val.span();

            if !is_table || is_frozen {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.as_str()));
            }
            if has_header {
                return Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::DuplicateTable {
                        name: key.as_str().to_string(),
                        first: val_span,
                    },
                ));
            }
            if has_dotted {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.as_str()));
            }
            let existing = table.get_mut_at(idx);
            let st = unsafe { existing.as_spanned_table_mut_unchecked() };
            st.set_header_tag();
            st.set_span_start(header_start);
            st.set_span_end(header_end);
            Ok(Ctx {
                st,
                array_end_span: None,
            })
        } else {
            let inserted = self.insert_into_table(
                table,
                key,
                Value::table_header(value::Table::new(), Span::new(header_start, header_end)),
            );
            Ok(Ctx {
                st: unsafe { inserted.as_spanned_table_mut_unchecked() },
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
        st: &'b mut SpannedTable<'a>,
        key: RawStr<'a, '_>,
        header_start: u32,
        header_end: u32,
    ) -> Result<Ctx<'b, 'a>, ParseError> {
        let table = &mut *st.value;

        if let Some(idx) = self.indexed_find(table, key.as_str()) {
            let (existing_key, existing_val) = table.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_aot = existing_val.is_aot();
            let is_table = existing_val.is_table();

            if is_aot {
                let existing = table.get_mut_at(idx);
                let (end_flag, arr) = unsafe { existing.split_array_end_flag() };
                let entry_span = Span::new(header_start, header_end);
                arr.push(Value::table_header(value::Table::new(), entry_span));
                let entry = arr.last_mut().unwrap();
                Ok(Ctx {
                    st: unsafe { entry.as_spanned_table_mut_unchecked() },
                    array_end_span: Some(end_flag),
                })
            } else if is_table {
                Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::RedefineAsArray,
                ))
            } else {
                return Err(self.set_duplicate_key_error(first_key_span, key.span, key.as_str()));
            }
        } else {
            let entry_span = Span::new(header_start, header_end);
            let first_entry = Value::table_header(value::Table::new(), entry_span);
            let array_span = Span::new(header_start, header_end);
            let array_val = Value::array_aot(value::Array::with_single(first_entry), array_span);
            let inserted = self.insert_into_table(table, key, array_val);
            let (end_flag, arr) = unsafe { inserted.split_array_end_flag() };
            let entry = arr.last_mut().unwrap();
            Ok(Ctx {
                st: unsafe { entry.as_spanned_table_mut_unchecked() },
                array_end_span: Some(end_flag),
            })
        }
    }

    /// Insert a value into a table, checking for duplicates.
    fn insert_value(
        &mut self,
        table: &mut value::Table<'a>,
        key: Key<'a>,
        val: Value<'a>,
    ) -> Result<(), ParseError> {
        if let Some(idx) = self.indexed_find(table, &key.name) {
            let (existing_key, _) = table.get_key_value_at(idx);
            return Err(self.set_duplicate_key_error(existing_key.span, key.span, &key.name));
        }

        table.insert(key, val);
        self.index_after_insert(table);
        Ok(())
    }

    /// Look up a key name in a table, returning its entry index.
    /// Uses the hash index for tables at or above the threshold, otherwise
    /// falls back to a linear scan.
    fn indexed_find(&self, table: &value::Table<'a>, name: &str) -> Option<usize> {
        // NOTE: I would return a refernce to actual entry here, however this
        // runs into all sorts of NLL limitations.
        if table.len() >= INDEXED_TABLE_THRESHOLD {
            let first_key_span = table.first_key_span_start();
            let probe = KeyIndex {
                // Safety: Name is reference and therefore non-null. This check is only
                // used for probe so the lifetime doesn't matter.
                key_ptr: unsafe { NonNull::new_unchecked(name.as_ptr() as *mut u8) },
                len: name.len() as u32,
                first_key_span,
            };
            self.table_index.get(&probe).copied()
        } else {
            table.find_index(name)
        }
    }

    /// After inserting an entry into a table, update the hash index if the
    /// table has reached or exceeded the indexing threshold.
    fn index_after_insert(&mut self, table: &value::Table<'a>) {
        let len = table.len();
        if len >= INDEXED_TABLE_THRESHOLD {
            if len == INDEXED_TABLE_THRESHOLD {
                self.bulk_index_table(table);
                return;
            }
            self.index_last_entry(table);
        }
    }

    /// Populate the hash index with all entries of a table that just reached
    /// the threshold.
    fn bulk_index_table(&mut self, table: &value::Table<'a>) {
        let first_key_span = table.first_key_span_start();
        for (i, (key, _)) in table.entries().iter().enumerate() {
            let (ptr, slen) = key.name.as_raw_parts();
            let ki = KeyIndex {
                key_ptr: ptr,
                len: slen as u32,
                first_key_span,
            };
            self.table_index.insert(ki, i);
        }
    }

    /// Index only the last (just-inserted) entry of an already-indexed table.
    fn index_last_entry(&mut self, table: &value::Table<'a>) {
        let idx = table.len() - 1;
        let first_key_span = table.first_key_span_start();
        let (key, _) = table.get_key_value_at(idx);
        let (ptr, slen) = key.name.as_raw_parts();
        let ki = KeyIndex {
            key_ptr: ptr,
            len: slen as u32,
            first_key_span,
        };
        self.table_index.insert(ki, idx);
    }

    fn parse_document(
        &mut self,
        scratch: &mut Vec<u8>,
        root_st: &mut SpannedTable<'a>,
    ) -> Result<(), ParseError> {
        let mut ctx = Ctx {
            st: root_st,
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
                    ctx = match self.process_table_header(scratch, root_st) {
                        Ok(c) => c,
                        Err(e) => return Err(e),
                    };
                }
                Some(b'\r') => {
                    return Err(self.set_error(self.cursor, None, ErrorKind::Unexpected('\r')));
                }
                Some(_) => {
                    if let Err(e) = self.process_key_value(scratch, &mut ctx) {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    fn process_table_header<'b>(
        &mut self,
        scratch: &mut Vec<u8>,
        root_st: &'b mut SpannedTable<'a>,
    ) -> Result<Ctx<'b, 'a>, ParseError> {
        let header_start = self.cursor as u32;
        if let Err(e) = self.expect_byte(b'[') {
            return Err(e);
        }
        let is_array = self.eat_byte(b'[');

        let mut current = root_st;

        self.eat_whitespace();
        let mut key = match self.read_table_key(scratch) {
            Ok(k) => k,
            Err(e) => return Err(e),
        };
        loop {
            self.eat_whitespace();
            if self.eat_byte(b'.') {
                self.eat_whitespace();
                current = match self.navigate_header_intermediate(current, key) {
                    Ok(p) => p,
                    Err(e) => return Err(e),
                };
                key = match self.read_table_key(scratch) {
                    Ok(k) => k,
                    Err(e) => return Err(e),
                };
            } else {
                break;
            }
        }

        self.eat_whitespace();
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

    fn process_key_value(
        &mut self,
        scratch: &mut Vec<u8>,
        ctx: &mut Ctx<'_, 'a>,
    ) -> Result<(), ParseError> {
        let line_start = self.cursor as u32;
        // Borrow the Table payload from the SpannedTable. NLL drops this
        // borrow at its last use (the insert_value call), freeing ctx.st
        // for the span updates that follow.
        let mut table_ref: &mut value::Table<'a> = &mut ctx.st.value;

        let mut key = match self.read_table_key(scratch) {
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
            key = match self.read_table_key(scratch) {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            self.eat_whitespace();
        }

        if let Err(e) = self.expect_byte(b'=') {
            return Err(e);
        }
        self.eat_whitespace();
        let key: Key<'a> = key.into();
        let val = match self.value(scratch) {
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

        let start = ctx.st.span_start();
        ctx.st.set_span_start(start.min(line_start));
        ctx.st.extend_span_end(line_end);

        if let Some(end_flag) = &mut ctx.array_end_span {
            let old = **end_flag;
            let current = old >> value::FLAG_SHIFT;
            **end_flag = (current.max(line_end) << value::FLAG_SHIFT) | (old & value::FLAG_BIT);
        }

        Ok(())
    }
}

/// Parses a toml string into a table [`Value`]
pub fn parse(s: &str) -> Result<Value<'_>, Error> {
    // Tag bits use the low 3 bits of start_and_tag, limiting span.start to
    // 29 bits (512 MiB). The FLAG_BIT uses the low bit of end_and_flag,
    // limiting span.end to 31 bits (2 GiB).
    const MAX_SIZE: usize = (1u32 << 29) as usize;

    if s.len() > MAX_SIZE {
        return Err(Error {
            kind: ErrorKind::FileTooLarge,
            span: Span::new(0, 0),
            line_info: None,
        });
    }

    let mut root = Value::table(value::Table::new(), Span::new(0, s.len() as u32));

    // SAFETY: root is a table, so the SpannedTable reinterpretation is valid.
    let root_st = unsafe { root.as_spanned_table_mut_unchecked() };

    let mut parser = Parser::new(s);
    let mut scratch = Vec::new();
    match parser.parse_document(&mut scratch, root_st) {
        Ok(()) => {}
        Err(_) => return Err(parser.take_error()),
    }

    Ok(root)
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

#[cfg(test)]
#[path = "./parser_tests.rs"]
mod tests;
