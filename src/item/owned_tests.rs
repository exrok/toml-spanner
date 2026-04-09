use super::*;
use crate::Arena;
use crate::Span;
use crate::item::array::InternalArray;
use crate::item::table::InnerTable;
use crate::item::{Key, TAG_STRING};

fn sp(s: u32, e: u32) -> Span {
    Span::new(s, e)
}

#[test]
fn owned_item() {
    let arena = Arena::new();

    // Scalar types and forwarding methods
    let s = OwnedItem::from(&Item::string_spanned(arena.alloc_str("hello"), sp(0, 5)));
    assert_eq!(s.as_str(), Some("hello"));
    assert_eq!(s.kind(), crate::Kind::String);
    assert_eq!(s.item().tag(), TAG_STRING);

    let i = OwnedItem::from(&Item::integer_spanned(42, sp(0, 2)));
    assert_eq!(i.as_i64(), Some(42));
    assert_eq!(i.as_i128(), Some(42));
    assert_eq!(i.as_u64(), Some(42));
    assert_eq!(i.as_f64(), Some(42.0));

    let f = OwnedItem::from(&Item::float_spanned(3.14, sp(0, 4)));
    assert_eq!(f.as_f64(), Some(3.14));

    let b = OwnedItem::from(&Item::boolean(true, sp(0, 4)));
    assert_eq!(b.as_bool(), Some(true));

    // From<Item> by value
    assert_eq!(
        OwnedItem::from(Item::integer_spanned(99, sp(0, 2))).as_i64(),
        Some(99)
    );

    // value() dispatches correctly
    assert!(matches!(s.value(), crate::Value::String(_)));
    assert!(matches!(i.value(), crate::Value::Integer(_)));
    assert!(matches!(f.value(), crate::Value::Float(_)));
    assert!(matches!(b.value(), crate::Value::Boolean(_)));

    // Clone and PartialEq
    assert_eq!(s.clone(), s);
    assert_ne!(s, i);

    // Empty string (zero-size allocation path)
    let empty = OwnedItem::from(&Item::string_spanned(arena.alloc_str(""), sp(0, 0)));
    assert_eq!(empty.as_str(), Some(""));

    // Non-ASCII strings
    let unicode = OwnedItem::from(&Item::string_spanned(
        arena.alloc_str("日本語 🎉 émojis"),
        sp(0, 30),
    ));
    assert_eq!(unicode.as_str(), Some("日本語 🎉 émojis"));

    // Datetime from parsed document
    let doc = crate::parse("ts = 2024-01-15T08:30:00Z", &arena).unwrap();
    let dt = OwnedItem::from(doc.table().get("ts").unwrap());
    let date = dt.as_datetime().unwrap().date().unwrap();
    assert_eq!((date.year, date.month, date.day), (2024, 1, 15));

    // Empty containers (zero-size allocation path)
    let empty_tab = OwnedItem::from(&Item::table(InnerTable::new(), sp(0, 0)));
    assert_eq!(empty_tab.as_table().unwrap().len(), 0);
    assert!(empty_tab.has_keys() == false);

    let empty_arr = OwnedItem::from(&Item::array(InternalArray::new(), sp(0, 0)));
    assert_eq!(empty_arr.as_array().unwrap().len(), 0);

    // String survives arena drop
    let owned = {
        let tmp = Arena::new();
        OwnedItem::from(&Item::string_spanned(tmp.alloc_str("ephemeral"), sp(0, 9)))
    };
    assert_eq!(owned.as_str(), Some("ephemeral"));

    // Table survives arena drop
    let owned = {
        let tmp = Arena::new();
        let mut tab = InnerTable::new();
        tab.insert_unique(
            Key {
                name: tmp.alloc_str("key"),
                span: sp(0, 3),
            },
            Item::string_spanned(tmp.alloc_str("value"), sp(4, 9)),
            &tmp,
        );
        OwnedItem::from(&Item::table(tab, sp(0, 9)))
    };
    let t = owned.as_table().unwrap();
    assert_eq!(t.get("key").unwrap().as_str(), Some("value"));
    assert!(owned.has_key("key"));
    assert!(owned.has_keys());

    // Array survives arena drop
    let owned = {
        let tmp = Arena::new();
        let mut arr = InternalArray::new();
        arr.push(Item::string_spanned(tmp.alloc_str("one"), sp(0, 3)), &tmp);
        arr.push(Item::string_spanned(tmp.alloc_str("two"), sp(4, 7)), &tmp);
        OwnedItem::from(&Item::array(arr, sp(0, 7)))
    };
    let a = owned.as_array().unwrap();
    assert_eq!(
        (a.len(), a[0].as_str(), a[1].as_str()),
        (2, Some("one"), Some("two"))
    );

    // Nested table-in-array survives drop
    let owned = {
        let tmp = Arena::new();
        let mut t1 = InnerTable::new();
        t1.insert_unique(
            Key {
                name: tmp.alloc_str("name"),
                span: sp(0, 4),
            },
            Item::string_spanned(tmp.alloc_str("alice"), sp(5, 10)),
            &tmp,
        );
        let mut t2 = InnerTable::new();
        t2.insert_unique(
            Key {
                name: tmp.alloc_str("name"),
                span: sp(11, 15),
            },
            Item::string_spanned(tmp.alloc_str("bob"), sp(16, 19)),
            &tmp,
        );
        let mut arr = InternalArray::new();
        arr.push(Item::table(t1, sp(0, 10)), &tmp);
        arr.push(Item::table(t2, sp(11, 19)), &tmp);
        let mut root = InnerTable::new();
        root.insert_unique(
            Key {
                name: tmp.alloc_str("people"),
                span: sp(0, 6),
            },
            Item::array(arr, sp(0, 19)),
            &tmp,
        );
        OwnedItem::from(&Item::table(root, sp(0, 19)))
    };
    let people = owned
        .as_table()
        .unwrap()
        .get("people")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        people[0].as_table().unwrap().get("name").unwrap().as_str(),
        Some("alice")
    );
    assert_eq!(
        people[1].as_table().unwrap().get("name").unwrap().as_str(),
        Some("bob")
    );

    // Deeply nested from parsed document
    let owned = {
        let tmp = Arena::new();
        let doc = crate::parse("[a]\n[a.b]\n[a.b.c]\nval = \"deep\"", &tmp).unwrap();
        OwnedItem::from(doc.table().get("a").unwrap())
    };
    let c = owned
        .as_table()
        .unwrap()
        .get("b")
        .unwrap()
        .as_table()
        .unwrap()
        .get("c")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(c.get("val").unwrap().as_str(), Some("deep"));

    // Large table (>6 entries triggers hash index)
    let owned = {
        let tmp = Arena::new();
        let mut tab = InnerTable::new();
        for j in 0..20u32 {
            tab.insert_unique(
                Key {
                    name: tmp.alloc_str(&format!("key_{j}")),
                    span: sp(j * 10, j * 10 + 5),
                },
                Item::integer_spanned(j as i128, sp(j * 10 + 6, j * 10 + 8)),
                &tmp,
            );
        }
        OwnedItem::from(&Item::table(tab, sp(0, 200)))
    };
    let t = owned.as_table().unwrap();
    assert_eq!(t.len(), 20);
    for j in 0..20u32 {
        assert_eq!(t.get(&format!("key_{j}")).unwrap().as_i64(), Some(j as i64));
    }

    // Multiple independent items from same parse
    let (x, y) = {
        let tmp = Arena::new();
        let doc = crate::parse("x = \"aaa\"\ny = \"bbb\"", &tmp).unwrap();
        (
            OwnedItem::from(doc.table().get("x").unwrap()),
            OwnedItem::from(doc.table().get("y").unwrap()),
        )
    };
    assert_eq!(x.as_str(), Some("aaa"));
    assert_eq!(y.as_str(), Some("bbb"));
}

#[cfg(feature = "to-toml")]
#[test]
fn serialization() {
    use crate::{FromToml, ToToml};

    // OwnedItem: FromToml
    let arena = Arena::new();
    let mut doc = crate::parse("val = true", &arena).unwrap();
    let (ctx, table) = doc.split();
    let owned = OwnedItem::from_toml(ctx, table.get("val").unwrap()).unwrap();
    assert_eq!(owned.as_bool(), Some(true));

    // OwnedItem: ToToml roundtrip
    let owned = {
        let tmp = Arena::new();
        let doc = crate::parse("x = 42\ny = \"hello\"", &tmp).unwrap();
        OwnedItem::from(doc.into_item())
    };
    let arena = Arena::new();
    let item = owned.to_toml(&arena).unwrap();
    let t = item.as_table().unwrap();
    assert_eq!(t.get("x").unwrap().as_i64(), Some(42));
    assert_eq!(t.get("y").unwrap().as_str(), Some("hello"));

    // OwnedTable: FromToml
    let arena = Arena::new();
    let mut doc = crate::parse("[data]\nk = 1", &arena).unwrap();
    let (ctx, table) = doc.split();
    let owned = OwnedTable::from_toml(ctx, table.get("data").unwrap()).unwrap();
    assert_eq!(owned.get("k").unwrap().as_i64(), Some(1));

    // OwnedTable: ToToml roundtrip
    let arena = Arena::new();
    let rt = owned.to_toml(&arena).unwrap();
    assert_eq!(rt.as_table().unwrap().get("k").unwrap().as_i64(), Some(1));
}

#[test]
fn owned_table() {
    let arena = Arena::new();
    let doc = crate::parse(
        "\
[server]
host = 'localhost'
port = 8080
",
        &arena,
    )
    .unwrap();
    let owned = OwnedTable::from(doc.table().get("server").unwrap().as_table().unwrap());

    // Forwarding methods
    assert_eq!(owned.len(), 2);
    assert!(!owned.is_empty());
    assert!(owned.contains_key("host"));
    assert!(!owned.contains_key("missing"));
    assert_eq!(owned.get("host").unwrap().as_str(), Some("localhost"));
    assert_eq!(owned.get("port").unwrap().as_i64(), Some(8080));

    let (key, val) = owned.get_key_value("host").unwrap();
    assert_eq!(key.name, "host");
    assert_eq!(val.as_str(), Some("localhost"));

    assert_eq!(owned.style(), crate::TableStyle::Header);
    assert_eq!(owned.as_item().kind(), crate::Kind::Table);

    // entries and iter
    let entries = owned.entries();
    assert_eq!(entries.len(), 2);
    let via_iter: Vec<_> = owned.iter().map(|(k, _)| k.name).collect();
    assert_eq!(via_iter, ["host", "port"]);

    // IntoIterator
    let via_for: Vec<_> = (&owned).into_iter().map(|(k, _)| k.name).collect();
    assert_eq!(via_for, ["host", "port"]);

    // Clone and PartialEq
    let cloned = owned.clone();
    assert_eq!(cloned, owned);
    assert_eq!(cloned.len(), 2);

    // Debug
    let debug = format!("{:?}", owned);
    assert!(debug.contains("localhost"));

    // Survives arena drop
    let owned = {
        let tmp = Arena::new();
        let doc = crate::parse("a = 1\nb = 2", &tmp).unwrap();
        OwnedTable::from(doc.table())
    };
    assert_eq!(owned.get("a").unwrap().as_i64(), Some(1));
    assert_eq!(owned.get("b").unwrap().as_i64(), Some(2));

    // Empty table
    let empty = OwnedTable::from(&crate::Table::new());
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
}
