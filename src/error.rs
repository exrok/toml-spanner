#![allow(clippy::question_mark)]
use crate::Span;
use std::fmt::{self, Debug, Display};

/// Error that can occur when deserializing TOML.
#[derive(Debug, Clone)]
pub struct Error {
    /// The error kind
    pub kind: ErrorKind,
    /// The span where the error occurs.
    ///
    /// Note some [`ErrorKind`] contain additional span information
    pub span: Span,
    /// Line and column information, only available for errors coming from the parser
    pub line_info: Option<(usize, usize)>,
}

impl std::error::Error for Error {}

impl From<(ErrorKind, Span)> for Error {
    fn from((kind, span): (ErrorKind, Span)) -> Self {
        Self {
            kind,
            span,
            line_info: None,
        }
    }
}

/// Errors that can occur when deserializing a type.
#[derive(Clone)]
pub enum ErrorKind {
    /// EOF was reached when looking for a value.
    UnexpectedEof,

    /// The input file is larger than the maximum supported size of 4GiB.
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

    /// An unterminated string was found where EOF was found before the ending
    /// EOF mark.
    UnterminatedString,

    /// A number failed to parse.
    InvalidNumber,

    /// The number in the toml file cannot be losslessly converted to the specified
    /// number type
    OutOfRange(&'static str),

    /// Wanted one sort of token, but found another.
    Wanted {
        /// Expected token type.
        expected: &'static str,
        /// Actually found token type.
        found: &'static str,
    },

    /// A duplicate table definition was found.
    DuplicateTable {
        /// The name of the duplicate table
        name: String,
        /// The span where the table was first defined
        first: Span,
    },

    /// Duplicate key in table.
    DuplicateKey {
        /// The duplicate key
        key: String,
        /// The span where the first key is located
        first: Span,
    },

    /// A previously defined table was redefined as an array.
    RedefineAsArray,

    /// Multiline strings are not allowed for key.
    MultilineStringKey,

    /// A custom error which could be generated when deserializing a particular
    /// type.
    Custom(std::borrow::Cow<'static, str>),

    /// Dotted key attempted to extend something that is not a table.
    DottedKeyInvalidType {
        /// The span where the non-table value was defined
        first: Span,
    },

    /// An unexpected key was encountered.
    ///
    /// Used when deserializing a struct with a limited set of fields.
    UnexpectedKeys {
        /// The unexpected keys.
        keys: Vec<(String, Span)>,
        /// The list of keys that were expected for the table
        expected: Vec<String>,
    },

    /// Unquoted string was found when quoted one was expected.
    UnquotedString,

    /// A required field is missing from a table
    MissingField(&'static str),

    /// A field in the table is deprecated and the new key should be used instead
    Deprecated {
        /// The deprecated key name
        old: &'static str,
        /// The key name that should be used instead
        new: &'static str,
    },

    /// An unexpected value was encountered
    UnexpectedValue {
        /// The list of values that could have been used, eg. typically enum variants
        expected: &'static [&'static str],
        /// The actual value that was found.
        value: Option<String>,
    },
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::UnexpectedEof => "unexpected-eof",
            Self::FileTooLarge => "file-too-large",
            Self::Custom(..) => "custom",
            Self::DottedKeyInvalidType { .. } => "dotted-key-invalid-type",
            Self::DuplicateKey { .. } => "duplicate-key",
            Self::DuplicateTable { .. } => "duplicate-table",
            Self::UnexpectedKeys { .. } => "unexpected-keys",
            Self::UnquotedString => "unquoted-string",
            Self::MultilineStringKey => "multiline-string-key",
            Self::RedefineAsArray => "redefine-as-array",
            Self::InvalidCharInString(..) => "invalid-char-in-string",
            Self::InvalidEscape(..) => "invalid-escape",
            Self::InvalidEscapeValue(..) => "invalid-escape-value",
            Self::InvalidHexEscape(..) => "invalid-hex-escape",
            Self::Unexpected(..) => "unexpected",
            Self::UnterminatedString => "unterminated-string",
            Self::InvalidNumber => "invalid-number",
            Self::OutOfRange(_) => "out-of-range",
            Self::Wanted { .. } => "wanted",
            Self::MissingField(..) => "missing-field",
            Self::Deprecated { .. } => "deprecated",
            Self::UnexpectedValue { .. } => "unexpected-value",
        };
        f.write_str(text)
    }
}

impl Debug for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

struct Escape(char);

impl fmt::Display for Escape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write as _;

        if self.0.is_whitespace() {
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
impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::UnexpectedEof => f.write_str("unexpected eof encountered"),
            ErrorKind::FileTooLarge => f.write_str("file is too large (maximum 4GiB)"),
            ErrorKind::InvalidCharInString(c) => {
                rtry!(f.write_str("invalid character in string: `"));
                rtry!(std::fmt::Display::fmt(c, f));
                f.write_str("`")
            }
            ErrorKind::InvalidEscape(c) => {
                rtry!(f.write_str("invalid escape character in string: `"));
                rtry!(Escape(*c).fmt(f));
                f.write_str("`")
            }
            ErrorKind::InvalidHexEscape(c) => {
                rtry!(f.write_str("invalid hex escape character in string: `"));
                rtry!(std::fmt::Display::fmt(c, f));
                f.write_str("`")
            }
            ErrorKind::InvalidEscapeValue(c) => {
                rtry!(f.write_str("invalid escape value: `"));
                rtry!(std::fmt::Display::fmt(c, f));
                f.write_str("`")
            }
            ErrorKind::Unexpected(c) => {
                rtry!(f.write_str("unexpected character found: `"));
                rtry!(std::fmt::Display::fmt(c, f));
                f.write_str("`")
            }
            ErrorKind::UnterminatedString => f.write_str("unterminated string"),
            ErrorKind::Wanted { expected, found } => {
                rtry!(f.write_str("expected "));
                rtry!(f.write_str(expected));
                rtry!(f.write_str(", found "));
                f.write_str(found)
            }
            ErrorKind::InvalidNumber => f.write_str("invalid number"),
            ErrorKind::OutOfRange(kind) => {
                rtry!(f.write_str("out of range of '"));
                rtry!(f.write_str(kind));
                f.write_str("'")
            }
            ErrorKind::DuplicateTable { name, .. } => {
                rtry!(f.write_str("redefinition of table `"));
                rtry!(f.write_str(name));
                f.write_str("`")
            }
            ErrorKind::DuplicateKey { key, .. } => {
                rtry!(f.write_str("duplicate key: `"));
                rtry!(f.write_str(key));
                f.write_str("`")
            }
            ErrorKind::RedefineAsArray => f.write_str("table redefined as array"),
            ErrorKind::MultilineStringKey => {
                f.write_str("multiline strings are not allowed for key")
            }
            ErrorKind::Custom(message) => f.write_str(message),
            ErrorKind::DottedKeyInvalidType { .. } => {
                f.write_str("dotted key attempted to extend non-table type")
            }
            ErrorKind::UnexpectedKeys { keys, expected } => {
                rtry!(f.write_str("unexpected keys in table: ["));
                let mut first = true;
                for (key, _) in keys {
                    if !first {
                        rtry!(f.write_str(", "));
                    }
                    first = false;
                    rtry!(f.write_str("\""));
                    rtry!(f.write_str(key));
                    rtry!(f.write_str("\""));
                }
                rtry!(f.write_str("]\nexpected: ["));
                let mut first = true;
                for key in expected {
                    if !first {
                        rtry!(f.write_str(", "));
                    }
                    first = false;
                    rtry!(f.write_str("\""));
                    rtry!(f.write_str(key));
                    rtry!(f.write_str("\""));
                }
                f.write_str("]")
            }
            ErrorKind::UnquotedString => {
                f.write_str("invalid TOML value, did you mean to use a quoted string?")
            }
            ErrorKind::MissingField(field) => {
                rtry!(f.write_str("missing field '"));
                rtry!(f.write_str(field));
                f.write_str("' in table")
            }
            ErrorKind::Deprecated { old, new } => {
                rtry!(f.write_str("field '"));
                rtry!(f.write_str(old));
                rtry!(f.write_str("' is deprecated, '"));
                rtry!(f.write_str(new));
                f.write_str("' has replaced it")
            }
            ErrorKind::UnexpectedValue { expected, .. } => {
                rtry!(f.write_str("expected '["));
                let mut first = true;
                for val in *expected {
                    if !first {
                        rtry!(f.write_str(", "));
                    }
                    first = false;
                    rtry!(f.write_str(val));
                }
                f.write_str("]'")
            }
        }
    }
}

#[cfg(feature = "reporting")]
#[cfg_attr(docsrs, doc(cfg(feature = "reporting")))]
impl Error {
    /// Converts this [`Error`] into a [`codespan_reporting::diagnostic::Diagnostic`]
    pub fn to_diagnostic<FileId: Copy + PartialEq>(
        &self,
        fid: FileId,
    ) -> codespan_reporting::diagnostic::Diagnostic<FileId> {
        let diag =
            codespan_reporting::diagnostic::Diagnostic::error().with_code(self.kind.to_string());

        use codespan_reporting::diagnostic::Label;

        match &self.kind {
            ErrorKind::DuplicateKey { first, .. } => diag.with_labels(vec![
                Label::secondary(fid, *first).with_message("first key instance"),
                Label::primary(fid, self.span).with_message("duplicate key"),
            ]),
            ErrorKind::Unexpected(c) => diag.with_labels(vec![
                Label::primary(fid, self.span)
                    .with_message(format!("unexpected character '{}'", Escape(*c))),
            ]),
            ErrorKind::InvalidCharInString(c) => diag.with_labels(vec![
                Label::primary(fid, self.span)
                    .with_message(format!("invalid character '{}' in string", Escape(*c))),
            ]),
            ErrorKind::InvalidEscape(c) => {
                diag.with_labels(vec![Label::primary(fid, self.span).with_message(format!(
                    "invalid escape character '{}' in string",
                    Escape(*c)
                ))])
            }
            ErrorKind::InvalidEscapeValue(_) => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message("invalid escape value"),
            ]),
            ErrorKind::InvalidNumber => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message("unable to parse number"),
            ]),
            ErrorKind::OutOfRange(kind) => diag
                .with_message(format!("number is out of range of '{kind}'"))
                .with_labels(vec![Label::primary(fid, self.span)]),
            ErrorKind::Wanted { expected, .. } => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message(format!("expected {expected}")),
            ]),
            ErrorKind::MultilineStringKey => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message("multiline keys are not allowed"),
            ]),
            ErrorKind::UnterminatedString => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message("eof reached before string terminator"),
            ]),
            ErrorKind::DuplicateTable { first, .. } => diag.with_labels(vec![
                Label::secondary(fid, *first).with_message("first table instance"),
                Label::primary(fid, self.span).with_message("duplicate table"),
            ]),
            ErrorKind::InvalidHexEscape(c) => diag.with_labels(vec![
                Label::primary(fid, self.span)
                    .with_message(format!("invalid hex escape '{}'", Escape(*c))),
            ]),
            ErrorKind::UnquotedString => diag.with_labels(vec![
                Label::primary(fid, self.span).with_message("string is not quoted"),
            ]),
            ErrorKind::UnexpectedKeys { keys, expected } => diag
                .with_message(format!(
                    "found {} unexpected keys, expected: {expected:?}",
                    keys.len()
                ))
                .with_labels(
                    keys.iter()
                        .map(|(_name, span)| Label::secondary(fid, *span))
                        .collect(),
                ),
            ErrorKind::MissingField(field) => diag
                .with_message(format!("missing field '{field}'"))
                .with_labels(vec![
                    Label::primary(fid, self.span).with_message("table with missing field"),
                ]),
            ErrorKind::Deprecated { new, .. } => diag
                .with_message(format!(
                    "deprecated field enountered, '{new}' should be used instead"
                ))
                .with_labels(vec![
                    Label::primary(fid, self.span).with_message("deprecated field"),
                ]),
            ErrorKind::UnexpectedValue { expected, .. } => diag
                .with_message(format!("expected '{expected:?}'"))
                .with_labels(vec![
                    Label::primary(fid, self.span).with_message("unexpected value"),
                ]),
            ErrorKind::UnexpectedEof => diag
                .with_message("unexpected end of file")
                .with_labels(vec![Label::primary(fid, self.span)]),
            ErrorKind::DottedKeyInvalidType { first } => {
                diag.with_message(self.to_string()).with_labels(vec![
                    Label::primary(fid, self.span).with_message("attempted to extend table here"),
                    Label::secondary(fid, *first).with_message("non-table"),
                ])
            }
            ErrorKind::RedefineAsArray => diag
                .with_message(self.to_string())
                .with_labels(vec![Label::primary(fid, self.span)]),
            ErrorKind::Custom(msg) => diag
                .with_message(msg.to_string())
                .with_labels(vec![Label::primary(fid, self.span)]),
            ErrorKind::FileTooLarge => diag
                .with_message("file is too large (maximum 4GiB)")
                .with_labels(vec![Label::primary(fid, self.span)]),
        }
    }
}

/// When deserializing, it's possible to collect multiple errors instead of earlying
/// out at the first error
#[derive(Debug)]
pub struct DeserError {
    /// The set of errors that occurred during deserialization
    pub errors: Box<Vec<Error>>,
}

impl DeserError {
    /// Merges errors from another [`Self`]
    #[inline]
    pub fn merge(&mut self, mut other: Self) {
        self.errors.append(&mut other.errors);
    }
}

impl std::error::Error for DeserError {}

impl From<Error> for DeserError {
    fn from(value: Error) -> Self {
        Self {
            errors: Box::new(vec![value]),
        }
    }
}

impl fmt::Display for DeserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for err in &*self.errors {
            writeln!(f, "{err}")?;
        }

        Ok(())
    }
}
