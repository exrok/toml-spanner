use super::*;

#[test]
fn display_all_error_kinds() {
    let cases: Vec<(ErrorKind, &str)> = vec![
        (ErrorKind::UnexpectedEof, "unexpected-eof"),
        (ErrorKind::FileTooLarge, "file-too-large"),
        (
            ErrorKind::DottedKeyInvalidType {
                first: Span::new(0, 1),
            },
            "dotted-key-invalid-type",
        ),
        (
            ErrorKind::DuplicateKey {
                first: Span::new(0, 1),
            },
            "duplicate-key",
        ),
        (
            ErrorKind::DuplicateTable {
                name: Span::new(0, 1),
                first: Span::new(0, 1),
            },
            "duplicate-table",
        ),
        (ErrorKind::UnexpectedKey, "unexpected-key"),
        (ErrorKind::UnquotedString, "unquoted-string"),
        (ErrorKind::MultilineStringKey, "multiline-string-key"),
        (ErrorKind::RedefineAsArray, "redefine-as-array"),
        (
            ErrorKind::InvalidCharInString('x'),
            "invalid-char-in-string",
        ),
        (ErrorKind::InvalidEscape('z'), "invalid-escape"),
        (
            ErrorKind::InvalidEscapeValue(0xDEAD),
            "invalid-escape-value",
        ),
        (ErrorKind::InvalidHexEscape('g'), "invalid-hex-escape"),
        (ErrorKind::Unexpected('!'), "unexpected"),
        (ErrorKind::UnterminatedString('"'), "unterminated-string"),
        (ErrorKind::InvalidInteger(""), "invalid-integer"),
        (
            ErrorKind::InvalidInteger("integer overflow"),
            "invalid-integer",
        ),
        (ErrorKind::InvalidFloat(""), "invalid-float"),
        (
            ErrorKind::InvalidFloat("float overflow"),
            "invalid-float",
        ),
        (ErrorKind::InvalidDateTime(""), "invalid-datetime"),
        (
            ErrorKind::InvalidDateTime("month is out of range"),
            "invalid-datetime",
        ),
        (ErrorKind::OutOfRange("i8"), "out-of-range"),
        (
            ErrorKind::Wanted {
                expected: &"a string",
                found: &"an integer",
            },
            "wanted",
        ),
        (ErrorKind::MissingField("name"), "missing-field"),
        (ErrorKind::DuplicateField("x"), "duplicate-field"),
        (
            ErrorKind::Deprecated {
                old: &"old_field",
                new: &"new_field",
            },
            "deprecated",
        ),
        (
            ErrorKind::UnexpectedValue {
                expected: &["a", "b"],
            },
            "unexpected-value",
        ),
        (ErrorKind::MissingArrayComma, "missing-array-comma"),
        (ErrorKind::UnclosedArray, "unclosed-array"),
        (
            ErrorKind::MissingInlineTableComma,
            "missing-inline-table-comma",
        ),
        (ErrorKind::UnclosedInlineTable, "unclosed-inline-table"),
    ];

    for (kind, expected) in &cases {
        assert_eq!(
            format!("{kind}"),
            *expected,
            "Display mismatch for {expected}"
        );
    }
}

#[test]
fn error_display_all_variants() {
    let span = Span::new(0, 1);
    let cases: Vec<(Error, &str)> = vec![
        (
            Error::new(ErrorKind::UnexpectedEof, span),
            "unexpected eof encountered",
        ),
        (
            Error::new(ErrorKind::FileTooLarge, span),
            "file is too large (maximum 512 MiB)",
        ),
        (
            Error::new(ErrorKind::InvalidCharInString('x'), span),
            "invalid character in string: `x`",
        ),
        (
            Error::new(ErrorKind::InvalidEscape('\t'), span),
            "invalid escape character in string: `\\t`",
        ),
        (
            Error::new(ErrorKind::InvalidEscape('z'), span),
            "invalid escape character in string: `z`",
        ),
        (
            Error::new(ErrorKind::InvalidHexEscape('g'), span),
            "invalid hex escape character in string: `g`",
        ),
        (
            Error::new(ErrorKind::InvalidEscapeValue(0xDEAD), span),
            "invalid escape value: `57005`",
        ),
        (
            Error::new(ErrorKind::Unexpected('!'), span),
            "unexpected character found: `!`",
        ),
        (
            Error::new(ErrorKind::UnterminatedString('"'), span),
            "invalid basic string, expected `\"`",
        ),
        (
            Error::new(ErrorKind::UnterminatedString('\''), span),
            "invalid literal string, expected `'`",
        ),
        (
            Error::new(
                ErrorKind::Wanted {
                    expected: &"a string",
                    found: &"an integer",
                },
                span,
            ),
            "expected a string, found an integer",
        ),
        (
            Error::new(ErrorKind::InvalidInteger(""), span),
            "invalid integer",
        ),
        (
            Error::new(ErrorKind::InvalidInteger("integer overflow"), span),
            "invalid integer: integer overflow",
        ),
        (
            Error::new(ErrorKind::InvalidFloat(""), span),
            "invalid float",
        ),
        (
            Error::new(ErrorKind::InvalidFloat("float overflow"), span),
            "invalid float: float overflow",
        ),
        (
            Error::new(ErrorKind::InvalidDateTime(""), span),
            "invalid datetime",
        ),
        (
            Error::new(
                ErrorKind::InvalidDateTime("month is out of range"),
                span,
            ),
            "invalid datetime: month is out of range",
        ),
        (
            Error::new(ErrorKind::OutOfRange("i8"), span),
            "out of range of 'i8'",
        ),
        (
            Error::new(
                ErrorKind::DuplicateTable {
                    name: span,
                    first: span,
                },
                span,
            ),
            "redefinition of table",
        ),
        (
            Error::new(ErrorKind::DuplicateKey { first: span }, span),
            "duplicate key",
        ),
        (
            Error::new(ErrorKind::RedefineAsArray, span),
            "table redefined as array",
        ),
        (
            Error::new(ErrorKind::MultilineStringKey, span),
            "multiline strings are not allowed for key",
        ),
        (
            Error::custom("custom message", span),
            "custom message",
        ),
        (
            Error::new(ErrorKind::DottedKeyInvalidType { first: span }, span),
            "dotted key attempted to extend non-table type",
        ),
        (
            Error::new(ErrorKind::UnexpectedKey, span),
            "unexpected key",
        ),
        (
            Error::new(ErrorKind::UnquotedString, span),
            "invalid TOML value, did you mean to use a quoted string?",
        ),
        (
            Error::new(ErrorKind::MissingField("name"), span),
            "missing field 'name' in table",
        ),
        (
            Error::new(
                ErrorKind::Deprecated {
                    old: &"old_key",
                    new: &"new_key",
                },
                span,
            ),
            "field 'old_key' is deprecated, 'new_key' has replaced it",
        ),
        (
            Error::new(
                ErrorKind::UnexpectedValue {
                    expected: &["x", "y"],
                },
                span,
            ),
            "expected '[x, y]'",
        ),
        (
            Error::new(ErrorKind::MissingArrayComma, span),
            "missing comma between array elements, expected `,`",
        ),
        (
            Error::new(ErrorKind::UnclosedArray, span),
            "unclosed array, expected `]`",
        ),
        (
            Error::new(ErrorKind::MissingInlineTableComma, span),
            "missing comma in inline table, expected `,`",
        ),
        (
            Error::new(ErrorKind::UnclosedInlineTable, span),
            "unclosed inline table, expected `}`",
        ),
    ];

    for (error, expected) in &cases {
        assert_eq!(format!("{error}"), *expected, "mismatch for {expected}");
    }
}

#[test]
fn error_constructors() {
    let err = Error::custom("something broke", Span::new(5, 10));
    assert_eq!(err.span(), Span::new(5, 10));
    assert!(err.kind().is_none());
    assert_eq!(format!("{err}"), "something broke");

    let err: Error = (ErrorKind::InvalidInteger(""), Span::new(0, 5)).into();
    assert_eq!(err.span(), Span::new(0, 5));
    assert!(matches!(err.kind(), Some(ErrorKind::InvalidInteger(""))));

    let _: &dyn std::error::Error = &err;
}

#[test]
fn duplicate_field_display() {
    let err = Error::new(ErrorKind::DuplicateField("name"), Span::new(0, 4));
    assert_eq!(format!("{err}"), "duplicate field 'name'");

    let kind = ErrorKind::DuplicateField("x");
    assert_eq!(format!("{kind}"), "duplicate-field");
}

#[cfg(feature = "from-toml")]
#[test]
fn from_toml_error_display_and_debug() {
    use crate::FromTomlError;

    let err = FromTomlError {
        errors: vec![Error::new(
            ErrorKind::MissingField("name"),
            Span::new(0, 5),
        )],
    };
    assert_eq!(format!("{err}"), "missing field 'name' in table");
    let debug = format!("{err:?}");
    assert!(debug.contains("FromTomlError"));

    let err = FromTomlError {
        errors: vec![
            Error::new(ErrorKind::MissingField("a"), Span::new(0, 1)),
            Error::new(ErrorKind::MissingField("b"), Span::new(2, 3)),
            Error::new(ErrorKind::MissingField("c"), Span::new(4, 5)),
        ],
    };
    let display = format!("{err}");
    assert!(display.contains("(+2 more errors)"), "got: {display}");

    let err = FromTomlError {
        errors: vec![
            Error::new(ErrorKind::InvalidInteger(""), Span::new(0, 1)),
            Error::new(ErrorKind::InvalidInteger(""), Span::new(2, 3)),
        ],
    };
    let display = format!("{err}");
    assert!(display.contains("(+1 more error)"), "got: {display}");

    let err = FromTomlError { errors: vec![] };
    assert_eq!(format!("{err}"), "deserialization failed");

    let _: &dyn std::error::Error = &err;

    let single_err = Error::new(ErrorKind::InvalidInteger(""), Span::new(0, 1));
    let from: FromTomlError = single_err.into();
    assert_eq!(from.errors.len(), 1);

    let errors = vec![Error::new(
        ErrorKind::InvalidInteger(""),
        Span::new(0, 1),
    )];
    let from: FromTomlError = errors.into();
    assert_eq!(from.errors.len(), 1);
}

#[cfg(feature = "to-toml")]
#[test]
fn to_toml_error_display_and_debug() {
    use crate::ToTomlError;

    let err = ToTomlError {
        message: "something went wrong".into(),
    };
    assert_eq!(format!("{err}"), "something went wrong");
    let debug = format!("{err:?}");
    assert!(debug.contains("ToTomlError"));

    let _: &dyn std::error::Error = &err;

    let err: ToTomlError = std::borrow::Cow::Borrowed("static msg").into();
    assert_eq!(format!("{err}"), "static msg");

    let err: ToTomlError = "plain str".into();
    assert_eq!(format!("{err}"), "plain str");
}
