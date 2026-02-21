use super::Deserialize;
use crate::Item;
use crate::arena::Arena;
use crate::span::Spanned;

fn parse_val<'a, T: Deserialize<'a>>(input: &'a str, arena: &'a Arena) -> Result<T, crate::Error> {
    let mut root = crate::parser::parse(input, arena).unwrap();
    let result = {
        let mut helper = root.helper();
        helper.required::<T>("v")
    };
    match result {
        Ok(val) => Ok(val),
        Err(_) => Err(root.ctx.errors.remove(0)),
    }
}

#[test]
fn deser_strings() {
    let arena = Arena::new();
    // String (owned)
    let val: String = parse_val(r#"v = "hello""#, &arena).unwrap();
    assert_eq!(val, "hello");

    // Str (borrowed)
    let val: &str = parse_val(r#"v = "borrowed""#, &arena).unwrap();
    assert_eq!(val, "borrowed");

    // Cow<str>
    let val: std::borrow::Cow<'_, str> = parse_val(r#"v = "cow""#, &arena).unwrap();
    assert_eq!(&*val, "cow");
}

#[test]
fn deser_booleans() {
    let arena = Arena::new();

    // true
    let val: bool = parse_val("v = true", &arena).unwrap();
    assert!(val);

    // false
    let val: bool = parse_val("v = false", &arena).unwrap();
    assert!(!val);

    // wrong type
    let err = parse_val::<bool>(r#"v = "not a bool""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
}

#[test]
fn deser_integers() {
    let arena = Arena::new();

    // Signed types
    let val: i8 = parse_val("v = 42", &arena).unwrap();
    assert_eq!(val, 42);

    let val: i16 = parse_val("v = 1000", &arena).unwrap();
    assert_eq!(val, 1000);

    let val: i32 = parse_val("v = 100000", &arena).unwrap();
    assert_eq!(val, 100000);

    let val: i64 = parse_val("v = 9999999999", &arena).unwrap();
    assert_eq!(val, 9999999999);

    let val: isize = parse_val("v = -42", &arena).unwrap();
    assert_eq!(val, -42);

    // Unsigned types
    let val: u8 = parse_val("v = 255", &arena).unwrap();
    assert_eq!(val, 255);

    let val: u16 = parse_val("v = 65535", &arena).unwrap();
    assert_eq!(val, 65535);

    let val: u32 = parse_val("v = 100000", &arena).unwrap();
    assert_eq!(val, 100000);

    let val: u64 = parse_val("v = 9999999999", &arena).unwrap();
    assert_eq!(val, 9999999999);

    let val: usize = parse_val("v = 42", &arena).unwrap();
    assert_eq!(val, 42);

    // Out-of-range errors
    let err = parse_val::<i8>("v = 200", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::OutOfRange("i8")));

    let err = parse_val::<u8>("v = 256", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::OutOfRange("u8")));

    let err = parse_val::<u64>("v = -1", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::OutOfRange("u64")));

    let err = parse_val::<usize>("v = -1", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::OutOfRange("usize")));

    // Wrong type
    let err = parse_val::<i32>(r#"v = "not an int""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
}

#[test]
fn deser_floats() {
    let arena = Arena::new();

    // f32
    let val: f32 = parse_val("v = 3.15", &arena).unwrap();
    assert!((val - 3.15_f32).abs() < 0.001);

    // f64
    let val: f64 = parse_val("v = 3.15", &arena).unwrap();
    assert!((val - 3.15).abs() < f64::EPSILON);

    // Wrong type
    let err = parse_val::<f64>(r#"v = "not a float""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    let err = parse_val::<f32>(r#"v = "not a float""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
}

#[test]
fn deser_vecs() {
    let arena = Arena::new();

    // Integers
    let val: Vec<i64> = parse_val("v = [1, 2, 3]", &arena).unwrap();
    assert_eq!(val, vec![1, 2, 3]);

    // Strings
    let val: Vec<String> = parse_val(r#"v = ["a", "b"]"#, &arena).unwrap();
    assert_eq!(val, vec!["a", "b"]);

    // Empty
    let val: Vec<i64> = parse_val("v = []", &arena).unwrap();
    assert!(val.is_empty());

    // Wrong type
    let err = parse_val::<Vec<i64>>(r#"v = "not an array""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
}

#[test]
fn deser_spanned() {
    let arena = Arena::new();
    let input = "v = 42";
    let mut root = crate::parser::parse(input, &arena).unwrap();
    let val: Spanned<i64> = {
        let mut helper = root.helper();
        helper.required("v").unwrap()
    };
    assert_eq!(val.value, 42);
    assert_eq!(&input[val.span.start as usize..val.span.end as usize], "42");
}

#[test]
fn into_remaining() {
    fn check(key_count: usize, use_every_nth: usize) {
        let mut toml = String::new();
        for i in 0..key_count {
            if !toml.is_empty() {
                toml.push('\n');
            }
            toml.push_str(&format!("k{i} = {i}"));
        }
        let arena = Arena::new();
        let mut root = crate::parser::parse(&toml, &arena).unwrap();
        let mut helper = root.helper();

        let mut expected_remaining = Vec::new();
        for i in 0..key_count {
            let name = format!("k{i}");
            if use_every_nth > 0 && i % use_every_nth == 0 {
                let _: Option<i64> = helper.optional(&name);
            } else {
                expected_remaining.push(name);
            }
        }

        let keys: Vec<_> = helper.into_remaining().map(|(k, _)| k.name).collect();
        assert_eq!(
            keys, expected_remaining,
            "key_count={key_count} use_every_nth={use_every_nth}"
        );
    }

    // Empty table.
    check(0, 0);
    // Single bucket, none/some/all used.
    check(3, 0);
    check(3, 2);
    check(3, 1);
    // Exact bucket boundary.
    check(64, 0);
    check(64, 2);
    check(64, 1);
    // Multi-bucket, non-aligned.
    check(65, 0);
    check(65, 3);

    // too slow under mirir.
    if !cfg!(miri) {
        check(65, 1);
        // Two full buckets + partial third.
        check(150, 0);
        check(150, 5);
        check(150, 1);
    }
}

#[test]
fn table_helper_workflows() {
    let arena = Arena::new();

    // expect_empty succeeds when all fields are consumed
    let mut root = crate::parser::parse("a = 1\nb = 2", &arena).unwrap();
    {
        let mut helper = root.helper();
        let _: i64 = helper.required("a").unwrap();
        let _: i64 = helper.required("b").unwrap();
        assert_eq!(helper.remaining_count(), 0);
        helper.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // expect_empty fails with unexpected keys when fields are not consumed
    let mut root = crate::parser::parse("a = 1\nb = 2\nc = 3", &arena).unwrap();
    {
        let mut helper = root.helper();
        let _: i64 = helper.required("a").unwrap();
        assert_eq!(helper.remaining_count(), 2);
        assert!(helper.expect_empty().is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::UnexpectedKeys { .. }
    ));

    // required() returns MissingField error for nonexistent key
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut helper = root.helper();
        assert!(helper.required::<i64>("nonexistent").is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::MissingField("nonexistent")
    ));

    // optional() returns None for missing key (no error) and None with
    // error on type mismatch
    let mut root = crate::parser::parse(r#"a = "string""#, &arena).unwrap();
    {
        let mut helper = root.helper();
        assert!(helper.optional::<i64>("nonexistent").is_none());
        assert!(helper.optional::<i64>("a").is_none());
    }
    assert_eq!(root.ctx.errors.len(), 1);

    // Indexed table (7+ entries) exercises get_entry hash path
    let mut lines = Vec::new();
    for i in 0..8 {
        lines.push(format!("k{i} = {i}"));
    }
    let input = lines.join("\n");
    let mut root = crate::parser::parse(&input, &arena).unwrap();
    {
        let mut helper = root.helper();
        let v: i64 = helper.required("k0").unwrap();
        assert_eq!(v, 0);
        let v: i64 = helper.required("k7").unwrap();
        assert_eq!(v, 7);
        assert!(helper.required::<i64>("nonexistent").is_err());
        assert_eq!(helper.remaining_count(), 6);
    }
}

#[test]
fn deser_boxed_and_array_types() {
    let arena = Arena::new();

    // Box<str>
    let val: Box<str> = parse_val(r#"v = "boxed""#, &arena).unwrap();
    assert_eq!(&*val, "boxed");
    let err = parse_val::<Box<str>>("v = 42", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // Box<T>
    let val: Box<i64> = parse_val("v = 42", &arena).unwrap();
    assert_eq!(*val, 42);
    let err = parse_val::<Box<i64>>(r#"v = "nope""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // Box<[T]>
    let val: Box<[i64]> = parse_val("v = [1, 2, 3]", &arena).unwrap();
    assert_eq!(&*val, &[1, 2, 3]);
    let err = parse_val::<Box<[i64]>>(r#"v = "nope""#, &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // String wrong type
    let err = parse_val::<String>("v = 42", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // [T; N] correct size
    let val: [i64; 3] = parse_val("v = [1, 2, 3]", &arena).unwrap();
    assert_eq!(val, [1, 2, 3]);

    // [T; N] wrong size
    let err = parse_val::<[i64; 2]>("v = [1, 2, 3]", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Custom(..)));

    // &str and Cow<str> wrong type errors
    let err = parse_val::<&str>("v = 42", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
    let err = parse_val::<std::borrow::Cow<'_, str>>("v = 42", &arena).unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // Vec<T> with element type errors
    let mut root = crate::parser::parse(r#"v = [1, "bad", 3, "worse"]"#, &arena).unwrap();
    let result = {
        let mut helper = root.helper();
        helper.required::<Vec<i64>>("v")
    };
    assert!(result.is_err());
    assert_eq!(root.ctx.errors.len(), 2);
}

#[test]
fn expect_custom_string_and_context_errors() {
    let arena = Arena::new();

    // expect_custom_string via parse and helper
    let mut root = crate::parser::parse("ip = \"127.0.0.1\"\nport = 8080", &arena).unwrap();
    {
        let helper = root.helper();
        let (_, ip_item) = helper.get_entry("ip").unwrap();
        assert_eq!(
            ip_item
                .expect_custom_string(helper.ctx, "an IPv4 address")
                .unwrap(),
            "127.0.0.1"
        );
        let (_, port_item) = helper.get_entry("port").unwrap();
        assert!(
            port_item
                .expect_custom_string(helper.ctx, "an IPv4 address")
                .is_err()
        );
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::Wanted {
            expected: "an IPv4 address",
            ..
        }
    ));

    // table_helper() on table vs non-table items
    let mut root = crate::parser::parse("[sub]\na = 1\nval = 42", &arena).unwrap();
    {
        let helper = root.helper();
        let (_, sub_item) = helper.get_entry("sub").unwrap();
        let mut th = sub_item.table_helper(helper.ctx).unwrap();
        let v: i64 = th.required("a").unwrap();
        assert_eq!(v, 1);
    }
    let mut root = crate::parser::parse("val = 42", &arena).unwrap();
    {
        let helper = root.helper();
        let (_, val_item) = helper.get_entry("val").unwrap();
        assert!(val_item.table_helper(helper.ctx).is_err());
    }

    // Context::error_message_at and push_error
    let span = crate::Span::new(0, 5);
    let mut ctx = super::Context {
        arena: &arena,
        index: Default::default(),
        errors: Vec::new(),
    };
    let _ = ctx.error_message_at("something went wrong", span);
    let _ = ctx.push_error(crate::Error {
        kind: crate::ErrorKind::InvalidNumber,
        span,
    });
    assert_eq!(ctx.errors.len(), 2);
    assert!(matches!(ctx.errors[0].kind, crate::ErrorKind::Custom(..)));
    assert!(matches!(
        ctx.errors[1].kind,
        crate::ErrorKind::InvalidNumber
    ));
}

#[test]
fn root_methods() {
    let arena = Arena::new();

    // table(), errors(), has_errors(), Debug, Index
    let root = crate::parser::parse("a = 1\nb = 2", &arena).unwrap();
    assert_eq!(root.table().len(), 2);
    assert_eq!(root["a"].as_i64(), Some(1));
    assert!(root.errors().is_empty());
    assert!(!root.has_errors());
    let debug = format!("{:?}", root);
    assert!(debug.contains("a"));

    // into_item() converts root table to item
    let root = crate::parser::parse("x = 42", &arena).unwrap();
    let item = root.into_item();
    assert_eq!(item.as_table().unwrap().len(), 1);

    // deserialize() on root (type mismatch: root is table, asking for i64)
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    assert!(root.deserialize::<i64>().is_err());
    assert!(root.has_errors());
}

#[test]
fn required_item_and_optional_item() {
    let arena = Arena::new();

    // required_item succeeds for each value kind
    let mut root = crate::parser::parse(
        r#"
s = "hello"
i = 42
f = 3.15
b = true
a = [1, 2]

[t]
x = 1
"#,
        &arena,
    )
    .unwrap();
    {
        let mut h = root.helper();

        let item = h.required_item("s").unwrap();
        assert_eq!(item.as_str(), Some("hello"));

        let item = h.required_item("i").unwrap();
        assert_eq!(item.as_i64(), Some(42));

        let item = h.required_item("f").unwrap();
        assert!((item.as_f64().unwrap() - 3.15).abs() < f64::EPSILON);

        let item = h.required_item("b").unwrap();
        assert_eq!(item.as_bool(), Some(true));

        let item = h.required_item("a").unwrap();
        assert_eq!(item.as_array().unwrap().len(), 2);

        let item = h.required_item("t").unwrap();
        assert!(item.as_table().is_some());

        assert_eq!(h.remaining_count(), 0);
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // required_item fails for missing key
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.required_item("missing").is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::MissingField("missing")
    ));

    // optional_item returns Some for present key, None for absent
    let mut root = crate::parser::parse("x = 99\ny = true", &arena).unwrap();
    {
        let mut h = root.helper();

        let item = h.optional_item("x");
        assert!(item.is_some());
        assert_eq!(item.unwrap().as_i64(), Some(99));

        let item = h.optional_item("y");
        assert_eq!(item.unwrap().as_bool(), Some(true));

        assert!(h.optional_item("absent").is_none());

        assert_eq!(h.remaining_count(), 0);
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // optional_item does not record an error for missing keys
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.optional_item("nope").is_none());
        let _ = h.optional_item("a");
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // Consuming with required_item/optional_item makes expect_empty pass
    let mut root = crate::parser::parse("a = 1\nb = 2\nc = 3", &arena).unwrap();
    {
        let mut h = root.helper();
        h.required_item("a").unwrap();
        h.optional_item("b");
        h.required_item("c").unwrap();
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // Unconsumed fields after required_item cause expect_empty to fail
    let mut root = crate::parser::parse("a = 1\nb = 2\nc = 3", &arena).unwrap();
    {
        let mut h = root.helper();
        h.required_item("a").unwrap();
        assert_eq!(h.remaining_count(), 2);
        assert!(h.expect_empty().is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::UnexpectedKeys { .. }
    ));

    // Works with indexed tables (7+ entries)
    let mut lines = Vec::new();
    for i in 0..10 {
        lines.push(format!("k{i} = {i}"));
    }
    let input = lines.join("\n");
    let mut root = crate::parser::parse(&input, &arena).unwrap();
    {
        let mut h = root.helper();
        let item = h.required_item("k0").unwrap();
        assert_eq!(item.as_i64(), Some(0));
        let item = h.required_item("k9").unwrap();
        assert_eq!(item.as_i64(), Some(9));
        let item = h.optional_item("k5").unwrap();
        assert_eq!(item.as_i64(), Some(5));
        assert!(h.optional_item("nonexistent").is_none());
        assert!(h.required_item("also_missing").is_err());
    }

    // Duplicate calls to optional_item on the same key return the item but
    // don't double-count consumption.
    let mut root = crate::parser::parse("only = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        h.optional_item("only");
        h.optional_item("only");
        assert_eq!(h.remaining_count(), 0);
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());
}

#[test]
fn required_entry_and_optional_entry() {
    let arena = Arena::new();

    // required_entry returns both key and item
    let input = "name = \"alice\"\nage = 30";
    let mut root = crate::parser::parse(input, &arena).unwrap();
    {
        let mut h = root.helper();

        let (key, item) = h.required_entry("name").unwrap();
        assert_eq!(key.name, "name");
        assert_eq!(item.as_str(), Some("alice"));
        // Key span should cover "name" in the source
        let key_text = &input[key.span.start as usize..key.span.end as usize];
        assert_eq!(key_text, "name");

        let (key, item) = h.required_entry("age").unwrap();
        assert_eq!(key.name, "age");
        assert_eq!(item.as_i64(), Some(30));

        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // required_entry fails for missing key
    let mut root = crate::parser::parse("x = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.required_entry("missing").is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::MissingField("missing")
    ));

    // optional_entry returns Some with key+item for present key
    let input = "color = \"red\"";
    let mut root = crate::parser::parse(input, &arena).unwrap();
    {
        let mut h = root.helper();

        let entry = h.optional_entry("color");
        assert!(entry.is_some());
        let (key, item) = entry.unwrap();
        assert_eq!(key.name, "color");
        assert_eq!(item.as_str(), Some("red"));

        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // optional_entry returns None for absent key without error
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.optional_entry("nope").is_none());
        h.optional_entry("a");
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // Entries are marked consumed correctly
    let mut root = crate::parser::parse("a = 1\nb = 2\nc = 3", &arena).unwrap();
    {
        let mut h = root.helper();
        h.optional_entry("a");
        h.required_entry("c").unwrap();
        assert_eq!(h.remaining_count(), 1);
        assert!(h.expect_empty().is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::UnexpectedKeys { .. }
    ));

    // Works with indexed tables (7+ entries)
    let mut lines = Vec::new();
    for i in 0..8 {
        lines.push(format!("field{i} = \"{i}\""));
    }
    let input = lines.join("\n");
    let mut root = crate::parser::parse(&input, &arena).unwrap();
    {
        let mut h = root.helper();
        let (key, item) = h.required_entry("field0").unwrap();
        assert_eq!(key.name, "field0");
        assert_eq!(item.as_str(), Some("0"));
        let (key, item) = h.required_entry("field7").unwrap();
        assert_eq!(key.name, "field7");
        assert_eq!(item.as_str(), Some("7"));
        assert!(h.optional_entry("nonexistent").is_none());
    }

    // Key span is valid for quoted keys
    let input = r#""quoted-key" = 42"#;
    let mut root = crate::parser::parse(input, &arena).unwrap();
    {
        let mut h = root.helper();
        let (key, item) = h.required_entry("quoted-key").unwrap();
        assert_eq!(key.name, "quoted-key");
        assert_eq!(item.as_i64(), Some(42));
    }
}

#[test]
fn required_mapped_and_optional_mapped() {
    use std::net::Ipv4Addr;

    fn parse_positive_int(item: &crate::value::Item<'_>) -> Result<u32, crate::Error> {
        let val = item
            .as_i64()
            .ok_or_else(|| item.expected("a positive integer"))?;
        if val > 0 && val <= u32::MAX as i64 {
            Ok(val as u32)
        } else {
            Err(item.expected("a positive integer"))
        }
    }

    fn parse_uppercase(item: &crate::value::Item<'_>) -> Result<String, crate::Error> {
        let s = item.as_str().ok_or_else(|| item.expected("a string"))?;
        Ok(s.to_uppercase())
    }

    let arena = Arena::new();

    // required_mapped succeeds with a valid mapping function
    let mut root =
        crate::parser::parse("ip = \"192.168.1.1\"\ncount = 5\nname = \"hello\"", &arena).unwrap();
    {
        let mut h = root.helper();

        let ip: Ipv4Addr = h.required_mapped("ip", Item::parse::<Ipv4Addr>).unwrap();
        assert_eq!(ip, Ipv4Addr::new(192, 168, 1, 1));

        let count: u32 = h.required_mapped("count", parse_positive_int).unwrap();
        assert_eq!(count, 5);

        let name: String = h.required_mapped("name", parse_uppercase).unwrap();
        assert_eq!(name, "HELLO");

        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // required_mapped fails for missing key
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.required_mapped("missing", parse_positive_int).is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::MissingField("missing")
    ));

    // required_mapped fails when the mapping function returns an error
    let mut root = crate::parser::parse("ip = \"not-an-ip\"", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.required_mapped("ip", Item::parse::<Ipv4Addr>).is_err());
    }
    assert_eq!(root.ctx.errors.len(), 1);
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::Custom(..)
    ));

    // required_mapped fails when item is wrong type for mapping
    let mut root = crate::parser::parse("count = -5", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.required_mapped("count", parse_positive_int).is_err());
    }
    assert_eq!(root.ctx.errors.len(), 1);

    // optional_mapped returns Some for valid mapping
    let mut root = crate::parser::parse("ip = \"10.0.0.1\"\nport = 8080", &arena).unwrap();
    {
        let mut h = root.helper();

        let ip = h.optional_mapped("ip", Item::parse::<Ipv4Addr>);
        assert_eq!(ip, Some(Ipv4Addr::new(10, 0, 0, 1)));

        let port = h.optional_mapped("port", parse_positive_int);
        assert_eq!(port, Some(8080));

        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // optional_mapped returns None for missing key without recording an error
    let mut root = crate::parser::parse("a = 1", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(
            h.optional_mapped("absent", Item::parse::<Ipv4Addr>)
                .is_none()
        );
        h.optional_mapped("a", parse_positive_int);
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // optional_mapped returns None when mapping fails, records error
    let mut root = crate::parser::parse("ip = \"bad\"", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.optional_mapped("ip", Item::parse::<Ipv4Addr>).is_none());
        h.expect_empty().unwrap();
    }
    assert_eq!(root.ctx.errors.len(), 1);
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::Custom(..)
    ));

    // optional_mapped returns None when item is wrong type, records error
    let mut root = crate::parser::parse("name = 42", &arena).unwrap();
    {
        let mut h = root.helper();
        assert!(h.optional_mapped("name", parse_uppercase).is_none());
        h.expect_empty().unwrap();
    }
    assert_eq!(root.ctx.errors.len(), 1);

    // Both mark fields as consumed: mixing mapped with other helpers
    let mut root = crate::parser::parse(
        "ip = \"1.2.3.4\"\nname = \"test\"\ncount = 7\nextra = true",
        &arena,
    )
    .unwrap();
    {
        let mut h = root.helper();
        let _: Ipv4Addr = h.required_mapped("ip", Item::parse::<Ipv4Addr>).unwrap();
        let _: String = h.required("name").unwrap();
        let _ = h.optional_mapped("count", parse_positive_int);
        let _: bool = h.optional("extra").unwrap();
        assert_eq!(h.remaining_count(), 0);
        h.expect_empty().unwrap();
    }
    assert!(root.ctx.errors.is_empty());

    // Works with indexed tables (7+ entries)
    let mut lines = Vec::new();
    for i in 0..8 {
        lines.push(format!("n{i} = \"{i}\""));
    }
    let input = lines.join("\n");
    let mut root = crate::parser::parse(&input, &arena).unwrap();
    {
        let mut h = root.helper();
        let v: String = h.required_mapped("n0", parse_uppercase).unwrap();
        assert_eq!(v, "0");
        let v = h.optional_mapped("n7", parse_uppercase);
        assert_eq!(v.as_deref(), Some("7"));
        assert!(h.required_mapped("nonexistent", parse_uppercase).is_err());
        assert!(h.optional_mapped("also_missing", parse_uppercase).is_none());
    }
}
