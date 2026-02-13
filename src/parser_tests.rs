use crate::{ErrorKind, Table};

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
    assert_eq!(&input[span.start() as usize..span.end() as usize], "42");

    // span for string value
    let input = "key = \"hello\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(&input[span.start() as usize..span.end() as usize], "hello");
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

    // BOM-only input → empty table
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
