use super::*;
use crate::Span;

fn sp(s: u32, e: u32) -> Span {
    Span::new(s, e)
}

// -- Constructors + Drop for each variant -----------------------------------

#[test]
fn string_create_and_drop() {
    let v = Value::string(Str::from("hello"), sp(0, 5));
    assert_eq!(v.tag(), TAG_STRING);
    assert_eq!(v.as_str(), Some("hello"));
    assert_eq!(v.span(), sp(0, 5));
}

#[test]
fn string_owned_create_and_drop() {
    let v = Value::string(Str::from("owned"), sp(1, 6));
    assert_eq!(v.as_str(), Some("owned"));
}

#[test]
fn integer_create_access() {
    let v = Value::integer(42, sp(0, 2));
    assert_eq!(v.tag(), TAG_INTEGER);
    assert_eq!(v.as_integer(), Some(42));
    assert_eq!(v.span(), sp(0, 2));
}

#[test]
fn integer_negative() {
    let v = Value::integer(-9999, sp(0, 5));
    assert_eq!(v.as_integer(), Some(-9999));
}

#[test]
fn float_create_access() {
    let v = Value::float(3.14, sp(0, 4));
    assert_eq!(v.tag(), TAG_FLOAT);
    assert_eq!(v.as_float(), Some(3.14));
    assert_eq!(v.span(), sp(0, 4));
}

#[test]
fn boolean_create_access() {
    let t = Value::boolean(true, sp(0, 4));
    let f = Value::boolean(false, sp(5, 10));
    assert_eq!(t.tag(), TAG_BOOLEAN);
    assert_eq!(t.as_bool(), Some(true));
    assert_eq!(f.as_bool(), Some(false));
}

#[test]
fn array_create_and_drop() {
    let mut arr = Array::new();
    arr.push(Value::integer(1, sp(0, 1)));
    arr.push(Value::integer(2, sp(2, 3)));
    let v = Value::array(arr, sp(0, 3));
    assert_eq!(v.tag(), TAG_ARRAY);
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 2);
}

#[test]
fn table_create_and_drop() {
    let mut tab = Table::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Value::integer(10, sp(2, 4)),
    );
    let v = Value::table(tab, sp(0, 4));
    assert_eq!(v.tag(), TAG_TABLE);
    let t = v.as_table().unwrap();
    assert_eq!(t.len(), 1);
}

#[test]
fn table_header_create_drop() {
    let v = Value::table_header(Table::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE_HEADER);
    assert!(v.has_header_bit());
    assert!(v.as_table().is_some());
}

#[test]
fn table_dotted_create_drop() {
    let v = Value::table_dotted(Table::new(), sp(0, 10));
    assert_eq!(v.tag(), TAG_TABLE_DOTTED);
    assert!(v.has_dotted_bit());
    assert!(v.as_table().is_some());
}

// -- Span bit packing -------------------------------------------------------

#[test]
fn span_roundtrip_all_tags() {
    let tags_and_constructors: Vec<(u32, Value<'_>)> = vec![
        (TAG_STRING, Value::string(Str::from("x"), sp(100, 200))),
        (TAG_INTEGER, Value::integer(0, sp(100, 200))),
        (TAG_FLOAT, Value::float(0.0, sp(100, 200))),
        (TAG_BOOLEAN, Value::boolean(false, sp(100, 200))),
        (TAG_ARRAY, Value::array(Array::new(), sp(100, 200))),
        (TAG_TABLE, Value::table(Table::new(), sp(100, 200))),
        (
            TAG_TABLE_HEADER,
            Value::table_header(Table::new(), sp(100, 200)),
        ),
        (
            TAG_TABLE_DOTTED,
            Value::table_dotted(Table::new(), sp(100, 200)),
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
    let v = Value::integer(0, sp(max_start, max_end));
    assert_eq!(v.span().start(), max_start);
    assert_eq!(v.span().end(), max_end);
}

#[test]
fn span_zero() {
    let v = Value::integer(0, sp(0, 0));
    assert_eq!(v.span(), sp(0, 0));
}

// -- Flag bit ---------------------------------------------------------------

#[test]
fn flag_bit_aot() {
    let v = Value::array_aot(Array::new(), sp(10, 20));
    assert!(v.is_aot());
    assert!(v.is_frozen());
    assert_eq!(v.span(), sp(10, 20));
}

#[test]
fn flag_bit_frozen_table() {
    let v = Value::table_frozen(Table::new(), sp(5, 15));
    assert!(v.is_frozen());
    assert!(v.as_table().is_some());
    assert_eq!(v.span(), sp(5, 15));
}

// -- as_ref / as_mut --------------------------------------------------------

#[test]
fn as_ref_all_types() {
    let vals: Vec<Value<'_>> = vec![
        Value::string(Str::from("s"), sp(0, 1)),
        Value::integer(1, sp(0, 1)),
        Value::float(1.0, sp(0, 1)),
        Value::boolean(true, sp(0, 1)),
        Value::array(Array::new(), sp(0, 1)),
        Value::table(Table::new(), sp(0, 1)),
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
    let mut v = Value::integer(10, sp(0, 2));
    if let ValueMut::Integer(i) = v.as_mut() {
        *i = 99;
    }
    assert_eq!(v.as_integer(), Some(99));
}

#[test]
fn as_mut_modify_float() {
    let mut v = Value::float(1.0, sp(0, 3));
    if let ValueMut::Float(f) = v.as_mut() {
        *f = 2.5;
    }
    assert_eq!(v.as_float(), Some(2.5));
}

#[test]
fn as_mut_modify_boolean() {
    let mut v = Value::boolean(false, sp(0, 5));
    if let ValueMut::Boolean(b) = v.as_mut() {
        *b = true;
    }
    assert_eq!(v.as_bool(), Some(true));
}

#[test]
fn as_mut_modify_array() {
    let mut v = Value::array(Array::new(), sp(0, 2));
    if let ValueMut::Array(a) = v.as_mut() {
        a.push(Value::integer(42, sp(0, 2)));
    }
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[test]
fn as_mut_modify_table() {
    let mut v = Value::table(Table::new(), sp(0, 2));
    if let ValueMut::Table(t) = v.as_mut() {
        t.insert(
            Key {
                name: Str::from("x"),
                span: sp(0, 1),
            },
            Value::integer(1, sp(0, 1)),
        );
    }
    assert_eq!(v.as_table().unwrap().len(), 1);
}

// -- into_kind --------------------------------------------------------------

#[test]
fn into_kind_string() {
    let v = Value::string(Str::from("owned"), sp(0, 5));
    let ValueOwned::String(s) = v.into_kind() else {
        panic!("expected string")
    };
    assert_eq!(&*s, "owned");
}

#[test]
fn into_kind_integer() {
    let v = Value::integer(42, sp(0, 2));
    let ValueOwned::Integer(i) = v.into_kind() else {
        panic!("expected integer")
    };
    assert_eq!(i, 42);
}

#[test]
fn into_kind_array() {
    let mut arr = Array::new();
    arr.push(Value::integer(1, sp(0, 1)));
    arr.push(Value::integer(2, sp(0, 1)));
    let v = Value::array(arr, sp(0, 5));
    let ValueOwned::Array(a) = v.into_kind() else {
        panic!("expected array")
    };
    assert_eq!(a.len(), 2);
}

#[test]
fn into_kind_table() {
    let mut tab = Table::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Value::integer(1, sp(0, 1)),
    );
    let v = Value::table(tab, sp(0, 5));
    let ValueOwned::Table(t) = v.into_kind() else {
        panic!("expected table")
    };
    assert_eq!(t.len(), 1);
}

// -- Negative type checks ---------------------------------------------------

#[test]
fn negative_type_checks() {
    let v = Value::integer(42, sp(0, 2));
    assert!(v.as_str().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Value::string(Str::from("s"), sp(0, 1));
    assert!(v.as_integer().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_bool().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());

    let v = Value::boolean(true, sp(0, 4));
    assert!(v.as_str().is_none());
    assert!(v.as_integer().is_none());
    assert!(v.as_float().is_none());
    assert!(v.as_array().is_none());
    assert!(v.as_table().is_none());
}

// -- take / take_string / set_table -----------------------------------------

#[test]
fn take_replaces_with_boolean() {
    let mut v = Value::integer(42, sp(3, 7));
    let taken = v.take();
    let ValueOwned::Integer(i) = taken else {
        panic!("expected integer")
    };
    assert_eq!(i, 42);
    assert_eq!(v.as_bool(), Some(false));
    assert_eq!(v.span(), sp(3, 7));
}

#[test]
fn take_string_ok() {
    let mut v = Value::string(Str::from("hello"), sp(0, 5));
    let s = v.take_string(None).unwrap();
    assert_eq!(&*s, "hello");
}

#[test]
fn take_string_err() {
    let mut v = Value::integer(42, sp(0, 2));
    assert!(v.take_string(None).is_err());
}

#[test]
fn set_table_replaces() {
    let mut v = Value::integer(42, sp(0, 5));
    let mut tab = Table::new();
    tab.insert(
        Key {
            name: Str::from("k"),
            span: sp(0, 1),
        },
        Value::integer(1, sp(0, 1)),
    );
    v.set_table(tab);
    assert!(v.as_table().is_some());
    assert_eq!(v.as_table().unwrap().len(), 1);
    assert_eq!(v.span(), sp(0, 5));
}

// -- SpannedTable span helpers ----------------------------------------------

#[test]
fn spanned_table_set_span_preserves_tag() {
    let mut v = Value::table_header(Table::new(), sp(10, 20));
    let st = unsafe { v.as_spanned_table_mut_unchecked() };

    st.set_span_start(99);
    assert_eq!(v.tag(), TAG_TABLE_HEADER);
    assert_eq!(v.span().start(), 99);
}

// -- type_str / has_keys / has_key ------------------------------------------

#[test]
fn type_str_values() {
    assert_eq!(Value::string(Str::from(""), sp(0, 0)).type_str(), "string");
    assert_eq!(Value::integer(0, sp(0, 0)).type_str(), "integer");
    assert_eq!(Value::float(0.0, sp(0, 0)).type_str(), "float");
    assert_eq!(Value::boolean(false, sp(0, 0)).type_str(), "boolean");
    assert_eq!(Value::array(Array::new(), sp(0, 0)).type_str(), "array");
    assert_eq!(Value::table(Table::new(), sp(0, 0)).type_str(), "table");
    assert_eq!(
        Value::table_header(Table::new(), sp(0, 0)).type_str(),
        "table"
    );
    assert_eq!(
        Value::table_dotted(Table::new(), sp(0, 0)).type_str(),
        "table"
    );
}

#[test]
fn has_keys_and_has_key() {
    let empty = Value::table(Table::new(), sp(0, 0));
    assert!(!empty.has_keys());

    let mut tab = Table::new();
    tab.insert(
        Key {
            name: Str::from("x"),
            span: sp(0, 1),
        },
        Value::integer(1, sp(0, 1)),
    );
    let v = Value::table(tab, sp(0, 1));
    assert!(v.has_keys());
    assert!(v.has_key("x"));
    assert!(!v.has_key("y"));
}

// -- Debug ------------------------------------------------------------------

#[test]
fn debug_all_variants() {
    let vals: Vec<Value<'_>> = vec![
        Value::string(Str::from("s"), sp(0, 1)),
        Value::integer(42, sp(0, 2)),
        Value::float(3.14, sp(0, 4)),
        Value::boolean(true, sp(0, 4)),
        Value::array(Array::new(), sp(0, 2)),
        Value::table(Table::new(), sp(0, 2)),
        Value::table_header(Table::new(), sp(0, 2)),
        Value::table_dotted(Table::new(), sp(0, 2)),
    ];
    for v in &vals {
        let _ = format!("{v:?}");
    }
}
