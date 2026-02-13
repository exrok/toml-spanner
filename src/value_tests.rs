use super::*;
use crate::Span;
use crate::arena::Arena;

fn sp(s: u32, e: u32) -> Span {
    Span::new(s, e)
}

// -- Constructors + Drop for each variant -----------------------------------

#[test]
fn string_create_and_drop() {
    let v = Item::string(Str::from("hello"), sp(0, 5));
    assert_eq!(v.tag(), TAG_STRING);
    assert_eq!(v.as_str(), Some("hello"));
    assert_eq!(v.span(), sp(0, 5));
}

#[test]
fn string_owned_create_and_drop() {
    let v = Item::string(Str::from("owned"), sp(1, 6));
    assert_eq!(v.as_str(), Some("owned"));
}

#[test]
fn integer_create_access() {
    let v = Item::integer(42, sp(0, 2));
    assert_eq!(v.tag(), TAG_INTEGER);
    assert_eq!(v.as_integer(), Some(42));
    assert_eq!(v.span(), sp(0, 2));
}

#[test]
fn integer_negative() {
    let v = Item::integer(-9999, sp(0, 5));
    assert_eq!(v.as_integer(), Some(-9999));
}

#[test]
fn float_create_access() {
    let v = Item::float(3.14, sp(0, 4));
    assert_eq!(v.tag(), TAG_FLOAT);
    assert_eq!(v.as_float(), Some(3.14));
    assert_eq!(v.span(), sp(0, 4));
}

#[test]
fn boolean_create_access() {
    let t = Item::boolean(true, sp(0, 4));
    let f = Item::boolean(false, sp(5, 10));
    assert_eq!(t.tag(), TAG_BOOLEAN);
    assert_eq!(t.as_bool(), Some(true));
    assert_eq!(f.as_bool(), Some(false));
}

#[test]
fn array_create_and_drop() {
    let arena = Arena::new();
    let mut arr = Array::new();
    arr.push(Item::integer(1, sp(0, 1)), &arena);
    arr.push(Item::integer(2, sp(2, 3)), &arena);
    let v = Item::array(arr, sp(0, 3));
    assert_eq!(v.tag(), TAG_ARRAY);
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 2);
}

#[test]
fn table_create_and_drop() {
    let arena = Arena::new();
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
    let t = v.as_table().unwrap();
    assert_eq!(t.len(), 1);
}

#[test]
fn table_header_create_drop() {
    let v = Item::table_header(InnerTable::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE_HEADER);
    assert!(v.has_header_bit());
    assert!(v.as_table().is_some());
}

#[test]
fn table_dotted_create_drop() {
    let v = Item::table_dotted(InnerTable::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE_DOTTED);
    assert!(v.has_dotted_bit());
    assert!(v.as_table().is_some());
}

// -- Span bit packing -------------------------------------------------------

#[test]
fn span_roundtrip_all_tags() {
    let tags_and_constructors: Vec<(u32, Item<'_>)> = vec![
        (TAG_STRING, Item::string(Str::from("x"), sp(100, 200))),
        (TAG_INTEGER, Item::integer(0, sp(100, 200))),
        (TAG_FLOAT, Item::float(0.0, sp(100, 200))),
        (TAG_BOOLEAN, Item::boolean(false, sp(100, 200))),
        (TAG_ARRAY, Item::array(Array::new(), sp(100, 200))),
        (TAG_TABLE, Item::table(InnerTable::new(), sp(100, 200))),
        (
            TAG_TABLE_HEADER,
            Item::table_header(InnerTable::new(), sp(100, 200)),
        ),
        (
            TAG_TABLE_DOTTED,
            Item::table_dotted(InnerTable::new(), sp(100, 200)),
        ),
    ];
    for (expected_tag, v) in &tags_and_constructors {
        assert_eq!(v.tag(), *expected_tag);
        assert_eq!(v.span(), sp(100, 200), "tag={expected_tag}");
    }
}

#[test]
fn span_large_values() {
    let max_start = (1u32 << 29) - 1;
    let max_end = (1u32 << 31) - 1;
    let v = Item::integer(0, sp(max_start, max_end));
    assert_eq!(v.span().start, max_start);
    assert_eq!(v.span().end, max_end);
}

#[test]
fn span_zero() {
    let v = Item::integer(0, sp(0, 0));
    assert_eq!(v.span(), sp(0, 0));
}

// -- Flag bit ---------------------------------------------------------------

#[test]
fn flag_bit_aot() {
    let v = Item::array_aot(Array::new(), sp(10, 20));
    assert!(v.is_aot());
    assert!(v.is_frozen());
    assert_eq!(v.span(), sp(10, 20));
}

#[test]
fn flag_bit_frozen_table() {
    let v = Item::table_frozen(InnerTable::new(), sp(5, 15));
    assert!(v.is_frozen());
    assert!(v.as_table().is_some());
    assert_eq!(v.span(), sp(5, 15));
}

// -- as_ref / as_mut --------------------------------------------------------

#[test]
fn as_ref_all_types() {
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
        let kind = match v.as_ref() {
            ValueRef::String(_) => "string",
            ValueRef::Integer(_) => "integer",
            ValueRef::Float(_) => "float",
            ValueRef::Boolean(_) => "boolean",
            ValueRef::Array(_) => "array",
            ValueRef::Table(_) => "table",
        };
        assert_eq!(kind, *exp);
    }
}

#[test]
fn as_mut_modify_integer() {
    let mut v = Item::integer(10, sp(0, 2));
    if let ValueMut::Integer(i) = v.as_mut() {
        *i = 99;
    }
    assert_eq!(v.as_integer(), Some(99));
}

#[test]
fn as_mut_modify_float() {
    let mut v = Item::float(1.0, sp(0, 3));
    if let ValueMut::Float(f) = v.as_mut() {
        *f = 2.5;
    }
    assert_eq!(v.as_float(), Some(2.5));
}

#[test]
fn as_mut_modify_boolean() {
    let mut v = Item::boolean(false, sp(0, 5));
    if let ValueMut::Boolean(b) = v.as_mut() {
        *b = true;
    }
    assert_eq!(v.as_bool(), Some(true));
}

#[test]
fn as_mut_modify_array() {
    let arena = Arena::new();
    let mut v = Item::array(Array::new(), sp(0, 2));
    if let ValueMut::Array(a) = v.as_mut() {
        a.push(Item::integer(42, sp(0, 2)), &arena);
    }
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn as_mut_modify_table() {
    let arena = Arena::new();
    let mut v = Item::table(InnerTable::new(), sp(0, 2));
    if let ValueMut::Table(t) = v.as_mut() {
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

// -- accessor methods -------------------------------------------------------

#[test]
fn accessor_string() {
    let v = Item::string(Str::from("owned"), sp(0, 5));
    assert_eq!(v.as_str().unwrap(), "owned");
}

#[test]
fn accessor_integer() {
    let v = Item::integer(42, sp(0, 2));
    assert_eq!(v.as_integer().unwrap(), 42);
}

#[test]
fn accessor_array() {
    let arena = Arena::new();
    let mut arr = Array::new();
    arr.push(Item::integer(1, sp(0, 1)), &arena);
    arr.push(Item::integer(2, sp(0, 1)), &arena);
    let v = Item::array(arr, sp(0, 5));
    assert_eq!(v.as_array().unwrap().len(), 2);
}

#[test]
fn accessor_table() {
    let arena = Arena::new();
    let mut tab = InnerTable::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Item::integer(1, sp(0, 1)),
        &arena,
    );
    let v = Item::table(tab, sp(0, 5));
    assert_eq!(v.as_table().unwrap().len(), 1);
}

// -- Negative type checks ---------------------------------------------------

#[test]
fn negative_type_checks() {
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

// -- take_string / set_table ------------------------------------------------

#[test]
fn take_string_ok() {
    let mut v = Item::string(Str::from("hello"), sp(0, 5));
    let s = v.take_string(None).unwrap();
    assert_eq!(&*s, "hello");
}

#[test]
fn take_string_err() {
    let mut v = Item::integer(42, sp(0, 2));
    assert!(v.take_string(None).is_err());
}

// -- SpannedTable span helpers ----------------------------------------------

#[test]
fn spanned_table_set_span_preserves_tag() {
    let mut v = Item::table_header(InnerTable::new(), sp(10, 20));
    let st = unsafe { v.as_spanned_table_mut_unchecked() };

    st.set_span_start(99);
    assert_eq!(v.tag(), TAG_TABLE_HEADER);
    assert_eq!(v.span().start, 99);
}

// -- type_str / has_keys / has_key ------------------------------------------

#[test]
fn type_str_values() {
    assert_eq!(Item::string(Str::from(""), sp(0, 0)).type_str(), "string");
    assert_eq!(Item::integer(0, sp(0, 0)).type_str(), "integer");
    assert_eq!(Item::float(0.0, sp(0, 0)).type_str(), "float");
    assert_eq!(Item::boolean(false, sp(0, 0)).type_str(), "boolean");
    assert_eq!(Item::array(Array::new(), sp(0, 0)).type_str(), "array");
    assert_eq!(Item::table(InnerTable::new(), sp(0, 0)).type_str(), "table");
    assert_eq!(
        Item::table_header(InnerTable::new(), sp(0, 0)).type_str(),
        "table"
    );
    assert_eq!(
        Item::table_dotted(InnerTable::new(), sp(0, 0)).type_str(),
        "table"
    );
}

#[test]
fn has_keys_and_has_key() {
    let arena = Arena::new();
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

// -- Debug ------------------------------------------------------------------

#[test]
fn debug_all_variants() {
    let vals: Vec<Item<'_>> = vec![
        Item::string(Str::from("s"), sp(0, 1)),
        Item::integer(42, sp(0, 2)),
        Item::float(3.14, sp(0, 4)),
        Item::boolean(true, sp(0, 4)),
        Item::array(Array::new(), sp(0, 2)),
        Item::table(InnerTable::new(), sp(0, 2)),
        Item::table_header(InnerTable::new(), sp(0, 2)),
        Item::table_dotted(InnerTable::new(), sp(0, 2)),
    ];
    for v in &vals {
        let _ = format!("{v:?}");
    }
}
