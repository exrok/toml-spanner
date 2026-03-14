use super::*;

#[test]
fn display_all_error_kinds() {
    let cases: Vec<(ErrorKind, &str)> = vec![
        (ErrorKind::UnexpectedEof, "unexpected-eof"),
        (ErrorKind::FileTooLarge, "file-too-large"),
        (ErrorKind::Custom("msg".into()), "custom"),
        (
            ErrorKind::DottedKeyInvalidType {
                first: Span::new(0, 1),
            },
            "dotted-key-invalid-type",
        ),
        (
            ErrorKind::DuplicateKey {
                key: "k".into(),
                first: Span::new(0, 1),
            },
            "duplicate-key",
        ),
        (
            ErrorKind::DuplicateTable {
                name: "t".into(),
                first: Span::new(0, 1),
            },
            "duplicate-table",
        ),
        (
            ErrorKind::UnexpectedKeys {
                keys: vec![("k".into(), Span::new(0, 1))],
            },
            "unexpected-keys",
        ),
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
        (ErrorKind::UnterminatedString, "unterminated-string"),
        (ErrorKind::InvalidInteger(""), "invalid-integer"),
        (ErrorKind::InvalidInteger("integer overflow"), "invalid-integer"),
        (ErrorKind::InvalidFloat(""), "invalid-float"),
        (ErrorKind::InvalidFloat("float overflow"), "invalid-float"),
        (ErrorKind::InvalidDateTime(""), "invalid-datetime"),
        (ErrorKind::InvalidDateTime("month is out of range"), "invalid-datetime"),
        (ErrorKind::OutOfRange("i8"), "out-of-range"),
        (
            ErrorKind::Wanted {
                expected: "a string",
                found: "an integer",
            },
            "wanted",
        ),
        (ErrorKind::MissingField("name"), "missing-field"),
        (
            ErrorKind::Deprecated {
                old: "old_field",
                new: "new_field",
            },
            "deprecated",
        ),
        (
            ErrorKind::UnexpectedValue {
                expected: &["a", "b"],
                value: Some("c".into()),
            },
            "unexpected-value",
        ),
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
            Error {
                kind: ErrorKind::UnexpectedEof,
                span,
            },
            "unexpected eof encountered",
        ),
        (
            Error {
                kind: ErrorKind::FileTooLarge,
                span,
            },
            "file is too large (maximum 512 MiB)",
        ),
        (
            Error {
                kind: ErrorKind::InvalidCharInString('x'),
                span,
            },
            "invalid character in string: `x`",
        ),
        (
            Error {
                kind: ErrorKind::InvalidEscape('\t'),
                span,
            },
            "invalid escape character in string: `\\t`",
        ),
        (
            Error {
                kind: ErrorKind::InvalidEscape('z'),
                span,
            },
            "invalid escape character in string: `z`",
        ),
        (
            Error {
                kind: ErrorKind::InvalidHexEscape('g'),
                span,
            },
            "invalid hex escape character in string: `g`",
        ),
        (
            Error {
                kind: ErrorKind::InvalidEscapeValue(0xDEAD),
                span,
            },
            "invalid escape value: `57005`",
        ),
        (
            Error {
                kind: ErrorKind::Unexpected('!'),
                span,
            },
            "unexpected character found: `!`",
        ),
        (
            Error {
                kind: ErrorKind::UnterminatedString,
                span,
            },
            "unterminated string",
        ),
        (
            Error {
                kind: ErrorKind::Wanted {
                    expected: "a string",
                    found: "an integer",
                },
                span,
            },
            "expected a string, found an integer",
        ),
        (
            Error {
                kind: ErrorKind::InvalidInteger(""),
                span,
            },
            "invalid integer",
        ),
        (
            Error {
                kind: ErrorKind::InvalidInteger("integer overflow"),
                span,
            },
            "invalid integer: integer overflow",
        ),
        (
            Error {
                kind: ErrorKind::InvalidFloat(""),
                span,
            },
            "invalid float",
        ),
        (
            Error {
                kind: ErrorKind::InvalidFloat("float overflow"),
                span,
            },
            "invalid float: float overflow",
        ),
        (
            Error {
                kind: ErrorKind::InvalidDateTime(""),
                span,
            },
            "invalid datetime",
        ),
        (
            Error {
                kind: ErrorKind::InvalidDateTime("month is out of range"),
                span,
            },
            "invalid datetime: month is out of range",
        ),
        (
            Error {
                kind: ErrorKind::OutOfRange("i8"),
                span,
            },
            "out of range of 'i8'",
        ),
        (
            Error {
                kind: ErrorKind::DuplicateTable {
                    name: "mytable".into(),
                    first: span,
                },
                span,
            },
            "redefinition of table `mytable`",
        ),
        (
            Error {
                kind: ErrorKind::DuplicateKey {
                    key: "mykey".into(),
                    first: span,
                },
                span,
            },
            "duplicate key: `mykey`",
        ),
        (
            Error {
                kind: ErrorKind::RedefineAsArray,
                span,
            },
            "table redefined as array",
        ),
        (
            Error {
                kind: ErrorKind::MultilineStringKey,
                span,
            },
            "multiline strings are not allowed for key",
        ),
        (
            Error {
                kind: ErrorKind::Custom("custom message".into()),
                span,
            },
            "custom message",
        ),
        (
            Error {
                kind: ErrorKind::DottedKeyInvalidType { first: span },
                span,
            },
            "dotted key attempted to extend non-table type",
        ),
        (
            Error {
                kind: ErrorKind::UnexpectedKeys {
                    keys: vec![("only".into(), Span::new(0, 4))],
                },
                span: Span::new(0, 10),
            },
            "unexpected keys in table: [\"only\"]",
        ),
        (
            Error {
                kind: ErrorKind::UnexpectedKeys {
                    keys: vec![("a".into(), span), ("b".into(), span)],
                },
                span,
            },
            "unexpected keys in table: [\"a\", \"b\"]",
        ),
        (
            Error {
                kind: ErrorKind::UnquotedString,
                span,
            },
            "invalid TOML value, did you mean to use a quoted string?",
        ),
        (
            Error {
                kind: ErrorKind::MissingField("name"),
                span,
            },
            "missing field 'name' in table",
        ),
        (
            Error {
                kind: ErrorKind::Deprecated {
                    old: "old_key",
                    new: "new_key",
                },
                span,
            },
            "field 'old_key' is deprecated, 'new_key' has replaced it",
        ),
        (
            Error {
                kind: ErrorKind::UnexpectedValue {
                    expected: &["x", "y"],
                    value: Some("z".into()),
                },
                span,
            },
            "expected '[x, y]'",
        ),
    ];

    for (error, expected) in &cases {
        assert_eq!(format!("{error}"), *expected, "mismatch for {expected}");
    }
}

#[test]
fn error_constructors() {
    // Error::custom()
    let err = Error::custom("something broke", Span::new(5, 10));
    assert_eq!(err.span, Span::new(5, 10));
    assert!(matches!(err.kind, ErrorKind::Custom(..)));
    assert_eq!(format!("{err}"), "something broke");

    // From<(ErrorKind, Span)>
    let err: Error = (ErrorKind::InvalidInteger(""), Span::new(0, 5)).into();
    assert_eq!(err.span, Span::new(0, 5));
    assert!(matches!(err.kind, ErrorKind::InvalidInteger("")));

    // Error is std::error::Error
    let _: &dyn std::error::Error = &err;
}

#[test]
fn duplicate_field_display() {
    let err = Error {
        kind: ErrorKind::DuplicateField("name"),
        span: Span::new(0, 4),
    };
    assert_eq!(format!("{err}"), "duplicate field 'name'");

    let kind = ErrorKind::DuplicateField("x");
    assert_eq!(format!("{kind}"), "duplicate-field");
}

#[cfg(feature = "from-toml")]
#[test]
fn from_toml_error_display_and_debug() {
    use crate::FromTomlError;

    // Single error
    let err = FromTomlError {
        errors: vec![Error {
            kind: ErrorKind::MissingField("name"),
            span: Span::new(0, 5),
        }],
    };
    assert_eq!(format!("{err}"), "missing field 'name' in table");
    let debug = format!("{err:?}");
    assert!(debug.contains("FromTomlError"));

    // Multiple errors
    let err = FromTomlError {
        errors: vec![
            Error {
                kind: ErrorKind::MissingField("a"),
                span: Span::new(0, 1),
            },
            Error {
                kind: ErrorKind::MissingField("b"),
                span: Span::new(2, 3),
            },
            Error {
                kind: ErrorKind::MissingField("c"),
                span: Span::new(4, 5),
            },
        ],
    };
    let display = format!("{err}");
    assert!(display.contains("(+2 more errors)"), "got: {display}");

    // Single extra error (singular)
    let err = FromTomlError {
        errors: vec![
            Error {
                kind: ErrorKind::InvalidInteger(""),
                span: Span::new(0, 1),
            },
            Error {
                kind: ErrorKind::InvalidInteger(""),
                span: Span::new(2, 3),
            },
        ],
    };
    let display = format!("{err}");
    assert!(display.contains("(+1 more error)"), "got: {display}");

    // Empty errors vec
    let err = FromTomlError { errors: vec![] };
    assert_eq!(format!("{err}"), "deserialization failed");

    // std::error::Error impl
    let _: &dyn std::error::Error = &err;

    // From<Error>
    let single_err = Error {
        kind: ErrorKind::InvalidInteger(""),
        span: Span::new(0, 1),
    };
    let from: FromTomlError = single_err.into();
    assert_eq!(from.errors.len(), 1);

    // From<Vec<Error>>
    let errors = vec![Error {
        kind: ErrorKind::InvalidInteger(""),
        span: Span::new(0, 1),
    }];
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

    // std::error::Error impl
    let _: &dyn std::error::Error = &err;

    // From<Cow<'static, str>>
    let err: ToTomlError = std::borrow::Cow::Borrowed("static msg").into();
    assert_eq!(format!("{err}"), "static msg");

    // From<&'static str>
    let err: ToTomlError = "plain str".into();
    assert_eq!(format!("{err}"), "plain str");
}
