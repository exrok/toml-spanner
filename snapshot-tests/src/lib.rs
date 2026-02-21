#![allow(missing_docs)]

/// Loads a valid toml file and does a snapshot assertion against `toml`
#[macro_export]
macro_rules! valid {
    ($name:ident) => {
        #[test]
        fn $name() {
            let toml_str = std::fs::read_to_string(concat!("data/", stringify!($name), ".toml"))
                .expect(concat!("failed to load ", stringify!($name), ".toml"));
            let arena = toml_spanner::Arena::new();
            let valid_toml = toml_spanner::parse(&toml_str, &arena).expect("failed to parse toml");
            insta::assert_json_snapshot!(valid_toml);

            $crate::emit_spans!($name, valid_toml, &toml_str);
        }
    };
    ($name:ident, $toml:literal) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let valid_toml = toml_spanner::parse($toml, &arena).expect("failed to parse toml");
            insta::assert_json_snapshot!(valid_toml);

            $crate::emit_spans!($name, valid_toml, $toml);
        }
    };
}

/// Loads a valid toml file, deserializes it to the specified type and asserts
/// the debug snapshot matches
#[macro_export]
macro_rules! valid_de {
    ($name:ident, $kind:ty) => {
        #[test]
        fn $name() {
            let toml_str = std::fs::read_to_string(concat!("data/", stringify!($name), ".toml"))
                .expect(concat!("failed to load ", stringify!($name), ".toml"));
            let arena = toml_spanner::Arena::new();
            let mut root = toml_spanner::parse(&toml_str, &arena).expect("failed to parse toml");

            match root.deserialize::<$kind>() {
                Ok(de) if !root.has_errors() => {
                    insta::assert_debug_snapshot!(de);
                }
                _ => {
                    let file = $crate::File::new(stringify!($name), &toml_str);
                    let diags = root.errors().iter().map(|e| e.to_diagnostic(()));
                    let error = $crate::emit_diags(&file, diags);
                    panic!("unexpected toml deserialization errors:\n{error}");
                }
            }
        }
    };
    ($name:ident, $kind:ty, $toml:literal) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let mut root = toml_spanner::parse($toml, &arena).expect("failed to parse toml");

            match root.deserialize::<$kind>() {
                Ok(de) if !root.has_errors() => {
                    insta::assert_debug_snapshot!(de);
                }
                _ => {
                    let file = $crate::File::new(stringify!($name), $toml);
                    let diags = root.errors().iter().map(|e| e.to_diagnostic(()));
                    let error = $crate::emit_diags(&file, diags);
                    panic!("unexpected toml deserialization errors:\n{error}");
                }
            }
        }
    };
}

/// Loads a valid toml file, deserializes it to the specified type and asserts
/// the appropriate errors are produced
#[macro_export]
macro_rules! invalid_de {
    ($name:ident, $kind:ty) => {
        #[test]
        fn $name() {
            let toml_str = std::fs::read_to_string(concat!("data/", stringify!($name), ".toml"))
                .expect(concat!("failed to load ", stringify!($name), ".toml"));
            let arena = toml_spanner::Arena::new();
            let mut root = toml_spanner::parse(&toml_str, &arena).expect("failed to parse toml");

            match root.deserialize::<$kind>() {
                Ok(de) if !root.has_errors() => {
                    panic!("expected errors but deserialized '{de:#?}' successfully");
                }
                _ => {
                    let diags: Vec<_> = root.errors().iter().map(|e| e.to_diagnostic(())).collect();
                    $crate::error_snapshot!($name, diags, &toml_str);
                }
            }
        }
    };
    ($name:ident, $kind:ty, $toml:literal) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let mut root = toml_spanner::parse($toml, &arena).expect("failed to parse toml");

            match root.deserialize::<$kind>() {
                Ok(de) if !root.has_errors() => {
                    panic!("expected errors but deserialized '{de:#?}' successfully");
                }
                _ => {
                    let diags: Vec<_> = root.errors().iter().map(|e| e.to_diagnostic(())).collect();
                    $crate::error_snapshot!($name, diags, $toml);
                }
            }
        }
    };
}

pub type File<'s> = codespan_reporting::files::SimpleFile<&'static str, &'s str>;

pub fn emit_diags(
    f: &File<'_>,
    error: impl IntoIterator<Item = codespan_reporting::diagnostic::Diagnostic<()>>,
) -> String {
    let mut output = codespan_reporting::term::termcolor::NoColor::new(Vec::new());

    for diag in error {
        codespan_reporting::term::emit_to_write_style(
            &mut output,
            &codespan_reporting::term::Config::default(),
            f,
            &diag,
        )
        .expect("uhm...oops?");
    }

    String::from_utf8(output.into_inner()).unwrap()
}

/// Creates a codespan diagnostic for an error and asserts the emitted diagnostic
/// matches a snapshot
#[macro_export]
macro_rules! error_snapshot {
    ($name:ident, $err:expr, $toml:expr) => {
        let file = $crate::File::new(stringify!($name), $toml);
        let error = $crate::emit_diags(&file, $err);
        insta::assert_snapshot!(error);
    };
}

use codespan_reporting::diagnostic::Diagnostic;

pub fn collect_spans(key: &str, val: &toml_spanner::Item<'_>, diags: &mut Vec<Diagnostic<()>>) {
    use codespan_reporting::diagnostic::Label;
    use toml_spanner::Value;

    let code = match val.value() {
        Value::String(_s) => "string",
        Value::Integer(_s) => "integer",
        Value::Float(_s) => "float",
        Value::Boolean(_s) => "bool",
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                collect_spans(&format!("{key}_{i}"), v, diags);
            }

            "array"
        }
        Value::Table(tab) => {
            for (k, v) in tab {
                collect_spans(&format!("{key}_{}", k.name), v, diags);
            }

            "table"
        }
        Value::DateTime(_) => "datetime",
    };

    diags.push(
        Diagnostic::note()
            .with_code(code)
            .with_message(key)
            .with_labels(vec![Label::primary((), val.span())]),
    );
}

#[macro_export]
macro_rules! emit_spans {
    ($name:ident, $val:expr, $toml:expr) => {
        let file = $crate::File::new(stringify!($name), $toml);

        let mut spans = Vec::new();

        let root_val = $val.into_item();
        $crate::collect_spans("root", &root_val, &mut spans);

        let spans = $crate::emit_diags(&file, spans);
        insta::assert_snapshot!(spans);
    };
}

/// Loads an invalid toml file and does a snapshot assertion on the error
#[macro_export]
macro_rules! invalid {
    ($name:ident) => {
        #[test]
        fn $name() {
            let toml_str =
                std::fs::read_to_string(dbg!(concat!("data/", stringify!($name), ".toml")))
                    .expect(concat!("failed to load ", stringify!($name), ".toml"));
            let arena = toml_spanner::Arena::new();
            let error = toml_spanner::parse(&toml_str, &arena).unwrap_err();
            $crate::error_snapshot!($name, Some(error.to_diagnostic(())), &toml_str);
        }
    };
    ($name:ident, $toml:expr) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let error = toml_spanner::parse($toml, &arena).unwrap_err();
            $crate::error_snapshot!($name, Some(error.to_diagnostic(())), $toml);
        }
    };
}
