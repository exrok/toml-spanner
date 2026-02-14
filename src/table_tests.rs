use super::*;
use crate::Span;
use crate::arena::Arena;
use crate::str::Str;
use crate::value::Item;

fn sp() -> Span {
    Span::new(0, 0)
}

fn key(name: &str) -> Key<'_> {
    Key {
        name: Str::from(name),
        span: sp(),
    }
}

fn ival(i: i64) -> Item<'static> {
    Item::integer(i, sp())
}

// -- Empty table ------------------------------------------------------------

#[test]
fn new_empty_drop() {
    let t = InnerTable::new();
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
}

#[test]
fn default_is_empty() {
    let t = InnerTable::default();
    assert!(t.is_empty());
}

// -- Insert + allocation thresholds -----------------------------------------

#[test]
fn insert_first_triggers_alloc() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    assert_eq!(t.len(), 1);
    assert_eq!(t.get("a").unwrap().as_integer(), Some(1));
}

#[test]
fn insert_two_fills_capacity() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    assert_eq!(t.len(), 2);
    assert_eq!(t.get("a").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("b").unwrap().as_integer(), Some(2));
}

#[test]
fn insert_realloc_2_to_4() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("k0"), ival(0), &arena);
    t.insert(key("k1"), ival(1), &arena);
    t.insert(key("k2"), ival(2), &arena);
    assert_eq!(t.len(), 3);
    assert_eq!(t.get("k0").unwrap().as_integer(), Some(0));
    assert_eq!(t.get("k1").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("k2").unwrap().as_integer(), Some(2));
}

#[test]
fn insert_realloc_4_to_8() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("k0"), ival(0), &arena);
    t.insert(key("k1"), ival(1), &arena);
    t.insert(key("k2"), ival(2), &arena);
    t.insert(key("k3"), ival(3), &arena);
    t.insert(key("k4"), ival(4), &arena);
    assert_eq!(t.len(), 5);
    assert_eq!(t.get("k0").unwrap().as_integer(), Some(0));
    assert_eq!(t.get("k4").unwrap().as_integer(), Some(4));
}

// -- get / get_key_value / get_mut ------------------------------------------

#[test]
fn get_not_found() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    assert!(t.get("b").is_none());
}

#[test]
fn get_key_value_found() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("mykey"), ival(42), &arena);
    let (k, v) = t.get_key_value("mykey").unwrap();
    assert_eq!(&*k.name, "mykey");
    assert_eq!(v.as_integer(), Some(42));
}

#[test]
fn get_mut_modifies() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(10), &arena);
    let v = t.get_mut("a").unwrap();
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(t.get("a").unwrap().as_integer(), Some(99));
}

#[test]
fn get_mut_not_found() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    assert!(t.get_mut("b").is_none());
}

// -- Internal index access --------------------------------------------------

#[test]
fn get_key_value_at_valid() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("first"), ival(1), &arena);
    t.insert(key("second"), ival(2), &arena);
    let (k, v) = t.get_key_value_at(0);
    assert_eq!(&*k.name, "first");
    assert_eq!(v.as_integer(), Some(1));
    let (k, v) = t.get_key_value_at(1);
    assert_eq!(&*k.name, "second");
    assert_eq!(v.as_integer(), Some(2));
}

#[test]
fn get_mut_at_valid() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(10), &arena);
    t.insert(key("b"), ival(20), &arena);
    let v = t.get_mut_at(1);
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(t.get("b").unwrap().as_integer(), Some(99));
}

#[test]
fn first_key_span_start_works() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(
        Key {
            name: Str::from("a"),
            span: Span::new(10, 11),
        },
        ival(1),
        &arena,
    );
    assert_eq!(t.first_key_span_start(), 10);
}

// -- contains_key -----------------------------------------------------------

#[test]
fn contains_key_found_and_not_found() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("present"), ival(1), &arena);
    assert!(t.contains_key("present"));
    assert!(!t.contains_key("absent"));
}

// -- Remove -----------------------------------------------------------------

#[test]
fn remove_only_element() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    let v = t.remove("a").unwrap();
    assert_eq!(v.as_integer(), Some(1));
    assert!(t.is_empty());
}

#[test]
fn remove_first_swaps_with_last() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("a").unwrap();
    assert_eq!(v.as_integer(), Some(1));
    assert_eq!(t.len(), 2);
    let entries = t.entries();
    assert_eq!(&*entries[0].0.name, "c");
    assert_eq!(&*entries[1].0.name, "b");
}

#[test]
fn remove_middle_swaps_with_last() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("b").unwrap();
    assert_eq!(v.as_integer(), Some(2));
    assert_eq!(t.len(), 2);
    let entries = t.entries();
    assert_eq!(&*entries[0].0.name, "a");
    assert_eq!(&*entries[1].0.name, "c");
}

#[test]
fn remove_last() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("c").unwrap();
    assert_eq!(v.as_integer(), Some(3));
    assert_eq!(t.len(), 2);
}

#[test]
fn remove_not_found() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    assert!(t.remove("b").is_none());
}

#[test]
fn remove_entry_returns_key_and_value() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("mykey"), ival(42), &arena);
    let (k, v) = t.remove_entry("mykey").unwrap();
    assert_eq!(&*k.name, "mykey");
    assert_eq!(v.as_integer(), Some(42));
}

// -- values_mut -------------------------------------------------------------

#[test]
fn values_mut_modifies() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    for v in t.values_mut() {
        if let crate::value::ValueMut::Integer(i) = v.value_mut() {
            *i += 100;
        }
    }
    assert_eq!(t.get("a").unwrap().as_integer(), Some(101));
    assert_eq!(t.get("b").unwrap().as_integer(), Some(102));
}

// -- Drop with heap-owning values -------------------------------------------

#[test]
fn drop_with_owned_strings() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), Item::string(Str::from("owned1"), sp()), &arena);
    t.insert(key("b"), Item::string(Str::from("owned2"), sp()), &arena);
}

#[test]
fn drop_with_nested_tables() {
    let arena = Arena::new();
    let mut inner = InnerTable::new();
    inner.insert(key("inner_key"), ival(1), &arena);
    let mut outer = InnerTable::new();
    outer.insert(key("nested"), Item::table(inner, sp()), &arena);
    outer.insert(key("plain"), ival(2), &arena);
}

// -- Debug ------------------------------------------------------------------

#[test]
fn debug_format() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    let s = format!("{t:?}");
    assert!(s.contains('a'));
}
