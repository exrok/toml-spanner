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
    assert_eq!(&*val, "borrowed");

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
    let val: f32 = parse_val("v = 3.14", &arena).unwrap();
    assert!((val - 3.14_f32).abs() < 0.001);

    // f64
    let val: f64 = parse_val("v = 3.14", &arena).unwrap();
    assert!((val - 3.14).abs() < f64::EPSILON);

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
