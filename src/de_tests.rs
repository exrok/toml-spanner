use super::Deserialize;
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
        assert!(port_item
            .expect_custom_string(helper.ctx, "an IPv4 address")
            .is_err());
    }
    assert!(matches!(
        root.ctx.errors[0].kind,
        crate::ErrorKind::Wanted {
            expected: "an IPv4 address",
            ..
        }
    ));

    // table_helper() on table vs non-table items
    let mut root =
        crate::parser::parse("[sub]\na = 1\nval = 42", &arena).unwrap();
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
