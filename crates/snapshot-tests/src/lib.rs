#![allow(missing_docs)]

pub fn error_to_diagnostic(
    error: &toml_spanner::Error,
    source: &str,
    fid: (),
) -> codespan_reporting::diagnostic::Diagnostic<()> {
    use codespan_reporting::diagnostic::Label;

    let message = error.message_with_path(source);

    let mut labels = Vec::new();
    if let Some((span, text)) = error.secondary_label() {
        labels.push(Label::secondary(fid, span).with_message(text));
    }

    if let Some((span, label)) = error.primary_label() {
        let l = Label::primary(fid, span);
        labels.push(if label.is_empty() {
            l
        } else {
            l.with_message(label)
        });
    }

    codespan_reporting::diagnostic::Diagnostic::error()
        .with_code(error.kind().kind_name())
        .with_message(message)
        .with_labels(labels)
}

pub fn error_to_snippet<'s>(
    error: &toml_spanner::Error,
    source: &'s str,
    path: &'s str,
) -> annotate_snippets::Group<'s> {
    use annotate_snippets::{AnnotationKind, Level, Snippet};

    let message = error.message_with_path(source);

    let mut snippet = Snippet::source(source).path(path).fold(true);

    if let Some((span, text)) = error.secondary_label() {
        snippet = snippet.annotation(AnnotationKind::Context.span(span.range()).label(text));
    }

    if let Some((span, label)) = error.primary_label() {
        let ann = AnnotationKind::Primary.span(span.range());
        snippet = snippet.annotation(if label.is_empty() {
            ann
        } else {
            ann.label(label)
        });
    }

    Level::ERROR.primary_title(message).element(snippet)
}

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
            let mut doc = toml_spanner::parse(&toml_str, &arena).expect("failed to parse toml");

            match doc.to::<$kind>() {
                Ok(de) => {
                    let combined = format!("--- input ---\n{toml_str}\n--- output ---\n{de:#?}");
                    insta::assert_snapshot!(combined);
                }
                Err(e) => {
                    let file = $crate::File::new(stringify!($name), &toml_str);
                    let diags = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_diagnostic(e, &toml_str, ()));
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
            let mut doc = toml_spanner::parse($toml, &arena).expect("failed to parse toml");

            match doc.to::<$kind>() {
                Ok(de) => {
                    let combined = format!("--- input ---\n{}\n--- output ---\n{de:#?}", $toml);
                    insta::assert_snapshot!(combined);
                }
                Err(e) => {
                    let file = $crate::File::new(stringify!($name), $toml);
                    let diags = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_diagnostic(e, $toml, ()));
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
            let mut doc = toml_spanner::parse(&toml_str, &arena).expect("failed to parse toml");

            match doc.to::<$kind>() {
                Ok(de) => {
                    panic!("expected errors but deserialized '{de:#?}' successfully");
                }
                Err(e) => {
                    let diags: Vec<_> = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_diagnostic(e, &toml_str, ()))
                        .collect();
                    let groups: Vec<_> = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_snippet(e, &toml_str, stringify!($name)))
                        .collect();
                    $crate::error_snapshot!($name, diags, &toml_str, debug: e.errors, snippets: groups);
                }
            }
        }
    };
    ($name:ident, $kind:ty, $toml:literal) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let mut doc = toml_spanner::parse($toml, &arena).expect("failed to parse toml");

            match doc.to::<$kind>() {
                Ok(de) => {
                    panic!("expected errors but deserialized '{de:#?}' successfully");
                }
                Err(e) => {
                    let diags: Vec<_> = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_diagnostic(e, $toml, ()))
                        .collect();
                    let groups: Vec<_> = e
                        .errors
                        .iter()
                        .map(|e| $crate::error_to_snippet(e, $toml, stringify!($name)))
                        .collect();
                    $crate::error_snapshot!($name, diags, $toml, debug: e.errors, snippets: groups);
                }
            }
        }
    };
}

/// Deserializes successfully but expects non-fatal errors (e.g. deprecated fields)
#[macro_export]
macro_rules! warnings_de {
    ($name:ident, $kind:ty, $toml:literal) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let mut doc = toml_spanner::parse($toml, &arena).expect("failed to parse toml");

            let (_val, e) = doc
                .to_allowing_errors::<$kind>()
                .expect("deserialization should succeed");
            assert!(
                !e.errors.is_empty(),
                "expected non-fatal errors but got none"
            );
            let diags: Vec<_> = e
                .errors
                .iter()
                .map(|e| $crate::error_to_diagnostic(e, $toml, ()))
                .collect();
            $crate::error_snapshot!($name, diags, $toml, debug: e.errors);
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

/// Asserts the emitted diagnostic matches a snapshot
#[macro_export]
macro_rules! error_snapshot {
    ($name:ident, $err:expr, $toml:expr) => {
        let file = $crate::File::new(stringify!($name), $toml);
        let error = $crate::emit_diags(&file, $err);
        insta::assert_snapshot!(error);
    };
    ($name:ident, $err:expr, $toml:expr, debug: $debug:expr) => {
        let file = $crate::File::new(stringify!($name), $toml);
        let diag = $crate::emit_diags(&file, $err);
        let combined = format!("{diag}\n{:#?}", $debug);
        insta::assert_snapshot!(combined);
    };
    ($name:ident, $err:expr, $toml:expr, debug: $debug:expr, snippets: $snippets:expr) => {
        let file = $crate::File::new(stringify!($name), $toml);
        let diag = $crate::emit_diags(&file, $err);
        let snippet = $crate::render_snippets(&$snippets);
        let combined = format!("{diag}\n{snippet}\n{:#?}", $debug);
        insta::assert_snapshot!(combined);
    };
}

pub fn collect_spans(
    key: &str,
    val: &toml_spanner::Item<'_>,
    diags: &mut Vec<codespan_reporting::diagnostic::Diagnostic<()>>,
) {
    use codespan_reporting::diagnostic::{Diagnostic, Label};
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
            let group = $crate::error_to_snippet(&error, &toml_str, stringify!($name));
            $crate::error_snapshot!(
                $name,
                Some($crate::error_to_diagnostic(&error, &toml_str, ())),
                &toml_str,
                debug: error,
                snippets: [group]
            );
        }
    };
    ($name:ident, $toml:expr) => {
        #[test]
        fn $name() {
            let arena = toml_spanner::Arena::new();
            let error = toml_spanner::parse($toml, &arena).unwrap_err();
            let group = $crate::error_to_snippet(&error, $toml, stringify!($name));
            $crate::error_snapshot!(
                $name,
                Some($crate::error_to_diagnostic(&error, $toml, ())),
                $toml,
                debug: error,
                snippets: [group]
            );
        }
    };
}

pub fn render_snippets(groups: &[annotate_snippets::Group<'_>]) -> String {
    let renderer = annotate_snippets::Renderer::plain();
    renderer.render(groups).to_string()
}
