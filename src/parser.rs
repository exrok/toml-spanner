// Deliberately avoid `?` operator throughout this module for compile-time
// performance: explicit match/if-let prevents the compiler from generating
// From::from conversion and drop-glue machinery at every call site.
#![allow(clippy::question_mark)]
#![allow(unsafe_code)]

use crate::{
    Span,
    error::{Error, ErrorKind},
    str::Str,
    value::{self, Key, Value},
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

// ---------------------------------------------------------------------------
// Lightweight internal error -- zero-sized, no drop glue.
// When a method returns Err(ParseError), the full error details have already
// been written into Parser::error_kind / Parser::error_span.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone)]
struct ParseError;

// ---------------------------------------------------------------------------
// Context stack entry -- tracks the current table scope during parsing.
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    // /// Pointer to the `Table` we are inserting key-value pairs into.
    // table: *mut value::Table<'a>,
    /// Pointer to the Value containing that table (for span updates).
    value_ptr: *mut Value<'a>,
    /// If this table is an entry in an array-of-tables, points to the Array Value
    /// so its span can be extended alongside the entry.
    array_ptr: Option<NonNull<Value<'a>>>,
}

// ---------------------------------------------------------------------------
// Key index for O(1) table lookups
// ---------------------------------------------------------------------------

/// Tables with at least this many entries use the hash index for lookups.
const INDEXED_TABLE_THRESHOLD: usize = 6;

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

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    /// Raw bytes of the input. Always valid UTF-8 (derived from `&str`).
    bytes: &'a [u8],
    cursor: usize,

    // Error context -- populated just before returning ParseError
    error_span: Span,
    error_kind: Option<ErrorKind>,

    // Reusable scratch buffers
    string_buf: Vec<u8>,

    // Navigation stack for direct construction
    ctx: Vec<Ctx<'a>>,

    // Global key-index for O(1) lookups in large tables.
    // Maps (table-discriminator, key-name) → entry index in the table.
    table_index: foldhash::HashMap<KeyIndex, usize>,
}

#[allow(unsafe_code)]
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            bytes: input.as_bytes(),
            cursor: 0,
            error_span: Span::new(0, 0),
            error_kind: None,
            string_buf: Vec::new(),
            ctx: Vec::new(),
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

    // -- error helpers ------------------------------------------------------

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

    // -- cursor operations --------------------------------------------------

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

    /// Read the next character (with CRLF folding).
    fn next_char(&mut self) -> Option<(usize, char)> {
        let i = self.cursor;
        let &b = self.bytes.get(i)?;

        if b == b'\r' && self.bytes.get(i + 1) == Some(&b'\n') {
            self.cursor = i + 2;
            return Some((i, '\n'));
        }

        if b < 0x80 {
            self.cursor = i + 1;
            Some((i, b as char))
        } else {
            // SAFETY: self.bytes is valid UTF-8
            let remaining = unsafe { std::str::from_utf8_unchecked(&self.bytes[i..]) };
            let ch = remaining.chars().next().unwrap();
            self.cursor = i + ch.len_utf8();
            Some((i, ch))
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
        // Consume comment content (valid bytes: tab, 0x20..=0x7E, 0x80..=0xFF)
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

    // -- keylike parsing ----------------------------------------------------

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

    fn read_table_key(&mut self) -> Result<Key<'a>, ParseError> {
        match self.peek_byte() {
            Some(b'"') => {
                let start = self.cursor;
                self.advance();
                let (span, val, multiline) = match self.read_string(start, b'"') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(start + val.len()),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(Key {
                    span,
                    name: val,
                })
            }
            Some(b'\'') => {
                let start = self.cursor;
                self.advance();
                let (span, val, multiline) = match self.read_string(start, b'\'') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                if multiline {
                    return Err(self.set_error(
                        start,
                        Some(start + val.len()),
                        ErrorKind::MultilineStringKey,
                    ));
                }
                Ok(Key {
                    span,
                    name: val,
                })
            }
            Some(b) if is_keylike_byte(b) => {
                let start = self.cursor;
                let k = self.read_keylike();
                let span = Span::new(start as u32, self.cursor as u32);
                Ok(Key {
                    span,
                    name: Str::from(k),
                })
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

    // -- string parsing -----------------------------------------------------

    /// Read a basic (double-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `"`.
    fn read_string(
        &mut self,
        start: usize,
        delim: u8,
    ) -> Result<(Span, Str<'a>, bool), ParseError> {
        let mut multiline = false;
        if self.eat_byte(delim) {
            if self.eat_byte(delim) {
                multiline = true;
            } else {
                return Ok((
                    Span::new(start as u32, (start + 1) as u32),
                    Str::from(""),
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
        if self.cursor >= self.bytes.len() {
            return;
        }
        let b = self.bytes[self.cursor];
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
    ) -> Result<(Span, Str<'a>, bool), ParseError> {
        let mut owned = false;
        loop {
            // Fast-scan past plain bytes (8 at a time via SWAR).
            let plain_start = self.cursor;
            self.skip_string_plain(delim);
            if owned && plain_start < self.cursor {
                self.string_buf
                    .extend_from_slice(&self.bytes[plain_start..self.cursor]);
            }

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
                        if owned {
                            self.string_buf.push(b'\r');
                            self.string_buf.push(b'\n');
                        }
                    } else {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\r')));
                    }
                }
                b'\n' => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                    if owned {
                        self.string_buf.push(b'\n');
                    }
                }
                d if d == delim => {
                    if multiline {
                        if !self.eat_byte(delim) {
                            if owned {
                                self.string_buf.push(delim);
                            }
                            continue;
                        }
                        if !self.eat_byte(delim) {
                            if owned {
                                self.string_buf.push(delim);
                                self.string_buf.push(delim);
                            }
                            continue;
                        }
                        let mut extra = 0usize;
                        if self.eat_byte(delim) {
                            if owned {
                                self.string_buf.push(delim);
                            }
                            extra += 1;
                        }
                        if self.eat_byte(delim) {
                            if owned {
                                self.string_buf.push(delim);
                            }
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
                        let val = if owned {
                            // SAFETY: string_buf contains valid UTF-8.
                            let s: &str =
                                unsafe { std::str::from_utf8_unchecked(&self.string_buf) };
                            let boxed: Box<str> = s.into();
                            self.string_buf.clear();
                            Str::from(boxed)
                        } else {
                            Str::from(unsafe { self.str_slice(content_start, i + extra) })
                        };
                        return Ok((span, val, true));
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    let val = if owned {
                        // SAFETY: string_buf contains valid UTF-8.
                        let s: &str =
                            unsafe { std::str::from_utf8_unchecked(&self.string_buf) };
                        let boxed: Box<str> = s.into();
                        self.string_buf.clear();
                        Str::from(boxed)
                    } else {
                        Str::from(unsafe { self.str_slice(content_start, i) })
                    };
                    return Ok((span, val, false));
                }
                b'\\' if delim == b'"' => {
                    if !owned {
                        self.string_buf.clear();
                        self.string_buf
                            .extend_from_slice(&self.bytes[content_start..i]);
                        owned = true;
                    }
                    if let Err(e) = self.read_basic_escape(start, multiline) {
                        return Err(e);
                    }
                }
                // Tab or backslash-in-literal-string: benign false positives
                // from the SWAR scan.
                0x09 | 0x20..=0x7E | 0x80.. => {
                    if owned {
                        self.string_buf.push(b);
                    }
                }
                _ => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(b as char)));
                }
            }
        }
    }

    fn read_basic_escape(&mut self, string_start: usize, multi: bool) -> Result<(), ParseError> {
        let i = self.cursor;
        let Some(&b) = self.bytes.get(i) else {
            return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
        };
        self.cursor = i + 1;

        match b {
            b'"' => self.string_buf.push(b'"'),
            b'\\' => self.string_buf.push(b'\\'),
            b'b' => self.string_buf.push(0x08),
            b'f' => self.string_buf.push(0x0C),
            b'n' => self.string_buf.push(b'\n'),
            b'r' => self.string_buf.push(b'\r'),
            b't' => self.string_buf.push(b'\t'),
            b'e' => self.string_buf.push(0x1B),
            b'u' => {
                let ch = self.read_hex(4, string_start, i);
                match ch {
                    Ok(ch) => {
                        let mut buf = [0u8; 4];
                        let len = ch.encode_utf8(&mut buf).len();
                        self.string_buf.extend_from_slice(&buf[..len]);
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
                        self.string_buf.extend_from_slice(&buf[..len]);
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
                        self.string_buf.extend_from_slice(&buf[..len]);
                    }
                    Err(e) => return Err(e),
                }
            }
            b' ' | b'\t' | b'\n' | b'\r' if multi => {
                // Line-ending backslash (CRLF folding: \r\n counts as \n)
                let c = if b == b'\r' && self.peek_byte() == Some(b'\n') {
                    self.advance();
                    '\n'
                } else {
                    b as char
                };
                if c != '\n' {
                    // Consume remaining whitespace until newline
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
                // Consume subsequent whitespace/newlines
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
                // Decode the byte as a char for the error message
                if b < 0x80 {
                    return Err(self.set_error(i, None, ErrorKind::InvalidEscape(b as char)));
                }
                // Multi-byte UTF-8 char in escape position
                self.cursor = i; // back up
                let (ei, ec) = self.next_char().unwrap();
                return Err(self.set_error(ei, None, ErrorKind::InvalidEscape(ec)));
            }
        }
        Ok(())
    }

    fn read_hex(
        &mut self,
        n: usize,
        string_start: usize,
        escape_start: usize,
    ) -> Result<char, ParseError> {
        let mut buf = [0u8; 8];
        for b in buf[..n].iter_mut() {
            let Some(&byte) = self.bytes.get(self.cursor) else {
                return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
            };
            if byte.is_ascii_hexdigit() {
                *b = byte;
                self.cursor += 1;
            } else {
                if byte < 0x80 {
                    let i = self.cursor;
                    self.cursor += 1;
                    return Err(self.set_error(i, None, ErrorKind::InvalidHexEscape(byte as char)));
                }
                let (i, ch) = self.next_char().unwrap();
                return Err(self.set_error(i, None, ErrorKind::InvalidHexEscape(ch)));
            }
        }
        let val =
            u32::from_str_radix(unsafe { std::str::from_utf8_unchecked(&buf[..n]) }, 16).unwrap();
        match char::from_u32(val) {
            Some(ch) => Ok(ch),
            None => Err(self.set_error(
                escape_start,
                Some(escape_start + n),
                ErrorKind::InvalidEscapeValue(val),
            )),
        }
    }

    // -- number parsing -----------------------------------------------------

    fn number(&mut self, start: u32, end: u32, s: &'a str) -> Result<Value<'a>, ParseError> {
        let span = |s, e| Span::new(s, e);
        if let Some(s) = s.strip_prefix("0x") {
            match self.integer(s, 16) {
                Ok(v) => Ok(Value::integer(v, span(start, end))),
                Err(e) => Err(e),
            }
        } else if let Some(s) = s.strip_prefix("0o") {
            match self.integer(s, 8) {
                Ok(v) => Ok(Value::integer(v, span(start, end))),
                Err(e) => Err(e),
            }
        } else if let Some(s) = s.strip_prefix("0b") {
            match self.integer(s, 2) {
                Ok(v) => Ok(Value::integer(v, span(start, end))),
                Err(e) => Err(e),
            }
        } else if s.contains('e') || s.contains('E') {
            match self.float(s, None) {
                Ok(f) => Ok(Value::float(f, span(start, self.cursor as u32))),
                Err(e) => Err(e),
            }
        } else if self.eat_byte(b'.') {
            let at = self.cursor;
            match self.peek_byte() {
                Some(b) if is_keylike_byte(b) => {
                    let after = self.read_keylike();
                    match self.float(s, Some(after)) {
                        Ok(f) => Ok(Value::float(f, span(start, self.cursor as u32))),
                        Err(e) => Err(e),
                    }
                }
                _ => Err(self.set_error(at, Some(end as usize), ErrorKind::InvalidNumber)),
            }
        } else if s == "inf" {
            Ok(Value::float(f64::INFINITY, span(start, end)))
        } else if s == "-inf" {
            Ok(Value::float(f64::NEG_INFINITY, span(start, end)))
        } else if s == "nan" {
            Ok(Value::float(f64::NAN.copysign(1.0), span(start, end)))
        } else if s == "-nan" {
            Ok(Value::float(f64::NAN.copysign(-1.0), span(start, end)))
        } else {
            match self.integer(s, 10) {
                Ok(v) => Ok(Value::integer(v, span(start, end))),
                Err(e) => Err(e),
            }
        }
    }

    fn number_leading_plus(&mut self, plus_start: u32) -> Result<Value<'a>, ParseError> {
        match self.peek_byte() {
            Some(b) if is_keylike_byte(b) => {
                let s = self.read_keylike();
                let end = self.cursor as u32;
                self.number(plus_start, end, s)
            }
            _ => Err(self.set_error(
                plus_start as usize,
                Some(self.cursor),
                ErrorKind::InvalidNumber,
            )),
        }
    }

    fn integer(&mut self, s: &'a str, radix: u32) -> Result<i64, ParseError> {
        let allow_sign = radix == 10;
        let allow_leading_zeros = radix != 10;
        let (prefix, suffix) = match self.parse_integer(s, allow_sign, allow_leading_zeros, radix) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let s_start = self.substr_offset(s);
        if !suffix.is_empty() {
            return Err(self.set_error(s_start, Some(s_start + s.len()), ErrorKind::InvalidNumber));
        }
        match i64::from_str_radix(prefix.replace('_', "").trim_start_matches('+'), radix) {
            Ok(v) => Ok(v),
            Err(_) => {
                Err(self.set_error(s_start, Some(s_start + s.len()), ErrorKind::InvalidNumber))
            }
        }
    }

    fn parse_integer(
        &mut self,
        s: &'a str,
        allow_sign: bool,
        allow_leading_zeros: bool,
        radix: u32,
    ) -> Result<(&'a str, &'a str), ParseError> {
        let s_start = self.substr_offset(s);
        let send = s_start + s.len();

        let mut first = true;
        let mut first_zero = false;
        let mut underscore = false;
        let mut end = s.len();
        for (i, c) in s.char_indices() {
            let at = i + s_start;
            if i == 0 && (c == '+' || c == '-') && allow_sign {
                continue;
            }

            if c == '0' && first {
                first_zero = true;
            } else if c.is_digit(radix) {
                if !first && first_zero && !allow_leading_zeros {
                    return Err(self.set_error(at, Some(send), ErrorKind::InvalidNumber));
                }
                underscore = false;
            } else if c == '_' && first {
                return Err(self.set_error(at, Some(send), ErrorKind::InvalidNumber));
            } else if c == '_' && !underscore {
                underscore = true;
            } else {
                end = i;
                break;
            }
            first = false;
        }
        if first || underscore {
            return Err(self.set_error(s_start, Some(send), ErrorKind::InvalidNumber));
        }
        Ok((&s[..end], &s[end..]))
    }

    fn float(&mut self, s: &'a str, after_decimal: Option<&'a str>) -> Result<f64, ParseError> {
        let (integral, mut suffix) = match self.parse_integer(s, true, false, 10) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let s_start = self.substr_offset(integral);

        let mut fraction = None;
        if let Some(after) = after_decimal {
            if !suffix.is_empty() {
                return Err(self.set_error(
                    s_start,
                    Some(s_start + s.len()),
                    ErrorKind::InvalidNumber,
                ));
            }
            let (a, b) = match self.parse_integer(after, false, true, 10) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            fraction = Some(a);
            suffix = b;
        }

        let mut exponent = None;
        if suffix.starts_with('e') || suffix.starts_with('E') {
            let (a, b) = if suffix.len() == 1 {
                self.eat_byte(b'+');
                match self.peek_byte() {
                    Some(b) if is_keylike_byte(b) => {
                        let next = self.read_keylike();
                        match self.parse_integer(next, false, true, 10) {
                            Ok(v) => v,
                            Err(e) => return Err(e),
                        }
                    }
                    _ => {
                        return Err(self.set_error(
                            s_start,
                            Some(s_start + s.len()),
                            ErrorKind::InvalidNumber,
                        ));
                    }
                }
            } else {
                match self.parse_integer(&suffix[1..], true, true, 10) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                }
            };
            if !b.is_empty() {
                return Err(self.set_error(
                    s_start,
                    Some(s_start + s.len()),
                    ErrorKind::InvalidNumber,
                ));
            }
            exponent = Some(a);
        } else if !suffix.is_empty() {
            return Err(self.set_error(s_start, Some(s_start + s.len()), ErrorKind::InvalidNumber));
        }

        // Build the float string in string_buf to avoid allocation
        self.string_buf.clear();
        self.string_buf.extend(
            integral
                .trim_start_matches('+')
                .bytes()
                .filter(|b| *b != b'_'),
        );
        if let Some(fraction) = fraction {
            self.string_buf.push(b'.');
            self.string_buf
                .extend(fraction.bytes().filter(|b| *b != b'_'));
        }
        if let Some(exponent) = exponent {
            self.string_buf.push(b'E');
            self.string_buf
                .extend(exponent.bytes().filter(|b| *b != b'_'));
        }
        let n: f64 = match unsafe { std::str::from_utf8_unchecked(&self.string_buf) }.parse() {
            Ok(n) => n,
            Err(_) => {
                return Err(self.set_error(
                    s_start,
                    Some(s_start + s.len()),
                    ErrorKind::InvalidNumber,
                ));
            }
        };
        if n.is_finite() {
            Ok(n)
        } else {
            Err(self.set_error(s_start, Some(s_start + s.len()), ErrorKind::InvalidNumber))
        }
    }

    fn substr_offset(&self, s: &str) -> usize {
        let a = self.bytes.as_ptr() as usize;
        let b = s.as_ptr() as usize;
        assert!(a <= b);
        b - a
    }

    // -- value parsing ------------------------------------------------------

    fn value(&mut self) -> Result<Value<'a>, ParseError> {
        let at = self.cursor;
        let Some(byte) = self.peek_byte() else {
            return Err(self.set_error(self.bytes.len(), None, ErrorKind::UnexpectedEof));
        };
        match byte {
            b'"' => {
                self.advance();
                let (span, val, _multiline) = match self.read_string(self.cursor - 1, b'"') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::string(val, span))
            }
            b'\'' => {
                self.advance();
                let (span, val, _multiline) = match self.read_string(self.cursor - 1, b'\'') {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::string(val, span))
            }
            b'{' => {
                let start = self.cursor as u32;
                self.advance();
                let mut table = value::Table::new();
                let end_span = match self.inline_table_contents(&mut table) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                // Frozen table (inline tables are immutable)
                Ok(Value::table_frozen(table, Span::new(start, end_span.end())))
            }
            b'[' => {
                let start = self.cursor as u32;
                self.advance();
                let mut arr = value::Array::new();
                let end_span = match self.array_contents(&mut arr) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Value::array(arr, Span::new(start, end_span.end())))
            }
            b'+' => {
                let start = self.cursor as u32;
                self.advance();
                self.number_leading_plus(start)
            }
            b if is_keylike_byte(b) => {
                let start = self.cursor as u32;
                let key = self.read_keylike();
                let end = self.cursor as u32;
                let span = Span::new(start, end);

                match key {
                    "true" => Ok(Value::boolean(true, span)),
                    "false" => Ok(Value::boolean(false, span)),
                    "inf" | "nan" => self.number(start, end, key),
                    _ => {
                        let first_char = key.chars().next().expect("key should not be empty");
                        match first_char {
                            '-' | '0'..='9' => self.number(start, end, key),
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

    fn inline_table_contents(&mut self, out: &mut value::Table<'a>) -> Result<Span, ParseError> {
        if let Err(e) = self.eat_inline_table_whitespace() {
            return Err(e);
        }
        if let Some(span) = self.eat_byte_spanned(b'}') {
            return Ok(span);
        }
        loop {
            // Parse key, navigating through dotted segments
            let mut table_ptr: *mut value::Table<'a> = out;
            let mut key = match self.read_table_key() {
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
                table_ptr = match self.navigate_dotted_key(table_ptr, &key) {
                    Ok(p) => p,
                    Err(e) => return Err(e),
                };
                key = match self.read_table_key() {
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
            let val = match self.value() {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            if let Err(e) = self.insert_value(table_ptr, key, val) {
                return Err(e);
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

    fn array_contents(&mut self, out: &mut value::Array<'a>) -> Result<Span, ParseError> {
        loop {
            if let Err(e) = self.eat_intermediate() {
                return Err(e);
            }
            if let Some(span) = self.eat_byte_spanned(b']') {
                return Ok(span);
            }
            let val = match self.value() {
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

    // -- navigation ---------------------------------------------------------

    /// Navigate into an existing or new table for a dotted-key intermediate
    /// segment. Checks frozen and header bits.
    /// New tables are created with the `DOTTED` tag.
    fn navigate_dotted_key(
        &mut self,
        table: *mut value::Table<'a>,
        key: &Key<'a>,
    ) -> Result<*mut value::Table<'a>, ParseError> {
        let table_ref = unsafe { &mut *table };

        if let Some(idx) = self.indexed_find(table_ref, key.name.as_ref()) {
            let (existing_key, existing_val) = table_ref.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let ok = existing_val.is_table()
                && !existing_val.is_frozen()
                && !existing_val.has_header_bit();

            if !ok {
                return Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DottedKeyInvalidType {
                        first: first_key_span,
                    },
                ));
            }
            let existing = table_ref.get_mut_at(idx);
            let t = unsafe { existing.as_table_mut_unchecked() };
            Ok(t as *mut _)
        } else {
            let new_val = Value::table_dotted(value::Table::new(), key.span);
            table_ref.insert(key.clone(), new_val);
            self.index_after_insert(unsafe { &*table });
            let last_idx = (unsafe { &*table }).len() as usize - 1;
            let inserted = (unsafe { &mut *table }).get_mut_at(last_idx);
            let t = unsafe { inserted.as_table_mut_unchecked() };
            Ok(t as *mut _)
        }
    }

    /// Navigate an intermediate segment of a table header (e.g. `a` in `[a.b.c]`).
    /// Creates implicit tables (no flag bits) if not found.
    /// Handles arrays-of-tables by navigating into the last element.
    fn navigate_header_intermediate(
        &mut self,
        key: &Key<'a>,
        header_start: u32,
        header_end: u32,
    ) -> Result<(), ParseError> {
        let table_ptr = self.current_table();
        let table_ref = unsafe { &mut *table_ptr };

        if let Some(idx) = self.indexed_find(table_ref, key.name.as_ref()) {
            let (existing_key, existing_val) = table_ref.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_table = existing_val.is_table();
            let is_array = existing_val.is_array();
            let is_frozen = existing_val.is_frozen();
            let is_aot = existing_val.is_aot();

            if is_table {
                if is_frozen {
                    return Err(self.set_error(
                        key.span.start() as usize,
                        Some(key.span.end() as usize),
                        ErrorKind::DuplicateKey {
                            key: key.name.to_string(),
                            first: first_key_span,
                        },
                    ));
                }
                let existing = table_ref.get_mut_at(idx);
                self.push_table_ctx(existing as *mut _, None);
                Ok(())
            } else if is_array && is_aot {
                let existing = table_ref.get_mut_at(idx);
                let arr_ptr = unsafe { NonNull::new_unchecked(existing as *mut Value<'a>) };
                let arr = unsafe { (*arr_ptr.as_ptr()).as_array_mut() }.unwrap();
                let last = arr.last_mut().unwrap();
                if !last.is_table() {
                    return Err(self.set_error(
                        key.span.start() as usize,
                        Some(key.span.end() as usize),
                        ErrorKind::DuplicateKey {
                            key: key.name.to_string(),
                            first: first_key_span,
                        },
                    ));
                }
                self.push_table_ctx(last as *mut _, Some(arr_ptr));
                Ok(())
            } else {
                Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DuplicateKey {
                        key: key.name.to_string(),
                        first: first_key_span,
                    },
                ))
            }
        } else {
            let span = Span::new(header_start, header_end);
            let new_val = Value::table(value::Table::new(), span);
            table_ref.insert(key.clone(), new_val);
            self.index_after_insert(unsafe { &*table_ptr });
            let last_idx = (unsafe { &*table_ptr }).len() - 1;
            let inserted = (unsafe { &mut *table_ptr }).get_mut_at(last_idx);
            self.push_table_ctx(inserted as *mut _, None);
            Ok(())
        }
    }

    /// Handle the final segment of a standard table header `[a.b.c]`.
    fn navigate_header_table_final(
        &mut self,
        key: &Key<'a>,
        header_start: u32,
        header_end: u32,
    ) -> Result<(), ParseError> {
        let table_ptr = self.current_table();
        let table_ref = unsafe { &mut *table_ptr };

        if let Some(idx) = self.indexed_find(table_ref, key.name.as_ref()) {
            let (existing_key, existing_val) = table_ref.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_table = existing_val.is_table();
            let is_frozen = existing_val.is_frozen();
            let has_header = existing_val.has_header_bit();
            let has_dotted = existing_val.has_dotted_bit();
            let val_span = existing_val.span();

            if !is_table {
                return Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DuplicateKey {
                        key: key.name.to_string(),
                        first: first_key_span,
                    },
                ));
            }
            if is_frozen {
                return Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DuplicateKey {
                        key: key.name.to_string(),
                        first: first_key_span,
                    },
                ));
            }
            if has_header {
                return Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::DuplicateTable {
                        name: key.name.to_string(),
                        first: val_span,
                    },
                ));
            }
            if has_dotted {
                return Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DuplicateKey {
                        key: key.name.to_string(),
                        first: first_key_span,
                    },
                ));
            }
            // Implicitly created table — now explicitly define it.
            let existing = table_ref.get_mut_at(idx);
            existing.set_header_tag();
            existing.set_span_start(header_start);
            existing.set_span_end(header_end);
            self.push_table_ctx(existing as *mut _, None);
            Ok(())
        } else {
            let new_val =
                Value::table_header(value::Table::new(), Span::new(header_start, header_end));
            table_ref.insert(key.clone(), new_val);
            self.index_after_insert(unsafe { &*table_ptr });
            let last_idx = (unsafe { &*table_ptr }).len() - 1;
            let inserted = (unsafe { &mut *table_ptr }).get_mut_at(last_idx);
            self.push_table_ctx(inserted as *mut _, None);
            Ok(())
        }
    }

    /// Handle the final segment of an array-of-tables header `[[a.b.c]]`.
    fn navigate_header_array_final(
        &mut self,
        key: &Key<'a>,
        header_start: u32,
        header_end: u32,
    ) -> Result<(), ParseError> {
        let table_ptr = self.current_table();
        let table_ref = unsafe { &mut *table_ptr };

        if let Some(idx) = self.indexed_find(table_ref, key.name.as_ref()) {
            let (existing_key, existing_val) = table_ref.get_key_value_at(idx);
            let first_key_span = existing_key.span;
            let is_aot = existing_val.is_aot();
            let is_table = existing_val.is_table();

            if is_aot {
                let existing = table_ref.get_mut_at(idx);
                let arr_ptr = unsafe { NonNull::new_unchecked(existing as *mut Value<'a>) };
                let arr = unsafe { (*arr_ptr.as_ptr()).as_array_mut() }.unwrap();
                let entry_span = Span::new(header_start, header_end);
                arr.push(Value::table_header(value::Table::new(), entry_span));
                let entry = arr.last_mut().unwrap();
                self.push_table_ctx(entry as *mut _, Some(arr_ptr));
                Ok(())
            } else if is_table {
                Err(self.set_error(
                    header_start as usize,
                    Some(header_end as usize),
                    ErrorKind::RedefineAsArray,
                ))
            } else {
                Err(self.set_error(
                    key.span.start() as usize,
                    Some(key.span.end() as usize),
                    ErrorKind::DuplicateKey {
                        key: key.name.to_string(),
                        first: first_key_span,
                    },
                ))
            }
        } else {
            let entry_span = Span::new(header_start, header_end);
            let first_entry = Value::table_header(value::Table::new(), entry_span);
            let array_span = Span::new(header_start, header_end);
            let array_val = Value::array_aot(value::Array::with_single(first_entry), array_span);
            table_ref.insert(key.clone(), array_val);
            self.index_after_insert(unsafe { &*table_ptr });

            let last_idx = (unsafe { &*table_ptr }).len() - 1;
            let inserted = (unsafe { &mut *table_ptr }).get_mut_at(last_idx);
            let arr_ptr = unsafe { NonNull::new_unchecked(inserted as *mut Value<'a>) };
            let arr = unsafe { (*arr_ptr.as_ptr()).as_array_mut() }.unwrap();
            let entry = arr.last_mut().unwrap();
            self.push_table_ctx(entry as *mut _, Some(arr_ptr));
            Ok(())
        }
    }

    /// Insert a value into a table, checking for duplicates.
    fn insert_value(
        &mut self,
        table: *mut value::Table<'a>,
        key: Key<'a>,
        val: Value<'a>,
    ) -> Result<(), ParseError> {
        let table_ref = unsafe { &mut *table };

        if let Some(idx) = self.indexed_find(table_ref, key.name.as_ref()) {
            let (existing_key, _) = table_ref.get_key_value_at(idx);
            let first_span = existing_key.span;
            return Err(self.set_error(
                key.span.start() as usize,
                Some(key.span.end() as usize),
                ErrorKind::DuplicateKey {
                    key: key.name.to_string(),
                    first: first_span,
                },
            ));
        }

        table_ref.insert(key, val);
        self.index_after_insert(unsafe { &*table });
        Ok(())
    }

    // -- key index helpers ----------------------------------------------------

    /// Look up a key name in a table, returning its entry index.
    /// Uses the hash index for tables at or above the threshold, otherwise
    /// falls back to a linear scan.
    fn indexed_find(&self, table: &value::Table<'a>, name: &str) -> Option<usize> {
        if table.len() >= INDEXED_TABLE_THRESHOLD {
            let first_key_span = table.first_key_span_start();
            let probe = KeyIndex {
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
        if len == INDEXED_TABLE_THRESHOLD {
            self.bulk_index_table(table);
        } else if len > INDEXED_TABLE_THRESHOLD {
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
            self.table_index.insert(ki, i as usize);
        }
    }

    /// Index only the last (just-inserted) entry of an already-indexed table.
    fn index_last_entry(&mut self, table: &value::Table<'a>) {
        let idx = (table.len() - 1) as usize;
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

    // -- context helpers ------------------------------------------------------

    #[inline]
    fn current_table(&self) -> *mut value::Table<'a> {
        unsafe { self.ctx.last().unwrap().value_ptr.byte_add(8).cast() }
    }

    /// Push a context entry for a `Value` known to contain a `Table`.
    ///
    /// SAFETY: `value_ptr` must point to a live `Value` whose tag is a table
    /// variant. The table pointer is derived from `value_ptr` so its provenance
    /// is a child — later span-field projections on `value_ptr` will not
    /// invalidate the table tag under Stacked Borrows.
    fn push_table_ctx(&mut self, value_ptr: *mut Value<'a>, array_ptr: Option<NonNull<Value<'a>>>) {
        self.ctx.push(Ctx {
            value_ptr,
            array_ptr,
        });
    }

    fn reset_to_root(&mut self, root_value: *mut Value<'a>) {
        self.ctx.clear();
        self.ctx.push(Ctx {
            value_ptr: root_value,
            array_ptr: None,
        });
    }

    fn parse_document(&mut self, root_value: *mut Value<'a>) -> Result<(), ParseError> {
        self.reset_to_root(root_value);

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
                    if let Err(e) = self.process_table_header(root_value) {
                        return Err(e);
                    }
                }
                Some(b'\r') => {
                    return Err(self.set_error(self.cursor, None, ErrorKind::Unexpected('\r')));
                }
                Some(_) => {
                    if let Err(e) = self.process_key_value() {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    fn process_table_header(&mut self, root_value: *mut Value<'a>) -> Result<(), ParseError> {
        let header_start = self.cursor as u32;
        if let Err(e) = self.expect_byte(b'[') {
            return Err(e);
        }
        let is_array = self.eat_byte(b'[');

        self.reset_to_root(root_value);
        let ctx_base = self.ctx.len();

        // Parse and navigate key segments inline
        self.eat_whitespace();
        let mut key = match self.read_table_key() {
            Ok(k) => k,
            Err(e) => return Err(e),
        };
        loop {
            self.eat_whitespace();
            if self.eat_byte(b'.') {
                self.eat_whitespace();
                // Navigate intermediate key (header_end not yet known; use 0
                // as placeholder — patched below after we parse the closing
                // bracket).
                if let Err(e) = self.navigate_header_intermediate(&key, header_start, 0) {
                    return Err(e);
                }
                key = match self.read_table_key() {
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

        // Patch header_end on any implicit tables created during intermediate
        // navigation (they were inserted with end=0 as a placeholder).
        //
        // Use raw-pointer field writes to avoid `&mut Value` Unique retags
        // that would invalidate sibling table pointers in each Ctx.
        for ctx in &self.ctx[ctx_base..] {
            unsafe {
                if Value::ptr_raw_end(ctx.value_ptr) == 0 {
                    Value::ptr_set_span_end(ctx.value_ptr, header_end);
                }
            }
        }

        if is_array {
            self.navigate_header_array_final(&key, header_start, header_end)
        } else {
            self.navigate_header_table_final(&key, header_start, header_end)
        }
    }

    fn process_key_value(&mut self) -> Result<(), ParseError> {
        let line_start = self.cursor as u32;
        let mut table_ptr = self.current_table();

        // Read first key
        let mut key = match self.read_table_key() {
            Ok(k) => k,
            Err(e) => return Err(e),
        };
        self.eat_whitespace();

        // Navigate through dotted key segments
        while self.eat_byte(b'.') {
            self.eat_whitespace();
            table_ptr = match self.navigate_dotted_key(table_ptr, &key) {
                Ok(p) => p,
                Err(e) => return Err(e),
            };
            key = match self.read_table_key() {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            self.eat_whitespace();
        }

        // Parse value
        if let Err(e) = self.expect_byte(b'=') {
            return Err(e);
        }
        self.eat_whitespace();
        let val = match self.value() {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let line_end = self.cursor as u32;

        // Trailing whitespace / comment / newline
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

        // Insert and extend current table span
        if let Err(e) = self.insert_value(table_ptr, key, val) {
            return Err(e);
        }

        // Extend the span of the current context's table value.
        //
        // Use raw-pointer field writes to avoid creating `&mut Value`, which
        // would produce a Unique retag that invalidates the sibling table
        // pointer stored in `ctx.table`.
        let ctx = self.ctx.last().unwrap();
        unsafe {
            let start = Value::ptr_raw_start(ctx.value_ptr);
            Value::ptr_set_span_start(ctx.value_ptr, start.min(line_start));
            Value::ptr_extend_span_end(ctx.value_ptr, line_end);

            // Also extend the parent array-of-tables span if applicable
            if let Some(arr_ptr) = ctx.array_ptr {
                Value::ptr_extend_span_end(arr_ptr.as_ptr(), line_end);
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Top-level parse entry point
// ---------------------------------------------------------------------------

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

    // SAFETY: derive root_value first, then root_table from it, so that
    // root_table's provenance is a child of root_value (not invalidated by it).
    let root_value: *mut Value<'_> = &mut root;

    let mut parser = Parser::new(s);
    match parser.parse_document(root_value) {
        Ok(()) => {}
        Err(_) => return Err(parser.take_error()),
    }

    // No strip_flags needed — the public span() accessor masks out tag/flag bits.
    Ok(root)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
