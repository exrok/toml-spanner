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
        match crate::parse(input, &self.arena) {
            Ok(value) => panic!(
                "For input `{input}` expected error but parsed successfully: {:?}",
                value
            ),
            Err(err) => err,
        }
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
    assert_eq!(v["a"].as_str(), Some("hello"));

    // integer
    let v = ctx.parse_ok("a = 42");
    assert_eq!(v["a"].as_i64(), Some(42));

    // negative integer
    let v = ctx.parse_ok("a = -100");
    assert_eq!(v["a"].as_i64(), Some(-100));

    // float
    let v = ctx.parse_ok("a = 3.14");
    let f = v["a"].as_f64().unwrap();
    assert!((f - 3.14).abs() < f64::EPSILON);

    // booleans
    let v = ctx.parse_ok("a = true");
    assert_eq!(v["a"].as_bool(), Some(true));
    let v = ctx.parse_ok("a = false");
    assert_eq!(v["a"].as_bool(), Some(false));

    // multiple keys
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3");
    assert_eq!(v.len(), 3);
    assert_eq!(v["a"].as_i64(), Some(1));
    assert_eq!(v["c"].as_i64(), Some(3));
}

#[test]
fn string_escapes() {
    let ctx = TestCtx::new();

    let cases = [
        (r#"a = "line1\nline2""#, "line1\nline2"),
        (r#"a = "col1\tcol2""#, "col1\tcol2"),
        (r#"a = "path\\to""#, "path\\to"),
        (r#"a = "say \"hi\"""#, "say \"hi\""),
        (r#"a = "\u0041""#, "A"),
        (r#"a = "\U00000041""#, "A"),
    ];

    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }
}

#[test]
fn string_types() {
    let ctx = TestCtx::new();

    let cases = [
        ("a = \"\"\"\nhello\nworld\"\"\"", "hello\nworld"),
        ("a = '''\nhello\nworld'''", "hello\nworld"),
        (r#"a = 'no\escape'"#, "no\\escape"),
        (r#"a = """#, ""),
    ];

    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }
}

#[test]
fn number_formats() {
    let ctx = TestCtx::new();

    let int_cases = [
        ("a = 0xDEAD", 0xDEAD),
        ("a = 0o777", 0o777),
        ("a = 0b1010", 0b1010),
        ("a = 1_000_000", 1_000_000),
    ];

    for (input, expected) in int_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }

    let float_cases = [
        ("a = inf", f64::INFINITY, false),
        ("a = -inf", f64::NEG_INFINITY, false),
        ("a = nan", f64::NAN, true),
        ("a = -nan", f64::NAN, true),
    ];

    for (input, expected, is_nan) in float_cases {
        let v = ctx.parse_ok(input);
        let f = v["a"].as_f64().unwrap();
        if is_nan {
            assert!(f.is_nan(), "input: {input}");
        } else {
            assert_eq!(f, expected, "input: {input}");
        }
    }

    let float_approx_cases = [
        ("a = 1e10", 1e10, 1.0),
        ("a = 1.5E-3", 1.5e-3, 1e-10),
        ("a = 1_000.5", 1000.5, f64::EPSILON),
    ];

    for (input, expected, epsilon) in float_approx_cases {
        let v = ctx.parse_ok(input);
        let f = v["a"].as_f64().unwrap();
        assert!((f - expected).abs() < epsilon, "input: {input}");
    }
}

#[test]
fn arrays() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = [1, 2, 3]");
    assert_eq!(v["a"].as_array().unwrap().len(), 3);
    assert_eq!(v["a"][0].as_i64(), Some(1));
    assert_eq!(v["a"][2].as_i64(), Some(3));

    // empty
    let v = ctx.parse_ok("a = []");
    assert!(v["a"].as_array().unwrap().is_empty());

    // nested
    let v = ctx.parse_ok("a = [[1, 2], [3, 4]]");
    assert_eq!(v["a"].as_array().unwrap().len(), 2);
    assert_eq!(v["a"][0].as_array().unwrap().len(), 2);
}

#[test]
fn split_keys_error() {
    let ctx = TestCtx::new();

    ctx.parse_err("a.\nb = 1");
    ctx.parse_err("a\n.b = 1");
    ctx.parse_err("a\n.\nb = 1");
    ctx.parse_err("[a\n.\nb]\nc = 1");
    ctx.parse_err("[a\n.b]\nc = 1");
    ctx.parse_err("[a.\nb]\nc = 1");
    ctx.parse_err("a={a\n.b=1}");
    ctx.parse_err("a={a.\nb=1}");
    ctx.parse_err("a={a\n.\nb=1}");
}

#[test]
fn inline_tables() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = {x = 1, y = 2}");
    assert_eq!(v["a"].as_table().unwrap().len(), 2);
    assert_eq!(v["a"]["x"].as_i64(), Some(1));
    assert_eq!(v["a"]["y"].as_i64(), Some(2));

    // empty
    let v = ctx.parse_ok("a = {}");
    assert!(v["a"].as_table().unwrap().is_empty());

    // nested
    let v = ctx.parse_ok("a = {b = {c = 1}}");
    assert_eq!(v["a"]["b"]["c"].as_i64(), Some(1));

    // array of inline tables
    let v = ctx.parse_ok("a = [{x = 1}, {x = 2}]");
    assert_eq!(v["a"].as_array().unwrap().len(), 2);
    assert_eq!(v["a"][0]["x"].as_i64(), Some(1));
}

#[test]
fn table_headers_and_structure() {
    let ctx = TestCtx::new();

    // simple header
    let v = ctx.parse_ok("[table]\nkey = 1");
    assert_eq!(v["table"]["key"].as_i64(), Some(1));

    // multiple headers
    let v = ctx.parse_ok("[a]\nx = 1\n[b]\ny = 2");
    assert_eq!(v["a"]["x"].as_i64(), Some(1));
    assert_eq!(v["b"]["y"].as_i64(), Some(2));

    // dotted header
    let v = ctx.parse_ok("[a.b.c]\nkey = 1");
    assert_eq!(v["a"]["b"]["c"]["key"].as_i64(), Some(1));

    // dotted key-value
    let v = ctx.parse_ok("a.b.c = 1");
    assert_eq!(v["a"]["b"]["c"].as_i64(), Some(1));

    // dotted key multiple
    let v = ctx.parse_ok("a.x = 1\na.y = 2");
    assert_eq!(v["a"]["x"].as_i64(), Some(1));
    assert_eq!(v["a"]["y"].as_i64(), Some(2));

    // array of tables
    let v = ctx.parse_ok("[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"");
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["items"][0]["name"].as_str(), Some("a"));
    assert_eq!(v["items"][1]["name"].as_str(), Some("b"));

    // array of tables with subtable
    let v = ctx.parse_ok("[[fruit]]\nname = \"apple\"\n[fruit.physical]\ncolor = \"red\"");
    assert_eq!(v["fruit"][0]["name"].as_str(), Some("apple"));
    assert_eq!(v["fruit"][0]["physical"]["color"].as_str(), Some("red"));

    // implicit table via header
    let v = ctx.parse_ok("[a.b]\nx = 1\n[a]\ny = 2");
    assert_eq!(v["a"]["y"].as_i64(), Some(2));
    assert_eq!(v["a"]["b"]["x"].as_i64(), Some(1));
}

#[test]
fn table_indexing_thresholds() {
    let ctx = TestCtx::new();

    // 5 keys — linear scan
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5");
    assert_eq!(v.len(), 5);
    assert_eq!(v["e"].as_i64(), Some(5));

    // 6 keys — bulk index
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6");
    assert_eq!(v.len(), 6);
    assert_eq!(v["a"].as_i64(), Some(1));
    assert_eq!(v["f"].as_i64(), Some(6));

    // 7 keys — incremental index
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7");
    assert_eq!(v.len(), 7);
    assert_eq!(v["g"].as_i64(), Some(7));

    // 20 keys
    let mut lines = Vec::new();
    for i in 0..20 {
        lines.push(format!("key{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = ctx.parse_ok(&input);
    assert_eq!(v.len(), 20);
    assert_eq!(v["key0"].as_i64(), Some(0));
    assert_eq!(v["key19"].as_i64(), Some(19));

    // subtable crossing threshold
    let mut lines = vec!["[sub]".to_string()];
    for i in 0..6 {
        lines.push(format!("k{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = ctx.parse_ok(&input);
    assert_eq!(v["sub"].as_table().unwrap().len(), 6);
    assert_eq!(v["sub"]["k5"].as_i64(), Some(5));
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
    assert_eq!(v["quoted key"].as_i64(), Some(1));

    // quoted key with escape
    let v = ctx.parse_ok(r#""key\nwith\nnewlines" = 1"#);
    assert_eq!(v["key\nwith\nnewlines"].as_i64(), Some(1));

    // literal quoted key
    let v = ctx.parse_ok("'literal key' = 1");
    assert_eq!(v["literal key"].as_i64(), Some(1));

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
    assert_eq!(v["a"].as_i64(), Some(1));

    let v = ctx.parse_ok("\n\n\na = 1\n\n\n");
    assert_eq!(v["a"].as_i64(), Some(1));
}

#[test]
fn value_into_kind() {
    let ctx = TestCtx::new();
    let mut v = ctx.parse_ok("a = \"hello\"\nb = 42\nc = [1, 2]\nd = {x = 1}");

    let a = v.remove("a").unwrap();
    assert_eq!(a.as_str().unwrap(), "hello");

    let b = v.remove("b").unwrap();
    assert_eq!(b.as_i64().unwrap(), 42);

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
    assert_eq!(v["title"].as_str(), Some("TOML Example"));
    assert_eq!(v["count"].as_i64(), Some(100));
    assert_eq!(v["database"]["ports"].as_array().unwrap().len(), 3);
    assert_eq!(v["servers"]["alpha"]["ip"].as_str(), Some("10.0.0.1"));
    assert_eq!(v["products"].as_array().unwrap().len(), 2);
    assert_eq!(v["products"][0]["name"].as_str(), Some("Hammer"));
}

#[test]
fn utf8_bom_is_skipped() {
    let ctx = TestCtx::new();

    // BOM-only input -> empty table
    let v = ctx.parse_ok("\u{FEFF}");
    assert!(v.is_empty());

    // BOM followed by key-value
    let v = ctx.parse_ok("\u{FEFF}a = 1");
    assert_eq!(v["a"].as_i64(), Some(1));

    // BOM followed by table header
    let v = ctx.parse_ok("\u{FEFF}[section]\nkey = \"val\"");
    assert_eq!(v["section"]["key"].as_str(), Some("val"));
}

#[test]
fn crlf_handling() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = 1\r\nb = 2\r\n");
    assert_eq!(v["a"].as_i64(), Some(1), "input: a = 1\\r\\nb = 2\\r\\n");
    assert_eq!(v["b"].as_i64(), Some(2), "input: a = 1\\r\\nb = 2\\r\\n");

    let valid_str_cases = [
        ("a = \"\"\"\r\nhello\r\nworld\"\"\"", "hello\r\nworld"),
        ("a = \"\"\"\\\r\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\n\r\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\r\ncontent\"\"\"", "content"),
        ("a = \"\"\"\r\nline1\r\nline2\"\"\"", "line1\r\nline2"),
    ];

    for (input, expected) in valid_str_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }

    let v = ctx.parse_ok("[table]\r\nkey = 1\r\n");
    assert_eq!(
        v["table"]["key"].as_i64(),
        Some(1),
        "input: [table]\\r\\nkey = 1\\r\\n"
    );

    let e = ctx.parse_err("a = \"hello\rworld\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidCharInString('\r')),
        "input: a = \"hello\\rworld\""
    );

    let e = ctx.parse_err("a = \"hello\r\nworld\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidCharInString('\n')),
        "input: a = \"hello\\r\\nworld\""
    );

    // Bare CR in array
    let e = ctx.parse_err("a = [ \r ]");
    assert!(
        matches!(e.kind, ErrorKind::Unexpected('\r')),
        "bare CR in array: {:?}",
        e.kind,
    );

    // Bare CR in inline table (hits read_table_key, reported as "a carriage return")
    let e = ctx.parse_err("a = { \r }");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                found: "a carriage return",
            }
        ),
        "bare CR in inline table: {:?}",
        e.kind,
    );

    // Bare CR after key-value (via eat_newline_or_eof)
    let e = ctx.parse_err("a = 1\r");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "newline",
                found: "a carriage return",
            }
        ),
        "bare CR after key-value: {:?}",
        e.kind,
    );

    // Bare CR after table header (via eat_newline_or_eof)
    let e = ctx.parse_err("[a]\r");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "newline",
                found: "a carriage return",
            }
        ),
        "bare CR after table header: {:?}",
        e.kind,
    );
}

#[test]
fn escape_sequences() {
    let ctx = TestCtx::new();

    let valid_cases = [
        (r#"a = "\b\f""#, "\x08\x0C"),
        (r#"a = "\e""#, "\x1B"),
        (r#"a = "\x41""#, "A"),
        (r#"a = "\r""#, "\r"),
    ];

    for (input, expected) in valid_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }

    let e = ctx.parse_err(r#"a = "\uGGGG""#);
    assert!(
        matches!(e.kind, ErrorKind::InvalidHexEscape('G')),
        "input: a = \"\\uGGGG\""
    );

    let e = ctx.parse_err(r#"a = "\UFFFFFFFF""#);
    assert!(
        matches!(e.kind, ErrorKind::InvalidEscapeValue(_)),
        "input: a = \"\\UFFFFFFFF\""
    );

    let e = ctx.parse_err("a = \"\\u00");
    assert!(
        matches!(e.kind, ErrorKind::UnterminatedString),
        "input: a = \"\\u00"
    );

    let e = ctx.parse_err(r#"a = "\xGG""#);
    assert!(
        matches!(e.kind, ErrorKind::InvalidHexEscape('G')),
        "input: a = \"\\xGG\""
    );
}

#[test]
fn multiline_string_edge_cases() {
    let ctx = TestCtx::new();

    let valid_cases = [
        ("a = \"\"\"\\\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\  \n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\t\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\r\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\n\n\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"content\"\"\"\"\"", "content\"\""),
        ("a = \"\"\"content\"\"\"\"", "content\""),
        ("a = \"\"\"he said \"hi\" ok\"\"\"", "he said \"hi\" ok"),
        ("a = \"\"\"two \"\" here\"\"\"", "two \"\" here"),
    ];

    for (input, expected) in valid_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }

    let e = ctx.parse_err("a = \"\"\"\\  x\"\"\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidEscape(' ')),
        "input: a = \"\"\"\\  x\"\"\""
    );

    // Bare CR after line-ending backslash is invalid (only space/tab are
    // valid whitespace before the newline per the TOML ABNF: mlb-escaped-nl
    // = escape ws newline, where ws = *wschar and wschar = SP / HTAB).
    let e = ctx.parse_err("a = \"\"\"\\\r\r\n\"\"\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidCharInString('\r')),
        "bare CR after line-ending backslash: {:?}",
        e.kind,
    );
}

#[test]
fn number_valid_edge_cases() {
    let ctx = TestCtx::new();

    let int_cases = [
        ("a = 0xDEAD_BEEF", 0xDEAD_BEEF),
        ("a = 0o755", 0o755),
        ("a = 0b1111_0000", 0b1111_0000),
        ("a = +42", 42),
    ];

    for (input, expected) in int_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }

    let v = ctx.parse_ok("a = +3.14");
    let f = v["a"].as_f64().unwrap();
    assert!((f - 3.14).abs() < f64::EPSILON, "input: a = +3.14");

    let v = ctx.parse_ok("a = +inf");
    assert_eq!(v["a"].as_f64(), Some(f64::INFINITY), "input: a = +inf");

    let v = ctx.parse_ok("a = +nan");
    assert!(v["a"].as_f64().unwrap().is_nan(), "input: a = +nan");
}

#[test]
fn number_format_errors() {
    let ctx = TestCtx::new();

    let error_cases = [
        "a = +0xFF",
        "a = +",
        "a = 0o89",
        "a = 0b102",
        "a = 0x",
        "a = 0o",
        "a = 0b",
        "a = 0x_FF",
        "a = 0xFF_",
        "a = 0o77_",
        "a = 0b11_",
        "a = 0xF__F",
        "a = 0o7__7",
        "a = 0b1__0",
        "a = 01",
        "a = 123_",
        "a = 1__2",
    ];

    for input in error_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }
}

#[test]
fn float_valid_edge_cases() {
    let ctx = TestCtx::new();

    let cases = [
        ("a = 5e2", 500.0, 0.01),
        ("a = -1.5e-3", -1.5e-3, 1e-10),
        ("a = 1_000.5_00", 1000.5, f64::EPSILON),
        ("a = 1e+5", 1e5, 0.01),
        ("a = 1.5E+3", 1.5e3, 0.01),
    ];

    for (input, expected, epsilon) in cases {
        let v = ctx.parse_ok(input);
        let f = v["a"].as_f64().unwrap();
        assert!((f - expected).abs() < epsilon, "input: {input}");
    }
}

#[test]
fn float_format_errors() {
    let ctx = TestCtx::new();

    let error_cases = [
        "a = 00.5",
        "a = 1.",
        "a = .1",
        "a = 1.5_",
        "a = --1.0",
        "a = --1",
        "a = -+1",
        "a = +-1",
        "a = +-1.0",
        "a = +-1E",
        "a = --1E",
        "a = ++1E",
        "a = -00.5",
        "a = 50E+-1",
        "a = 50E-+1",
        "a = 1._5",
        "a = 1e\nb = 2",
    ];

    for input in error_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }
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
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

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

    let v = ctx.parse_ok("a = {b.c = 1}");
    assert_eq!(v["a"]["b"]["c"].as_i64(), Some(1), "input: a = {{b.c = 1}}");

    let table_len_cases = [
        ("a = {x = 1, y = 2,}", 2),
        ("a = {\n  x = 1,\n  y = 2\n}", 2),
        ("a = {\n  x = 1, # comment\n  y = 2\n}", 2),
    ];

    for (input, expected_len) in table_len_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(
            v["a"].as_table().unwrap().len(),
            expected_len,
            "input: {input}"
        );
    }
}

#[test]
fn array_edge_cases() {
    let ctx = TestCtx::new();

    let cases = [
        ("a = [\n  1, # first\n  2, # second\n  3\n]", 3),
        ("a = [1, 2, 3,]", 3),
    ];

    for (input, expected_len) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(
            v["a"].as_array().unwrap().len(),
            expected_len,
            "input: {input}"
        );
    }
}

#[test]
fn dotted_key_features() {
    let ctx = TestCtx::new();

    // deep dotted key
    let v = ctx.parse_ok("a.b.c.d = 1");
    assert_eq!(v["a"]["b"]["c"]["d"].as_i64(), Some(1));

    // extend existing table via dotted key
    let v = ctx.parse_ok("a.b = 1\na.c = 2");
    assert_eq!(v["a"]["b"].as_i64(), Some(1));
    assert_eq!(v["a"]["c"].as_i64(), Some(2));

    // table header then dotted key
    let v = ctx.parse_ok("[a]\nb.c = 1\nb.d = 2");
    assert_eq!(v["a"]["b"]["c"].as_i64(), Some(1));
    assert_eq!(v["a"]["b"]["d"].as_i64(), Some(2));
}

#[test]
fn aot_and_implicit_tables() {
    let ctx = TestCtx::new();

    // array of tables with subtable
    let v = ctx.parse_ok("[[a]]\nb = 1\n[a.c]\nd = 2\n[[a]]\nb = 3");
    assert_eq!(v["a"].as_array().unwrap().len(), 2);
    assert_eq!(v["a"][0]["b"].as_i64(), Some(1));
    assert_eq!(v["a"][0]["c"]["d"].as_i64(), Some(2));

    // navigate header intermediate on array of tables
    let v = ctx.parse_ok("[[a]]\nb = 1\n[a.c]\nd = 2");
    assert_eq!(v["a"][0]["b"].as_i64(), Some(1));
    assert_eq!(v["a"][0]["c"]["d"].as_i64(), Some(2));

    // implicit table then explicit define
    let v = ctx.parse_ok("[a.b]\nc = 1\n[a]\nd = 2");
    assert_eq!(v["a"]["d"].as_i64(), Some(2));
    assert_eq!(v["a"]["b"]["c"].as_i64(), Some(1));
}

#[test]
fn integer_boundary_values() {
    let ctx = TestCtx::new();

    let cases = [
        ("a = -9223372036854775808", i64::MIN),
        ("a = 9223372036854775807", i64::MAX),
    ];

    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }
}

#[test]
fn integer_overflow_errors() {
    let ctx = TestCtx::new();

    let error_cases = [
        "a = 0xFFFFFFFFFFFFFFFF",
        "a = 0o7777777777777777777777",
        "a = 0b1111111111111111111111111111111111111111111111111111111111111111",
        "a = 99999999999999999999",
    ];

    for input in error_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }
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
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

    // expected value found }
    let e = ctx.parse_err("a = }");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));

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

#[test]
fn crlf_in_intermediate_contexts() {
    let ctx = TestCtx::new();

    // CRLF as newline inside array (eat_newline in eat_intermediate)
    let array_cases = [
        ("a = [\r\n1\r\n]", 1),
        ("a = [\r\n1,\r\n2\r\n]", 2),
        ("a = [\r\n  1, # comment\r\n  2\r\n]", 2),
    ];
    for (input, expected_len) in array_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(
            v["a"].as_array().unwrap().len(),
            expected_len,
            "input: {input}"
        );
    }

    // CRLF in inline table whitespace (eat_newline in eat_inline_table_whitespace)
    let v = ctx.parse_ok("a = {\r\nx = 1\r\n}");
    assert_eq!(v["a"]["x"].as_i64(), Some(1));

    // Multiline string: backslash followed by space then CRLF
    let ml_cases = [
        ("a = \"\"\"\\ \r\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\t\r\n   trimmed\"\"\"", "trimmed"),
        ("a = \"\"\"\\\r\n\r\n   trimmed\"\"\"", "trimmed"),
    ];
    for (input, expected) in ml_cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }
}

#[test]
fn scan_token_desc_branches() {
    let ctx = TestCtx::new();

    // Non-key characters at table-header key position trigger
    // read_table_key -> scan_token_desc_and_end with different found descriptions.
    let header_key_cases: [(&str, &str); 4] = [
        ("[#]", "a comment"),
        ("[.]", "a period"),
        ("[:]", "a colon"),
        ("[+]", "a plus"),
    ];
    for (input, expected_found) in header_key_cases {
        let e = ctx.parse_err(input);
        assert!(
            matches!(e.kind, ErrorKind::Wanted { found, .. } if found == expected_found),
            "input: {input}, got: {:?}",
            e.kind
        );
    }

    // Identifier branch: missing comma, cursor at next identifier
    let e = ctx.parse_err("a = {x = 1 y = 2}");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                found: "an identifier",
                ..
            }
        ),
        "input: a = {{x = 1 y = 2}}, got: {:?}",
        e.kind
    );

    // Generic character branch: non-special byte
    let e = ctx.parse_err("a = {x = 1 @}");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                found: "a character",
                ..
            }
        ),
        "input: a = {{x = 1 @}}, got: {:?}",
        e.kind
    );
}

#[test]
fn string_eof_edge_cases() {
    let ctx = TestCtx::new();

    let eof_cases = ["a = \"", "a = \"\\", "a = \"\"\"", "a = \"\"\"\\"];
    for input in eof_cases {
        let e = ctx.parse_err(input);
        assert!(
            matches!(e.kind, ErrorKind::UnterminatedString),
            "input: {input}"
        );
    }
}

#[test]
fn number_identifier_not_inf_nan() {
    let ctx = TestCtx::new();

    // Keylike with - prefix followed by i/n but not -inf/-nan.
    let cases = ["a = -infix", "a = -nah", "a = -infinity", "a = -nano"];
    for input in cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }
}

#[test]
fn integer_overflow_specific_paths() {
    let ctx = TestCtx::new();

    // Decimal: i64::MAX + 1 overflows positive max
    let e = ctx.parse_err("a = 9223372036854775808");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "input: i64::MAX + 1"
    );

    // Decimal: negative overflow past i64::MIN
    let e = ctx.parse_err("a = -9223372036854775809");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "input: -(i64::MIN) - 1"
    );

    // Hex: invalid hex digit
    let hex_invalid = ["a = 0xGG", "a = 0xZZ"];
    for input in hex_invalid {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }

    // Hex: overflow via acc >> 60 != 0 (17 hex digits)
    let e = ctx.parse_err("a = 0xFFFFFFFFFFFFFFFFF");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "input: 0x 17 F's"
    );

    let e = ctx.parse_err("a = 0x10000000000000000");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: 0x 2^64");

    // Octal: acc > i64::MAX (2^63 in octal, passes per-digit check)
    let e = ctx.parse_err("a = 0o1000000000000000000000");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: 0o 2^63");

    // Binary: overflow via acc >> 63 != 0 (65 binary digits)
    let e =
        ctx.parse_err("a = 0b11111111111111111111111111111111111111111111111111111111111111111");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "input: 0b 65 ones"
    );
}

#[test]
fn float_validation_edge_cases() {
    let ctx = TestCtx::new();

    // push_strip_underscores fails on integer part (trailing underscore before dot)
    let integer_part_cases = ["a = 1_.5", "a = 1__.5"];
    for input in integer_part_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }

    // push_strip_underscores fails on exponent part (leading/trailing underscore)
    let exponent_cases = ["a = 1e+_5", "a = 1E+_5", "a = 1e+5_"];
    for input in exponent_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }

    // Result overflows to infinity, rejected as non-finite
    let non_finite_cases = ["a = 1e999", "a = 1e9999", "a = 1.0e999", "a = -1e999"];
    for input in non_finite_cases {
        let e = ctx.parse_err(input);
        assert!(matches!(e.kind, ErrorKind::InvalidNumber), "input: {input}");
    }
}

#[test]
fn inline_table_error_paths() {
    let ctx = TestCtx::new();

    // eat_inline_table_whitespace error at start (bare CR after comment)
    let e = ctx.parse_err("a = {#bad\r}");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comment in inline table start"
    );

    // read_table_key error in loop (non-key character)
    let e = ctx.parse_err("a = {!}");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                ..
            }
        ),
        "input: a = {{!}}"
    );

    // eat_inline_table_whitespace error after key (bare CR)
    let e = ctx.parse_err("a = {x #bad\r= 1}");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after key in inline table"
    );

    // navigate_dotted_key error (dotted key on non-table value)
    let e = ctx.parse_err("a = {x = 1, x.y = 2}");
    assert!(
        matches!(e.kind, ErrorKind::DottedKeyInvalidType { .. }),
        "input: dotted key on integer in inline table"
    );

    // read_table_key error after dot in dotted key
    let e = ctx.parse_err("a = {x.! = 1}");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                ..
            }
        ),
        "input: invalid key after dot in inline table"
    );

    // expect_byte(b'=') error (missing equals)
    let e = ctx.parse_err("a = {x}");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "an equals",
                ..
            }
        ),
        "input: a = {{x}}"
    );

    // eat_inline_table_whitespace error after value (bare CR)
    let e = ctx.parse_err("a = {x = 1 #bad\r}");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after value in inline table"
    );

    // eat_inline_table_whitespace error after comma (bare CR)
    let e = ctx.parse_err("a = {x = 1, #bad\r}");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comma in inline table"
    );
}

#[test]
fn array_error_paths() {
    let ctx = TestCtx::new();

    // eat_intermediate error at start of array (bare CR after comment)
    let e = ctx.parse_err("a = [#bad\r]");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comment at array start"
    );

    // eat_intermediate error after value (bare CR after comment)
    let e = ctx.parse_err("a = [1 #bad\r]");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comment after array value"
    );

    // eat_intermediate error after comma
    let e = ctx.parse_err("a = [1, #bad\r]");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comment after comma in array"
    );
}

#[test]
fn parse_document_error_paths() {
    let ctx = TestCtx::new();

    // eat_comment error at top level (bare CR after comment)
    let e = ctx.parse_err("#bad\r");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after top-level comment"
    );

    let e = ctx.parse_err("a = 1\n#bad\r");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: bare CR after comment following key-value"
    );

    // Bare CR at top level (not in comment or string)
    let e = ctx.parse_err("\r");
    assert!(
        matches!(e.kind, ErrorKind::Unexpected('\r')),
        "input: bare CR at document top level"
    );

    let e = ctx.parse_err("a = 1\n\r");
    assert!(
        matches!(e.kind, ErrorKind::Unexpected('\r')),
        "input: bare CR after newline at document top level"
    );
}

#[test]
fn table_header_error_paths() {
    let ctx = TestCtx::new();

    // read_table_key error after dot in header (trailing dot)
    let e = ctx.parse_err("[a.]\nk = 1");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                ..
            }
        ),
        "input: [a.]"
    );

    // Missing second ] for array-of-tables
    let e = ctx.parse_err("[[a]\nk = 1");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a right bracket",
                ..
            }
        ),
        "input: [[a] missing second bracket"
    );

    // Comment on same line as header (success path)
    let v = ctx.parse_ok("[a] # comment\nk = 1");
    assert_eq!(v["a"]["k"].as_i64(), Some(1), "input: [a] # comment");

    // Junk after header (no newline, not EOF)
    let e = ctx.parse_err("[a]x");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "newline",
                ..
            }
        ),
        "input: [a]x"
    );

    // eat_comment error after header (bare CR)
    let e = ctx.parse_err("[a]#bad\r");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: [a]#bad\\r"
    );
}

#[test]
fn key_value_error_paths() {
    let ctx = TestCtx::new();

    // read_table_key error in dotted key loop
    let e = ctx.parse_err("x.! = 1");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                ..
            }
        ),
        "input: x.! = 1"
    );

    let e = ctx.parse_err("[t]\nx.= = 1");
    assert!(
        matches!(
            e.kind,
            ErrorKind::Wanted {
                expected: "a table key",
                ..
            }
        ),
        "input: x.= = 1"
    );

    // eat_comment error after key-value (bare CR)
    let e = ctx.parse_err("a = 1 #bad\r");
    assert!(
        matches!(e.kind, ErrorKind::Wanted { .. }),
        "input: a = 1 #bad\\r"
    );
}

#[test]
fn hex_numbers_with_zero_digits() {
    let ctx = TestCtx::new();

    // Hex digits that include '0' after the prefix, exercising the
    // HEX lookup table for the 0-9 range as well as lowercase a-f.
    let cases: [(&str, i64); 6] = [
        ("a = 0x10", 0x10),
        ("a = 0x0F", 0x0F),
        ("a = 0x00", 0x00),
        ("a = 0x0a", 0x0a),
        ("a = 0xab", 0xab),
        ("a = 0xAbCd", 0xAbCd),
    ];
    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }
}

#[test]
fn integer_base_max_boundary() {
    let ctx = TestCtx::new();

    // Hex: i64::MAX = 0x7FFFFFFFFFFFFFFF
    let v = ctx.parse_ok("a = 0x7FFFFFFFFFFFFFFF");
    assert_eq!(v["a"].as_i64(), Some(i64::MAX));

    // Hex: i64::MAX + 1 should overflow
    let e = ctx.parse_err("a = 0x8000000000000000");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber), "hex i64::MAX+1");

    // Octal: i64::MAX = 0o777777777777777777777
    let v = ctx.parse_ok("a = 0o777777777777777777777");
    assert_eq!(v["a"].as_i64(), Some(i64::MAX));

    // Octal: i64::MAX + 1 should overflow
    let e = ctx.parse_err("a = 0o1000000000000000000000");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "octal i64::MAX+1"
    );

    // Binary: i64::MAX = 0b followed by 0 then 62 ones
    let v = ctx.parse_ok("a = 0b0111111111111111111111111111111111111111111111111111111111111111");
    assert_eq!(v["a"].as_i64(), Some(i64::MAX));

    // Binary: i64::MAX + 1 should overflow
    let e = ctx.parse_err("a = 0b1000000000000000000000000000000000000000000000000000000000000000");
    assert!(
        matches!(e.kind, ErrorKind::InvalidNumber),
        "binary i64::MAX+1"
    );
}

#[test]
fn literal_string_span() {
    let ctx = TestCtx::new();

    // Span of a single-quoted (literal) string value should cover just the content.
    let input = "key = 'hello'";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(
        &input[span.start as usize..span.end as usize],
        "hello",
        "literal string span"
    );

    // Empty literal string: span covers the opening delimiter
    let input = "key = ''";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(span.start, 6, "empty literal string span start");
    assert_eq!(span.end, 7, "empty literal string span end");
}

#[test]
fn multiline_string_spans() {
    let ctx = TestCtx::new();

    // Multiline basic string span should cover the content (after opening newline trim).
    let input = "key = \"\"\"\nhello\nworld\"\"\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(
        &input[span.start as usize..span.end as usize],
        "hello\nworld",
        "multiline basic string span"
    );

    // Multiline basic string with CRLF opening
    let input = "key = \"\"\"\r\nhello\"\"\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(
        &input[span.start as usize..span.end as usize],
        "hello",
        "multiline basic string span with CRLF opening"
    );

    // Multiline basic string without leading newline
    let input = "key = \"\"\"hello\"\"\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(
        &input[span.start as usize..span.end as usize],
        "hello",
        "multiline basic string span without leading newline"
    );

    // Multiline literal string span
    let input = "key = '''\nhello\nworld'''";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(
        &input[span.start as usize..span.end as usize],
        "hello\nworld",
        "multiline literal string span"
    );

    // Empty string: span covers the opening delimiter
    let input = "key = \"\"";
    let v = ctx.parse_ok(input);
    let span = v.get("key").unwrap().span();
    assert_eq!(span.start, 6, "empty basic string span start");
    assert_eq!(span.end, 7, "empty basic string span end");
}

#[test]
fn backslash_whitespace_in_nonmultiline_string() {
    let ctx = TestCtx::new();

    // Backslash followed by space in a non-multiline basic string is an error.
    let e = ctx.parse_err("a = \"hello\\ world\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidEscape(' ')),
        "backslash-space in basic string: {:?}",
        e.kind
    );

    // Backslash followed by tab in a non-multiline basic string is an error.
    let e = ctx.parse_err("a = \"hello\\\tworld\"");
    assert!(
        matches!(e.kind, ErrorKind::InvalidEscape('\t')),
        "backslash-tab in basic string: {:?}",
        e.kind
    );
}

#[test]
fn lowercase_hex_escapes() {
    let ctx = TestCtx::new();

    // Lowercase hex digits in \u and \U escapes
    let cases = [
        (r#"a = "\u0061""#, "a"),             // lowercase a-f digits
        (r#"a = "\u00ff""#, "\u{ff}"),        // all lowercase hex
        (r#"a = "\U0001f600""#, "\u{1f600}"), // emoji via lowercase hex
        (r#"a = "\x61""#, "a"),               // \x with lowercase
    ];
    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(
            v.get("a").unwrap().as_str(),
            Some(expected),
            "input: {input}"
        );
    }
}

#[test]
fn nan_sign_preservation() {
    let ctx = TestCtx::new();

    let v = ctx.parse_ok("a = nan");
    let f = v["a"].as_f64().unwrap();
    assert!(f.is_nan());
    assert!(f.is_sign_positive(), "nan should be positive");

    let v = ctx.parse_ok("a = -nan");
    let f = v["a"].as_f64().unwrap();
    assert!(f.is_nan());
    assert!(f.is_sign_negative(), "-nan should be negative");

    let v = ctx.parse_ok("a = +nan");
    let f = v["a"].as_f64().unwrap();
    assert!(f.is_nan());
    assert!(f.is_sign_positive(), "+nan should be positive");
}

#[test]
fn indexed_table_nonexistent_key() {
    let ctx = TestCtx::new();

    // Table with 6+ entries uses hash index for lookups.
    // Verify that a nonexistent key returns None.
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6");
    assert!(
        v.get("nonexistent").is_none(),
        "nonexistent key in 6-entry table"
    );

    // Also verify all 6 keys are correctly found (not just first/last).
    assert_eq!(v["a"].as_i64(), Some(1));
    assert_eq!(v["b"].as_i64(), Some(2));
    assert_eq!(v["c"].as_i64(), Some(3));
    assert_eq!(v["d"].as_i64(), Some(4));
    assert_eq!(v["e"].as_i64(), Some(5));
    assert_eq!(v["f"].as_i64(), Some(6));

    // 7+ entries: incremental indexing after initial bulk
    let v = ctx.parse_ok("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7");
    assert!(
        v.get("nonexistent").is_none(),
        "nonexistent key in 7-entry table"
    );
    assert_eq!(v["a"].as_i64(), Some(1));
    assert_eq!(v["g"].as_i64(), Some(7));
}

#[test]
fn long_string_exercises_swar_path() {
    let ctx = TestCtx::new();

    // Strings longer than 8 bytes exercise the SWAR fast path in skip_string_plain.
    // Include special characters at various positions to test boundary detection.
    let cases = [
        // Plain ASCII > 8 bytes
        (r#"a = "abcdefghijklmnop""#, "abcdefghijklmnop"),
        // Special char at position 9 (after one SWAR chunk)
        ("a = \"abcdefgh\\nrest\"", "abcdefgh\nrest"),
        // Delimiter at position 10
        (r#"a = "abcdefghij""#, "abcdefghij"),
        // Tab (benign for SWAR, should pass through)
        ("a = \"abc\tdefghijklmno\"", "abc\tdefghijklmno"),
    ];
    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_str(), Some(expected), "input: {input}");
    }

    // Literal string > 8 bytes (skip_string_plain with delim=')
    let v = ctx.parse_ok("a = 'abcdefghijklmnop'");
    assert_eq!(v["a"].as_str(), Some("abcdefghijklmnop"));
}

#[test]
fn integer_plus_sign_in_decimal() {
    let ctx = TestCtx::new();

    // Explicit + sign on decimal integers
    let cases: [(&str, i64); 3] = [
        ("a = +0", 0),
        ("a = +1", 1),
        ("a = +9223372036854775807", i64::MAX),
    ];
    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }
}

#[test]
fn octal_digit_validation() {
    let ctx = TestCtx::new();

    // Valid octal
    let cases: [(&str, i64); 3] = [("a = 0o0", 0), ("a = 0o7", 7), ("a = 0o10", 8)];
    for (input, expected) in cases {
        let v = ctx.parse_ok(input);
        assert_eq!(v["a"].as_i64(), Some(expected), "input: {input}");
    }

    // Invalid octal digits (8, 9)
    let e = ctx.parse_err("a = 0o8");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber), "octal digit 8");

    let e = ctx.parse_err("a = 0o9");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber), "octal digit 9");
}

#[test]
#[ignore] // requires 512+ MiB allocation
fn file_too_large() {
    let ctx = TestCtx::new();
    let size = (1u32 << 29) as usize + 1;
    let big = " ".repeat(size);
    let e = ctx.parse_err(&big);
    assert!(matches!(e.kind, ErrorKind::FileTooLarge));
}
