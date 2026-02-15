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

// == InnerTable tests ========================================================

#[test]
fn inner_insert_and_realloc() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);

    // First insert triggers initial allocation
    t.insert(key("k0"), ival(0), &arena);
    assert_eq!(t.len(), 1);
    assert_eq!(t.get("k0").unwrap().as_integer(), Some(0));

    // Second insert fills initial capacity (2)
    t.insert(key("k1"), ival(1), &arena);
    assert_eq!(t.len(), 2);
    assert_eq!(t.get("k0").unwrap().as_integer(), Some(0));
    assert_eq!(t.get("k1").unwrap().as_integer(), Some(1));

    // Third insert triggers realloc from 2 to 4
    t.insert(key("k2"), ival(2), &arena);
    assert_eq!(t.len(), 3);
    assert_eq!(t.get("k0").unwrap().as_integer(), Some(0));
    assert_eq!(t.get("k1").unwrap().as_integer(), Some(1));
    assert_eq!(t.get("k2").unwrap().as_integer(), Some(2));

    // Fourth and fifth inserts trigger realloc from 4 to 8
    t.insert(key("k3"), ival(3), &arena);
    t.insert(key("k4"), ival(4), &arena);
    assert_eq!(t.len(), 5);
    assert!(!t.is_empty());
    for i in 0..5 {
        let name = format!("k{i}");
        assert_eq!(t.get(&name).unwrap().as_integer(), Some(i));
    }
}

#[test]
fn inner_get_and_mutate() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(10), &arena);
    t.insert(key("b"), ival(20), &arena);

    // get: found and not-found
    assert_eq!(t.get("a").unwrap().as_integer(), Some(10));
    assert!(t.get("missing").is_none());

    // get_key_value: returns both key and value
    let (k, v) = t.get_key_value("a").unwrap();
    assert_eq!(&*k.name, "a");
    assert_eq!(v.as_integer(), Some(10));

    // get_mut: modify in place
    let v = t.get_mut("a").unwrap();
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(t.get("a").unwrap().as_integer(), Some(99));

    // get_mut: not-found
    assert!(t.get_mut("missing").is_none());

    // contains_key
    assert!(t.contains_key("a"));
    assert!(t.contains_key("b"));
    assert!(!t.contains_key("missing"));
}

#[test]
fn inner_index_access() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(
        Key {
            name: Str::from("first"),
            span: Span::new(10, 15),
        },
        ival(1),
        &arena,
    );
    t.insert(key("second"), ival(2), &arena);

    // get_key_value_at
    let (k, v) = t.get_key_value_at(0);
    assert_eq!(&*k.name, "first");
    assert_eq!(v.as_integer(), Some(1));
    let (k, v) = t.get_key_value_at(1);
    assert_eq!(&*k.name, "second");
    assert_eq!(v.as_integer(), Some(2));

    // get_mut_at: modify value at index 1
    let v = t.get_mut_at(1);
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(t.get("second").unwrap().as_integer(), Some(99));

    // first_key_span_start returns span.start of the first key
    assert_eq!(t.first_key_span_start(), 10);
}

#[test]
fn inner_remove() {
    let arena = Arena::new();

    // Remove only element
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    let v = t.remove("a").unwrap();
    assert_eq!(v.as_integer(), Some(1));
    assert!(t.is_empty());

    // Remove not-found
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    assert!(t.remove("missing").is_none());
    assert_eq!(t.len(), 1);

    // Swap-remove behavior: removing first swaps last element into its slot
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("a").unwrap();
    assert_eq!(v.as_integer(), Some(1));
    assert_eq!(t.len(), 2);
    let entries = t.entries();
    assert_eq!(&*entries[0].0.name, "c"); // last swapped into first
    assert_eq!(&*entries[1].0.name, "b");

    // Swap-remove behavior: removing middle swaps last element into its slot
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("b").unwrap();
    assert_eq!(v.as_integer(), Some(2));
    assert_eq!(t.len(), 2);
    let entries = t.entries();
    assert_eq!(&*entries[0].0.name, "a");
    assert_eq!(&*entries[1].0.name, "c"); // last swapped into middle

    // Removing last element: no swap needed
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);
    let v = t.remove("c").unwrap();
    assert_eq!(v.as_integer(), Some(3));
    assert_eq!(t.len(), 2);

    // remove_entry returns both key and value
    let mut t = InnerTable::new();
    t.insert(key("mykey"), ival(42), &arena);
    let (k, v) = t.remove_entry("mykey").unwrap();
    assert_eq!(&*k.name, "mykey");
    assert_eq!(v.as_integer(), Some(42));
    assert!(t.is_empty());
}

#[test]
fn inner_iterators() {
    let arena = Arena::new();
    let mut t = InnerTable::new();
    t.insert(key("a"), ival(1), &arena);
    t.insert(key("b"), ival(2), &arena);
    t.insert(key("c"), ival(3), &arena);

    // values_mut: mutate all values
    for v in t.values_mut() {
        if let crate::value::ValueMut::Integer(i) = v.value_mut() {
            *i += 100;
        }
    }
    assert_eq!(t.get("a").unwrap().as_integer(), Some(101));
    assert_eq!(t.get("b").unwrap().as_integer(), Some(102));
    assert_eq!(t.get("c").unwrap().as_integer(), Some(103));

    // IntoIter: collect and verify
    let iter = IntoIter { table: t, index: 0 };
    assert_eq!(iter.size_hint(), (3, Some(3)));
    assert_eq!(iter.len(), 3);
    let vals: Vec<_> = iter.collect();
    assert_eq!(vals.len(), 3);
    assert_eq!(vals[0].1.as_integer(), Some(101));

    // IntoIter: size_hint updates after next()
    let mut t2 = InnerTable::new();
    t2.insert(key("x"), ival(10), &arena);
    t2.insert(key("y"), ival(20), &arena);
    let mut iter = IntoIter {
        table: t2,
        index: 0,
    };
    assert_eq!(iter.size_hint(), (2, Some(2)));
    iter.next();
    assert_eq!(iter.size_hint(), (1, Some(1)));
}

// == Table wrapper tests =====================================================

fn make_table<'a>(arena: &'a Arena) -> Table<'a> {
    let mut table = Table::new(Span::new(0, 100));
    table.insert(key("a"), ival(1), arena);
    table.insert(key("b"), ival(2), arena);
    table.insert(key("c"), ival(3), arena);
    table
}

#[test]
fn table_access_and_mutation() {
    let arena = Arena::new();
    let mut table = make_table(&arena);

    // Basic properties
    assert_eq!(table.len(), 3);
    assert!(!table.is_empty());
    assert_eq!(table.span(), Span::new(0, 100));

    // get
    assert_eq!(table.get("a").unwrap().as_integer(), Some(1));
    assert!(table.get("missing").is_none());

    // get_key_value
    let (k, v) = table.get_key_value("b").unwrap();
    assert_eq!(&*k.name, "b");
    assert_eq!(v.as_integer(), Some(2));
    assert!(table.get_key_value("missing").is_none());

    // get_mut: modify in place
    let v = table.get_mut("a").unwrap();
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(table.get("a").unwrap().as_integer(), Some(99));
    assert!(table.get_mut("missing").is_none());

    // values_mut: mutate all values
    for v in table.values_mut() {
        if let crate::value::ValueMut::Integer(i) = v.value_mut() {
            *i += 10;
        }
    }
    assert_eq!(table.get("a").unwrap().as_integer(), Some(109));
    assert_eq!(table.get("b").unwrap().as_integer(), Some(12));

    // remove
    let v = table.remove("b").unwrap();
    assert_eq!(v.as_integer(), Some(12));
    assert_eq!(table.len(), 2);
    assert!(table.remove("missing").is_none());

    // remove_entry
    let (k, v) = table.remove_entry("a").unwrap();
    assert_eq!(&*k.name, "a");
    assert_eq!(v.as_integer(), Some(109));
    assert!(table.remove_entry("missing").is_none());

    // into_item
    let arena2 = Arena::new();
    let table2 = make_table(&arena2);
    let item = table2.into_item();
    assert!(item.as_table().is_some());
    assert_eq!(item.as_table().unwrap().len(), 3);
}

#[test]
fn table_deserialization_helpers() {
    let arena = Arena::new();

    // required: success consumes the key
    let mut table = make_table(&arena);
    let v: i64 = table.required("a").unwrap();
    assert_eq!(v, 1);
    assert!(table.get("a").is_none());

    // required: missing key returns MissingField error
    let err = table.required::<i64>("missing").unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::MissingField("missing")));

    // optional: present key returns Some
    let mut table = make_table(&arena);
    let v: Option<i64> = table.optional("a").unwrap();
    assert_eq!(v, Some(1));

    // optional: missing key returns None
    let v: Option<i64> = table.optional("missing").unwrap();
    assert!(v.is_none());

    // optional: wrong type returns Wanted error
    let mut table = make_table(&arena);
    let err = table.optional::<String>("a").unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::Wanted { .. }));

    // expect_empty: succeeds when all keys are consumed
    let mut table = make_table(&arena);
    table.remove("a");
    table.remove("b");
    table.remove("c");
    table.expect_empty().unwrap();

    // expect_empty: fails when keys remain
    let table = make_table(&arena);
    let err = table.expect_empty().unwrap_err();
    assert!(matches!(err.kind, crate::ErrorKind::UnexpectedKeys { .. }));
}

#[test]
fn table_iterators() {
    let arena = Arena::new();

    // Immutable iteration via &table
    let table = make_table(&arena);
    let mut count = 0;
    for (k, v) in &table {
        assert!(v.as_integer().is_some());
        assert!(!k.name.is_empty());
        count += 1;
    }
    assert_eq!(count, 3);

    // Mutable iteration via &mut table
    let mut table = make_table(&arena);
    for (_, v) in &mut table {
        if let crate::value::ValueMut::Integer(i) = v.value_mut() {
            *i += 100;
        }
    }
    assert_eq!(table.get("a").unwrap().as_integer(), Some(101));

    // Owned iteration via into_iter
    let table = make_table(&arena);
    let vals: Vec<(Key<'_>, Item<'_>)> = table.into_iter().collect();
    assert_eq!(vals.len(), 3);
}

#[test]
fn table_span_helpers() {
    let mut table = Table::new(Span::new(10, 20));

    // span_start / set_span_start
    assert_eq!(table.span_start(), 10);
    table.set_span_start(50);
    assert_eq!(table.span_start(), 50);
    assert_eq!(table.span().start, 50);

    // set_span_end
    table.set_span_end(100);
    assert_eq!(table.span().end, 100);

    // extend_span_end: only updates if new value is greater
    table.extend_span_end(90); // less than current 100, no change
    assert_eq!(table.span().end, 100);
    table.extend_span_end(200); // greater, updates
    assert_eq!(table.span().end, 200);

    // set_header_flag preserves span
    let mut table = Table::new(Span::new(10, 20));
    table.set_header_flag();
    assert_eq!(table.span(), Span::new(10, 20));
}

#[test]
fn default_and_debug() {
    let arena = Arena::new();

    // Table::default - public type
    let table: Table<'_> = Table::default();
    assert_eq!(table.len(), 0);
    assert!(table.span().is_empty());

    // Table::Debug - public type
    let mut table = Table::new(Span::new(0, 10));
    table.insert(key("y"), ival(99), &arena);
    let debug = format!("{:?}", table);
    assert!(debug.contains("y") || debug.contains("99"));

    // Table::entries - public API
    let table = make_table(&arena);
    let entries = table.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(&*entries[0].0.name, "a");
}

#[test]
fn index_operator() {
    let arena = Arena::new();
    let table = make_table(&arena);

    // Valid keys return MaybeItem with the value
    assert_eq!(table["a"].as_integer(), Some(1));
    assert_eq!(table["b"].as_integer(), Some(2));
    assert_eq!(table["c"].as_integer(), Some(3));

    // Missing keys return NONE (no panic)
    assert!(table["missing"].item().is_none());
    assert!(table[""].item().is_none());

    // NONE propagates through chained indexing
    assert!(table["missing"]["nested"].item().is_none());
    assert!(table["missing"][0].item().is_none());

    // Nested table indexing
    let mut inner = InnerTable::new();
    inner.insert(key("x"), ival(42), &arena);
    let mut outer = Table::new(Span::new(0, 50));
    outer.insert(
        key("nested"),
        Item::table(inner, Span::new(0, 20)),
        &arena,
    );
    assert_eq!(outer["nested"]["x"].as_integer(), Some(42));
    assert!(outer["nested"]["y"].item().is_none());

    // Empty table always returns NONE
    let empty = Table::new(Span::new(0, 0));
    assert!(empty["anything"].item().is_none());
}
