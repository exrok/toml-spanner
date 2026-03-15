#![allow(dead_code)]

use annotate_snippets::Renderer;
use toml_spanner::Arena;
use toml_spanner_macros::Toml;

fn render(name: &str, groups: &[annotate_snippets::Group<'_>]) {
    let renderer = Renderer::styled();
    let ansi = renderer.render(groups).to_string();

    anstream::println!("── {name} ──");
    anstream::println!("{ansi}");

    let mut svg = anstyle_svg::Term::new().render_svg(&ansi);
    svg = svg.replace("#000000", "#111111");
    svg = svg.replace("#5555FF", "#92B2CA");
    svg = svg.replace("#FF5555", "#D77C79");
    std::fs::create_dir_all("output").expect("failed to create output dir");
    std::fs::write(format!("output/{name}.svg"), svg).expect("failed to write svg");
}

fn parse_error(name: &str, path: &str, source: &str) {
    let arena = Arena::new();
    let Err(error) = toml_spanner::parse(source, &arena) else {
        panic!("{name}: expected parse error");
    };
    let group = error.to_snippet(source, path);
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
"d t" = { Set = [{ Due = "Tomorrow" }], Set = [{ Due = "Today" }] }
"d n" = { Set = [{ Due = "None" }] }
"#,
    );
}

fn deserialization_errors() {
    use std::net::IpAddr;
    use toml_spanner::helper::parse_string;

    #[derive(Debug, Toml)]
    #[toml(FromToml, rename_all = "kebab-case")]
    enum Visibility {
        Visible,
        Hidden,
        UntilRan,
    }

    #[derive(Debug, Toml)]
    #[toml(FromToml, deny_unknown_fields)]
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
        .map(|e| e.to_snippet(source, "devsm.toml"))
        .collect();
    render("deserialization_errors", &groups);
}
