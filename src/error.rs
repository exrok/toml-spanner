#![allow(clippy::question_mark)]

#[cfg(test)]
#[path = "./error_tests.rs"]
mod tests;

use crate::{Item, Key, Span};
use std::fmt::{self, Debug, Display};

#[derive(Clone, Copy)]
pub enum PathComponent<'de> {
    Key(Key<'de>),
    Index(usize),
}

/// Represents the dotted path to a value within a TOML document.
///
/// A path is a sequence of key and index components, where each entry is
/// either a table key or an array index. Displaying a `TomlPath` produces a
/// human-readable dotted string such as `dependencies.serde` or
/// `servers[0].host`.
///
/// Paths are computed lazily after deserialization and attached to each
/// [`Error`]. Retrieve them with [`Error::path`].
///
/// # Examples
///
/// ```
/// # use toml_spanner::Arena;
/// let arena = Arena::new();
/// let mut doc = toml_spanner::parse("[server]\nport = 'oops'", &arena).unwrap();
///
/// #[derive(Debug, toml_spanner::Toml)]
/// struct Config { server: Server }
/// #[derive(Debug, toml_spanner::Toml)]
/// struct Server { port: u16 }
///
/// let err = doc.to::<Config>().unwrap_err();
/// let path = err.errors[0].path().unwrap();
/// assert_eq!(path.to_string(), "server.port");
/// ```
#[repr(transparent)]
pub struct TomlPath<'a>([PathComponent<'a>]);

impl<'a> std::ops::Deref for TomlPath<'a> {
    type Target = [PathComponent<'a>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for TomlPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        push_toml_path(&mut out, self);
        Debug::fmt(&out, f)
    }
}

impl Display for TomlPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        push_toml_path(&mut out, self);
        f.write_str(&out)
    }
}

fn is_bare_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    for &b in key.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => (),
            _ => return false,
        }
    }
    true
}

pub(crate) struct MaybeTomlPath {
    ptr: std::ptr::NonNull<PathComponent<'static>>,
    len: u32,
    size: u32,
}

impl MaybeTomlPath {
    pub(crate) fn empty() -> Self {
        MaybeTomlPath {
            ptr: std::ptr::NonNull::dangling(),
            len: u32::MAX,
            size: 0,
        }
    }

    pub(crate) fn has_path(&self) -> bool {
        self.size > 0
    }

    pub(crate) fn from_components(components: &[PathComponent<'_>]) -> MaybeTomlPath {
        if components.is_empty() {
            return Self::empty();
        }

        let len = components.len();
        let mut total_string_bytes: usize = 0;
        for comp in components {
            if let PathComponent::Key(key) = comp {
                total_string_bytes += key.name.len();
            }
        }

        let comp_size = len * std::mem::size_of::<PathComponent<'static>>();
        let size = comp_size + total_string_bytes;

        // SAFETY: size > 0 because len >= 1 and size_of::<PathComponent>() > 0.
        let layout = std::alloc::Layout::from_size_align(
            size,
            std::mem::align_of::<PathComponent<'static>>(),
        )
        .unwrap();
        // SAFETY: layout has non-zero size
        let raw = unsafe { std::alloc::alloc(layout) };
        if raw.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        let base = raw.cast::<PathComponent<'static>>();
        let mut string_cursor = unsafe { raw.add(comp_size) };

        for (i, comp) in components.iter().enumerate() {
            let stored = match comp {
                PathComponent::Key(key) => {
                    let name_bytes = key.name.as_bytes();
                    let name_len = name_bytes.len();
                    // SAFETY: string_cursor points into the trailing region we allocated
                    unsafe {
                        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), string_cursor, name_len);
                    }
                    // SAFETY: we just wrote valid UTF-8 bytes here
                    let name: &'static str = unsafe {
                        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            string_cursor,
                            name_len,
                        ))
                    };
                    string_cursor = unsafe { string_cursor.add(name_len) };
                    PathComponent::Key(Key {
                        name,
                        span: key.span,
                    })
                }
                PathComponent::Index(idx) => PathComponent::Index(*idx),
            };
            // SAFETY: we allocated space for `len` PathComponents
            unsafe {
                base.add(i).write(stored);
            }
        }

        MaybeTomlPath {
            // SAFETY: raw was checked non-null above
            ptr: unsafe { std::ptr::NonNull::new_unchecked(base) },
            len: len as u32,
            size: size as u32,
        }
    }
    #[inline(always)]
    pub(crate) fn uncomputed(item_ptr: *const Item<'_>) -> Self {
        MaybeTomlPath {
            // SAFETY: item_ptr is non-null (points to an Item on the stack or in the arena).
            // We store it cast to PathComponent just to reuse the ptr field.
            ptr: unsafe {
                std::ptr::NonNull::new_unchecked(item_ptr as *mut PathComponent<'static>)
            },
            len: 0,
            size: 0,
        }
    }

    pub(crate) fn is_uncomputed(&self) -> bool {
        self.size == 0 && self.len != u32::MAX
    }

    pub(crate) fn uncomputed_ptr(&self) -> *const () {
        self.ptr.as_ptr() as *const ()
    }

    fn as_toml_path<'a>(&'a self) -> Option<&'a TomlPath<'a>> {
        if !self.has_path() {
            return None;
        }
        // SAFETY: components live in the allocation, strings point into
        // the same allocation. The returned TomlPath borrows self, so the
        // inner 'static is shortened to 'a, preventing it from escaping.
        let slice = unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) };
        Some(unsafe { &*(slice as *const [PathComponent<'static>] as *const TomlPath<'a>) })
    }
}

impl Drop for MaybeTomlPath {
    fn drop(&mut self) {
        let size = self.size as usize;
        if size > 0 {
            let layout = std::alloc::Layout::from_size_align(
                size,
                std::mem::align_of::<PathComponent<'static>>(),
            )
            .unwrap();
            // SAFETY: ptr was allocated with this layout
            unsafe {
                std::alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// SAFETY: TomlPath owns its allocation entirely and contains no thread-local state.
unsafe impl Send for MaybeTomlPath {}
// SAFETY: &TomlPath only gives &[PathComponent] access, which is safe to share.
unsafe impl Sync for MaybeTomlPath {}

/// A single error from parsing or converting a TOML document.
///
/// Errors arise from two phases:
///
/// - Parsing: syntax errors such as unterminated strings, duplicate keys,
///   or unexpected characters. [`parse`](crate::parse) returns the first such
///   error as `Err(Error)`.
///
/// - Conversion: type mismatches, missing fields, unknown keys, and
///   other constraint violations detected by [`FromToml`](crate::FromToml).
///   These accumulate so that a single pass surfaces as many problems as
///   possible.
///
/// [`parse_recoverable`](crate::parse_recoverable) combines both phases,
/// continuing past syntax errors and collecting them alongside conversion
/// errors into a single [`Document::errors`](crate::Document::errors) list.
///
/// # Extracting information
///
/// | Method                                        | Returns                                                         |
/// |-----------------------------------------------|-----------------------------------------------------------------|
/// | [`kind()`](Self::kind)                        | The [`ErrorKind`] variant for this error                        |
/// | [`span()`](Self::span)                        | Source [`Span`] (byte offsets), `0..0` when no location applies |
/// | [`path()`](Self::path)                        | Optional [`TomlPath`] to the offending value                    |
/// | [`message(source)`](Self::message)            | Human-readable diagnostic message                               |
/// | [`primary_label()`](Self::primary_label)      | Optional `(Span, String)` label for the error site              |
/// | [`secondary_label()`](Self::secondary_label)  | Optional `(Span, String)` for related locations                 |
///
/// The `message`, `primary_label`, and `secondary_label` methods provide
/// building blocks for rich diagnostics, mapping onto the label model used by
/// [`codespan-reporting`](https://docs.rs/codespan-reporting) and
/// [`annotate-snippets`](https://docs.rs/annotate-snippets).
///
/// # Integration with error reporting libraries
///
/// <details>
/// <summary><code>codespan-reporting</code> example</summary>
///
/// ```ignore
/// use codespan_reporting::diagnostic::{Diagnostic, Label};
///
/// fn error_to_diagnostic(
///     error: &toml_spanner::Error,
///     source: &str,
/// ) -> Diagnostic<()> {
///     let mut labels = Vec::new();
///     if let Some((span, text)) = error.secondary_label() {
///         labels.push(Label::secondary((), span).with_message(text));
///     }
///     if let Some((span, label)) = error.primary_label() {
///         let l = Label::primary((), span);
///         labels.push(if label.is_empty() {
///             l
///         } else {
///             l.with_message(label)
///         });
///     }
///     Diagnostic::error()
///         .with_code(error.kind().kind_name())
///         .with_message(error.message(source))
///         .with_labels(labels)
/// }
/// ```
///
/// </details>
///
/// <details>
/// <summary><code>annotate-snippets</code> example</summary>
///
/// ```ignore
/// use annotate_snippets::{AnnotationKind, Group, Level, Snippet};
///
/// fn error_to_snippet<'s>(
///     error: &toml_spanner::Error,
///     source: &'s str,
///     path: &'s str,
/// ) -> Group<'s> {
///     let message = error.message(source);
///     let mut snippet = Snippet::source(source).path(path).fold(true);
///     if let Some((span, text)) = error.secondary_label() {
///         snippet = snippet.annotation(
///             AnnotationKind::Context.span(span.range()).label(text),
///         );
///     }
///     if let Some((span, label)) = error.primary_label() {
///         let ann = AnnotationKind::Primary.span(span.range());
///         snippet = snippet.annotation(if label.is_empty() {
///             ann
///         } else {
///             ann.label(label)
///         });
///     }
///     Level::ERROR.primary_title(message).element(snippet)
/// }
/// ```
///
/// </details>
///
/// # Multiple error accumulation
///
/// During [`FromToml`](crate::FromToml) conversion, errors are pushed into a
/// shared [`Context`](crate::Context) rather than causing an immediate abort.
/// The sentinel type [`Failed`](crate::Failed) signals that a branch failed
/// without carrying error details itself. When
/// [`Document::to`](crate::Document::to) or [`from_str`](crate::from_str)
/// finishes, all accumulated errors are returned in
/// [`FromTomlError::errors`](crate::FromTomlError).
///
/// [`parse_recoverable`](crate::parse_recoverable) extends this to the
/// parsing phase, collecting syntax errors into the same list so valid
/// portions of the document remain available for inspection.
pub struct Error {
    pub(crate) kind: ErrorInner,
    pub(crate) span: Span,
    pub(crate) path: MaybeTomlPath,
}

pub(crate) enum ErrorInner {
    Static(ErrorKind<'static>),
    Custom(Box<str>),
}
/// The specific kind of error.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub enum ErrorKind<'a> {
    /// A custom error message
    Custom(&'a str),

    /// EOF was reached when looking for a value.
    UnexpectedEof,

    /// The input file is larger than the maximum supported size of 512 MiB.
    FileTooLarge,

    /// An invalid character not allowed in a string was found.
    InvalidCharInString(char),

    /// An invalid character was found as an escape.
    InvalidEscape(char),

    /// An invalid character was found in a hex escape.
    InvalidHexEscape(char),

    /// An invalid escape value was specified in a hex escape in a string.
    ///
    /// Valid values are in the plane of unicode codepoints.
    InvalidEscapeValue(u32),

    /// An unexpected character was encountered, typically when looking for a
    /// value.
    Unexpected(char),

    /// An unterminated string was found where EOF or a newline was reached
    /// before the closing delimiter.
    ///
    /// The `char` is the expected closing delimiter (`"` or `'`).
    UnterminatedString(char),

    /// An integer literal failed to parse, with an optional reason.
    InvalidInteger(&'static str),

    /// A float literal failed to parse, with an optional reason.
    InvalidFloat(&'static str),

    /// A datetime literal failed to parse, with an optional reason.
    InvalidDateTime(&'static str),

    /// The number in the toml file cannot be losslessly converted to the specified
    /// number type
    OutOfRange(&'static str),

    /// Wanted one sort of token, but found another.
    Wanted {
        /// Expected token type.
        expected: &'static &'static str,
        /// Actually found token type.
        found: &'static &'static str,
    },

    /// A duplicate table definition was found.
    DuplicateTable {
        /// The span of the table name (for extracting the name from source)
        name: Span,
        /// The span where the table was first defined
        first: Span,
    },

    /// Duplicate key in table.
    DuplicateKey {
        /// The span where the first key is located
        first: Span,
    },

    /// A previously defined table was redefined as an array.
    RedefineAsArray,

    /// Multiline strings are not allowed for key.
    MultilineStringKey,

    /// Dotted key attempted to extend something that is not a table.
    DottedKeyInvalidType {
        /// The span where the non-table value was defined
        first: Span,
    },

    /// An unexpected key was encountered.
    ///
    /// Used when converting a struct with a limited set of fields.
    UnexpectedKey {
        /// Developer provided association tag useful for programmatic filtering
        /// or adding additional messages or notes to diagnostics. Defaults to 0.
        tag: u32,
    },

    /// Unquoted string was found when quoted one was expected.
    UnquotedString,

    /// A required field is missing from a table
    MissingField(&'static str),

    /// A field was set more than once (e.g. via primary key and alias)
    DuplicateField(&'static str),

    /// A field in the table is deprecated and the new key should be used instead
    Deprecated {
        /// Developer Provider association tag useful for programmatic filtering
        /// or adding additional messages or notes to diagnoistics such as version
        /// info. Defaults to 0.
        tag: u32,
        /// The deprecated key name
        old: &'static &'static str,
        /// The key name that should be used instead
        new: &'static &'static str,
    },

    /// An unexpected value was encountered
    UnexpectedValue {
        /// The list of values that could have been used
        expected: &'static [&'static str],
    },

    /// A string did not match any known variant
    UnexpectedVariant {
        /// The list of variant names that would have been accepted
        expected: &'static [&'static str],
    },

    /// A comma is missing between elements in an array.
    MissingArrayComma,

    /// An array was not closed before EOF.
    UnclosedArray,

    /// A comma is missing between entries in an inline table.
    MissingInlineTableComma,

    /// An inline table was not closed before EOF or a newline.
    UnclosedInlineTable,
}

impl<'a> ErrorKind<'a> {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ErrorKind::Custom(_) => "Custom",
            ErrorKind::UnexpectedEof => "UnexpectedEof",
            ErrorKind::FileTooLarge => "FileTooLarge",
            ErrorKind::InvalidCharInString(_) => "InvalidCharInString",
            ErrorKind::InvalidEscape(_) => "InvalidEscape",
            ErrorKind::InvalidHexEscape(_) => "InvalidHexEscape",
            ErrorKind::InvalidEscapeValue(_) => "InvalidEscapeValue",
            ErrorKind::Unexpected(_) => "Unexpected",
            ErrorKind::UnterminatedString(_) => "UnterminatedString",
            ErrorKind::InvalidInteger(_) => "InvalidInteger",
            ErrorKind::InvalidFloat(_) => "InvalidFloat",
            ErrorKind::InvalidDateTime(_) => "InvalidDateTime",
            ErrorKind::OutOfRange(_) => "OutOfRange",
            ErrorKind::Wanted { .. } => "Wanted",
            ErrorKind::DuplicateTable { .. } => "DuplicateTable",
            ErrorKind::DuplicateKey { .. } => "DuplicateKey",
            ErrorKind::RedefineAsArray => "RedefineAsArray",
            ErrorKind::MultilineStringKey => "MultilineStringKey",
            ErrorKind::DottedKeyInvalidType { .. } => "DottedKeyInvalidType",
            ErrorKind::UnexpectedKey { .. } => "UnexpectedKey",
            ErrorKind::UnquotedString => "UnquotedString",
            ErrorKind::MissingField(_) => "MissingField",
            ErrorKind::DuplicateField(_) => "DuplicateField",
            ErrorKind::Deprecated { .. } => "Deprecated",
            ErrorKind::UnexpectedValue { .. } => "UnexpectedValue",
            ErrorKind::UnexpectedVariant { .. } => "UnexpectedVariant",
            ErrorKind::MissingArrayComma => "MissingArrayComma",
            ErrorKind::UnclosedArray => "UnclosedArray",
            ErrorKind::MissingInlineTableComma => "MissingInlineTableComma",
            ErrorKind::UnclosedInlineTable => "UnclosedInlineTable",
        }
    }
}

impl Error {
    /// Returns the source span where this error occurred.
    ///
    /// A span of `0..0` ([`Span::is_empty`]) means the error has no specific
    /// source location, as with [`ErrorKind::FileTooLarge`].
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the error kind.
    pub fn kind(&self) -> ErrorKind<'_> {
        match &self.kind {
            ErrorInner::Static(kind) => *kind,
            ErrorInner::Custom(error) => ErrorKind::Custom(error),
        }
    }

    /// Returns the TOML path where this error occurred, if available.
    pub fn path<'a>(&'a self) -> Option<&'a TomlPath<'a>> {
        self.path.as_toml_path()
    }

    /// Creates an error with a custom message at the given source span.
    pub fn custom(message: impl ToString, span: Span) -> Error {
        Error {
            kind: ErrorInner::Custom(message.to_string().into()),
            span,
            path: MaybeTomlPath::empty(),
        }
    }

    /// Creates an error with a static message at the given source span.
    pub(crate) fn custom_static(message: &'static str, span: Span) -> Error {
        Error {
            kind: ErrorInner::Static(ErrorKind::Custom(message)),
            span,
            path: MaybeTomlPath::empty(),
        }
    }

    /// Creates an error from a known error kind and span.
    pub(crate) fn new(kind: ErrorKind<'static>, span: Span) -> Error {
        Error {
            kind: ErrorInner::Static(kind),
            span,
            path: MaybeTomlPath::empty(),
        }
    }

    /// Creates an error from a known error kind, span, and TOML path.
    pub(crate) fn new_with_path(kind: ErrorKind<'static>, span: Span, path: MaybeTomlPath) -> Self {
        Error {
            kind: ErrorInner::Static(kind),
            span,
            path,
        }
    }
}

impl<'a> Debug for ErrorKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

fn kind_message(kind: ErrorKind<'_>) -> String {
    let mut out = String::new();
    kind_message_inner(kind, &mut out);
    out
}

#[inline(never)]
fn s_push(out: &mut String, s: &str) {
    out.push_str(s);
}

#[inline(never)]
fn s_push_char(out: &mut String, c: char) {
    out.push(c);
}

fn push_escape(out: &mut String, c: char) {
    if c.is_control() {
        for esc in c.escape_default() {
            s_push_char(out, esc);
        }
    } else {
        s_push_char(out, c);
    }
}

fn push_u32(out: &mut String, mut n: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    if n == 0 {
        s_push_char(out, '0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    // SAFETY: digits are always valid ASCII/UTF-8
    s_push(out, unsafe { std::str::from_utf8_unchecked(&buf[i..]) });
}

fn kind_message_inner(kind: ErrorKind<'_>, out: &mut String) {
    match kind {
        ErrorKind::Custom(message) => s_push(out, message),
        ErrorKind::UnexpectedEof => s_push(out, "unexpected eof encountered"),
        ErrorKind::FileTooLarge => s_push(out, "file is too large (maximum 512 MiB)"),
        ErrorKind::InvalidCharInString(c) => {
            s_push(out, "invalid character in string: `");
            push_escape(out, c);
            s_push_char(out, '`');
        }
        ErrorKind::InvalidEscape(c) => {
            s_push(out, "invalid escape character in string: `");
            push_escape(out, c);
            s_push_char(out, '`');
        }
        ErrorKind::InvalidHexEscape(c) => {
            s_push(out, "invalid hex escape character in string: `");
            push_escape(out, c);
            s_push_char(out, '`');
        }
        ErrorKind::InvalidEscapeValue(c) => {
            s_push(out, "invalid escape value: `");
            push_unicode_escape(out, c);
            s_push_char(out, '`');
        }
        ErrorKind::Unexpected(c) => {
            s_push(out, "unexpected character found: `");
            push_escape(out, c);
            s_push_char(out, '`');
        }
        ErrorKind::UnterminatedString(delim) => {
            if delim == '\'' {
                s_push(out, "invalid literal string, expected `'`");
            } else {
                s_push(out, "invalid basic string, expected `\"`");
            }
        }
        ErrorKind::Wanted { expected, found } => {
            s_push(out, "expected ");
            s_push(out, expected);
            s_push(out, ", found ");
            s_push(out, found);
        }
        ErrorKind::InvalidInteger(reason)
        | ErrorKind::InvalidFloat(reason)
        | ErrorKind::InvalidDateTime(reason) => {
            let prefix = match kind {
                ErrorKind::InvalidInteger(_) => "invalid integer",
                ErrorKind::InvalidFloat(_) => "invalid float",
                _ => "invalid datetime",
            };
            s_push(out, prefix);
            if !reason.is_empty() {
                s_push(out, ": ");
                s_push(out, reason);
            }
        }
        ErrorKind::OutOfRange(ty) => {
            s_push(out, "out of range of '");
            s_push(out, ty);
            s_push_char(out, '\'');
        }
        ErrorKind::DuplicateTable { .. } => s_push(out, "redefinition of table"),
        ErrorKind::DuplicateKey { .. } => s_push(out, "duplicate key"),
        ErrorKind::RedefineAsArray => s_push(out, "table redefined as array"),
        ErrorKind::MultilineStringKey => {
            s_push(out, "multiline strings are not allowed for key");
        }
        ErrorKind::DottedKeyInvalidType { .. } => {
            s_push(out, "dotted key attempted to extend non-table type");
        }
        ErrorKind::UnexpectedKey { .. } => s_push(out, "unexpected key"),
        ErrorKind::UnquotedString => {
            s_push(
                out,
                "invalid TOML value, did you mean to use a quoted string?",
            );
        }
        ErrorKind::MissingField(field) => {
            s_push(out, "missing field '");
            s_push(out, field);
            s_push(out, "' in table");
        }
        ErrorKind::DuplicateField(field) => {
            s_push(out, "duplicate field '");
            s_push(out, field);
            s_push_char(out, '\'');
        }
        ErrorKind::Deprecated { old, new, .. } => {
            s_push(out, "field '");
            s_push(out, old);
            s_push(out, "' is deprecated, '");
            s_push(out, new);
            s_push(out, "' has replaced it");
        }
        ErrorKind::UnexpectedValue { expected } => {
            s_push(out, "expected '[");
            let mut first = true;
            for val in expected {
                if !first {
                    s_push(out, ", ");
                }
                first = false;
                s_push(out, val);
            }
            s_push(out, "]'");
        }
        ErrorKind::UnexpectedVariant { expected } => {
            s_push(out, "unknown variant, expected one of: ");
            let mut first = true;
            for val in expected {
                if !first {
                    s_push(out, ", ");
                }
                first = false;
                s_push(out, val);
            }
        }
        ErrorKind::MissingArrayComma => {
            s_push(out, "missing comma between array elements, expected `,`");
        }
        ErrorKind::UnclosedArray => s_push(out, "unclosed array, expected `]`"),
        ErrorKind::MissingInlineTableComma => {
            s_push(out, "missing comma in inline table, expected `,`");
        }
        ErrorKind::UnclosedInlineTable => s_push(out, "unclosed inline table, expected `}`"),
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&kind_message(self.kind()))?;
        if let Some(path) = self.path() {
            f.write_str(" at `")?;
            Display::fmt(path, f)?;
            f.write_str("`")?;
        }
        Ok(())
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind();
        f.debug_struct("Error")
            .field("kind", &kind.kind_name())
            .field("message", &kind_message(kind))
            .field("span", &self.span().range())
            .field("path", &self.path())
            .finish()
    }
}

impl std::error::Error for Error {}

impl From<(ErrorKind<'static>, Span)> for Error {
    fn from((kind, span): (ErrorKind<'static>, Span)) -> Self {
        Self {
            kind: ErrorInner::Static(kind),
            span,
            path: MaybeTomlPath::empty(),
        }
    }
}

fn push_toml_path(out: &mut String, path: &TomlPath<'_>) {
    let mut first = true;
    for component in path.iter() {
        match component {
            PathComponent::Key(key) => {
                if !first {
                    s_push_char(out, '.');
                }
                first = false;
                if is_bare_key(key.name) {
                    s_push(out, key.name);
                } else {
                    s_push_char(out, '"');
                    for ch in key.name.chars() {
                        match ch {
                            '"' => s_push(out, "\\\""),
                            '\\' => s_push(out, "\\\\"),
                            '\n' => s_push(out, "\\n"),
                            '\r' => s_push(out, "\\r"),
                            '\t' => s_push(out, "\\t"),
                            c if c.is_control() => {
                                push_unicode_escape(out, c as u32);
                            }
                            c => s_push_char(out, c),
                        }
                    }
                    s_push_char(out, '"');
                }
            }
            PathComponent::Index(idx) => {
                s_push_char(out, '[');
                push_u32(out, *idx as u32);
                s_push_char(out, ']');
            }
        }
    }
}

fn push_unicode_escape(out: &mut String, n: u32) {
    s_push(out, "\\u");
    let mut buf = [0u8; 8];
    let digits = if n <= 0xFFFF { 4 } else { 6 };
    for i in (0..digits).rev() {
        let nibble = ((n >> (i * 4)) & 0xF) as u8;
        buf[digits - 1 - i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
    }
    // SAFETY: hex digits are valid ASCII
    s_push(out, unsafe {
        std::str::from_utf8_unchecked(&buf[..digits])
    });
}

impl Error {
    /// Returns the diagnostic message for this error.
    ///
    /// Some error kinds extract names from `source` for richer messages.
    pub fn message(&self, source: &str) -> String {
        let mut out = String::new();
        self.message_inner(source, &mut out);
        out
    }

    fn message_inner(&self, source: &str, out: &mut String) {
        let span = self.span;
        let kind = self.kind();
        let path = self.path();

        match kind {
            ErrorKind::DuplicateKey { .. } => {
                if let Some(name) = source.get(span.range()) {
                    s_push(out, "the key `");
                    s_push(out, name);
                    s_push(out, "` is defined multiple times");
                } else {
                    kind_message_inner(kind, out);
                }
            }
            ErrorKind::DuplicateTable { name, .. } => {
                if let Some(table_name) = source.get(name.range()) {
                    s_push(out, "redefinition of table `");
                    s_push(out, table_name);
                    s_push_char(out, '`');
                } else {
                    kind_message_inner(kind, out);
                }
            }
            ErrorKind::UnexpectedKey { .. } if path.is_none() => {
                if let Some(key_name) = source.get(span.range()) {
                    s_push(out, "unexpected key `");
                    s_push(out, key_name);
                    s_push_char(out, '`');
                } else {
                    kind_message_inner(kind, out);
                }
            }
            ErrorKind::UnexpectedVariant { .. } => {
                if let Some(value) = source.get(span.range()) {
                    s_push(out, "unknown variant ");
                    match value.split_once('\n') {
                        Some((first, _)) => {
                            s_push(out, first);
                            s_push(out, "...");
                        }
                        None => s_push(out, value),
                    }
                } else {
                    kind_message_inner(kind, out);
                }
            }
            _ => kind_message_inner(kind, out),
        }

        if let Some(p) = path {
            s_push(out, " at `");
            push_toml_path(out, p);
            s_push_char(out, '`');
        }
    }

    /// Returns the primary label span and text for this error, if any.
    pub fn primary_label(&self) -> Option<(Span, String)> {
        let mut out = String::new();
        self.primary_label_inner(&mut out);
        Some((self.span, out))
    }

    fn primary_label_inner(&self, out: &mut String) {
        let kind = self.kind();
        match kind {
            ErrorKind::DuplicateKey { .. } => s_push(out, "duplicate key"),
            ErrorKind::DuplicateTable { .. } => s_push(out, "duplicate table"),
            ErrorKind::DottedKeyInvalidType { .. } => {
                s_push(out, "attempted to extend table here");
            }
            ErrorKind::Unexpected(c) => {
                s_push(out, "unexpected character '");
                push_escape(out, c);
                s_push_char(out, '\'');
            }
            ErrorKind::InvalidCharInString(c) => {
                s_push(out, "invalid character '");
                push_escape(out, c);
                s_push(out, "' in string");
            }
            ErrorKind::InvalidEscape(c) => {
                s_push(out, "invalid escape character '");
                push_escape(out, c);
                s_push(out, "' in string");
            }
            ErrorKind::InvalidEscapeValue(_) => s_push(out, "invalid escape value"),
            ErrorKind::InvalidInteger(_)
            | ErrorKind::InvalidFloat(_)
            | ErrorKind::InvalidDateTime(_) => kind_message_inner(kind, out),
            ErrorKind::InvalidHexEscape(c) => {
                s_push(out, "invalid hex escape '");
                push_escape(out, c);
                s_push_char(out, '\'');
            }
            ErrorKind::Wanted { expected, .. } => {
                s_push(out, "expected ");
                s_push(out, expected);
            }
            ErrorKind::MultilineStringKey => s_push(out, "multiline keys are not allowed"),
            ErrorKind::UnterminatedString(delim) => {
                s_push(out, "expected `");
                s_push_char(out, delim);
                s_push_char(out, '`');
            }
            ErrorKind::UnquotedString => s_push(out, "string is not quoted"),
            ErrorKind::UnexpectedKey { .. } => s_push(out, "unexpected key"),
            ErrorKind::MissingField(_) => s_push(out, "table with missing field"),
            ErrorKind::DuplicateField(_) => s_push(out, "duplicate field"),
            ErrorKind::Deprecated { .. } => s_push(out, "deprecated field"),
            ErrorKind::UnexpectedValue { .. } => s_push(out, "unexpected value"),
            ErrorKind::UnexpectedVariant { expected } => {
                s_push(out, "expected one of: ");
                let mut first = true;
                for val in expected {
                    if !first {
                        s_push(out, ", ");
                    }
                    first = false;
                    s_push(out, val);
                }
            }
            ErrorKind::MissingArrayComma => s_push(out, "expected `,`"),
            ErrorKind::UnclosedArray => s_push(out, "expected `]`"),
            ErrorKind::MissingInlineTableComma => s_push(out, "expected `,`"),
            ErrorKind::UnclosedInlineTable => s_push(out, "expected `}`"),
            ErrorKind::OutOfRange(_)
            | ErrorKind::UnexpectedEof
            | ErrorKind::RedefineAsArray
            | ErrorKind::FileTooLarge
            | ErrorKind::Custom(..) => {}
        }
    }

    /// Returns the secondary label span and text, if any.
    ///
    /// Some errors reference a related location (for example, the first
    /// definition of a duplicate key).
    pub fn secondary_label(&self) -> Option<(Span, String)> {
        let (first, text) = match self.kind() {
            ErrorKind::DuplicateKey { first } => (first, "first key instance"),
            ErrorKind::DuplicateTable { first, .. } => (first, "first table instance"),
            ErrorKind::DottedKeyInvalidType { first } => (first, "non-table"),
            _ => return None,
        };
        Some((first, String::from(text)))
    }
}
