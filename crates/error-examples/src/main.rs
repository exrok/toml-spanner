#![allow(dead_code)]

use annotate_snippets::Renderer;
use toml_spanner::Arena;
use toml_spanner_macros::Toml;

fn error_to_snippet<'s>(
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

    let level = match error.kind() {
        toml_spanner::ErrorKind::UnexpectedKey { .. }
        | toml_spanner::ErrorKind::Deprecated { .. } => Level::WARNING,
        _ => Level::ERROR,
    };

    level.primary_title(message).element(snippet)
}

fn render(name: &str, groups: &[annotate_snippets::Group<'_>]) {
    let renderer = Renderer::styled();
    let ansi = renderer.render(groups).to_string();

    anstream::println!("── {name} ──");
    anstream::println!("{ansi}");

    let mut svg = anstyle_svg::Term::new().render_svg(&ansi);
    svg = svg.replace("#AAAAAA", "#bdbdbd");
    svg = svg.replace("#000000", "#0D1117");
    svg = svg.replace("#5555FF", "#85b4d7");
    svg = svg.replace("#FF5555", "#e36b67");
    svg = svg.replace("#AA5500", "#f5db7d");
    std::fs::create_dir_all("output").expect("failed to create output dir");
    std::fs::write(format!("output/{name}.svg"), svg).expect("failed to write svg");
}

fn parse_error(name: &str, path: &str, source: &str) {
    let arena = Arena::new();
    let Err(error) = toml_spanner::parse(source, &arena) else {
        panic!("{name}: expected parse error");
    };
    let group = error_to_snippet(&error, source, path);
    render(name, &[group]);
}

fn main() {
    unterminated_string();
    duplicate_key();
    deserialization_errors();
}

fn unterminated_string() {
    parse_error(
        "unterminated_string",
        "extask.toml",
        r#"[bindings.normal]
"j" = "Down"
"k" = "Up"
"G" = "Bottom
"C-u" = "UpHalfPage"
"C-d" = "DownHalfPage"
"#,
    );
}

fn duplicate_key() {
    parse_error(
        "duplicate_key",
        "extask.toml",
        r#"[bindings.normal]
"j" = "Down"
"k" = "Up"
"t" = { Set = [{ Due = "Tomorrow" }], Set = [{ Due = "Today" }] }
"d n" = { Set = [{ Due = "None" }] }
"#,
    );
}

fn deserialization_errors() {
    #[derive(Debug, Toml)]
    #[toml(FromToml, rename_all = "kebab-case")]
    enum Visibility {
        Visible,
        Hidden,
        UntilRan,
    }

    #[derive(Debug, Toml)]
    #[toml(FromToml, recoverable)]
    struct Service {
        port: Option<u16>,
        hidden: Option<Visibility>,
    }

    #[derive(Debug, Toml)]
    #[toml(FromToml)]
    struct Config {
        service: ServiceTable,
    }

    #[derive(Debug, Toml)]
    #[toml(FromToml)]
    struct ServiceTable {
        backend: Service,
    }

    let source = r#"[service.backend]
unknown = "key"
port = "https"
hidden = "collapsed"
"#;

    let arena = Arena::new();
    let mut doc = toml_spanner::parse(source, &arena).expect("should parse");
    let Err(e) = doc.to::<Config>() else {
        panic!("expected deserialization errors");
    };

    let groups: Vec<_> = e
        .errors
        .iter()
        .map(|e| error_to_snippet(e, source, "devsm.toml"))
        .collect();
    render("deserialization_errors", &groups);
}
