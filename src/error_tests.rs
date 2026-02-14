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
        (ErrorKind::InvalidNumber, "invalid-number"),
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
            "file is too large (maximum 4GiB)",
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
                kind: ErrorKind::InvalidNumber,
                span,
            },
            "invalid number",
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
fn error_debug_fmt() {
    let span = Span::new(10, 20);
    let error = Error {
        kind: ErrorKind::UnexpectedEof,
        span,
    };
    let debug = format!("{:?}", error);
    assert!(debug.contains("Error"));
    assert!(debug.contains("kind"));
    assert!(debug.contains("span"));

    // Test ErrorKind Debug (which delegates to Display, returns the kind discriminant)
    let kind = ErrorKind::Custom("test".into());
    let debug = format!("{:?}", kind);
    assert_eq!(debug, "custom");

    let kind = ErrorKind::Wanted {
        expected: "string",
        found: "integer",
    };
    let debug = format!("{:?}", kind);
    assert_eq!(debug, "wanted");
}
