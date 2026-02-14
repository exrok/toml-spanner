use crate::ErrorKind;
use crate::table::Table;

struct TestCtx {
    arena: crate::arena::Arena,
}

impl TestCtx {
    fn new() -> Self {
        Self {
            arena: crate::arena::Arena::new(),
        }
    }

    fn parse_ok<'a>(&'a self, input: &'a str) -> Table<'a> {
        crate::parse(input, &self.arena)
            .unwrap_or_else(|e| panic!("parse failed for {input:?}: {e}"))
    }

    fn parse_err(&self, input: &str) -> crate::Error {
        crate::parse(input, &self.arena).unwrap_err()
    }
}

#[test]
fn basic_scalar_values() {
    let ctx = TestCtx::new();

    // empty document
    let v = ctx.parse_ok("");
    assert!(v.is_empty());

    // string
    let v = ctx.parse_ok("a = \"hello\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("hello"));

    // integer
    let v = ctx.parse_ok("a = 42");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(42));

    // negative integer
    let v = ctx.parse_ok("a = -100");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(-100));

    // float
    let v = ctx.parse_ok("a = 3.14");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 3.14).abs() < f64::EPSILON);

    // booleans
    let v = ctx.parse_ok("a = true");
    assert_eq!(v.get("a").unwrap().as_bool(), Some(true));
    let v = ctx.parse_ok("a = false");
    assert_eq!(v.get("a").unwrap().as_bool(), Some(false));

    // multiple keys
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3");
    assert_eq!(v.len(), 3);
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(v.get("c").unwrap().as_integer(), Some(3));
}

#[test]
fn string_escapes() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok(r#"a = "line1\nline2""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("line1\nline2"));

    let v = ctx.parse_ok(r#"a = "col1\tcol2""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("col1\tcol2"));

    let v = ctx.parse_ok(r#"a = "path\\to""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("path\\to"));

    let v = ctx.parse_ok(r#"a = "say \"hi\"""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("say \"hi\""));

    // unicode short \uXXXX
    let v = ctx.parse_ok(r#"a = "\u0041""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("A"));

    // unicode long \UXXXXXXXX
    let v = ctx.parse_ok(r#"a = "\U00000041""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("A"));
}

#[test]
fn string_types() {
    let ctx = TestCtx::new();

    // multiline basic
    let v = ctx.parse_ok("a = \"\"\"\nhello\nworld\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("hello\nworld"));

    // multiline literal
    let v = ctx.parse_ok("a = '''\nhello\nworld'''");
    assert_eq!(v.get("a").unwrap().as_str(), Some("hello\nworld"));

    // literal — no escape processing
    let v = ctx.parse_ok(r#"a = 'no\escape'"#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("no\\escape"));

    // empty string
    let v = ctx.parse_ok(r#"a = """#);
    assert_eq!(v.get("a").unwrap().as_str(), Some(""));
}

#[test]
fn number_formats() {
    let ctx = TestCtx::new();

    // hex, octal, binary
    let v = ctx.parse_ok("a = 0xDEAD");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0xDEAD));
    let v = ctx.parse_ok("a = 0o777");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0o777));
    let v = ctx.parse_ok("a = 0b1010");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0b1010));

    // special floats
    let v = ctx.parse_ok("a = inf");
    assert_eq!(v.get("a").unwrap().as_float(), Some(f64::INFINITY));
    let v = ctx.parse_ok("a = -inf");
    assert_eq!(v.get("a").unwrap().as_float(), Some(f64::NEG_INFINITY));
    let v = ctx.parse_ok("a = nan");
    assert!(v.get("a").unwrap().as_float().unwrap().is_nan());
    let v = ctx.parse_ok("a = -nan");
    assert!(v.get("a").unwrap().as_float().unwrap().is_nan());

    // exponent notation
    let v = ctx.parse_ok("a = 1e10");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1e10).abs() < 1.0);
    let v = ctx.parse_ok("a = 1.5E-3");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1.5e-3).abs() < 1e-10);

    // underscores
    let v = ctx.parse_ok("a = 1_000_000");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1_000_000));
    let v = ctx.parse_ok("a = 1_000.5");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1000.5).abs() < f64::EPSILON);
}

#[test]
fn arrays() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = [1, 2, 3]");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr.get(0).unwrap().as_integer(), Some(1));
    assert_eq!(arr.get(2).unwrap().as_integer(), Some(3));

    // empty
    let v = ctx.parse_ok("a = []");
    assert!(v.get("a").unwrap().as_array().unwrap().is_empty());

    // nested
    let v = ctx.parse_ok("a = [[1, 2], [3, 4]]");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr.get(0).unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn inline_tables() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = {x = 1, y = 2}");
    let t = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t.get("x").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("y").unwrap().as_integer(), Some(2));

    // empty
    let v = ctx.parse_ok("a = {}");
    assert!(v.get("a").unwrap().as_table().unwrap().is_empty());

    // nested
    let v = ctx.parse_ok("a = {b = {c = 1}}");
    let b = v
        .get("a")
        .unwrap()
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));

    // array of inline tables
    let v = ctx.parse_ok("a = [{x = 1}, {x = 2}]");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(
        arr.get(0)
            .unwrap()
            .as_table()
            .unwrap()
            .get("x")
            .unwrap()
            .as_integer(),
        Some(1)
    );
}

#[test]
fn table_headers_and_structure() {
    let ctx = TestCtx::new();

    // simple header
    let v = ctx.parse_ok("[table]\nkey = 1");
    let t = v.get("table").unwrap().as_table().unwrap();
    assert_eq!(t.get("key").unwrap().as_integer(), Some(1));

    // multiple headers
    let v = ctx.parse_ok("[a]\nx = 1\n[b]\ny = 2");
    assert_eq!(
        v.get("a")
            .unwrap()
            .as_table()
            .unwrap()
            .get("x")
            .unwrap()
            .as_integer(),
        Some(1)
    );
    assert_eq!(
        v.get("b")
            .unwrap()
            .as_table()
            .unwrap()
            .get("y")
            .unwrap()
            .as_integer(),
        Some(2)
    );

    // dotted header
    let v = ctx.parse_ok("[a.b.c]\nkey = 1");
    let c = v
        .get("a")
        .unwrap()
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap()
        .get("c")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(c.get("key").unwrap().as_integer(), Some(1));

    // dotted key-value
    let v = ctx.parse_ok("a.b.c = 1");
    let b = v
        .get("a")
        .unwrap()
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));

    // dotted key multiple
    let v = ctx.parse_ok("a.x = 1\na.y = 2");
    let a = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("x").unwrap().as_integer(), Some(1));
    assert_eq!(a.get("y").unwrap().as_integer(), Some(2));

    // array of tables
    let v = ctx.parse_ok("[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"");
    let arr = v.get("items").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(
        arr.get(0)
            .unwrap()
            .as_table()
            .unwrap()
            .get("name")
            .unwrap()
            .as_str(),
        Some("a")
    );
    assert_eq!(
        arr.get(1)
            .unwrap()
            .as_table()
            .unwrap()
            .get("name")
            .unwrap()
            .as_str(),
        Some("b")
    );

    // array of tables with subtable
    let v = ctx.parse_ok("[[fruit]]\nname = \"apple\"\n[fruit.physical]\ncolor = \"red\"");
    let fruit = v
        .get("fruit")
        .unwrap()
        .as_array()
        .unwrap()
        .get(0)
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(fruit.get("name").unwrap().as_str(), Some("apple"));
    assert_eq!(
        fruit
            .get("physical")
            .unwrap()
            .as_table()
            .unwrap()
            .get("color")
            .unwrap()
            .as_str(),
        Some("red")
    );

    // implicit table via header
    let v = ctx.parse_ok("[a.b]\nx = 1\n[a]\ny = 2");
    let a = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("y").unwrap().as_integer(), Some(2));
    assert_eq!(
        a.get("b")
            .unwrap()
            .as_table()
            .unwrap()
            .get("x")
            .unwrap()
            .as_integer(),
        Some(1)
    );
}

#[test]
fn table_indexing_thresholds() {
    let ctx = TestCtx::new();

    // 5 keys — linear scan
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5");
    assert_eq!(v.len(), 5);
    assert_eq!(v.get("e").unwrap().as_integer(), Some(5));

    // 6 keys — bulk index
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6");
    assert_eq!(v.len(), 6);
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(v.get("f").unwrap().as_integer(), Some(6));

    // 7 keys — incremental index
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7");
    assert_eq!(v.len(), 7);
    assert_eq!(v.get("g").unwrap().as_integer(), Some(7));

    // 20 keys
    let mut lines = Vec::new();
    for i in 0..20 {
        lines.push(format!("key{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = ctx.parse_ok(&input);
    assert_eq!(v.len(), 20);
    assert_eq!(v.get("key0").unwrap().as_integer(), Some(0));
    assert_eq!(v.get("key19").unwrap().as_integer(), Some(19));

    // subtable crossing threshold
    let mut lines = vec!["[sub]".to_string()];
    for i in 0..6 {
        lines.push(format!("k{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = ctx.parse_ok(&input);
    let sub = v.get("sub").unwrap().as_table().unwrap();
    assert_eq!(sub.len(), 6);
    assert_eq!(sub.get("k5").unwrap().as_integer(), Some(5));
}

#[test]
fn parse_errors() {
    let ctx = TestCtx::new();

    let e = ctx.parse_err("a = 1\na = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    let e = ctx.parse_err("a = \"unterminated");
    assert!(matches!(e.kind, ErrorKind::UnterminatedString));

    let e = ctx.parse_err(r#"a = "\z""#);
    assert!(matches!(e.kind, ErrorKind::InvalidEscape('z')));

    let e = ctx.parse_err("[t]\na = 1\n[t]\nb = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateTable { .. }));

    let e = ctx.parse_err("a = ");
    assert!(matches!(e.kind, ErrorKind::UnexpectedEof));

    let e = ctx.parse_err("a = 0x");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    let e = ctx.parse_err("a = 1\n[a]\nb = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. } | ErrorKind::DuplicateKey { .. }
    ));

    let e = ctx.parse_err("a = {x = 1}\n[a]\ny = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. } | ErrorKind::DuplicateKey { .. }
    ));
}

#[test]
fn quoted_keys_and_spans() {
    let ctx = TestCtx::new();

    // basic quoted key
    let v = ctx.parse_ok(r#""quoted key" = 1"#);
    assert_eq!(v.get("quoted key").unwrap().as_integer(), Some(1));

    // quoted key with escape
    let v = ctx.parse_ok(r#""key\nwith\nnewlines" = 1"#);
    assert_eq!(v.get("key\nwith\nnewlines").unwrap().as_integer(), Some(1));

    // literal quoted key
    let v = ctx.parse_ok("'literal key' = 1");
    assert_eq!(v.get("literal key").unwrap().as_integer(), Some(1));

    // span for integer value
    let input = "key = 42";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(&input[span.start as usize..span.end as usize], "42");

    // span for string value
    let input = "key = \"hello\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(&input[span.start as usize..span.end as usize], "hello");
}

#[test]
fn comments_and_whitespace() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("# comment\na = 1 # inline comment\n# another");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));

    let v = ctx.parse_ok("\n\n\na = 1\n\n\n");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));
}

#[test]
fn value_into_kind() {
    let ctx = TestCtx::new();
    let mut v = ctx.parse_ok("a = \"hello\"\nb = 42\nc = [1, 2]\nd = {x = 1}");

    let a = v.remove("a").unwrap();
    assert_eq!(a.as_str().unwrap(), "hello");

    let b = v.remove("b").unwrap();
    assert_eq!(b.as_integer().unwrap(), 42);

    let c = v.remove("c").unwrap();
    assert_eq!(c.as_array().unwrap().len(), 2);

    let d = v.remove("d").unwrap();
    assert_eq!(d.as_table().unwrap().len(), 1);
}

#[test]
fn recursion_depth_at_limit() {
    let depth = super::MAX_RECURSION_DEPTH as usize;
    let ctx = TestCtx::new();

    // Nested arrays: [[[...1...]]]
    let mut input = String::from("a = ");
    for _ in 0..depth {
        input.push('[');
    }
    input.push('1');
    for _ in 0..depth {
        input.push(']');
    }
    ctx.parse_ok(&input);

    // Nested inline tables: {b= {b= {b= ...1...}}}
    let mut input = String::from("a = ");
    for _ in 0..depth {
        input.push_str("{b= ");
    }
    input.push('1');
    for _ in 0..depth {
        input.push('}');
    }
    ctx.parse_ok(&input);

    // Mixed: [{b= [{b= ...1...}]}]
    let mut input = String::from("a = ");
    for _ in 0..depth / 2 {
        input.push_str("[{b= ");
    }
    input.push('1');
    for _ in 0..depth / 2 {
        input.push_str("}]");
    }
    ctx.parse_ok(&input);
}

#[test]
fn recursion_depth_over_limit() {
    let depth = super::MAX_RECURSION_DEPTH as usize;
    let ctx = TestCtx::new();

    // Nested arrays one past the limit
    let mut input = String::from("a = ");
    for _ in 0..=depth {
        input.push('[');
    }
    input.push('1');
    for _ in 0..=depth {
        input.push(']');
    }
    let e = ctx.parse_err(&input);
    assert!(matches!(
        e.kind,
        ErrorKind::OutOfRange("Max recursion depth exceeded")
    ));

    // Nested inline tables one past the limit
    let mut input = String::from("a = ");
    for _ in 0..=depth {
        input.push_str("{b= ");
    }
    input.push('1');
    for _ in 0..=depth {
        input.push('}');
    }
    let e = ctx.parse_err(&input);
    assert!(matches!(
        e.kind,
        ErrorKind::OutOfRange("Max recursion depth exceeded")
    ));

    // Mixed nesting one past the limit
    let mut input = String::from("a = ");
    for _ in 0..=depth / 2 {
        input.push_str("[{b= ");
    }
    input.push('1');
    for _ in 0..=depth / 2 {
        input.push_str("}]");
    }
    let e = ctx.parse_err(&input);
    assert!(matches!(
        e.kind,
        ErrorKind::OutOfRange("Max recursion depth exceeded")
    ));
}

#[test]
fn mixed_content() {
    let ctx = TestCtx::new();
    let input = r#"
title = "TOML Example"
enabled = true
count = 100
ratio = 0.5

[database]
server = "192.168.1.1"
ports = [8001, 8001, 8002]
enabled = true

[servers.alpha]
ip = "10.0.0.1"

[servers.beta]
ip = "10.0.0.2"

[[products]]
name = "Hammer"
sku = 738594937

[[products]]
name = "Nail"
sku = 284758393
"#;
    let v = ctx.parse_ok(input);
    assert_eq!(v.get("title").unwrap().as_str(), Some("TOML Example"));
    assert_eq!(v.get("count").unwrap().as_integer(), Some(100));

    let db = v.get("database").unwrap().as_table().unwrap();
    assert_eq!(db.get("ports").unwrap().as_array().unwrap().len(), 3);

    let servers = v.get("servers").unwrap().as_table().unwrap();
    let alpha = servers.get("alpha").unwrap().as_table().unwrap();
    assert_eq!(alpha.get("ip").unwrap().as_str(), Some("10.0.0.1"));

    let products = v.get("products").unwrap().as_array().unwrap();
    assert_eq!(products.len(), 2);
    let p0 = products.get(0).unwrap().as_table().unwrap();
    assert_eq!(p0.get("name").unwrap().as_str(), Some("Hammer"));
}

#[test]
fn utf8_bom_is_skipped() {
    let ctx = TestCtx::new();

    // BOM-only input -> empty table
    let v = ctx.parse_ok("\u{FEFF}");
    assert!(v.is_empty());

    // BOM followed by key-value
    let v = ctx.parse_ok("\u{FEFF}a = 1");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));

    // BOM followed by table header
    let v = ctx.parse_ok("\u{FEFF}[section]\nkey = \"val\"");
    let section = v.get("section").unwrap().as_table().unwrap();
    assert_eq!(section.get("key").unwrap().as_str(), Some("val"));
}

#[test]
fn crlf_handling() {
    let ctx = TestCtx::new();

    // CRLF line endings in key-value pairs
    let v = ctx.parse_ok("a = 1\r\nb = 2\r\n");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(v.get("b").unwrap().as_integer(), Some(2));

    // CRLF in multiline string (at start, stripped per spec)
    let v = ctx.parse_ok("a = \"\"\"\r\nhello\r\nworld\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("hello\r\nworld"));

    // CRLF after table header
    let v = ctx.parse_ok("[table]\r\nkey = 1\r\n");
    let t = v.get("table").unwrap().as_table().unwrap();
    assert_eq!(t.get("key").unwrap().as_integer(), Some(1));

    // bare CR in basic string is an error
    let e = ctx.parse_err("a = \"hello\rworld\"");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString('\r')));

    // backslash-CRLF continuation in multiline
    let v = ctx.parse_ok("a = \"\"\"\\\r\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // backslash-newline then CRLF blank lines in multiline
    let v = ctx.parse_ok("a = \"\"\"\\\n\r\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // CRLF at beginning of multiline basic string
    let v = ctx.parse_ok("a = \"\"\"\r\ncontent\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("content"));

    // CRLF inside multiline string body
    let v = ctx.parse_ok("a = \"\"\"\r\nline1\r\nline2\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("line1\r\nline2"));

    // CRLF in basic string (not multiline) is an error
    let e = ctx.parse_err("a = \"hello\r\nworld\"");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString('\n')));
}

#[test]
fn escape_sequences() {
    let ctx = TestCtx::new();

    // backspace and formfeed
    let v = ctx.parse_ok(r#"a = "\b\f""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("\x08\x0C"));

    // ESC character (TOML 1.1)
    let v = ctx.parse_ok(r#"a = "\e""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("\x1B"));

    // \x hex escape
    let v = ctx.parse_ok(r#"a = "\x41""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("A"));

    // carriage return
    let v = ctx.parse_ok(r#"a = "\r""#);
    assert_eq!(v.get("a").unwrap().as_str(), Some("\r"));

    // invalid hex digit in \u escape
    let e = ctx.parse_err(r#"a = "\uGGGG""#);
    assert!(matches!(e.kind, ErrorKind::InvalidHexEscape('G')));

    // invalid escape value (out of Unicode range)
    let e = ctx.parse_err(r#"a = "\UFFFFFFFF""#);
    assert!(matches!(e.kind, ErrorKind::InvalidEscapeValue(_)));

    // unterminated hex escape (string ends before all hex digits)
    let e = ctx.parse_err("a = \"\\u00");
    assert!(matches!(e.kind, ErrorKind::UnterminatedString));

    // invalid hex digit in \x escape
    let e = ctx.parse_err(r#"a = "\xGG""#);
    assert!(matches!(e.kind, ErrorKind::InvalidHexEscape('G')));
}

#[test]
fn multiline_string_edge_cases() {
    let ctx = TestCtx::new();

    // backslash-newline trims leading whitespace
    let v = ctx.parse_ok("a = \"\"\"\\\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // backslash-space-then-newline trims
    let v = ctx.parse_ok("a = \"\"\"\\  \n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // backslash-tab-then-newline trims
    let v = ctx.parse_ok("a = \"\"\"\\\t\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // backslash-CRLF trims
    let v = ctx.parse_ok("a = \"\"\"\\\r\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // backslash-newline across multiple blank lines trims
    let v = ctx.parse_ok("a = \"\"\"\\\n\n\n   trimmed\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("trimmed"));

    // up to 2 extra quotes allowed before closing """
    let v = ctx.parse_ok("a = \"\"\"content\"\"\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("content\"\""));

    // one extra quote before closing """
    let v = ctx.parse_ok("a = \"\"\"content\"\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("content\""));

    // single quote inside multiline
    let v = ctx.parse_ok("a = \"\"\"he said \"hi\" ok\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("he said \"hi\" ok"));

    // two quotes inside multiline
    let v = ctx.parse_ok("a = \"\"\"two \"\" here\"\"\"");
    assert_eq!(v.get("a").unwrap().as_str(), Some("two \"\" here"));

    // backslash-space without newline is an error
    let e = ctx.parse_err("a = \"\"\"\\  x\"\"\"");
    assert!(matches!(e.kind, ErrorKind::InvalidEscape(' ')));
}

#[test]
fn number_valid_edge_cases() {
    let ctx = TestCtx::new();

    // hex with underscores
    let v = ctx.parse_ok("a = 0xDEAD_BEEF");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0xDEAD_BEEF));

    // octal values
    let v = ctx.parse_ok("a = 0o755");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0o755));

    // binary with underscores
    let v = ctx.parse_ok("a = 0b1111_0000");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(0b1111_0000));

    // leading plus integer
    let v = ctx.parse_ok("a = +42");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(42));

    // leading plus float
    let v = ctx.parse_ok("a = +3.14");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 3.14).abs() < f64::EPSILON);

    // leading plus inf
    let v = ctx.parse_ok("a = +inf");
    assert_eq!(v.get("a").unwrap().as_float(), Some(f64::INFINITY));

    // leading plus nan
    let v = ctx.parse_ok("a = +nan");
    assert!(v.get("a").unwrap().as_float().unwrap().is_nan());
}

#[test]
fn number_format_errors() {
    let ctx = TestCtx::new();

    // plus on base prefix
    let e = ctx.parse_err("a = +0xFF");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // plus alone
    let e = ctx.parse_err("a = +");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // invalid octal digit
    let e = ctx.parse_err("a = 0o89");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // invalid binary digit
    let e = ctx.parse_err("a = 0b102");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // empty hex
    let e = ctx.parse_err("a = 0x");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // empty octal
    let e = ctx.parse_err("a = 0o");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // empty binary
    let e = ctx.parse_err("a = 0b");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // underscore at start
    let e = ctx.parse_err("a = 0x_FF");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // trailing underscore hex
    let e = ctx.parse_err("a = 0xFF_");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // trailing underscore octal
    let e = ctx.parse_err("a = 0o77_");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // trailing underscore binary
    let e = ctx.parse_err("a = 0b11_");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // double underscore hex
    let e = ctx.parse_err("a = 0xF__F");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // double underscore octal
    let e = ctx.parse_err("a = 0o7__7");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // double underscore binary
    let e = ctx.parse_err("a = 0b1__0");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // decimal leading zero
    let e = ctx.parse_err("a = 01");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // decimal trailing underscore
    let e = ctx.parse_err("a = 123_");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // decimal double underscore
    let e = ctx.parse_err("a = 1__2");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));
}

#[test]
fn float_valid_edge_cases() {
    let ctx = TestCtx::new();

    // exponent only (no decimal)
    let v = ctx.parse_ok("a = 5e2");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 500.0).abs() < 0.01);

    // negative exponent
    let v = ctx.parse_ok("a = -1.5e-3");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - (-1.5e-3)).abs() < 1e-10);

    // underscore in decimal part
    let v = ctx.parse_ok("a = 1_000.5_00");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1000.5).abs() < f64::EPSILON);

    // exponent with explicit plus
    let v = ctx.parse_ok("a = 1e+5");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1e5).abs() < 0.01);

    // decimal with exponent and explicit plus
    let v = ctx.parse_ok("a = 1.5E+3");
    let f = v.get("a").unwrap().as_float().unwrap();
    assert!((f - 1.5e3).abs() < 0.01);
}

#[test]
fn float_format_errors() {
    let ctx = TestCtx::new();

    // leading zero
    let e = ctx.parse_err("a = 00.5");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // dot with nothing after
    let e = ctx.parse_err("a = 1.");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // bad underscore in decimal part
    let e = ctx.parse_err("a = 1.5_");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // negative leading zero
    let e = ctx.parse_err("a = -00.5");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // decimal starts with underscore
    let e = ctx.parse_err("a = 1._5");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // exponent with no digits after
    let e = ctx.parse_err("a = 1e\nb = 2");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));
}

#[test]
fn structural_errors() {
    let ctx = TestCtx::new();

    // dotted key on non-table value
    let e = ctx.parse_err("a = 1\na.b = 2");
    assert!(matches!(e.kind, ErrorKind::DottedKeyInvalidType { .. }));

    // dotted key on frozen inline table
    let e = ctx.parse_err("a = {b = 1}\na.c = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. }
            | ErrorKind::DuplicateKey { .. }
            | ErrorKind::DottedKeyInvalidType { .. }
    ));

    // table then array of tables
    let e = ctx.parse_err("[a]\nb = 1\n[[a]]");
    assert!(matches!(e.kind, ErrorKind::RedefineAsArray));

    // duplicate table header
    let e = ctx.parse_err("[a]\nb = 1\n[a]\nc = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateTable { .. }));

    // multiline basic string as key
    let e = ctx.parse_err("\"\"\"key\"\"\" = 1");
    assert!(matches!(e.kind, ErrorKind::MultilineStringKey));

    // multiline literal string as key
    let e = ctx.parse_err("'''key''' = 1");
    assert!(matches!(e.kind, ErrorKind::MultilineStringKey));

    // unquoted string value
    let e = ctx.parse_err("a = not_a_keyword");
    assert!(matches!(e.kind, ErrorKind::UnquotedString));

    // missing value at EOF
    let e = ctx.parse_err("a = ");
    assert!(matches!(e.kind, ErrorKind::UnexpectedEof));

    // missing equals sign
    let e = ctx.parse_err("key 1");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // newline in basic string
    let e = ctx.parse_err("a = \"line1\nline2\"");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString('\n')));

    // newline in literal string
    let e = ctx.parse_err("a = 'line1\nline2'");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString('\n')));

    // DEL character in string
    let e = ctx.parse_err("a = \"hello\x7Fworld\"");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString(_)));
}

#[test]
fn inline_table_edge_cases() {
    let ctx = TestCtx::new();

    // dotted key inside inline table
    let v = ctx.parse_ok("a = {b.c = 1}");
    let b = v
        .get("a")
        .unwrap()
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));

    // trailing comma (TOML 1.1)
    let v = ctx.parse_ok("a = {x = 1, y = 2,}");
    let t = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(t.len(), 2);

    // newlines inside inline table (TOML 1.1)
    let v = ctx.parse_ok("a = {\n  x = 1,\n  y = 2\n}");
    let t = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(t.len(), 2);

    // comments inside inline table
    let v = ctx.parse_ok("a = {\n  x = 1, # comment\n  y = 2\n}");
    let t = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(t.len(), 2);
}

#[test]
fn array_edge_cases() {
    let ctx = TestCtx::new();

    // array with comments
    let v = ctx.parse_ok("a = [\n  1, # first\n  2, # second\n  3\n]");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // trailing comma
    let v = ctx.parse_ok("a = [1, 2, 3,]");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn dotted_key_features() {
    let ctx = TestCtx::new();

    // deep dotted key
    let v = ctx.parse_ok("a.b.c.d = 1");
    let d = v
        .get("a")
        .unwrap()
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap()
        .get("c")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(d.get("d").unwrap().as_integer(), Some(1));

    // extend existing table via dotted key
    let v = ctx.parse_ok("a.b = 1\na.c = 2");
    let a = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("b").unwrap().as_integer(), Some(1));
    assert_eq!(a.get("c").unwrap().as_integer(), Some(2));

    // table header then dotted key
    let v = ctx.parse_ok("[a]\nb.c = 1\nb.d = 2");
    let a = v.get("a").unwrap().as_table().unwrap();
    let b = a.get("b").unwrap().as_table().unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));
    assert_eq!(b.get("d").unwrap().as_integer(), Some(2));
}

#[test]
fn aot_and_implicit_tables() {
    let ctx = TestCtx::new();

    // array of tables with subtable
    let v = ctx.parse_ok("[[a]]\nb = 1\n[a.c]\nd = 2\n[[a]]\nb = 3");
    let arr = v.get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let first = arr.get(0).unwrap().as_table().unwrap();
    assert_eq!(first.get("b").unwrap().as_integer(), Some(1));
    let c = first.get("c").unwrap().as_table().unwrap();
    assert_eq!(c.get("d").unwrap().as_integer(), Some(2));

    // navigate header intermediate on array of tables
    let v = ctx.parse_ok("[[a]]\nb = 1\n[a.c]\nd = 2");
    let arr = v.get("a").unwrap().as_array().unwrap();
    let first = arr.get(0).unwrap().as_table().unwrap();
    assert_eq!(first.get("b").unwrap().as_integer(), Some(1));
    let c = first.get("c").unwrap().as_table().unwrap();
    assert_eq!(c.get("d").unwrap().as_integer(), Some(2));

    // implicit table then explicit define
    let v = ctx.parse_ok("[a.b]\nc = 1\n[a]\nd = 2");
    let a = v.get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("d").unwrap().as_integer(), Some(2));
    let b = a.get("b").unwrap().as_table().unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));
}

#[test]
fn integer_boundary_values() {
    let ctx = TestCtx::new();

    // minimum i64
    let v = ctx.parse_ok("a = -9223372036854775808");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(i64::MIN));

    // maximum i64
    let v = ctx.parse_ok("a = 9223372036854775807");
    assert_eq!(v.get("a").unwrap().as_integer(), Some(i64::MAX));
}

#[test]
fn integer_overflow_errors() {
    let ctx = TestCtx::new();

    // hex overflow
    let e = ctx.parse_err("a = 0xFFFFFFFFFFFFFFFF");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // octal overflow
    let e = ctx.parse_err("a = 0o7777777777777777777777");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // binary overflow
    let e = ctx.parse_err("a = 0b1111111111111111111111111111111111111111111111111111111111111111");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // decimal overflow
    let e = ctx.parse_err("a = 99999999999999999999");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));
}

#[test]
fn more_parse_errors() {
    let ctx = TestCtx::new();

    // inline table duplicate key
    let e = ctx.parse_err("a = {x = 1, x = 2}");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    // dotted key duplicate in table
    let e = ctx.parse_err("[a]\nb = 1\nb = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    // expected value found ]
    let e = ctx.parse_err("a = ]");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // expected value found }
    let e = ctx.parse_err("a = }");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // dash-leading invalid number
    let e = ctx.parse_err("a = -_bad");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // EOF in key position
    let e = ctx.parse_err("[");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // invalid key character (= with no key)
    let e = ctx.parse_err("= 1");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // bare CR at document level
    let e = ctx.parse_err("a = 1\r");
    assert!(matches!(
        e.kind,
        ErrorKind::Wanted { .. } | ErrorKind::Unexpected('\r')
    ));

    // missing closing bracket in table header
    let e = ctx.parse_err("[table\na = 1");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // missing closing brace in inline table
    let e = ctx.parse_err("a = {x = 1");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // missing closing bracket in array
    let e = ctx.parse_err("a = [1, 2");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // junk after value on same line
    let e = ctx.parse_err("a = 1 b = 2");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // dotted key in inline table on frozen table
    let e = ctx.parse_err("a = {b = 1}\na.b.c = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateKey { .. } | ErrorKind::DottedKeyInvalidType { .. }
    ));

    // header intermediate on scalar
    let e = ctx.parse_err("a = 1\n[a.b]");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    // dotted key conflicts with header
    let e = ctx.parse_err("[a]\nb = 1\n[a.b]\nc = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DottedKeyInvalidType { .. } | ErrorKind::DuplicateKey { .. }
    ));

    // redefine dotted as header
    let e = ctx.parse_err("a.b = 1\n[a.b]\nc = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateKey { .. } | ErrorKind::DuplicateTable { .. }
    ));

    // header on frozen inline table
    let e = ctx.parse_err("a = {b = 1}\n[a.c]");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    // array-of-tables on existing non-table
    let e = ctx.parse_err("a = 1\n[[a]]");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));

    // dotted header on dotted table
    let e = ctx.parse_err("a.b = 1\n[a]\nc = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. } | ErrorKind::DuplicateKey { .. }
    ));

    // unterminated quoted key
    let e = ctx.parse_err(r#""unterminated = 1"#);
    assert!(matches!(e.kind, ErrorKind::UnterminatedString));

    // unterminated literal key
    let e = ctx.parse_err("'unterminated = 1");
    assert!(matches!(e.kind, ErrorKind::UnterminatedString));

    // expected key found equals
    let e = ctx.parse_err("= 1");
    assert!(matches!(
        e.kind,
        ErrorKind::Wanted {
            expected: "a table key",
            ..
        }
    ));

    // expected key found comma
    let e = ctx.parse_err("[a]\n, = 1");
    assert!(matches!(e.kind, ErrorKind::Wanted { .. }));

    // CRLF in basic string (not multiline) is an error
    let e = ctx.parse_err("a = \"hello\r\nworld\"");
    assert!(matches!(e.kind, ErrorKind::InvalidCharInString('\n')));
}
