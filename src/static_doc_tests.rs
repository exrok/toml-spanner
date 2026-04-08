use super::*;

#[test]
fn parse_and_access() {
    let doc = StaticDocument::parse("key = 'value'").unwrap();
    assert_eq!(doc.table()["key"].as_str(), Some("value"));
    assert_eq!(doc.table()["missing"].as_str(), None);
}

#[test]
fn nested_tables() {
    let input = r#"
[server]
host = "localhost"
port = 8080

[server.tls]
enabled = true
"#;
    let doc = StaticDocument::parse(input).unwrap();
    assert_eq!(doc.table()["server"]["host"].as_str(), Some("localhost"));
    assert_eq!(doc.table()["server"]["port"].as_i64(), Some(8080));
    assert_eq!(doc.table()["server"]["tls"]["enabled"].as_bool(), Some(true));
}

#[test]
fn array_of_tables() {
    let input = r#"
[[items]]
name = "one"

[[items]]
name = "two"
"#;
    let doc = StaticDocument::parse(input).unwrap();
    assert_eq!(doc.table()["items"][0]["name"].as_str(), Some("one"));
    assert_eq!(doc.table()["items"][1]["name"].as_str(), Some("two"));
}

#[test]
fn escaped_strings() {
    let doc = StaticDocument::parse(r#"key = "hello\nworld""#).unwrap();
    assert_eq!(doc.table()["key"].as_str(), Some("hello\nworld"));
}

#[test]
fn returned_from_function() {
    fn make_doc() -> StaticDocument {
        StaticDocument::parse("x = 1").unwrap()
    }
    let doc = make_doc();
    assert_eq!(doc.table()["x"].as_i64(), Some(1));
}

#[test]
fn is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<StaticDocument>();
}

#[cfg(feature = "from-toml")]
#[test]
fn deserialize_hashmap() {
    use std::collections::HashMap;
    let mut doc = StaticDocument::parse("a = 1\nb = 2").unwrap();
    let map = doc.to::<HashMap<String, i64>>().unwrap();
    assert_eq!(map["a"], 1);
    assert_eq!(map["b"], 2);
}

#[cfg(feature = "from-toml")]
#[test]
fn deserialize_multiple_times() {
    use std::collections::HashMap;
    let mut doc = StaticDocument::parse("x = 'hello'").unwrap();
    let m1 = doc.to::<HashMap<String, String>>().unwrap();
    let m2 = doc.to::<HashMap<String, String>>().unwrap();
    assert_eq!(m1, m2);
}

#[cfg(feature = "from-toml")]
#[test]
fn source_preserved() {
    let input = "key = 'value'";
    let doc = StaticDocument::parse(input).unwrap();
    assert_eq!(doc.source(), input);
}

#[test]
fn debug_output() {
    let doc = StaticDocument::parse("x = 1").unwrap();
    let dbg = format!("{:?}", doc);
    assert!(dbg.contains("x"));
}

#[test]
fn table_access() {
    let doc = StaticDocument::parse("a = 1\nb = 2").unwrap();
    let table = doc.table();
    assert_eq!(table.len(), 2);
}

#[test]
fn parse_error() {
    let result = StaticDocument::parse("= invalid");
    assert!(result.is_err());
}

// --- BorrowedValue / StaticDocumentWith tests ---

#[derive(Debug)]
struct BorrowedConfig<'a> {
    name: &'a str,
    port: i64,
}

#[cfg(feature = "from-toml")]
impl<'de> crate::FromToml<'de> for BorrowedConfig<'de> {
    fn from_toml(
        ctx: &mut crate::de::Context<'de>,
        item: &crate::Item<'de>,
    ) -> Result<Self, crate::Failed> {
        let mut th = item.table_helper(ctx)?;
        let name = th.required("name")?;
        let port = th.required("port")?;
        th.expect_empty()?;
        Ok(BorrowedConfig { name, port })
    }
}

crate::impl_borrowed_value!(BorrowedConfig);

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_basic() {
    let doc = StaticDocument::parse("name = 'hello'\nport = 8080").unwrap();
    let with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();
    let config = with.value();
    assert_eq!(config.name, "hello");
    assert_eq!(config.port, 8080);
}

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_returned_from_function() {
    fn make() -> StaticDocumentWith<BorrowedConfig<'static>> {
        let doc = StaticDocument::parse("name = 'world'\nport = 443").unwrap();
        doc.to_borrowed().unwrap()
    }
    let with = make();
    assert_eq!(with.value().name, "world");
    assert_eq!(with.value().port, 443);
}

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_table_still_accessible() {
    let doc = StaticDocument::parse("name = 'test'\nport = 80").unwrap();
    let with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();
    assert_eq!(with.table()["name"].as_str(), Some("test"));
    assert_eq!(with.value().name, "test");
}

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_escaped_strings() {
    let doc = StaticDocument::parse(r#"name = "hello\nworld"
port = 1"#)
        .unwrap();
    let with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();
    assert_eq!(with.value().name, "hello\nworld");
}

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<StaticDocumentWith<BorrowedConfig<'static>>>();
}

#[cfg(feature = "from-toml")]
#[test]
fn to_borrowed_with_vec() {
    #[derive(Debug)]
    struct Multi<'a> {
        tags: Vec<&'a str>,
    }

    impl<'de> crate::FromToml<'de> for Multi<'de> {
        fn from_toml(
            ctx: &mut crate::de::Context<'de>,
            item: &crate::Item<'de>,
        ) -> Result<Self, crate::Failed> {
            let mut th = item.table_helper(ctx)?;
            let tags = th.required("tags")?;
            th.expect_empty()?;
            Ok(Multi { tags })
        }
    }

    crate::impl_borrowed_value!(Multi);

    let doc = StaticDocument::parse(r#"tags = ["a", "b", "c"]"#).unwrap();
    let with = doc.to_borrowed::<Multi<'static>>().unwrap();
    assert_eq!(with.value().tags, vec!["a", "b", "c"]);
}

// --- with_value_mut tests ---

#[cfg(feature = "from-toml")]
#[test]
fn with_value_mut_scalar() {
    let doc = StaticDocument::parse("name = 'hello'\nport = 80").unwrap();
    let mut with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();

    with.with_value_mut(|cfg| {
        cfg.port = 443;
    });

    assert_eq!(with.value().port, 443);
    assert_eq!(with.value().name, "hello");
}

#[cfg(feature = "from-toml")]
#[test]
fn with_value_mut_static_str() {
    let doc = StaticDocument::parse("name = 'old'\nport = 80").unwrap();
    let mut with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();

    with.with_value_mut(|cfg| {
        cfg.name = "new_literal";
    });

    assert_eq!(with.value().name, "new_literal");
}

#[cfg(feature = "from-toml")]
#[test]
fn with_value_mut_returns_value() {
    let doc = StaticDocument::parse("name = 'test'\nport = 80").unwrap();
    let mut with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();

    let old_port = with.with_value_mut(|cfg| {
        let old = cfg.port;
        cfg.port = 9090;
        old
    });

    assert_eq!(old_port, 80);
    assert_eq!(with.value().port, 9090);
}

#[cfg(feature = "from-toml")]
#[test]
fn with_value_mut_multiple_calls() {
    let doc = StaticDocument::parse("name = 'start'\nport = 1").unwrap();
    let mut with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();

    with.with_value_mut(|cfg| cfg.port = 2);
    with.with_value_mut(|cfg| cfg.port = 3);
    with.with_value_mut(|cfg| cfg.name = "end");

    assert_eq!(with.value().port, 3);
    assert_eq!(with.value().name, "end");
}

#[cfg(feature = "from-toml")]
#[test]
fn with_value_mut_read_and_modify() {
    let doc = StaticDocument::parse("name = 'hello'\nport = 80").unwrap();
    let mut with = doc.to_borrowed::<BorrowedConfig<'static>>().unwrap();

    with.with_value_mut(|cfg| {
        if cfg.name.starts_with("hello") {
            cfg.port = 8080;
        }
    });

    assert_eq!(with.value().port, 8080);
}
