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
fn from_string_item() {
    let arena = Arena::new();
    let item = Item::string_spanned(arena.alloc_str("hello"), sp(0, 5));
    let owned = OwnedItem::from(&item);
    assert_eq!(owned.item().as_str(), Some("hello"));
}

#[test]
fn from_integer_item() {
    let item = Item::integer_spanned(42, sp(0, 2));
    let owned = OwnedItem::from(&item);
    assert_eq!(owned.item().as_i64(), Some(42));
}

#[test]
fn from_float_item() {
    let item = Item::float_spanned(3.14, sp(0, 4));
    let owned = OwnedItem::from(&item);
    assert_eq!(owned.item().as_f64(), Some(3.14));
}

#[test]
fn from_boolean_item() {
    let item = Item::boolean(true, sp(0, 4));
    let owned = OwnedItem::from(&item);
    assert_eq!(owned.item().as_bool(), Some(true));
}

#[test]
fn from_item_by_value() {
    let item = Item::integer_spanned(99, sp(0, 2));
    let owned = OwnedItem::from(item);
    assert_eq!(owned.item().as_i64(), Some(99));
}

#[test]
fn survives_source_arena_drop() {
    let owned = {
        let arena = Arena::new();
        let s = arena.alloc_str("ephemeral");
        let item = Item::string_spanned(s, sp(0, 9));
        OwnedItem::from(&item)
    };
    assert_eq!(owned.item().as_str(), Some("ephemeral"));
}

#[test]
fn table_survives_source_arena_drop() {
    let owned = {
        let arena = Arena::new();
        let mut tab = InnerTable::new();
        tab.insert_unique(
            Key {
                name: arena.alloc_str("key"),
                span: sp(0, 3),
            },
            Item::string_spanned(arena.alloc_str("value"), sp(4, 9)),
            &arena,
        );
        let item = Item::table(tab, sp(0, 9));
        OwnedItem::from(&item)
    };
    let t = owned.item().as_table().unwrap();
    assert_eq!(t.get("key").unwrap().as_str(), Some("value"));
}

#[test]
fn array_survives_source_arena_drop() {
    let owned = {
        let arena = Arena::new();
        let mut arr = InternalArray::new();
        arr.push(
            Item::string_spanned(arena.alloc_str("one"), sp(0, 3)),
            &arena,
        );
        arr.push(
            Item::string_spanned(arena.alloc_str("two"), sp(4, 7)),
            &arena,
        );
        let item = Item::array(arr, sp(0, 7));
        OwnedItem::from(&item)
    };
    let a = owned.item().as_array().unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!(a[0].as_str(), Some("one"));
    assert_eq!(a[1].as_str(), Some("two"));
}

#[test]
fn nested_table_in_array_survives_drop() {
    let owned = {
        let arena = Arena::new();

        let mut t1 = InnerTable::new();
        t1.insert_unique(
            Key {
                name: arena.alloc_str("name"),
                span: sp(0, 4),
            },
            Item::string_spanned(arena.alloc_str("alice"), sp(5, 10)),
            &arena,
        );

        let mut t2 = InnerTable::new();
        t2.insert_unique(
            Key {
                name: arena.alloc_str("name"),
                span: sp(11, 15),
            },
            Item::string_spanned(arena.alloc_str("bob"), sp(16, 19)),
            &arena,
        );

        let mut arr = InternalArray::new();
        arr.push(Item::table(t1, sp(0, 10)), &arena);
        arr.push(Item::table(t2, sp(11, 19)), &arena);

        let mut root = InnerTable::new();
        root.insert_unique(
            Key {
                name: arena.alloc_str("people"),
                span: sp(0, 6),
            },
            Item::array(arr, sp(0, 19)),
            &arena,
        );
        let item = Item::table(root, sp(0, 19));
        OwnedItem::from(&item)
    };

    let t = owned.item().as_table().unwrap();
    let people = t.get("people").unwrap().as_array().unwrap();
    assert_eq!(people.len(), 2);
    assert_eq!(
        people[0].as_table().unwrap().get("name").unwrap().as_str(),
        Some("alice")
    );
    assert_eq!(
        people[1].as_table().unwrap().get("name").unwrap().as_str(),
        Some("bob")
    );
}

#[test]
fn from_parsed_document() {
    let owned = {
        let arena = Arena::new();
        let doc = crate::parse("[server]\nhost = \"localhost\"\nport = 8080", &arena).unwrap();
        let server = doc.table().get("server").unwrap();
        OwnedItem::from(server)
    };

    let t = owned.item().as_table().unwrap();
    assert_eq!(t.get("host").unwrap().as_str(), Some("localhost"));
    assert_eq!(t.get("port").unwrap().as_i64(), Some(8080));
}

#[test]
fn empty_string() {
    let owned = {
        let arena = Arena::new();
        let item = Item::string_spanned(arena.alloc_str(""), sp(0, 0));
        OwnedItem::from(&item)
    };
    assert_eq!(owned.item().as_str(), Some(""));
}

#[test]
fn empty_table() {
    let tab = InnerTable::new();
    let item = Item::table(tab, sp(0, 0));
    let owned = OwnedItem::from(&item);
    let t = owned.item().as_table().unwrap();
    assert_eq!(t.len(), 0);
}

#[test]
fn empty_array() {
    let arr = InternalArray::new();
    let item = Item::array(arr, sp(0, 0));
    let owned = OwnedItem::from(&item);
    let a = owned.item().as_array().unwrap();
    assert_eq!(a.len(), 0);
}

#[test]
fn large_table_with_hash_index() {
    let owned = {
        let arena = Arena::new();
        let mut tab = InnerTable::new();
        for i in 0..20u32 {
            let key_name = arena.alloc_str(&format!("key_{i}"));
            tab.insert_unique(
                Key {
                    name: key_name,
                    span: sp(i * 10, i * 10 + 5),
                },
                Item::integer_spanned(i as i128, sp(i * 10 + 6, i * 10 + 8)),
                &arena,
            );
        }
        let item = Item::table(tab, sp(0, 200));
        OwnedItem::from(&item)
    };

    let t = owned.item().as_table().unwrap();
    assert_eq!(t.len(), 20);
    for i in 0..20u32 {
        let key = format!("key_{i}");
        assert_eq!(t.get(&key).unwrap().as_i64(), Some(i as i64));
    }
}

#[test]
fn preserves_item_kind() {
    let arena = Arena::new();
    let item = Item::string_spanned(arena.alloc_str("test"), sp(0, 4));
    let owned = OwnedItem::from(&item);
    assert_eq!(owned.item().tag(), TAG_STRING);
    assert_eq!(owned.item().kind(), crate::Kind::String);
}

#[test]
fn string_with_non_ascii() {
    let owned = {
        let arena = Arena::new();
        let s = arena.alloc_str("日本語テスト 🎉 émojis");
        let item = Item::string_spanned(s, sp(0, 30));
        OwnedItem::from(&item)
    };
    assert_eq!(owned.item().as_str(), Some("日本語テスト 🎉 émojis"));
}

#[test]
fn datetime_from_parse() {
    let owned = {
        let arena = Arena::new();
        let doc = crate::parse("ts = 2024-01-15T08:30:00Z", &arena).unwrap();
        let ts = doc.table().get("ts").unwrap();
        OwnedItem::from(ts)
    };
    let dt = owned.item().as_datetime().unwrap();
    assert_eq!(dt.date().unwrap().year, 2024);
    assert_eq!(dt.date().unwrap().month, 1);
    assert_eq!(dt.date().unwrap().day, 15);
}

#[test]
fn deeply_nested_structure() {
    let owned = {
        let arena = Arena::new();
        let doc = crate::parse("[a]\n[a.b]\n[a.b.c]\nval = \"deep\"", &arena).unwrap();
        OwnedItem::from(doc.table().get("a").unwrap())
    };

    let a = owned.item().as_table().unwrap();
    let b = a.get("b").unwrap().as_table().unwrap();
    let c = b.get("c").unwrap().as_table().unwrap();
    assert_eq!(c.get("val").unwrap().as_str(), Some("deep"));
}

#[cfg(feature = "to-toml")]
#[test]
fn to_toml_roundtrip() {
    use crate::ToToml;

    let owned = {
        let arena = Arena::new();
        let doc = crate::parse("x = 42\ny = \"hello\"", &arena).unwrap();
        OwnedItem::from(doc.into_item())
    };

    let arena = Arena::new();
    let item = owned.to_toml(&arena).unwrap();
    let t = item.as_table().unwrap();
    assert_eq!(t.get("x").unwrap().as_i64(), Some(42));
    assert_eq!(t.get("y").unwrap().as_str(), Some("hello"));
}

#[cfg(feature = "from-toml")]
#[test]
fn from_toml_impl() {
    use crate::FromToml;

    let arena = Arena::new();
    let mut doc = crate::parse("val = true", &arena).unwrap();
    let (ctx, table) = doc.split();
    let val_item = table.get("val").unwrap();

    let owned = OwnedItem::from_toml(ctx, val_item).unwrap();
    assert_eq!(owned.item().as_bool(), Some(true));
}

#[test]
fn multiple_owned_items_independent() {
    let (a, b) = {
        let arena = Arena::new();
        let doc = crate::parse("x = \"aaa\"\ny = \"bbb\"", &arena).unwrap();
        let a = OwnedItem::from(doc.table().get("x").unwrap());
        let b = OwnedItem::from(doc.table().get("y").unwrap());
        (a, b)
    };
    assert_eq!(a.item().as_str(), Some("aaa"));
    assert_eq!(b.item().as_str(), Some("bbb"));
}
