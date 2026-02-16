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
    let v = Item::string("hello", sp(0, 5));
    assert_eq!(v.tag(), TAG_STRING);
    assert_eq!(v.as_str(), Some("hello"));
    assert_eq!(v.span(), sp(0, 5));
    assert_eq!(v.type_str(), "string");

    // Integer (positive and negative)
    let v = Item::integer(42, sp(0, 2));
    assert_eq!(v.tag(), TAG_INTEGER);
    assert_eq!(v.as_i64(), Some(42));
    assert_eq!(v.span(), sp(0, 2));
    assert_eq!(v.type_str(), "integer");

    let v = Item::integer(-9999, sp(0, 5));
    assert_eq!(v.as_i64(), Some(-9999));

    // Float
    let v = Item::float(3.14, sp(0, 4));
    assert_eq!(v.tag(), TAG_FLOAT);
    assert_eq!(v.as_f64(), Some(3.14));
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
            name: "k",
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
            name: ("x"),
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
        (TAG_STRING, Item::string("x", sp(100, 200))),
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
        Item::string("s", sp(0, 1)),
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
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Item::string("s", sp(0, 1));
    assert!(v.as_i64().is_none());
    assert!(v.as_f64().is_none());
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Item::boolean(true, sp(0, 4));
    assert!(v.as_str().is_none());
    assert!(v.as_i64().is_none());
    assert!(v.as_f64().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());
}

#[test]
fn as_f64_converts_integers() {
    // Float value returned directly
    let v = Item::float(3.14, sp(0, 4));
    assert_eq!(v.as_f64(), Some(3.14));

    // Integer value converted to f64
    let v = Item::integer(42, sp(0, 2));
    assert_eq!(v.as_f64(), Some(42.0));

    let v = Item::integer(-100, sp(0, 4));
    assert_eq!(v.as_f64(), Some(-100.0));

    let v = Item::integer(0, sp(0, 1));
    assert_eq!(v.as_f64(), Some(0.0));

    // Non-numeric types still return None
    assert!(Item::string("s", sp(0, 1)).as_f64().is_none());
    assert!(Item::boolean(true, sp(0, 4)).as_f64().is_none());
    assert!(Item::array(Array::new(), sp(0, 2)).as_f64().is_none());
    assert!(Item::table(InnerTable::new(), sp(0, 2)).as_f64().is_none());

    // MaybeItem also converts integers
    let item = Item::integer(99, sp(0, 2));
    let maybe = MaybeItem::from_ref(&item);
    assert_eq!(maybe.as_f64(), Some(99.0));

    let item = Item::float(2.5, sp(0, 3));
    let maybe = MaybeItem::from_ref(&item);
    assert_eq!(maybe.as_f64(), Some(2.5));

    // MaybeItem NONE returns None
    assert!(NONE.as_f64().is_none());
}

#[test]
fn value_mut_all_types() {
    let arena = Arena::new();

    // String
    let mut v = Item::string("hello", sp(0, 5));
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
    assert_eq!(v.as_i64(), Some(99));

    // Float
    let mut v = Item::float(1.0, sp(0, 3));
    if let ValueMut::Float(f) = v.value_mut() {
        *f = 2.5;
    }
    assert_eq!(v.as_f64(), Some(2.5));

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
                name: ("x"),
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
    assert!(matches!(
        err.kind,
        crate::ErrorKind::Wanted {
            expected: "a string",
            found: "integer"
        }
    ));
    assert_eq!(err.span, sp(0, 2));

    // take_string success
    let mut v = Item::string("hello", sp(0, 5));
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
            name: ("k"),
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
            name: ("k"),
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
    let mut v = Item::string("42", sp(0, 2));
    let parsed: i32 = v.parse::<i32, _>().unwrap();
    assert_eq!(parsed, 42);

    // Parse failure (invalid content)
    let mut v = Item::string("not_a_number", sp(0, 12));
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
        name: ("mykey"),
        span: sp(0, 5),
    };
    assert_eq!(k.as_str(), "mykey");

    // Equality ignores span
    let a = Key {
        name: ("same"),
        span: sp(0, 4),
    };
    let b = Key {
        name: ("same"),
        span: sp(10, 14),
    };
    assert_eq!(a, b);

    // Ordering
    let a = Key {
        name: ("aaa"),
        span: sp(0, 3),
    };
    let b = Key {
        name: ("bbb"),
        span: sp(0, 3),
    };
    assert!(a < b);
    assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);

    // Borrow trait
    let k = Key {
        name: ("test"),
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
    let v = Item::string("hello", sp(0, 5));
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
    assert_eq!(debug, "{}");
}

#[test]
fn item_index_operators() {
    let arena = Arena::new();

    // Index table Item by &str
    let mut tab = InnerTable::new();
    tab.insert(
        Key {
            name: ("name"),
            span: sp(0, 4),
        },
        Item::string("alice", sp(5, 10)),
        &arena,
    );
    tab.insert(
        Key {
            name: ("age"),
            span: sp(11, 14),
        },
        Item::integer(30, sp(15, 17)),
        &arena,
    );
    let item = Item::table(tab, sp(0, 17));
    assert_eq!(item["name"].as_str(), Some("alice"));
    assert_eq!(item["age"].as_i64(), Some(30));
    assert!(item["missing"].item().is_none());

    // Index array Item by usize
    let mut arr = Array::new();
    arr.push(Item::integer(10, sp(0, 2)), &arena);
    arr.push(Item::string("two", sp(3, 6)), &arena);
    arr.push(Item::boolean(true, sp(7, 11)), &arena);
    let item = Item::array(arr, sp(0, 11));
    assert_eq!(item[0].as_i64(), Some(10));
    assert_eq!(item[1].as_str(), Some("two"));
    assert_eq!(item[2].as_bool(), Some(true));
    assert!(item[3].item().is_none());
    assert!(item[100].item().is_none());

    // &str index on non-table types returns NONE
    let int_item = Item::integer(42, sp(0, 2));
    assert!(int_item["anything"].item().is_none());
    assert!(Item::string("s", sp(0, 1))["k"].item().is_none());
    assert!(Item::boolean(true, sp(0, 4))["k"].item().is_none());
    assert!(Item::array(Array::new(), sp(0, 2))["k"].item().is_none());

    // usize index on non-array types returns NONE
    assert!(int_item[0].item().is_none());
    assert!(Item::string("s", sp(0, 1))[0].item().is_none());
    assert!(Item::table(InnerTable::new(), sp(0, 2))[0].item().is_none());
}

#[test]
fn maybe_item_chained_and_none_propagation() {
    let arena = Arena::new();

    // Build nested: { users: [{ name: "alice", scores: [100, 200] }] }
    let mut scores = Array::new();
    scores.push(Item::integer(100, sp(0, 3)), &arena);
    scores.push(Item::integer(200, sp(4, 7)), &arena);

    let mut user_tab = InnerTable::new();
    user_tab.insert(
        Key {
            name: ("name"),
            span: sp(0, 4),
        },
        Item::string("alice", sp(5, 10)),
        &arena,
    );
    user_tab.insert(
        Key {
            name: ("scores"),
            span: sp(11, 17),
        },
        Item::array(scores, sp(18, 25)),
        &arena,
    );

    let mut users = Array::new();
    users.push(Item::table(user_tab, sp(0, 25)), &arena);

    let mut root = InnerTable::new();
    root.insert(
        Key {
            name: ("users"),
            span: sp(0, 5),
        },
        Item::array(users, sp(6, 30)),
        &arena,
    );
    let root_item = Item::table(root, sp(0, 30));

    // Deep chained access
    assert_eq!(root_item["users"][0]["name"].as_str(), Some("alice"));
    assert_eq!(root_item["users"][0]["scores"][0].as_i64(), Some(100));
    assert_eq!(root_item["users"][0]["scores"][1].as_i64(), Some(200));

    // Missing at various depths returns NONE
    assert!(root_item["users"][0]["scores"][2].item().is_none());
    assert!(root_item["users"][0]["missing"].item().is_none());
    assert!(root_item["users"][1].item().is_none());
    assert!(root_item["nope"][0]["name"].item().is_none());

    // span()/value() on valid vs NONE MaybeItem
    let maybe = &root_item["users"][0]["name"];
    assert!(maybe.span().is_some());
    assert!(matches!(maybe.value(), Some(Value::String(_))));
    let none = &root_item["missing"];
    assert!(none.span().is_none());
    assert!(none.value().is_none());

    // NONE propagates through arbitrary chains
    let item = Item::integer(42, sp(0, 2));
    assert!(item["a"]["b"]["c"].item().is_none());
    assert!(item["a"][0]["b"][1].item().is_none());
    assert!(item[0][1][2].item().is_none());
    assert!(item[0]["key"][1]["nested"].item().is_none());

    // All accessors on NONE return None
    let none = &item["missing"];
    assert!(none.as_str().is_none());
    assert!(none.as_i64().is_none());
    assert!(none.as_f64().is_none());
    assert!(none.as_bool().is_none());
    assert!(none.as_array().is_none());
    assert!(none.as_table().is_none());
}
