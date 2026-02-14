use super::*;
use crate::Span;
use crate::arena::Arena;

fn sp(s: u32, e: u32) -> Span {
    Span::new(s, e)
}



#[test]
fn constructors() {
    let arena = Arena::new();

    // String
    let v = Item::string(Str::from("hello"), sp(0, 5));
    assert_eq!(v.tag(), TAG_STRING);
    assert_eq!(v.as_str(), Some("hello"));
    assert_eq!(v.span(), sp(0, 5));
    assert_eq!(v.type_str(), "string");

    // Integer (positive and negative)
    let v = Item::integer(42, sp(0, 2));
    assert_eq!(v.tag(), TAG_INTEGER);
    assert_eq!(v.as_integer(), Some(42));
    assert_eq!(v.span(), sp(0, 2));
    assert_eq!(v.type_str(), "integer");

    let v = Item::integer(-9999, sp(0, 5));
    assert_eq!(v.as_integer(), Some(-9999));

    // Float
    let v = Item::float(3.14, sp(0, 4));
    assert_eq!(v.tag(), TAG_FLOAT);
    assert_eq!(v.as_float(), Some(3.14));
    assert_eq!(v.span(), sp(0, 4));
    assert_eq!(v.type_str(), "float");

    // Boolean
    let t = Item::boolean(true, sp(0, 4));
    let f = Item::boolean(false, sp(5, 10));
    assert_eq!(t.tag(), TAG_BOOLEAN);
    assert_eq!(t.as_bool(), Some(true));
    assert_eq!(f.as_bool(), Some(false));
    assert_eq!(t.type_str(), "boolean");

    // Array
    let mut arr = Array::new();
    arr.push(Item::integer(1, sp(0, 1)), &arena);
    arr.push(Item::integer(2, sp(2, 3)), &arena);
    let v = Item::array(arr, sp(0, 3));
    assert_eq!(v.tag(), TAG_ARRAY);
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!(v.type_str(), "array");

    // Table
    let mut tab = InnerTable::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Item::integer(10, sp(2, 4)),
        &arena,
    );
    let v = Item::table(tab, sp(0, 4));
    assert_eq!(v.tag(), TAG_TABLE);
    assert_eq!(v.as_table().unwrap().len(), 1);
    assert_eq!(v.type_str(), "table");

    // type_str for table variants
    assert_eq!(
        Item::table_header(InnerTable::new(), sp(0, 0)).type_str(),
        "table"
    );
    assert_eq!(
        Item::table_dotted(InnerTable::new(), sp(0, 0)).type_str(),
        "table"
    );

    // has_keys / has_key
    let empty = Item::table(InnerTable::new(), sp(0, 0));
    assert!(!empty.has_keys());

    let mut tab = InnerTable::new();
    tab.insert(
        Key {
            name: Str::from("x"),
            span: sp(0, 1),
        },
        Item::integer(1, sp(0, 1)),
        &arena,
    );
    let v = Item::table(tab, sp(0, 1));
    assert!(v.has_keys());
    assert!(v.has_key("x"));
    assert!(!v.has_key("y"));
}



#[test]
fn table_variant_flags() {
    // Header
    let v = Item::table_header(InnerTable::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE);
    assert_eq!(v.flag(), FLAG_HEADER);
    assert!(v.has_header_bit());
    assert!(v.as_table().is_some());

    // Dotted
    let v = Item::table_dotted(InnerTable::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE);
    assert_eq!(v.flag(), FLAG_DOTTED);
    assert!(v.has_dotted_bit());
    assert!(v.as_table().is_some());

    // AOT (array of tables)
    let v = Item::array_aot(Array::new(), sp(10, 20));
    assert!(v.is_aot());
    assert_eq!(v.flag(), FLAG_AOT);
    assert_eq!(v.span(), sp(10, 20));

    // Frozen
    let v = Item::table_frozen(InnerTable::new(), sp(5, 15));
    assert!(v.is_frozen());
    assert!(v.as_table().is_some());
    assert_eq!(v.span(), sp(5, 15));
}



#[test]
fn span_bit_packing() {
    // Roundtrip across all tags
    let tags_and_constructors: Vec<(u32, Item<'_>)> = vec![
        (TAG_STRING, Item::string(Str::from("x"), sp(100, 200))),
        (TAG_INTEGER, Item::integer(0, sp(100, 200))),
        (TAG_FLOAT, Item::float(0.0, sp(100, 200))),
        (TAG_BOOLEAN, Item::boolean(false, sp(100, 200))),
        (TAG_ARRAY, Item::array(Array::new(), sp(100, 200))),
        (TAG_TABLE, Item::table(InnerTable::new(), sp(100, 200))),
        (
            TAG_TABLE,
            Item::table_header(InnerTable::new(), sp(100, 200)),
        ),
        (
            TAG_TABLE,
            Item::table_dotted(InnerTable::new(), sp(100, 200)),
        ),
    ];
    for (expected_tag, v) in &tags_and_constructors {
        assert_eq!(v.tag(), *expected_tag);
        assert_eq!(v.span(), sp(100, 200), "tag={expected_tag}");
    }

    // Large span values near the 29-bit limit
    let max_start = (1u32 << 29) - 1;
    let max_end = (1u32 << 29) - 1;
    let v = Item::integer(0, sp(max_start, max_end));
    assert_eq!(v.span().start, max_start);
    assert_eq!(v.span().end, max_end);
}



#[test]
fn value_and_type_checks() {
    // value() returns the correct variant for each type
    let vals: Vec<Item<'_>> = vec![
        Item::string(Str::from("s"), sp(0, 1)),
        Item::integer(1, sp(0, 1)),
        Item::float(1.0, sp(0, 1)),
        Item::boolean(true, sp(0, 1)),
        Item::array(Array::new(), sp(0, 1)),
        Item::table(InnerTable::new(), sp(0, 1)),
    ];
    let expected = ["string", "integer", "float", "boolean", "array", "table"];
    for (v, exp) in vals.iter().zip(expected.iter()) {
        let kind = match v.value() {
            Value::String(_) => "string",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Array(_) => "array",
            Value::Table(_) => "table",
        };
        assert_eq!(kind, *exp);
    }

    // Negative type checks: as_* returns None for wrong types
    let v = Item::integer(42, sp(0, 2));
    assert!(v.as_str().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Item::string(Str::from("s"), sp(0, 1));
    assert!(v.as_integer().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Item::boolean(true, sp(0, 4));
    assert!(v.as_str().is_none());
    assert!(v.as_integer().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());
}



#[test]
fn value_mut_all_types() {
    let arena = Arena::new();

    // String
    let mut v = Item::string(Str::from("hello"), sp(0, 5));
    if let ValueMut::String(s) = v.value_mut() {
        assert_eq!(&**s, "hello");
    } else {
        panic!("expected String");
    }

    // Integer
    let mut v = Item::integer(10, sp(0, 2));
    if let ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(v.as_integer(), Some(99));

    // Float
    let mut v = Item::float(1.0, sp(0, 3));
    if let ValueMut::Float(f) = v.value_mut() {
        *f = 2.5;
    }
    assert_eq!(v.as_float(), Some(2.5));

    // Boolean
    let mut v = Item::boolean(false, sp(0, 5));
    if let ValueMut::Boolean(b) = v.value_mut() {
        *b = true;
    }
    assert_eq!(v.as_bool(), Some(true));

    // Array
    let mut v = Item::array(Array::new(), sp(0, 2));
    if let ValueMut::Array(a) = v.value_mut() {
        a.push(Item::integer(42, sp(0, 2)), &arena);
    }
    assert_eq!(v.as_array().unwrap().len(), 1);

    // Table
    let mut v = Item::table(InnerTable::new(), sp(0, 2));
    if let ValueMut::Table(t) = v.value_mut() {
        t.insert(
            Key {
                name: Str::from("x"),
                span: sp(0, 1),
            },
            Item::integer(1, sp(0, 1)),
            &arena,
        );
    }
    assert_eq!(v.as_table().unwrap().len(), 1);
}



#[test]
fn type_error_helpers() {
    // expected() produces correct error
    let v = Item::integer(42, sp(0, 2));
    let err = v.expected("a string");
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { expected: "a string", found: "integer" }));
    assert_eq!(err.span, sp(0, 2));

    // take_string success
    let mut v = Item::string(Str::from("hello"), sp(0, 5));
    let s = v.take_string(None).unwrap();
    assert_eq!(&*s, "hello");

    // take_string wrong type returns error
    let mut v = Item::integer(42, sp(0, 2));
    assert!(v.take_string(None).is_err());

    // take_string custom error message is preserved
    let mut v = Item::integer(42, sp(0, 2));
    let err = v.take_string(Some("a custom msg")).unwrap_err();
    if let crate::ErrorKind::Wanted { expected, .. } = err.kind {
        assert_eq!(expected, "a custom msg");
    } else {
        panic!("expected Wanted error");
    }
}



#[test]
fn expect_and_mut_accessors() {
    let arena = Arena::new();

    // expect_array success
    let mut arr = Array::new();
    arr.push(Item::integer(1, sp(0, 1)), &arena);
    let mut v = Item::array(arr, sp(0, 5));
    let a = v.expect_array().unwrap();
    assert_eq!(a.len(), 1);

    // expect_array type mismatch
    let mut v = Item::integer(42, sp(0, 2));
    let err = v.expect_array().unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // expect_table success
    let mut tab = InnerTable::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Item::integer(1, sp(0, 1)),
        &arena,
    );
    let mut v = Item::table(tab, sp(0, 5));
    let t = v.expect_table().unwrap();
    assert_eq!(t.len(), 1);

    // expect_table type mismatch
    let mut v = Item::integer(42, sp(0, 2));
    let err = v.expect_table().unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // as_array_mut on array
    let mut v = Item::array(Array::new(), sp(0, 2));
    let a = v.as_array_mut().unwrap();
    a.push(Item::integer(1, sp(0, 1)), &arena);
    assert_eq!(v.as_array().unwrap().len(), 1);

    // as_array_mut on non-array
    let mut v = Item::integer(42, sp(0, 2));
    assert!(v.as_array_mut().is_none());

    // as_table_mut on table
    let mut v = Item::table(InnerTable::new(), sp(0, 2));
    let t = v.as_table_mut().unwrap();
    t.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Item::integer(1, sp(0, 1)),
        &arena,
    );
    assert_eq!(v.as_table().unwrap().len(), 1);

    // as_table_mut on non-table
    let mut v = Item::integer(42, sp(0, 2));
    assert!(v.as_table_mut().is_none());
}



#[test]
fn parse_method() {
    // Success
    let mut v = Item::string(Str::from("42"), sp(0, 2));
    let parsed: i32 = v.parse::<i32, _>().unwrap();
    assert_eq!(parsed, 42);

    // Parse failure (invalid content)
    let mut v = Item::string(Str::from("not_a_number"), sp(0, 12));
    let err = v.parse::<i32, _>().unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Custom(..)));

    // Wrong type (not a string)
    let mut v = Item::integer(42, sp(0, 2));
    let err = v.parse::<i32, _>().unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));
}



#[test]
fn spanned_table_set_span_preserves_flag() {
    let mut v = Item::table_header(InnerTable::new(), sp(10, 20));

    unsafe { v.as_spanned_table_mut_unchecked() }.set_span_start(99);
    assert_eq!(v.tag(), TAG_TABLE);
    assert_eq!(v.flag(), FLAG_HEADER);
    assert_eq!(v.span().start, 99);

    unsafe { v.as_spanned_table_mut_unchecked() }.set_span_end(200);
    assert_eq!(v.flag(), FLAG_HEADER);
    assert_eq!(v.span().end, 200);
}



#[test]
fn key_basics() {
    let k = Key {
        name: Str::from("mykey"),
        span: sp(0, 5),
    };
    assert_eq!(k.as_str(), "mykey");

    // Equality ignores span
    let a = Key {
        name: Str::from("same"),
        span: sp(0, 4),
    };
    let b = Key {
        name: Str::from("same"),
        span: sp(10, 14),
    };
    assert_eq!(a, b);

    // Ordering
    let a = Key {
        name: Str::from("aaa"),
        span: sp(0, 3),
    };
    let b = Key {
        name: Str::from("bbb"),
        span: sp(0, 3),
    };
    assert!(a < b);
    assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);

    // Borrow trait
    let k = Key {
        name: Str::from("test"),
        span: sp(0, 4),
    };
    let borrowed: &str = std::borrow::Borrow::borrow(&k);
    assert_eq!(borrowed, "test");

    // Debug trait
    assert_eq!(format!("{:?}", k), "test");

    // Display trait
    assert_eq!(format!("{}", k), "test");
}

#[test]
fn item_debug_fmt() {
    // Test Debug formatting for all Item types
    let v = Item::string(Str::from("hello"), sp(0, 5));
    assert_eq!(format!("{:?}", v), "\"hello\"");

    let v = Item::integer(42, sp(0, 2));
    assert_eq!(format!("{:?}", v), "42");

    let v = Item::float(3.14, sp(0, 4));
    assert_eq!(format!("{:?}", v), "3.14");

    let v = Item::boolean(true, sp(0, 4));
    assert_eq!(format!("{:?}", v), "true");

    let arena = Arena::new();
    let mut arr = Array::new();
    arr.push(Item::integer(1, sp(0, 1)), &arena);
    let v = Item::array(arr, sp(0, 3));
    assert!(format!("{:?}", v).contains("1"));

    let v = Item::table(InnerTable::new(), sp(0, 2));
    let debug = format!("{:?}", v);
    // Table Debug may wrap in ManuallyDrop due to union layout
    assert!(debug.contains("{}"));
}
