use std::collections::BTreeMap;
use toml_spanner::{Arena, FromItem};
use toml_spanner_macros::Toml;

#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem)]
struct Config {
    name: String,
    port: u16,
    #[toml(default)]
    debug: bool,
    tags: Option<Vec<String>>,
}

#[test]
fn derive_from_item_basic() {
    let arena = Arena::new();
    let input = r#"
            name = "my-app"
            port = 8080
            debug = true
            tags = ["web", "api"]
        "#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = root.split();
    let config: Config = {
        let result = Config::from_item(ctx, table.as_item());
        result.unwrap()
    };
    assert_eq!(config.name, "my-app");
    assert_eq!(config.port, 8080);
    assert!(config.debug);
    assert_eq!(
        config.tags,
        Some(vec!["web".to_string(), "api".to_string()])
    );
}

#[test]
fn derive_from_item_defaults() {
    let arena = Arena::new();
    let input = r#"
            name = "minimal"
            port = 3000
        "#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();

    let (ctx, table) = root.split();
    let config: Config = {
        let result = Config::from_item(ctx, table.as_item());
        result.unwrap()
    };
    assert_eq!(config.name, "minimal");
    assert_eq!(config.port, 3000);
    assert!(!config.debug);
    assert_eq!(config.tags, None);
}

#[test]
fn derive_from_item_via_from_str() {
    let config: Config = toml_spanner::from_str(
        r#"
            name = "test"
            port = 9090
        "#,
    )
    .unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.port, 9090);
}

#[derive(Toml, Debug, PartialEq)]
#[toml(ToItem)]
struct Simple {
    name: String,
    count: u32,
    enabled: bool,
}

#[test]
fn derive_to_item_basic() {
    let s = Simple {
        name: "hello".to_string(),
        count: 42,
        enabled: true,
    };
    let result = toml_spanner::to_string(&s).unwrap();
    assert!(result.contains("name = \"hello\""), "got: {result}");
    assert!(result.contains("count = 42"), "got: {result}");
    assert!(result.contains("enabled = true"), "got: {result}");
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
struct RoundTrip {
    name: String,
    value: i64,
    #[toml(default)]
    flag: bool,
}

#[test]
fn derive_roundtrip() {
    let original = RoundTrip {
        name: "test".to_string(),
        value: 99,
        flag: true,
    };
    let toml_str = toml_spanner::to_string(&original).unwrap();
    let restored: RoundTrip = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(original, restored);
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
struct WithOption {
    required: String,
    maybe: Option<u32>,
}

#[test]
fn derive_option_present() {
    let input = r#"
            required = "yes"
            maybe = 10
        "#;
    let v: WithOption = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.required, "yes");
    assert_eq!(v.maybe, Some(10));
}

#[test]
fn derive_option_absent() {
    let input = r#"required = "yes""#;
    let v: WithOption = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.required, "yes");
    assert_eq!(v.maybe, None);
}

#[test]
fn derive_option_roundtrip() {
    let with = WithOption {
        required: "a".to_string(),
        maybe: Some(5),
    };
    let toml_str = toml_spanner::to_string(&with).unwrap();
    let restored: WithOption = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(with, restored);

    let without = WithOption {
        required: "b".to_string(),
        maybe: None,
    };
    let toml_str = toml_spanner::to_string(&without).unwrap();
    let restored: WithOption = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(without, restored);
}

// ── Enum tests ───────────────────────────────────────────────

// String enum (all-unit, external tag)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn enum_string_from_item() {
    let arena = Arena::new();
    let input = r#"color = "Red""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let color: Color = th.required("color").unwrap();
    assert_eq!(color, Color::Red);
}

#[test]
fn enum_string_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        color: Color,
    }
    let w = Wrapper {
        color: Color::Green,
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    assert!(toml_str.contains(r#"color = "Green""#), "got: {toml_str}");
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// String enum with rename_all
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem, rename_all = "snake_case")]
enum Status {
    InProgress,
    AllDone,
}

#[test]
fn enum_string_rename_all() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        status: Status,
    }
    let w = Wrapper {
        status: Status::InProgress,
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    assert!(
        toml_str.contains(r#"status = "in_progress""#),
        "got: {toml_str}"
    );
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// External tagging (mixed: unit + struct variants)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
enum Shape {
    Circle,
    Rect { w: u32, h: u32 },
}

#[test]
fn enum_external_unit_from_item() {
    let arena = Arena::new();
    let input = r#"shape = "Circle""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let shape: Shape = th.required("shape").unwrap();
    assert_eq!(shape, Shape::Circle);
}

#[test]
fn enum_external_struct_from_item() {
    let input = r#"
            [Rect]
            w = 10
            h = 20
        "#;
    let v: Shape = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Shape::Rect { w: 10, h: 20 });
}

#[test]
fn enum_external_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        shape: Shape,
    }
    let w = Wrapper {
        shape: Shape::Circle,
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    assert!(toml_str.contains(r#"shape = "Circle""#), "got: {toml_str}");
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// External tagging with tuple variant
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
enum Value {
    Text(String),
    Number(i64),
}

#[test]
fn enum_external_tuple_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        val: Value,
    }
    let w = Wrapper {
        val: Value::Text("hello".to_string()),
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// Internal tagging
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem, tag = "type")]
enum Message {
    Quit,
    Move { x: i32, y: i32 },
}

#[test]
fn enum_internal_unit_from_item() {
    let input = r#"type = "Quit""#;
    let v: Message = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Message::Quit);
}

#[test]
fn enum_internal_struct_from_item() {
    let input = r#"
            type = "Move"
            x = 10
            y = -5
        "#;
    let v: Message = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Message::Move { x: 10, y: -5 });
}

#[test]
fn enum_internal_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        msg: Message,
    }
    let w = Wrapper {
        msg: Message::Move { x: 3, y: 4 },
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// Adjacent tagging
#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem, tag = "kind", content = "data")]
enum Event {
    Click(String),
    Resize { w: u32, h: u32 },
    Close,
}

#[test]
fn enum_adjacent_unit_from_item() {
    let input = r#"kind = "Close""#;
    let v: Event = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Event::Close);
}

#[test]
fn enum_adjacent_tuple_from_item() {
    let input = r#"
            kind = "Click"
            data = "button1"
        "#;
    let v: Event = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Event::Click("button1".to_string()));
}

#[test]
fn enum_adjacent_struct_from_item() {
    let input = r#"
            kind = "Resize"
            [data]
            w = 800
            h = 600
        "#;
    let v: Event = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Event::Resize { w: 800, h: 600 });
}

#[test]
fn enum_adjacent_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromItem, ToItem)]
    struct Wrapper {
        event: Event,
    }
    let w = Wrapper {
        event: Event::Click("ok".to_string()),
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// ── Flatten tests ────────────────────────────────────────────

#[derive(Toml, Debug, PartialEq)]
#[toml(FromItem, ToItem)]
struct WithFlatten {
    name: String,
    #[toml(flatten)]
    extras: BTreeMap<String, String>,
}

#[test]
fn flatten_from_item() {
    let input = r#"
        name = "test"
        foo = "bar"
        baz = "qux"
    "#;
    let v: WithFlatten = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "test");
    assert_eq!(v.extras.len(), 2);
    assert_eq!(v.extras["foo"], "bar");
    assert_eq!(v.extras["baz"], "qux");
}

#[test]
fn flatten_to_item() {
    let mut extras = BTreeMap::new();
    extras.insert("alpha".to_string(), "one".to_string());
    extras.insert("beta".to_string(), "two".to_string());
    let v = WithFlatten {
        name: "hello".to_string(),
        extras,
    };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("name = \"hello\""), "got: {toml_str}");
    assert!(toml_str.contains("alpha = \"one\""), "got: {toml_str}");
    assert!(toml_str.contains("beta = \"two\""), "got: {toml_str}");
}

#[test]
fn flatten_roundtrip() {
    let mut extras = BTreeMap::new();
    extras.insert("x".to_string(), "1".to_string());
    extras.insert("y".to_string(), "2".to_string());
    let original = WithFlatten {
        name: "rt".to_string(),
        extras,
    };
    let toml_str = toml_spanner::to_string(&original).unwrap();
    let restored: WithFlatten = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn flatten_empty_extras() {
    let input = r#"name = "only""#;
    let v: WithFlatten = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "only");
    assert!(v.extras.is_empty());
}
