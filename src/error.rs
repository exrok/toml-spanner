#![allow(clippy::question_mark)]

#[cfg(test)]
#[path = "./error_tests.rs"]
mod tests;

use crate::Span;
use std::fmt::{self, Debug, Display};

pub struct Error {
    kind: ErrorInner,
    span: Span,
}

enum ErrorInner {
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
    UnexpectedKey,

    /// Unquoted string was found when quoted one was expected.
    UnquotedString,

    /// A required field is missing from a table
    MissingField(&'static str),

    /// A field was set more than once (e.g. via primary key and alias)
    DuplicateField(&'static str),

    /// A field in the table is deprecated and the new key should be used instead
    Deprecated {
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

struct Escape(char);

impl fmt::Display for Escape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write as _;

        if self.0.is_control() {
            for esc in self.0.escape_default() {
                f.write_char(esc)?;
            }
            Ok(())
        } else {
            f.write_char(self.0)
        }
    }
}

macro_rules! rtry {
    ($($tt:tt)*) => {
        if let Err(err) = $($tt)* {
            return Err(err);
        }
    };
}

impl Error {
    /// Returns the source span where this error occurred.
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

    /// Creates an error with a custom message at the given source span.
    pub fn custom(message: impl ToString, span: Span) -> Self {
        Self {
            kind: ErrorInner::Custom(message.to_string().into()),
            span,
        }
    }

    /// Creates an error with a static message at the given source span.
    pub(crate) fn custom_static(message: &'static str, span: Span) -> Self {
        Self {
            kind: ErrorInner::Static(ErrorKind::Custom(message)),
            span,
        }
    }

    /// Creates an error from a known error kind and span.
    pub(crate) fn new(kind: ErrorKind<'static>, span: Span) -> Self {
        Self {
            kind: ErrorInner::Static(kind),
            span,
        }
    }
}

impl<'a> Display for ErrorKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::UnexpectedEof => "unexpected-eof",
            Self::FileTooLarge => "file-too-large",
            Self::DottedKeyInvalidType { .. } => "dotted-key-invalid-type",
            Self::DuplicateKey { .. } => "duplicate-key",
            Self::DuplicateTable { .. } => "duplicate-table",
            Self::UnexpectedKey => "unexpected-key",
            Self::UnquotedString => "unquoted-string",
            Self::MultilineStringKey => "multiline-string-key",
            Self::RedefineAsArray => "redefine-as-array",
            Self::InvalidCharInString(..) => "invalid-char-in-string",
            Self::InvalidEscape(..) => "invalid-escape",
            Self::InvalidEscapeValue(..) => "invalid-escape-value",
            Self::InvalidHexEscape(..) => "invalid-hex-escape",
            Self::Unexpected(..) => "unexpected",
            Self::UnterminatedString(..) => "unterminated-string",
            Self::InvalidInteger(_) => "invalid-integer",
            Self::InvalidFloat(_) => "invalid-float",
            Self::InvalidDateTime(_) => "invalid-datetime",
            Self::OutOfRange(_) => "out-of-range",
            Self::Wanted { .. } => "wanted",
            Self::MissingField(..) => "missing-field",
            Self::DuplicateField(..) => "duplicate-field",
            Self::Deprecated { .. } => "deprecated",
            Self::UnexpectedValue { .. } => "unexpected-value",
            Self::UnexpectedVariant { .. } => "unexpected-variant",
            Self::MissingArrayComma => "missing-array-comma",
            Self::UnclosedArray => "unclosed-array",
            Self::MissingInlineTableComma => "missing-inline-table-comma",
            Self::UnclosedInlineTable => "unclosed-inline-table",
            Self::Custom(..) => "custom",
        };
        f.write_str(text)
    }
}

impl<'a> Debug for ErrorKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind();
        match kind {
            ErrorKind::Custom(message) => f.write_str(message),
            ErrorKind::UnexpectedEof => f.write_str("unexpected eof encountered"),
            ErrorKind::FileTooLarge => f.write_str("file is too large (maximum 512 MiB)"),
            ErrorKind::InvalidCharInString(c) => {
                rtry!(f.write_str("invalid character in string: `"));
                rtry!(Escape(c).fmt(f));
                f.write_str("`")
            }
            ErrorKind::InvalidEscape(c) => {
                rtry!(f.write_str("invalid escape character in string: `"));
                rtry!(Escape(c).fmt(f));
                f.write_str("`")
            }
            ErrorKind::InvalidHexEscape(c) => {
                rtry!(f.write_str("invalid hex escape character in string: `"));
                rtry!(Escape(c).fmt(f));
                f.write_str("`")
            }
            ErrorKind::InvalidEscapeValue(c) => {
                rtry!(f.write_str("invalid escape value: `"));
                rtry!(std::fmt::Display::fmt(&c, f));
                f.write_str("`")
            }
            ErrorKind::Unexpected(c) => {
                rtry!(f.write_str("unexpected character found: `"));
                rtry!(Escape(c).fmt(f));
                f.write_str("`")
            }
            ErrorKind::UnterminatedString(delim) => {
                if delim == '\'' {
                    f.write_str("invalid literal string, expected `'`")
                } else {
                    f.write_str("invalid basic string, expected `\"`")
                }
            }
            ErrorKind::Wanted { expected, found } => {
                rtry!(f.write_str("expected "));
                rtry!(f.write_str(expected));
                rtry!(f.write_str(", found "));
                f.write_str(found)
            }
            ErrorKind::InvalidInteger(reason)
            | ErrorKind::InvalidFloat(reason)
            | ErrorKind::InvalidDateTime(reason) => {
                rtry!(f.write_str(match kind {
                    ErrorKind::InvalidInteger(_) => "invalid integer",
                    ErrorKind::InvalidFloat(_) => "invalid float",
                    _ => "invalid datetime",
                }));
                if !reason.is_empty() {
                    rtry!(f.write_str(": "));
                    f.write_str(reason)
                } else {
                    Ok(())
                }
            }
            ErrorKind::OutOfRange(ty) => {
                rtry!(f.write_str("out of range of '"));
                rtry!(f.write_str(ty));
                f.write_str("'")
            }
            ErrorKind::DuplicateTable { .. } => f.write_str("redefinition of table"),
            ErrorKind::DuplicateKey { .. } => f.write_str("duplicate key"),
            ErrorKind::RedefineAsArray => f.write_str("table redefined as array"),
            ErrorKind::MultilineStringKey => {
                f.write_str("multiline strings are not allowed for key")
            }
            ErrorKind::DottedKeyInvalidType { .. } => {
                f.write_str("dotted key attempted to extend non-table type")
            }
            ErrorKind::UnexpectedKey => f.write_str("unexpected key"),
            ErrorKind::UnquotedString => {
                f.write_str("invalid TOML value, did you mean to use a quoted string?")
            }
            ErrorKind::MissingField(field) => {
                rtry!(f.write_str("missing field '"));
                rtry!(f.write_str(field));
                f.write_str("' in table")
            }
            ErrorKind::DuplicateField(field) => {
                rtry!(f.write_str("duplicate field '"));
                rtry!(f.write_str(field));
                f.write_str("'")
            }
            ErrorKind::Deprecated { old, new } => {
                rtry!(f.write_str("field '"));
                rtry!(f.write_str(old));
                rtry!(f.write_str("' is deprecated, '"));
                rtry!(f.write_str(new));
                f.write_str("' has replaced it")
            }
            ErrorKind::UnexpectedValue { expected } => {
                rtry!(f.write_str("expected '["));
                let mut first = true;
                for val in expected {
                    if !first {
                        rtry!(f.write_str(", "));
                    }
                    first = false;
                    rtry!(f.write_str(val));
                }
                f.write_str("]'")
            }
            ErrorKind::UnexpectedVariant { expected } => {
                rtry!(f.write_str("unknown variant, expected one of: "));
                let mut first = true;
                for val in expected {
                    if !first {
                        rtry!(f.write_str(", "));
                    }
                    first = false;
                    rtry!(f.write_str(val));
                }
                Ok(())
            }
            ErrorKind::MissingArrayComma => {
                f.write_str("missing comma between array elements, expected `,`")
            }
            ErrorKind::UnclosedArray => f.write_str("unclosed array, expected `]`"),
            ErrorKind::MissingInlineTableComma => {
                f.write_str("missing comma in inline table, expected `,`")
            }
            ErrorKind::UnclosedInlineTable => f.write_str("unclosed inline table, expected `}`"),
        }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl std::error::Error for Error {}

impl From<(ErrorKind<'static>, Span)> for Error {
    fn from((kind, span): (ErrorKind<'static>, Span)) -> Self {
        Self {
            kind: ErrorInner::Static(kind),
            span,
        }
    }
}

#[cfg(feature = "annotate-snippets")]
impl Error {
    /// Converts this error into an [`annotate_snippets::Group`] for rendering
    /// with [`annotate_snippets`].
    pub fn to_snippet<'s>(&self, source: &'s str, path: &'s str) -> annotate_snippets::Group<'s> {
        use annotate_snippets::{AnnotationKind, Level, Snippet};

        let span = self.span().range();
        let kind = self.kind();

        let title = match kind {
            ErrorKind::DuplicateKey { .. } => {
                if let Some(name) = source.get(span.clone()) {
                    format!("the key `{name}` is defined multiple times")
                } else {
                    self.to_string()
                }
            }
            ErrorKind::DuplicateTable { name, .. } => {
                if let Some(table_name) = source.get(name.range()) {
                    format!("redefinition of table `{table_name}`")
                } else {
                    self.to_string()
                }
            }
            ErrorKind::UnexpectedKey => {
                if let Some(key_name) = source.get(span.clone()) {
                    format!("unexpected key `{key_name}`")
                } else {
                    self.to_string()
                }
            }
            ErrorKind::UnexpectedVariant { .. } => {
                if let Some(value) = source.get(span.clone()) {
                    match value.split_once('\n') {
                        Some((first, _)) => format!("unknown variant {first}..."),
                        None => format!("unknown variant {value}"),
                    }
                } else {
                    self.to_string()
                }
            }
            _ => self.to_string(),
        };

        let mut snippet = Snippet::source(source).path(path).fold(true);

        match kind {
            ErrorKind::DuplicateKey { first } => {
                snippet = snippet
                    .annotation(
                        AnnotationKind::Context
                            .span(first.range())
                            .label("first key instance"),
                    )
                    .annotation(AnnotationKind::Primary.span(span).label("duplicate key"));
            }
            ErrorKind::Unexpected(c) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label(format!("unexpected character '{}'", Escape(c))),
                );
            }
            ErrorKind::InvalidCharInString(c) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label(format!("invalid character '{}' in string", Escape(c))),
                );
            }
            ErrorKind::InvalidEscape(c) => {
                snippet = snippet.annotation(AnnotationKind::Primary.span(span).label(format!(
                    "invalid escape character '{}' in string",
                    Escape(c)
                )));
            }
            ErrorKind::InvalidEscapeValue(_) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label("invalid escape value"),
                );
            }
            ErrorKind::InvalidInteger(_)
            | ErrorKind::InvalidFloat(_)
            | ErrorKind::InvalidDateTime(_) => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label(self.to_string()));
            }
            ErrorKind::OutOfRange(_) => {
                snippet = snippet.annotation(AnnotationKind::Primary.span(span));
            }
            ErrorKind::Wanted { expected, .. } => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label(format!("expected {expected}")),
                );
            }
            ErrorKind::MultilineStringKey => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label("multiline keys are not allowed"),
                );
            }
            ErrorKind::UnterminatedString(delim) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label(format!("expected `{delim}`")),
                );
            }
            ErrorKind::DuplicateTable { first, .. } => {
                snippet = snippet
                    .annotation(
                        AnnotationKind::Context
                            .span(first.range())
                            .label("first table instance"),
                    )
                    .annotation(AnnotationKind::Primary.span(span).label("duplicate table"));
            }
            ErrorKind::InvalidHexEscape(c) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label(format!("invalid hex escape '{}'", Escape(c))),
                );
            }
            ErrorKind::UnquotedString => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label("string is not quoted"),
                );
            }
            ErrorKind::UnexpectedKey => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("unexpected key"));
            }
            ErrorKind::MissingField(_) => {
                snippet = snippet.annotation(
                    AnnotationKind::Primary
                        .span(span)
                        .label("table with missing field"),
                );
            }
            ErrorKind::DuplicateField(_) => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("duplicate field"));
            }
            ErrorKind::Deprecated { .. } => {
                snippet = snippet
                    .annotation(AnnotationKind::Primary.span(span).label("deprecated field"));
            }
            ErrorKind::UnexpectedValue { .. } => {
                snippet = snippet
                    .annotation(AnnotationKind::Primary.span(span).label("unexpected value"));
            }
            ErrorKind::UnexpectedVariant { expected } => {
                let mut label = String::from("expected one of: ");
                for (i, val) in expected.iter().enumerate() {
                    if i > 0 {
                        label.push_str(", ");
                    }
                    label.push_str(val);
                }
                snippet = snippet
                    .annotation(AnnotationKind::Primary.span(span).label(label));
            }
            ErrorKind::UnexpectedEof => {
                snippet = snippet.annotation(AnnotationKind::Primary.span(span));
            }
            ErrorKind::DottedKeyInvalidType { first } => {
                snippet = snippet
                    .annotation(
                        AnnotationKind::Primary
                            .span(span)
                            .label("attempted to extend table here"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(first.range())
                            .label("non-table"),
                    );
            }
            ErrorKind::RedefineAsArray | ErrorKind::FileTooLarge | ErrorKind::Custom(..) => {
                snippet = snippet.annotation(AnnotationKind::Primary.span(span));
            }
            ErrorKind::MissingArrayComma => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("expected `,`"));
            }
            ErrorKind::UnclosedArray => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("expected `]`"));
            }
            ErrorKind::MissingInlineTableComma => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("expected `,`"));
            }
            ErrorKind::UnclosedInlineTable => {
                snippet =
                    snippet.annotation(AnnotationKind::Primary.span(span).label("expected `}`"));
            }
        }

        Level::ERROR.primary_title(title).element(snippet)
    }
}

#[cfg(feature = "codespan-reporting")]
impl Error {
    /// Converts this error into a [`codespan_reporting`](https://docs.rs/codespan-reporting) diagnostic.
    pub fn to_diagnostic<FileId: Copy + PartialEq>(
        &self,
        source: &str,
        fid: FileId,
    ) -> codespan_reporting::diagnostic::Diagnostic<FileId> {
        use codespan_reporting::diagnostic::Label;

        let diag = codespan_reporting::diagnostic::Diagnostic::error();
        let kind = self.kind();
        let error_span = self.span;
        let diag = diag.with_code(kind.to_string());

        match kind {
            ErrorKind::DuplicateKey { first } => {
                let msg = match source.get(self.span().range()) {
                    Some(name) => format!("the key `{name}` is defined multiple times"),
                    None => "duplicate key".into(),
                };
                diag.with_message(msg).with_labels(vec![
                    Label::secondary(fid, first).with_message("first key instance"),
                    Label::primary(fid, error_span).with_message("duplicate key"),
                ])
            }
            ErrorKind::Unexpected(c) => diag.with_labels(vec![
                Label::primary(fid, error_span)
                    .with_message(format!("unexpected character '{}'", Escape(c))),
            ]),
            ErrorKind::InvalidCharInString(c) => diag.with_labels(vec![
                Label::primary(fid, error_span)
                    .with_message(format!("invalid character '{}' in string", Escape(c))),
            ]),
            ErrorKind::InvalidEscape(c) => {
                diag.with_labels(vec![Label::primary(fid, error_span).with_message(format!(
                    "invalid escape character '{}' in string",
                    Escape(c)
                ))])
            }
            ErrorKind::InvalidEscapeValue(_) => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message("invalid escape value"),
            ]),
            ErrorKind::InvalidInteger(_)
            | ErrorKind::InvalidFloat(_)
            | ErrorKind::InvalidDateTime(_) => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message(self.to_string()),
            ]),
            ErrorKind::OutOfRange(ty) => diag
                .with_message(format!("number is out of range of '{ty}'"))
                .with_labels(vec![Label::primary(fid, error_span)]),
            ErrorKind::Wanted { expected, .. } => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message(format!("expected {expected}")),
            ]),
            ErrorKind::MultilineStringKey => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message("multiline keys are not allowed"),
            ]),
            ErrorKind::UnterminatedString(delim) => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message(format!("expected `{delim}`")),
            ]),
            ErrorKind::DuplicateTable { name, first } => {
                let msg = match source.get(name.range()) {
                    Some(table_name) => format!("redefinition of table `{table_name}`"),
                    None => "redefinition of table".into(),
                };
                diag.with_message(msg).with_labels(vec![
                    Label::secondary(fid, first).with_message("first table instance"),
                    Label::primary(fid, error_span).with_message("duplicate table"),
                ])
            }
            ErrorKind::InvalidHexEscape(c) => diag.with_labels(vec![
                Label::primary(fid, error_span)
                    .with_message(format!("invalid hex escape '{}'", Escape(c))),
            ]),
            ErrorKind::UnquotedString => diag.with_labels(vec![
                Label::primary(fid, error_span).with_message("string is not quoted"),
            ]),
            ErrorKind::UnexpectedKey => {
                let msg = match source.get(self.span().range()) {
                    Some(key_name) => format!("unexpected key `{key_name}`"),
                    None => "unexpected key".into(),
                };
                diag.with_message(msg).with_labels(vec![
                    Label::primary(fid, error_span).with_message("unexpected key"),
                ])
            }
            ErrorKind::MissingField(field) => diag
                .with_message(format!("missing field '{field}'"))
                .with_labels(vec![
                    Label::primary(fid, error_span).with_message("table with missing field"),
                ]),
            ErrorKind::DuplicateField(field) => diag
                .with_message(format!("duplicate field '{field}'"))
                .with_labels(vec![
                    Label::primary(fid, error_span).with_message("duplicate field"),
                ]),
            ErrorKind::Deprecated { new, .. } => diag
                .with_message(format!(
                    "deprecated field encountered, '{new}' should be used instead"
                ))
                .with_labels(vec![
                    Label::primary(fid, error_span).with_message("deprecated field"),
                ]),
            ErrorKind::UnexpectedValue { expected } => diag
                .with_message(format!("expected '{expected:?}'"))
                .with_labels(vec![
                    Label::primary(fid, error_span).with_message("unexpected value"),
                ]),
            ErrorKind::UnexpectedVariant { expected } => {
                let mut label = String::from("expected one of: ");
                for (i, val) in expected.iter().enumerate() {
                    if i > 0 {
                        label.push_str(", ");
                    }
                    label.push_str(val);
                }
                diag
                    .with_message(self.to_string())
                    .with_labels(vec![
                        Label::primary(fid, error_span).with_message(label),
                    ])
            }
            ErrorKind::UnexpectedEof => diag
                .with_message("unexpected end of file")
                .with_labels(vec![Label::primary(fid, error_span)]),
            ErrorKind::DottedKeyInvalidType { first } => {
                diag.with_message(self.to_string()).with_labels(vec![
                    Label::primary(fid, error_span).with_message("attempted to extend table here"),
                    Label::secondary(fid, first).with_message("non-table"),
                ])
            }
            ErrorKind::RedefineAsArray => diag
                .with_message(self.to_string())
                .with_labels(vec![Label::primary(fid, error_span)]),
            ErrorKind::FileTooLarge => diag
                .with_message("file is too large (maximum 512 MiB)")
                .with_labels(vec![Label::primary(fid, error_span)]),
            ErrorKind::MissingArrayComma => diag.with_message(self.to_string()).with_labels(vec![
                Label::primary(fid, error_span).with_message("expected `,`"),
            ]),
            ErrorKind::UnclosedArray => diag.with_message(self.to_string()).with_labels(vec![
                Label::primary(fid, error_span).with_message("expected `]`"),
            ]),
            ErrorKind::MissingInlineTableComma => {
                diag.with_message(self.to_string()).with_labels(vec![
                    Label::primary(fid, error_span).with_message("expected `,`"),
                ])
            }
            ErrorKind::UnclosedInlineTable => {
                diag.with_message(self.to_string()).with_labels(vec![
                    Label::primary(fid, error_span).with_message("expected `}`"),
                ])
            }
            ErrorKind::Custom(msg) => diag
                .with_message(msg.to_string())
                .with_labels(vec![Label::primary(fid, error_span)]),
        }
    }
}
