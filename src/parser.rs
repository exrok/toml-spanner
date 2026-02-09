// Deliberately avoid `?` operator throughout this module for compile-time
// performance: explicit match/if-let prevents the compiler from generating
// From::from conversion and drop-glue machinery at every call site.
#![allow(clippy::question_mark)]

use crate::{
    Span,
    error::{Error, ErrorKind},
    value::{self, Key, Value, ValueInner},
};
use smallvec::SmallVec;
use std::{
    borrow::Cow,
    collections::{HashMap, btree_map::Entry},
    ops::Range,
};

type DeStr<'de> = Cow<'de, str>;
type TablePair<'de> = (Key<'de>, Val<'de>);
type InlineVec<T> = SmallVec<[T; 5]>;

// ---------------------------------------------------------------------------
// Lightweight internal error -- zero-sized, no drop glue.
// When a method returns Err(ParseError), the full error details have already
// been written into Parser::error_kind / Parser::error_span.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone)]
struct ParseError;

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
    string_buf: String,
}

#[allow(unsafe_code)]
impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        let mut p = Parser {
            bytes: input.as_bytes(),
            cursor: 0,
            error_span: Span::new(0, 0),
            error_kind: None,
            string_buf: String::new(),
        };
        // Eat UTF-8 BOM
        if p.bytes.get(p.cursor..p.cursor + 3) == Some(b"\xef\xbb\xbf") {
            p.cursor += 3;
        }
        p
    }

    /// Get a `&str` slice from the underlying bytes.
    /// SAFETY: `self.bytes` is always valid UTF-8, and callers must ensure
    /// `start..end` falls on UTF-8 char boundaries.
    #[inline]
    unsafe fn str_slice(&self, start: usize, end: usize) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(&self.bytes[start..end]) }
    }

    // -- error helpers ------------------------------------------------------

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
        let line_info = Some(self.to_linecol(span.start as usize));
        Error {
            kind,
            span,
            line_info,
        }
    }

    /// Construct a full public `Error` directly (used by `DeserializeCtx` at the top level).
    fn error(&self, start: usize, end: Option<usize>, kind: ErrorKind) -> Error {
        let span = Span::new(start as u32, end.unwrap_or(start + 1) as u32);
        let line_info = Some(self.to_linecol(start));
        Error {
            span,
            kind,
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
        if self.cursor >= self.bytes.len() {
            return None;
        }
        let i = self.cursor;
        let b = self.bytes[i];

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

    /// Peek one char without consuming (with CRLF folding).
    fn peek_char(&self) -> Option<(usize, char)> {
        if self.cursor >= self.bytes.len() {
            return None;
        }
        let i = self.cursor;
        let b = self.bytes[i];

        if b == b'\r' && self.bytes.get(i + 1) == Some(&b'\n') {
            return Some((i, '\n'));
        }

        if b < 0x80 {
            Some((i, b as char))
        } else {
            // SAFETY: self.bytes is valid UTF-8
            let remaining = unsafe { std::str::from_utf8_unchecked(&self.bytes[i..]) };
            let ch = remaining.chars().next().unwrap();
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
        // Consume comment content (valid chars: tab, 0x20..=0x10ffff except DEL)
        loop {
            match self.peek_char() {
                Some((_, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    self.next_char();
                }
                _ => break,
            }
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
                let (span, val, multiline) = match self.read_basic_string(start) {
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
                Ok(Key { span, name: val })
            }
            Some(b'\'') => {
                let start = self.cursor;
                self.advance();
                let (span, val, multiline) = match self.read_literal_string(start) {
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
                Ok(Key { span, name: val })
            }
            Some(b) if is_keylike_byte(b) => {
                let start = self.cursor;
                let k = self.read_keylike();
                let span = Span::new(start as u32, self.cursor as u32);
                Ok(Key {
                    span,
                    name: Cow::Borrowed(k),
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

    fn read_dotted_key(&mut self, keys: &mut Vec<Key<'a>>) -> Result<(), ParseError> {
        keys.clear();
        match self.read_table_key() {
            Ok(k) => keys.push(k),
            Err(e) => return Err(e),
        }
        self.eat_whitespace();
        while self.eat_byte(b'.') {
            self.eat_whitespace();
            match self.read_table_key() {
                Ok(k) => keys.push(k),
                Err(e) => return Err(e),
            }
            self.eat_whitespace();
        }
        Ok(())
    }

    // -- string parsing -----------------------------------------------------

    /// Read a basic (double-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `"`.
    fn read_basic_string(
        &mut self,
        start: usize,
    ) -> Result<(Span, Cow<'a, str>, bool), ParseError> {
        let mut multiline = false;
        if self.eat_byte(b'"') {
            if self.eat_byte(b'"') {
                multiline = true;
            } else {
                // Empty string: ""
                return Ok((
                    Span::new(start as u32, (start + 1) as u32),
                    Cow::Borrowed(""),
                    false,
                ));
            }
        }

        let mut escaped = false;
        let content_start = self.cursor;
        let mut n = 0usize;

        loop {
            n += 1;
            match self.next_char() {
                Some((i, '\n')) => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                    if self.bytes[i] == b'\r' && !escaped {
                        // Bare \r in multiline -- need owned
                        if !escaped {
                            self.string_buf.clear();
                            self.string_buf
                                .push_str(unsafe { self.str_slice(content_start, i) });
                            escaped = true;
                        }
                    }
                    if n == 1 && !escaped {
                        // Skip leading newline in multiline, reset content start
                        return self.read_basic_string_inner(start, self.cursor, true);
                    }
                    if escaped {
                        self.string_buf.push('\n');
                    }
                }
                Some((mut i, '"')) => {
                    if multiline {
                        if !self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            continue;
                        }
                        if !self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                                self.string_buf.push('"');
                            }
                            continue;
                        }
                        // Optional extra quotes
                        if self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            i += 1;
                        }
                        if self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            i += 1;
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
                        let val = if escaped {
                            Cow::Owned(self.string_buf.split_off(0))
                        } else {
                            Cow::Borrowed(unsafe { self.str_slice(content_start, i) })
                        };
                        return Ok((span, val, true));
                    }

                    // Single-line string
                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    let val = if escaped {
                        Cow::Owned(self.string_buf.split_off(0))
                    } else {
                        Cow::Borrowed(unsafe { self.str_slice(content_start, i) })
                    };
                    return Ok((span, val, false));
                }
                Some((i, '\\')) => {
                    // Escape sequence
                    if !escaped {
                        self.string_buf.clear();
                        self.string_buf
                            .push_str(unsafe { self.str_slice(content_start, i) });
                        escaped = true;
                    }
                    if let Err(e) = self.read_basic_escape(start, multiline) {
                        return Err(e);
                    }
                }
                Some((i, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    if escaped {
                        self.string_buf.push(ch);
                    }
                    let _ = i;
                }
                Some((i, ch)) => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(ch)));
                }
                None => {
                    return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
                }
            }
        }
    }

    /// Continue reading a basic multiline string after the leading newline was skipped.
    fn read_basic_string_inner(
        &mut self,
        start: usize,
        content_start: usize,
        multiline: bool,
    ) -> Result<(Span, Cow<'a, str>, bool), ParseError> {
        let mut escaped = false;

        loop {
            match self.next_char() {
                Some((i, '\n')) => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                    if self.bytes[i] == b'\r' && !escaped {
                        self.string_buf.clear();
                        self.string_buf
                            .push_str(unsafe { self.str_slice(content_start, i) });
                        escaped = true;
                    }
                    if escaped {
                        self.string_buf.push('\n');
                    }
                }
                Some((mut i, '"')) => {
                    if multiline {
                        if !self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            continue;
                        }
                        if !self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                                self.string_buf.push('"');
                            }
                            continue;
                        }
                        if self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            i += 1;
                        }
                        if self.eat_byte(b'"') {
                            if escaped {
                                self.string_buf.push('"');
                            }
                            i += 1;
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
                        let val = if escaped {
                            Cow::Owned(self.string_buf.split_off(0))
                        } else {
                            Cow::Borrowed(unsafe { self.str_slice(content_start, i) })
                        };
                        return Ok((span, val, true));
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    let val = if escaped {
                        Cow::Owned(self.string_buf.split_off(0))
                    } else {
                        Cow::Borrowed(unsafe { self.str_slice(content_start, i) })
                    };
                    return Ok((span, val, false));
                }
                Some((i, '\\')) => {
                    if !escaped {
                        self.string_buf.clear();
                        self.string_buf
                            .push_str(unsafe { self.str_slice(content_start, i) });
                        escaped = true;
                    }
                    if let Err(e) = self.read_basic_escape(start, multiline) {
                        return Err(e);
                    }
                }
                Some((i, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    if escaped {
                        self.string_buf.push(ch);
                    }
                    let _ = i;
                }
                Some((i, ch)) => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(ch)));
                }
                None => {
                    return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
                }
            }
        }
    }

    fn read_basic_escape(&mut self, string_start: usize, multi: bool) -> Result<(), ParseError> {
        if self.cursor >= self.bytes.len() {
            return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
        }
        let i = self.cursor;
        let b = self.bytes[i];
        self.cursor = i + 1;

        match b {
            b'"' => self.string_buf.push('"'),
            b'\\' => self.string_buf.push('\\'),
            b'b' => self.string_buf.push('\u{8}'),
            b'f' => self.string_buf.push('\u{c}'),
            b'n' => self.string_buf.push('\n'),
            b'r' => self.string_buf.push('\r'),
            b't' => self.string_buf.push('\t'),
            b'e' => self.string_buf.push('\u{1b}'),
            b'u' => {
                let ch = self.read_hex(4, string_start, i);
                match ch {
                    Ok(ch) => self.string_buf.push(ch),
                    Err(e) => return Err(e),
                }
            }
            b'U' => {
                let ch = self.read_hex(8, string_start, i);
                match ch {
                    Ok(ch) => self.string_buf.push(ch),
                    Err(e) => return Err(e),
                }
            }
            b'x' => {
                let ch = self.read_hex(2, string_start, i);
                match ch {
                    Ok(ch) => self.string_buf.push(ch),
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
            if self.cursor >= self.bytes.len() {
                return Err(self.set_error(string_start, None, ErrorKind::UnterminatedString));
            }
            let byte = self.bytes[self.cursor];
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

    /// Read a literal (single-quoted) string. `start` is the byte offset of the
    /// opening quote. The cursor should be positioned right after the opening `'`.
    fn read_literal_string(
        &mut self,
        start: usize,
    ) -> Result<(Span, Cow<'a, str>, bool), ParseError> {
        let mut multiline = false;
        if self.eat_byte(b'\'') {
            if self.eat_byte(b'\'') {
                multiline = true;
            } else {
                return Ok((
                    Span::new(start as u32, (start + 1) as u32),
                    Cow::Borrowed(""),
                    false,
                ));
            }
        }

        let content_start = self.cursor;
        let mut n = 0usize;

        loop {
            n += 1;
            match self.next_char() {
                Some((i, '\n')) => {
                    if multiline {
                        if n == 1 {
                            // Skip leading newline, reset content start
                            return self.read_literal_string_inner(start, self.cursor, true);
                        }
                        // \r handling: if the original byte was \r, we need owned string
                        if self.bytes[i] == b'\r' {
                            self.string_buf.clear();
                            self.string_buf
                                .push_str(unsafe { self.str_slice(content_start, i) });
                            self.string_buf.push('\n');
                            return self.read_literal_string_owned(start, true);
                        }
                    } else {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                }
                Some((mut i, '\'')) => {
                    if multiline {
                        if !self.eat_byte(b'\'') {
                            continue;
                        }
                        if !self.eat_byte(b'\'') {
                            continue;
                        }
                        if self.eat_byte(b'\'') {
                            i += 1;
                        }
                        if self.eat_byte(b'\'') {
                            i += 1;
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
                        return Ok((
                            span,
                            Cow::Borrowed(unsafe { self.str_slice(content_start, i) }),
                            true,
                        ));
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    return Ok((
                        span,
                        Cow::Borrowed(unsafe { self.str_slice(content_start, i) }),
                        false,
                    ));
                }
                Some((i, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    let _ = i;
                }
                Some((i, ch)) => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(ch)));
                }
                None => {
                    return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
                }
            }
        }
    }

    fn read_literal_string_inner(
        &mut self,
        start: usize,
        content_start: usize,
        multiline: bool,
    ) -> Result<(Span, Cow<'a, str>, bool), ParseError> {
        loop {
            match self.next_char() {
                Some((i, '\n')) => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                    if self.bytes[i] == b'\r' {
                        self.string_buf.clear();
                        self.string_buf
                            .push_str(unsafe { self.str_slice(content_start, i) });
                        self.string_buf.push('\n');
                        return self.read_literal_string_owned(start, true);
                    }
                }
                Some((mut i, '\'')) => {
                    if multiline {
                        if !self.eat_byte(b'\'') {
                            continue;
                        }
                        if !self.eat_byte(b'\'') {
                            continue;
                        }
                        if self.eat_byte(b'\'') {
                            i += 1;
                        }
                        if self.eat_byte(b'\'') {
                            i += 1;
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
                        return Ok((
                            span,
                            Cow::Borrowed(unsafe { self.str_slice(content_start, i) }),
                            true,
                        ));
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    return Ok((
                        span,
                        Cow::Borrowed(unsafe { self.str_slice(content_start, i) }),
                        false,
                    ));
                }
                Some((i, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    let _ = i;
                }
                Some((i, ch)) => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(ch)));
                }
                None => {
                    return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
                }
            }
        }
    }

    /// Continue reading a literal multiline string after we started building into `string_buf`.
    fn read_literal_string_owned(
        &mut self,
        start: usize,
        multiline: bool,
    ) -> Result<(Span, Cow<'a, str>, bool), ParseError> {
        loop {
            match self.next_char() {
                Some((i, '\n')) => {
                    if !multiline {
                        return Err(self.set_error(i, None, ErrorKind::InvalidCharInString('\n')));
                    }
                    self.string_buf.push('\n');
                }
                Some((mut i, '\'')) => {
                    if multiline {
                        if !self.eat_byte(b'\'') {
                            self.string_buf.push('\'');
                            continue;
                        }
                        if !self.eat_byte(b'\'') {
                            self.string_buf.push('\'');
                            self.string_buf.push('\'');
                            continue;
                        }
                        if self.eat_byte(b'\'') {
                            self.string_buf.push('\'');
                            i += 1;
                        }
                        if self.eat_byte(b'\'') {
                            self.string_buf.push('\'');
                            i += 1;
                        }

                        let _ = i;
                        let maybe_nl = self.bytes[start + 3];
                        let start_off = if maybe_nl == b'\n' {
                            4
                        } else if maybe_nl == b'\r' {
                            5
                        } else {
                            3
                        };

                        let span = Span::new((start + start_off) as u32, (self.cursor - 3) as u32);
                        return Ok((span, Cow::Owned(self.string_buf.split_off(0)), true));
                    }

                    let span = Span::new((start + 1) as u32, (self.cursor - 1) as u32);
                    return Ok((span, Cow::Owned(self.string_buf.split_off(0)), false));
                }
                Some((i, ch))
                    if ch == '\u{09}'
                        || (ch != '\u{7f}' && ('\u{20}'..='\u{10ffff}').contains(&ch)) =>
                {
                    self.string_buf.push(ch);
                    let _ = i;
                }
                Some((i, ch)) => {
                    return Err(self.set_error(i, None, ErrorKind::InvalidCharInString(ch)));
                }
                None => {
                    return Err(self.set_error(start, None, ErrorKind::UnterminatedString));
                }
            }
        }
    }

    // -- number parsing -----------------------------------------------------

    fn number(&mut self, start: u32, end: u32, s: &'a str) -> Result<Val<'a>, ParseError> {
        let to_integer = |f| Val {
            e: E::Integer(f),
            start,
            end,
        };
        if let Some(s) = s.strip_prefix("0x") {
            self.integer(s, 16).map(to_integer)
        } else if let Some(s) = s.strip_prefix("0o") {
            self.integer(s, 8).map(to_integer)
        } else if let Some(s) = s.strip_prefix("0b") {
            self.integer(s, 2).map(to_integer)
        } else if s.contains('e') || s.contains('E') {
            self.float(s, None).map(|f| Val {
                e: E::Float(f),
                start,
                end: self.cursor as u32,
            })
        } else if self.eat_byte(b'.') {
            let at = self.cursor;
            match self.peek_byte() {
                Some(b) if is_keylike_byte(b) => {
                    let after = self.read_keylike();
                    self.float(s, Some(after)).map(|f| Val {
                        e: E::Float(f),
                        start,
                        end: self.cursor as u32,
                    })
                }
                _ => Err(self.set_error(at, Some(end as usize), ErrorKind::InvalidNumber)),
            }
        } else if s == "inf" {
            Ok(Val {
                e: E::Float(f64::INFINITY),
                start,
                end,
            })
        } else if s == "-inf" {
            Ok(Val {
                e: E::Float(f64::NEG_INFINITY),
                start,
                end,
            })
        } else if s == "nan" {
            Ok(Val {
                e: E::Float(f64::NAN.copysign(1.0)),
                start,
                end,
            })
        } else if s == "-nan" {
            Ok(Val {
                e: E::Float(f64::NAN.copysign(-1.0)),
                start,
                end,
            })
        } else {
            self.integer(s, 10).map(to_integer)
        }
    }

    fn number_leading_plus(&mut self, plus_start: u32) -> Result<Val<'a>, ParseError> {
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
        for c in integral
            .trim_start_matches('+')
            .chars()
            .filter(|c| *c != '_')
        {
            self.string_buf.push(c);
        }
        if let Some(fraction) = fraction {
            self.string_buf.push('.');
            self.string_buf
                .extend(fraction.chars().filter(|c| *c != '_'));
        }
        if let Some(exponent) = exponent {
            self.string_buf.push('E');
            self.string_buf
                .extend(exponent.chars().filter(|c| *c != '_'));
        }
        let n: f64 = match self.string_buf.parse() {
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

    fn value(&mut self) -> Result<Val<'a>, ParseError> {
        let at = self.cursor;
        match self.peek_byte() {
            Some(b'"') => {
                self.advance();
                let (span, val, _multiline) = match self.read_basic_string(self.cursor - 1) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Val {
                    e: E::String(val),
                    start: span.start,
                    end: span.end,
                })
            }
            Some(b'\'') => {
                self.advance();
                let (span, val, _multiline) = match self.read_literal_string(self.cursor - 1) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Val {
                    e: E::String(val),
                    start: span.start,
                    end: span.end,
                })
            }
            Some(b'{') => {
                let start = self.cursor as u32;
                self.advance();
                let mut table = TableValues::default();
                let end_span = match self.inline_table_contents(&mut table) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Val {
                    e: E::InlineTable(table),
                    start,
                    end: end_span.end,
                })
            }
            Some(b'[') => {
                let start = self.cursor as u32;
                self.advance();
                let mut arr = Vec::new();
                let end_span = match self.array_contents(&mut arr) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                Ok(Val {
                    e: E::Array(arr),
                    start,
                    end: end_span.end,
                })
            }
            Some(b'+') => {
                let start = self.cursor as u32;
                self.advance();
                self.number_leading_plus(start)
            }
            Some(b) if is_keylike_byte(b) => {
                let start = self.cursor as u32;
                let key = self.read_keylike();
                let end = self.cursor as u32;
                let span = Span::new(start, end);

                match key {
                    "true" => Ok(Val {
                        e: E::Boolean(true),
                        start: span.start,
                        end: span.end,
                    }),
                    "false" => Ok(Val {
                        e: E::Boolean(false),
                        start: span.start,
                        end: span.end,
                    }),
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
            None => Err(self.set_error(self.bytes.len(), None, ErrorKind::UnexpectedEof)),
            Some(_) => {
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

    fn inline_table_contents(&mut self, out: &mut TableValues<'a>) -> Result<Span, ParseError> {
        let mut keys = Vec::new();
        if let Err(e) = self.eat_inline_table_whitespace() {
            return Err(e);
        }
        if let Some(span) = self.eat_byte_spanned(b'}') {
            return Ok(span);
        }
        loop {
            if let Err(e) = self.read_dotted_key(&mut keys) {
                return Err(e);
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
            let value = match self.value() {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            if let Err(e) = self.add_dotted_key(&mut keys, value, out) {
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

    fn array_contents(&mut self, out: &mut Vec<Val<'a>>) -> Result<Span, ParseError> {
        loop {
            if let Err(e) = self.eat_intermediate() {
                return Err(e);
            }
            if let Some(span) = self.eat_byte_spanned(b']') {
                return Ok(span);
            }
            let value = match self.value() {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
            out.push(value);
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

    // -- table/line-level parsing -------------------------------------------

    fn tables(&mut self) -> Result<Vec<RawTable<'a>>, ParseError> {
        let mut tables = Vec::new();
        let mut cur_table = RawTable {
            at: 0,
            end: 0,
            header: InlineVec::new(),
            values: None,
            array: false,
        };

        let mut keys = Vec::new();

        loop {
            let line = match self.line(&mut keys) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => return Err(e),
            };
            match line {
                Line::Table {
                    at,
                    end,
                    header,
                    array,
                } => {
                    if !cur_table.header.is_empty() || cur_table.values.is_some() {
                        tables.push(cur_table);
                    }
                    cur_table = RawTable {
                        at,
                        end,
                        header,
                        values: Some(TableValues::default()),
                        array,
                    };
                }
                Line::KeyValue {
                    key,
                    value,
                    at,
                    end,
                } => {
                    let table_values = cur_table.values.get_or_insert_with(|| TableValues {
                        values: Vec::new(),
                        span: None,
                    });
                    let mut kv_keys = key;
                    if let Err(e) = self.add_dotted_key(&mut kv_keys, value, table_values) {
                        return Err(e);
                    }
                    match table_values.span {
                        Some(ref mut span) => {
                            span.start = span.start.min(at);
                            span.end = span.end.max(end);
                        }
                        None => {
                            table_values.span = Some(Span::new(at, end));
                        }
                    }
                }
            }
        }
        if !cur_table.header.is_empty() || cur_table.values.is_some() {
            tables.push(cur_table);
        }
        Ok(tables)
    }

    fn line(&mut self, keys: &mut Vec<Key<'a>>) -> Result<Option<Line<'a>>, ParseError> {
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
            break;
        }

        match self.peek_byte() {
            Some(b'[') => self.table_header().map(Some),
            Some(b'\r') => {
                // Stray \r without \n following
                Err(self.set_error(self.cursor, None, ErrorKind::Unexpected('\r')))
            }
            Some(_) => self.key_value(keys).map(Some),
            None => Ok(None),
        }
    }

    fn table_header(&mut self) -> Result<Line<'a>, ParseError> {
        let start = self.cursor as u32;
        if let Err(e) = self.expect_byte(b'[') {
            return Err(e);
        }
        let array = self.eat_byte(b'[');

        let mut header = InlineVec::new();
        self.eat_whitespace();
        match self.read_table_key() {
            Ok(k) => header.push(k),
            Err(e) => return Err(e),
        }
        loop {
            self.eat_whitespace();
            if self.eat_byte(b'.') {
                self.eat_whitespace();
                match self.read_table_key() {
                    Ok(k) => header.push(k),
                    Err(e) => return Err(e),
                }
            } else {
                break;
            }
        }
        self.eat_whitespace();
        if let Err(e) = self.expect_byte(b']') {
            return Err(e);
        }
        if array && let Err(e) = self.expect_byte(b']') {
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

        let end = self.cursor as u32;
        Ok(Line::Table {
            at: start,
            end,
            header,
            array,
        })
    }

    fn key_value(&mut self, keys: &mut Vec<Key<'a>>) -> Result<Line<'a>, ParseError> {
        let start = self.cursor as u32;
        if let Err(e) = self.read_dotted_key(keys) {
            return Err(e);
        }
        self.eat_whitespace();
        if let Err(e) = self.expect_byte(b'=') {
            return Err(e);
        }
        self.eat_whitespace();

        let value = match self.value() {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let end = self.cursor as u32;
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

        // Move keys out -- caller will get them via Line::KeyValue
        let owned_keys = std::mem::take(keys);
        Ok(Line::KeyValue {
            key: owned_keys,
            value,
            at: start,
            end,
        })
    }

    fn add_dotted_key(
        &mut self,
        key_parts: &mut Vec<Key<'a>>,
        value: Val<'a>,
        values: &mut TableValues<'a>,
    ) -> Result<(), ParseError> {
        // key_parts is drained from index 0
        Self::add_dotted_key_inner(
            &mut self.error_span,
            &mut self.error_kind,
            key_parts,
            0,
            value,
            values,
        )
    }

    fn add_dotted_key_inner(
        error_span: &mut Span,
        error_kind: &mut Option<ErrorKind>,
        key_parts: &[Key<'a>],
        idx: usize,
        value: Val<'a>,
        values: &mut TableValues<'a>,
    ) -> Result<(), ParseError> {
        let key = &key_parts[idx];
        if idx + 1 == key_parts.len() {
            values.values.push((key.clone(), value));
            return Ok(());
        }
        // Look for existing dotted table entry
        if let Some(existing) = values.values.iter_mut().find(|(k, _)| k.name == key.name) {
            if let E::DottedTable(ref mut v) = existing.1.e {
                return Self::add_dotted_key_inner(
                    error_span,
                    error_kind,
                    key_parts,
                    idx + 1,
                    value,
                    v,
                );
            } else {
                *error_span = Span::new(key.span.start, value.end);
                *error_kind = Some(ErrorKind::DottedKeyInvalidType {
                    first: existing.0.span,
                });
                return Err(ParseError);
            }
        }
        // Create new dotted table
        let table_values = Val {
            e: E::DottedTable(TableValues::default()),
            start: value.start,
            end: value.end,
        };
        values.values.push((key.clone(), table_values));
        let last_i = values.values.len() - 1;
        if let E::DottedTable(ref mut v) = values.values[last_i].1.e
            && let Err(e) =
                Self::add_dotted_key_inner(error_span, error_kind, key_parts, idx + 1, value, v)
        {
            return Err(e);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Intermediate types
// ---------------------------------------------------------------------------

struct Val<'a> {
    e: E<'a>,
    start: u32,
    end: u32,
}

enum E<'a> {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(DeStr<'a>),
    Array(Vec<Val<'a>>),
    InlineTable(TableValues<'a>),
    DottedTable(TableValues<'a>),
}

struct TableValues<'de> {
    values: Vec<TablePair<'de>>,
    span: Option<Span>,
}

#[allow(clippy::derivable_impls)]
impl Default for TableValues<'_> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            span: None,
        }
    }
}

struct RawTable<'de> {
    at: u32,
    end: u32,
    header: InlineVec<Key<'de>>,
    values: Option<TableValues<'de>>,
    array: bool,
}

enum Line<'a> {
    Table {
        at: u32,
        end: u32,
        header: InlineVec<Key<'a>>,
        array: bool,
    },
    KeyValue {
        at: u32,
        end: u32,
        key: Vec<Key<'a>>,
        value: Val<'a>,
    },
}

// ---------------------------------------------------------------------------
// DeserializeCtx -- table flattening state machine
// ---------------------------------------------------------------------------

struct DeserializeCtx<'de, 'b> {
    raw_tables: Vec<RawTable<'de>>,
    table_indices: &'b HashMap<InlineVec<DeStr<'de>>, Vec<usize>>,
    table_pindices: &'b HashMap<InlineVec<DeStr<'de>>, Vec<usize>>,
    parser: &'b Parser<'de>,
}

struct DeserializeTableIdx {
    table_idx: usize,
    depth: usize,
    idx_range: Range<usize>,
}

impl DeserializeTableIdx {
    fn get_header<'de>(&self, raw_tables: &[RawTable<'de>]) -> InlineVec<DeStr<'de>> {
        if self.depth == 0 {
            return InlineVec::new();
        }
        raw_tables[self.table_idx].header[0..self.depth]
            .iter()
            .map(|key| key.name.clone())
            .collect()
    }
}

impl<'de, 'b> DeserializeCtx<'de, 'b> {
    fn deserialize_entry(
        &mut self,
        table_idx: DeserializeTableIdx,
        additional_values: Vec<TablePair<'de>>,
    ) -> Result<value::ValueInner<'de>, Error> {
        let current_header = table_idx.get_header(&self.raw_tables);
        let matching_tables = self.get_matching_tables(&current_header, &table_idx.idx_range);

        let is_array = matching_tables
            .iter()
            .all(|idx| self.raw_tables[*idx].array)
            && !matching_tables.is_empty();

        if is_array {
            if table_idx.table_idx < matching_tables[0] {
                let array_tbl = &self.raw_tables[matching_tables[0]];
                return Err(self.parser.error(
                    array_tbl.at as usize,
                    Some(array_tbl.end as usize),
                    ErrorKind::RedefineAsArray,
                ));
            }
            assert!(additional_values.is_empty());

            let mut array = value::Array::new();
            for (i, array_entry_idx) in matching_tables.iter().copied().enumerate() {
                let entry_range_end = matching_tables
                    .get(i + 1)
                    .copied()
                    .unwrap_or(table_idx.idx_range.end);

                let span = Self::get_table_span(&self.raw_tables[array_entry_idx]);
                let values = self.raw_tables[array_entry_idx].values.take().unwrap();
                let array_entry = match self.deserialize_as_table(
                    &current_header,
                    array_entry_idx..entry_range_end,
                    values.values,
                ) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
                array.push(Value::with_span(ValueInner::Table(array_entry), span));
            }
            Ok(ValueInner::Array(array))
        } else {
            if matching_tables.len() > 1 {
                let first_tbl = &self.raw_tables[matching_tables[0]];
                let second_tbl = &self.raw_tables[matching_tables[1]];
                return Err(self.parser.error(
                    second_tbl.at as usize,
                    Some(second_tbl.end as usize),
                    ErrorKind::DuplicateTable {
                        name: current_header.last().unwrap().to_string(),
                        first: Span::new(first_tbl.at, first_tbl.end),
                    },
                ));
            }

            let mut values = matching_tables
                .first()
                .map(|idx| self.raw_tables[*idx].values.take().unwrap().values)
                .unwrap_or_default();
            values.extend(additional_values);
            let subtable =
                match self.deserialize_as_table(&current_header, table_idx.idx_range, values) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };

            Ok(ValueInner::Table(subtable))
        }
    }

    fn deserialize_as_table(
        &mut self,
        header: &[DeStr<'de>],
        range: Range<usize>,
        values: Vec<TablePair<'de>>,
    ) -> Result<value::Table<'de>, Error> {
        let mut table = value::Table::new();
        let mut dotted_keys: Vec<(Key<'de>, TableValues<'de>)> = Vec::new();

        for (key, val) in values {
            match val.e {
                E::DottedTable(mut tbl_vals) => {
                    tbl_vals.span = Some(Span::new(val.start, val.end));
                    dotted_keys.push((key, tbl_vals));
                }
                _ => {
                    if let Err(e) = table_insert(&mut table, key, val, self.parser) {
                        return Err(e);
                    }
                }
            }
        }

        let subtables = self.get_subtables(header, &range);
        for &subtable_idx in subtables {
            if self.raw_tables[subtable_idx].values.is_none() {
                continue;
            }

            let subtable_name = &self.raw_tables[subtable_idx].header[header.len()];

            let dotted_entries = if let Some(pos) = dotted_keys
                .iter()
                .position(|(k, _)| k.name == subtable_name.name)
            {
                let (previous_key, dotted_entry) = dotted_keys.swap_remove(pos);
                if self.raw_tables[subtable_idx].header.len() == header.len() + 1 {
                    return Err(self.parser.error(
                        subtable_name.span.start as usize,
                        Some(subtable_name.span.end as usize),
                        ErrorKind::DuplicateKey {
                            key: subtable_name.to_string(),
                            first: previous_key.span,
                        },
                    ));
                }
                dotted_entry.values
            } else {
                Vec::new()
            };

            match table.entry(subtable_name.clone()) {
                Entry::Vacant(vac) => {
                    let subtable_span = Self::get_table_span(&self.raw_tables[subtable_idx]);
                    let subtable_idx = DeserializeTableIdx {
                        table_idx: subtable_idx,
                        depth: header.len() + 1,
                        idx_range: range.clone(),
                    };
                    let entry = match self.deserialize_entry(subtable_idx, dotted_entries) {
                        Ok(v) => v,
                        Err(e) => return Err(e),
                    };
                    vac.insert(Value::with_span(entry, subtable_span));
                }
                Entry::Occupied(occ) => {
                    return Err(self.parser.error(
                        subtable_name.span.start as usize,
                        Some(subtable_name.span.end as usize),
                        ErrorKind::DuplicateKey {
                            key: subtable_name.to_string(),
                            first: occ.key().span,
                        },
                    ));
                }
            };
        }

        for (key, val) in dotted_keys {
            let val_span = val.span.unwrap();
            let val = Val {
                e: E::DottedTable(val),
                start: val_span.start,
                end: val_span.end,
            };
            if let Err(e) = table_insert(&mut table, key, val, self.parser) {
                return Err(e);
            }
        }

        Ok(table)
    }

    fn get_matching_tables(&self, header: &[DeStr<'de>], range: &Range<usize>) -> &'b [usize] {
        let matching_tables = self
            .table_indices
            .get(header)
            .map(Vec::as_slice)
            .unwrap_or_default();
        Self::get_subslice_in_range(matching_tables, range)
    }

    fn get_subtables(&self, header: &[DeStr<'de>], range: &Range<usize>) -> &'b [usize] {
        let subtables = self
            .table_pindices
            .get(header)
            .map(Vec::as_slice)
            .unwrap_or_default();
        Self::get_subslice_in_range(subtables, range)
    }

    fn get_subslice_in_range<'s>(slice: &'s [usize], range: &Range<usize>) -> &'s [usize] {
        let start_idx = slice.partition_point(|idx| *idx < range.start);
        let end_idx = slice.partition_point(|idx| *idx < range.end);
        &slice[start_idx..end_idx]
    }

    fn get_table_span(ttable: &RawTable<'de>) -> Span {
        ttable.values.as_ref().and_then(|v| v.span).map_or_else(
            || Span::new(ttable.at, ttable.end),
            |span| Span::new(ttable.at.min(span.start), ttable.end.max(span.end)),
        )
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn to_value<'de>(val: Val<'de>, parser: &Parser<'de>) -> Result<Value<'de>, Error> {
    let value = match val.e {
        E::String(s) => ValueInner::String(s),
        E::Boolean(b) => ValueInner::Boolean(b),
        E::Integer(i) => ValueInner::Integer(i),
        E::Float(f) => ValueInner::Float(f),
        E::Array(arr) => {
            let mut varr = Vec::new();
            for v in arr {
                match to_value(v, parser) {
                    Ok(v) => varr.push(v),
                    Err(e) => return Err(e),
                }
            }
            ValueInner::Array(varr)
        }
        E::DottedTable(tab) | E::InlineTable(tab) => {
            let mut ntable = value::Table::new();
            for (k, v) in tab.values {
                if let Err(e) = table_insert(&mut ntable, k, v, parser) {
                    return Err(e);
                }
            }
            ValueInner::Table(ntable)
        }
    };
    Ok(Value::with_span(value, Span::new(val.start, val.end)))
}

fn table_insert<'de>(
    table: &mut value::Table<'de>,
    key: Key<'de>,
    val: Val<'de>,
    parser: &Parser<'de>,
) -> Result<(), Error> {
    match table.entry(key.clone()) {
        Entry::Occupied(occ) => Err(parser.error(
            key.span.start as usize,
            Some(key.span.end as usize),
            ErrorKind::DuplicateKey {
                key: key.name.to_string(),
                first: occ.key().span,
            },
        )),
        Entry::Vacant(vac) => match to_value(val, parser) {
            Ok(v) => {
                vac.insert(v);
                Ok(())
            }
            Err(e) => Err(e),
        },
    }
}

fn build_table_indices<'de>(
    tables: &[RawTable<'de>],
) -> HashMap<InlineVec<DeStr<'de>>, Vec<usize>> {
    let mut res = HashMap::new();
    for (i, table) in tables.iter().enumerate() {
        let header = table
            .header
            .iter()
            .map(|v| v.name.clone())
            .collect::<InlineVec<_>>();
        res.entry(header).or_insert_with(Vec::new).push(i);
    }
    res
}

fn build_table_pindices<'de>(
    tables: &[RawTable<'de>],
) -> HashMap<InlineVec<DeStr<'de>>, Vec<usize>> {
    let mut res = HashMap::new();
    for (i, table) in tables.iter().enumerate() {
        let header = table
            .header
            .iter()
            .map(|v| v.name.clone())
            .collect::<InlineVec<_>>();
        for len in 0..header.len() {
            res.entry(header[..len].into())
                .or_insert_with(Vec::new)
                .push(i);
        }
    }
    res
}

// ---------------------------------------------------------------------------
// Top-level parse entry point
// ---------------------------------------------------------------------------

/// Parses a toml string into a [`ValueInner::Table`]
pub fn parse(s: &str) -> Result<Value<'_>, Error> {
    if s.len() > u32::MAX as usize {
        return Err(Error {
            kind: ErrorKind::FileTooLarge,
            span: Span::new(0, 0),
            line_info: None,
        });
    }

    let mut parser = Parser::new(s);
    let raw_tables = match parser.tables() {
        Ok(v) => v,
        Err(_) => return Err(parser.take_error()),
    };
    let mut ctx = DeserializeCtx {
        table_indices: &build_table_indices(&raw_tables),
        table_pindices: &build_table_pindices(&raw_tables),
        raw_tables,
        parser: &parser,
    };
    let root = match ctx.deserialize_entry(
        DeserializeTableIdx {
            table_idx: 0,
            depth: 0,
            idx_range: 0..ctx.raw_tables.len(),
        },
        Vec::new(),
    ) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };

    Ok(Value::with_span(root, Span::new(0, s.len() as u32)))
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
