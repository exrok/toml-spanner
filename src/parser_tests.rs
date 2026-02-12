use crate::value::ValueOwned;
use crate::{ErrorKind, Value};

fn parse_ok(input: &str) -> Value<'_> {
    crate::parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e}"))
}

fn parse_err(input: &str) -> crate::Error {
    crate::parse(input).unwrap_err()
}

fn root_table<'a, 'de>(v: &'a Value<'de>) -> &'a crate::value::Table<'de> {
    v.as_table().expect("root should be a table")
}

#[test]
fn empty_document() {
    let v = parse_ok("");
    assert!(root_table(&v).is_empty());
}

#[test]
fn simple_string() {
    let v = parse_ok("a = \"hello\"");
    let t = root_table(&v);
    assert_eq!(t.get("a").unwrap().as_str(), Some("hello"));
}

#[test]
fn simple_integer() {
    let v = parse_ok("a = 42");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(42));
}

#[test]
fn simple_negative_integer() {
    let v = parse_ok("a = -100");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(-100));
}

#[test]
fn simple_float() {
    let v = parse_ok("a = 3.14");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!((f - 3.14).abs() < f64::EPSILON);
}

#[test]
fn simple_boolean_true() {
    let v = parse_ok("a = true");
    assert_eq!(root_table(&v).get("a").unwrap().as_bool(), Some(true));
}

#[test]
fn simple_boolean_false() {
    let v = parse_ok("a = false");
    assert_eq!(root_table(&v).get("a").unwrap().as_bool(), Some(false));
}

#[test]
fn multiple_keys_span_extension() {
    let v = parse_ok("a = 1\nb = 2\nc = 3");
    let t = root_table(&v);
    assert_eq!(t.len(), 3);
    assert_eq!(t.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("c").unwrap().as_integer(), Some(3));
}

#[test]
fn escape_newline() {
    let v = parse_ok(r#"a = "line1\nline2""#);
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("line1\nline2")
    );
}

#[test]
fn escape_tab() {
    let v = parse_ok(r#"a = "col1\tcol2""#);
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("col1\tcol2")
    );
}

#[test]
fn escape_backslash() {
    let v = parse_ok(r#"a = "path\\to""#);
    assert_eq!(root_table(&v).get("a").unwrap().as_str(), Some("path\\to"));
}

#[test]
fn escape_quote() {
    let v = parse_ok(r#"a = "say \"hi\"""#);
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("say \"hi\"")
    );
}

#[test]
fn unicode_escape_short() {
    let v = parse_ok(r#"a = "\u0041""#);
    assert_eq!(root_table(&v).get("a").unwrap().as_str(), Some("A"));
}

#[test]
fn unicode_escape_long() {
    let v = parse_ok(r#"a = "\U00000041""#);
    assert_eq!(root_table(&v).get("a").unwrap().as_str(), Some("A"));
}

#[test]
fn multiline_basic_string() {
    let v = parse_ok("a = \"\"\"\nhello\nworld\"\"\"");
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("hello\nworld")
    );
}

#[test]
fn multiline_literal_string() {
    let v = parse_ok("a = '''\nhello\nworld'''");
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("hello\nworld")
    );
}

#[test]
fn literal_string_no_escapes() {
    let v = parse_ok(r#"a = 'no\escape'"#);
    assert_eq!(
        root_table(&v).get("a").unwrap().as_str(),
        Some("no\\escape")
    );
}

#[test]
fn empty_string() {
    let v = parse_ok(r#"a = """#);
    assert_eq!(root_table(&v).get("a").unwrap().as_str(), Some(""));
}

#[test]
fn hex_integer() {
    let v = parse_ok("a = 0xDEAD");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(0xDEAD));
}

#[test]
fn octal_integer() {
    let v = parse_ok("a = 0o777");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(0o777));
}

#[test]
fn binary_integer() {
    let v = parse_ok("a = 0b1010");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(0b1010));
}

#[test]
fn float_inf() {
    let v = parse_ok("a = inf");
    assert_eq!(
        root_table(&v).get("a").unwrap().as_float(),
        Some(f64::INFINITY)
    );
}

#[test]
fn float_neg_inf() {
    let v = parse_ok("a = -inf");
    assert_eq!(
        root_table(&v).get("a").unwrap().as_float(),
        Some(f64::NEG_INFINITY)
    );
}

#[test]
fn float_nan() {
    let v = parse_ok("a = nan");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!(f.is_nan());
}

#[test]
fn float_neg_nan() {
    let v = parse_ok("a = -nan");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!(f.is_nan());
}

#[test]
fn float_exponent() {
    let v = parse_ok("a = 1e10");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!((f - 1e10).abs() < 1.0);
}

#[test]
fn float_exponent_negative() {
    let v = parse_ok("a = 1.5E-3");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!((f - 1.5e-3).abs() < 1e-10);
}

#[test]
fn integer_underscores() {
    let v = parse_ok("a = 1_000_000");
    assert_eq!(
        root_table(&v).get("a").unwrap().as_integer(),
        Some(1_000_000)
    );
}

#[test]
fn float_underscores() {
    let v = parse_ok("a = 1_000.5");
    let f = root_table(&v).get("a").unwrap().as_float().unwrap();
    assert!((f - 1000.5).abs() < f64::EPSILON);
}

#[test]
fn array_of_integers() {
    let v = parse_ok("a = [1, 2, 3]");
    let arr = root_table(&v).get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr.get(0).unwrap().as_integer(), Some(1));
    assert_eq!(arr.get(2).unwrap().as_integer(), Some(3));
}

#[test]
fn array_empty() {
    let v = parse_ok("a = []");
    let arr = root_table(&v).get("a").unwrap().as_array().unwrap();
    assert!(arr.is_empty());
}

#[test]
fn array_nested() {
    let v = parse_ok("a = [[1, 2], [3, 4]]");
    let arr = root_table(&v).get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let inner = arr.get(0).unwrap().as_array().unwrap();
    assert_eq!(inner.len(), 2);
}

#[test]
fn inline_table() {
    let v = parse_ok("a = {x = 1, y = 2}");
    let t = root_table(&v).get("a").unwrap().as_table().unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t.get("x").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("y").unwrap().as_integer(), Some(2));
}

#[test]
fn inline_table_empty() {
    let v = parse_ok("a = {}");
    let t = root_table(&v).get("a").unwrap().as_table().unwrap();
    assert!(t.is_empty());
}

#[test]
fn nested_inline_table() {
    let v = parse_ok("a = {b = {c = 1}}");
    let a = root_table(&v).get("a").unwrap().as_table().unwrap();
    let b = a.get("b").unwrap().as_table().unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));
}

#[test]
fn array_of_inline_tables() {
    let v = parse_ok("a = [{x = 1}, {x = 2}]");
    let arr = root_table(&v).get("a").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let t0 = arr.get(0).unwrap().as_table().unwrap();
    assert_eq!(t0.get("x").unwrap().as_integer(), Some(1));
}

#[test]
fn table_header_simple() {
    let v = parse_ok("[table]\nkey = 1");
    let t = root_table(&v).get("table").unwrap().as_table().unwrap();
    assert_eq!(t.get("key").unwrap().as_integer(), Some(1));
}

#[test]
fn table_header_multiple() {
    let v = parse_ok("[a]\nx = 1\n[b]\ny = 2");
    let root = root_table(&v);
    assert_eq!(
        root.get("a")
            .unwrap()
            .as_table()
            .unwrap()
            .get("x")
            .unwrap()
            .as_integer(),
        Some(1)
    );
    assert_eq!(
        root.get("b")
            .unwrap()
            .as_table()
            .unwrap()
            .get("y")
            .unwrap()
            .as_integer(),
        Some(2)
    );
}

#[test]
fn dotted_table_header() {
    let v = parse_ok("[a.b.c]\nkey = 1");
    let a = root_table(&v).get("a").unwrap().as_table().unwrap();
    let b = a.get("b").unwrap().as_table().unwrap();
    let c = b.get("c").unwrap().as_table().unwrap();
    assert_eq!(c.get("key").unwrap().as_integer(), Some(1));
}

#[test]
fn dotted_key_value() {
    let v = parse_ok("a.b.c = 1");
    let a = root_table(&v).get("a").unwrap().as_table().unwrap();
    let b = a.get("b").unwrap().as_table().unwrap();
    assert_eq!(b.get("c").unwrap().as_integer(), Some(1));
}

#[test]
fn dotted_key_multiple() {
    let v = parse_ok("a.x = 1\na.y = 2");
    let a = root_table(&v).get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("x").unwrap().as_integer(), Some(1));
    assert_eq!(a.get("y").unwrap().as_integer(), Some(2));
}

#[test]
fn array_of_tables() {
    let v = parse_ok("[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"");
    let arr = root_table(&v).get("items").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let t0 = arr.get(0).unwrap().as_table().unwrap();
    assert_eq!(t0.get("name").unwrap().as_str(), Some("a"));
    let t1 = arr.get(1).unwrap().as_table().unwrap();
    assert_eq!(t1.get("name").unwrap().as_str(), Some("b"));
}

#[test]
fn array_of_tables_with_subtable() {
    let input = "[[fruit]]\nname = \"apple\"\n[fruit.physical]\ncolor = \"red\"";
    let v = parse_ok(input);
    let arr = root_table(&v).get("fruit").unwrap().as_array().unwrap();
    let fruit = arr.get(0).unwrap().as_table().unwrap();
    assert_eq!(fruit.get("name").unwrap().as_str(), Some("apple"));
    let phys = fruit.get("physical").unwrap().as_table().unwrap();
    assert_eq!(phys.get("color").unwrap().as_str(), Some("red"));
}

#[test]
fn implicit_table_via_header() {
    let v = parse_ok("[a.b]\nx = 1\n[a]\ny = 2");
    let a = root_table(&v).get("a").unwrap().as_table().unwrap();
    assert_eq!(a.get("y").unwrap().as_integer(), Some(2));
    let b = a.get("b").unwrap().as_table().unwrap();
    assert_eq!(b.get("x").unwrap().as_integer(), Some(1));
}

#[test]
fn table_5_keys_linear_scan() {
    let input = "a = 1\nb = 2\nc = 3\nd = 4\ne = 5";
    let v = parse_ok(input);
    let t = root_table(&v);
    assert_eq!(t.len(), 5);
    assert_eq!(t.get("e").unwrap().as_integer(), Some(5));
}

#[test]
fn table_6_keys_bulk_index() {
    let input = "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6";
    let v = parse_ok(input);
    let t = root_table(&v);
    assert_eq!(t.len(), 6);
    assert_eq!(t.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("f").unwrap().as_integer(), Some(6));
}

#[test]
fn table_7_keys_incremental_index() {
    let input = "a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7";
    let v = parse_ok(input);
    let t = root_table(&v);
    assert_eq!(t.len(), 7);
    assert_eq!(t.get("g").unwrap().as_integer(), Some(7));
}

#[test]
fn table_many_keys() {
    let mut lines = Vec::new();
    for i in 0..20 {
        lines.push(format!("key{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = parse_ok(&input);
    let t = root_table(&v);
    assert_eq!(t.len(), 20);
    assert_eq!(t.get("key0").unwrap().as_integer(), Some(0));
    assert_eq!(t.get("key19").unwrap().as_integer(), Some(19));
}

#[test]
fn subtable_6_keys_triggers_index() {
    let mut lines = vec!["[sub]".to_string()];
    for i in 0..6 {
        lines.push(format!("k{i} = {i}"));
    }
    let input = lines.join("\n");
    let v = parse_ok(&input);
    let sub = root_table(&v).get("sub").unwrap().as_table().unwrap();
    assert_eq!(sub.len(), 6);
    assert_eq!(sub.get("k5").unwrap().as_integer(), Some(5));
}

#[test]
fn duplicate_key_error() {
    let e = parse_err("a = 1\na = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateKey { .. }));
}

#[test]
fn unterminated_string_error() {
    let e = parse_err("a = \"unterminated");
    assert!(matches!(e.kind, ErrorKind::UnterminatedString));
}

#[test]
fn invalid_escape_error() {
    let e = parse_err(r#"a = "\z""#);
    println!("{}", e);
    assert!(matches!(e.kind, ErrorKind::InvalidEscape('z')));
}

#[test]
fn duplicate_table_error() {
    let e = parse_err("[t]\na = 1\n[t]\nb = 2");
    assert!(matches!(e.kind, ErrorKind::DuplicateTable { .. }));
}

#[test]
fn unexpected_eof_error() {
    let e = parse_err("a = ");
    assert!(matches!(e.kind, ErrorKind::UnexpectedEof));
}

#[test]
fn invalid_number_error() {
    let e = parse_err("a = 0x");
    assert!(matches!(e.kind, ErrorKind::InvalidNumber));
}

#[test]
fn redefine_value_as_table() {
    let e = parse_err("a = 1\n[a]\nb = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. } | ErrorKind::DuplicateKey { .. }
    ));
}

#[test]
fn inline_table_is_frozen() {
    let e = parse_err("a = {x = 1}\n[a]\ny = 2");
    assert!(matches!(
        e.kind,
        ErrorKind::DuplicateTable { .. } | ErrorKind::DuplicateKey { .. }
    ));
}

#[test]
fn comments_ignored() {
    let v = parse_ok("# comment\na = 1 # inline comment\n# another");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(1));
}

#[test]
fn blank_lines() {
    let v = parse_ok("\n\n\na = 1\n\n\n");
    assert_eq!(root_table(&v).get("a").unwrap().as_integer(), Some(1));
}

#[test]
fn parsed_value_into_kind() {
    let mut v = parse_ok("a = \"hello\"\nb = 42\nc = [1, 2]\nd = {x = 1}");
    let t = v.as_table_mut().unwrap();

    let a = t.get_mut("a").unwrap().take();
    let ValueOwned::String(s) = a else {
        panic!("expected string")
    };
    assert_eq!(&*s, "hello");

    let b = t.get_mut("b").unwrap().take();
    let ValueOwned::Integer(i) = b else {
        panic!("expected integer")
    };
    assert_eq!(i, 42);

    let c = t.get_mut("c").unwrap().take();
    let ValueOwned::Array(arr) = c else {
        panic!("expected array")
    };
    assert_eq!(arr.len(), 2);

    let d = t.get_mut("d").unwrap().take();
    let ValueOwned::Table(tab) = d else {
        panic!("expected table")
    };
    assert_eq!(tab.len(), 1);
}

#[test]
fn mixed_content() {
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
    let v = parse_ok(input);
    let root = root_table(&v);
    assert_eq!(root.get("title").unwrap().as_str(), Some("TOML Example"));
    assert_eq!(root.get("count").unwrap().as_integer(), Some(100));

    let db = root.get("database").unwrap().as_table().unwrap();
    assert_eq!(db.get("ports").unwrap().as_array().unwrap().len(), 3);

    let servers = root.get("servers").unwrap().as_table().unwrap();
    let alpha = servers.get("alpha").unwrap().as_table().unwrap();
    assert_eq!(alpha.get("ip").unwrap().as_str(), Some("10.0.0.1"));

    let products = root.get("products").unwrap().as_array().unwrap();
    assert_eq!(products.len(), 2);
    let p0 = products.get(0).unwrap().as_table().unwrap();
    assert_eq!(p0.get("name").unwrap().as_str(), Some("Hammer"));
}

#[test]
fn quoted_key_basic() {
    let v = parse_ok(r#""quoted key" = 1"#);
    assert_eq!(
        root_table(&v).get("quoted key").unwrap().as_integer(),
        Some(1)
    );
}

#[test]
fn quoted_key_with_escape() {
    let v = parse_ok(r#""key\nwith\nnewlines" = 1"#);
    assert_eq!(
        root_table(&v)
            .get("key\nwith\nnewlines")
            .unwrap()
            .as_integer(),
        Some(1)
    );
}

#[test]
fn literal_quoted_key() {
    let v = parse_ok("'literal key' = 1");
    assert_eq!(
        root_table(&v).get("literal key").unwrap().as_integer(),
        Some(1)
    );
}

#[test]
fn span_for_simple_value() {
    let input = "key = 42";
    let v = parse_ok(input);
    let val = root_table(&v).get("key").unwrap();
    let span = val.span();
    assert_eq!(&input[span.start() as usize..span.end() as usize], "42");
}

#[test]
fn span_for_string_value() {
    let input = "key = \"hello\"";
    let v = parse_ok(input);
    let val = root_table(&v).get("key").unwrap();
    let span = val.span();
    let spanned = &input[span.start() as usize..span.end() as usize];
    assert_eq!(spanned, "hello");
}
