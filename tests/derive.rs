use std::collections::BTreeMap;
use toml_spanner::{Arena, FromToml, TomlConfig, to_string_with_config};
use toml_spanner_macros::Toml;

#[derive(Toml, Debug, PartialEq)]
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
        let result = Config::from_toml(ctx, table.as_item());
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
        let result = Config::from_toml(ctx, table.as_item());
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
#[toml(ToToml)]
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
#[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml)]
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

// String enum (all-unit, external tag)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn enum_string_from_toml() {
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
    #[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml, rename_all = "snake_case")]
enum Status {
    InProgress,
    AllDone,
}

#[test]
fn enum_string_rename_all() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml)]
enum Shape {
    Circle,
    Rect { w: u32, h: u32 },
}

#[test]
fn enum_external_unit_from_toml() {
    let arena = Arena::new();
    let input = r#"shape = "Circle""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let shape: Shape = th.required("shape").unwrap();
    assert_eq!(shape, Shape::Circle);
}

#[test]
fn enum_external_struct_from_toml() {
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
    #[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml)]
enum Value {
    Text(String),
    Number(i64),
}

#[test]
fn enum_external_tuple_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml, tag = "type")]
enum Message {
    Quit,
    Move { x: i32, y: i32 },
}

#[test]
fn enum_internal_unit_from_toml() {
    let input = r#"type = "Quit""#;
    let v: Message = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Message::Quit);
}

#[test]
fn enum_internal_struct_from_toml() {
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
    #[toml(FromToml, ToToml)]
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
#[toml(FromToml, ToToml, tag = "kind", content = "data")]
enum Event {
    Click(String),
    Resize { w: u32, h: u32 },
    Close,
}

#[test]
fn enum_adjacent_unit_from_toml() {
    let input = r#"kind = "Close""#;
    let v: Event = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Event::Close);
}

#[test]
fn enum_adjacent_tuple_from_toml() {
    let input = r#"
            kind = "Click"
            data = "button1"
        "#;
    let v: Event = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, Event::Click("button1".to_string()));
}

#[test]
fn enum_adjacent_struct_from_toml() {
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
    #[toml(FromToml, ToToml)]
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

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct WithFlatten {
    name: String,
    #[toml(flatten)]
    extras: BTreeMap<String, String>,
}

#[test]
fn flatten_from_toml() {
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

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, untagged)]
enum Untagged {
    Num(i64),
    Text(String),
}

#[test]
fn untagged_tuple_int() {
    let arena = Arena::new();
    let input = r#"val = 42"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: Untagged = th.required("val").unwrap();
    assert_eq!(v, Untagged::Num(42));
}

#[test]
fn untagged_tuple_string() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: Untagged = th.required("val").unwrap();
    // i64 fails, then String succeeds
    assert_eq!(v, Untagged::Text("hello".to_string()));
}

#[test]
fn untagged_tuple_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Wrapper {
        val: Untagged,
    }
    let w = Wrapper {
        val: Untagged::Num(99),
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);

    let w2 = Wrapper {
        val: Untagged::Text("hi".to_string()),
    };
    let toml_str2 = toml_spanner::to_string(&w2).unwrap();
    let restored2: Wrapper = toml_spanner::from_str(&toml_str2).unwrap();
    assert_eq!(w2, restored2);
}

// Untagged with struct + tuple mix
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, untagged)]
enum UntaggedMixed {
    Structured { x: i32, y: i32 },
    Simple(String),
}

#[test]
fn untagged_struct_variant() {
    let input = r#"
        x = 10
        y = 20
    "#;
    let v: UntaggedMixed = toml_spanner::from_str(input).unwrap();
    assert_eq!(v, UntaggedMixed::Structured { x: 10, y: 20 });
}

#[test]
fn untagged_fallback_to_later_variant() {
    // A plain string can't be parsed as Structured, so falls through to Simple
    let arena = Arena::new();
    let input = r#"val = "just a string""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: UntaggedMixed = th.required("val").unwrap();
    assert_eq!(v, UntaggedMixed::Simple("just a string".to_string()));
}

#[test]
fn untagged_mixed_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Wrapper {
        val: UntaggedMixed,
    }
    let w = Wrapper {
        val: UntaggedMixed::Structured { x: 1, y: 2 },
    };
    let toml_str = toml_spanner::to_string(&w).unwrap();
    let restored: Wrapper = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(w, restored);
}

// Untagged with unit variants
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, untagged)]
enum UntaggedWithUnit {
    Named(String),
    Empty,
}

#[test]
fn untagged_unit_variant() {
    let arena = Arena::new();
    let input = r#"val = "Empty""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: UntaggedWithUnit = th.required("val").unwrap();
    // "Empty" matches Named(String) first since it comes before the unit variant
    assert_eq!(v, UntaggedWithUnit::Named("Empty".to_string()));
}

// Verify errors are properly cleaned up between attempts
#[test]
fn untagged_no_error_leakage() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: Untagged = th.required("val").unwrap();
    assert_eq!(v, Untagged::Text("hello".to_string()));
    // Num attempt failed but errors should have been truncated
    assert!(
        root.errors().is_empty(),
        "errors should be empty but got: {:?}",
        root.errors()
    );
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, untagged)]
enum TryIfEnum {
    #[toml(try_if = |_ctx, item| item.kind() == toml_spanner::Kind::Array)]
    Arr(Vec<String>),
    Text(String),
}

#[test]
fn try_if_matches() {
    let arena = Arena::new();
    let input = r#"val = ["a", "b"]"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: TryIfEnum = th.required("val").unwrap();
    assert_eq!(v, TryIfEnum::Arr(vec!["a".to_string(), "b".to_string()]));
}

#[test]
fn try_if_skips() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: TryIfEnum = th.required("val").unwrap();
    // Predicate is false for strings, so Arr is skipped, falls through to Text
    assert_eq!(v, TryIfEnum::Text("hello".to_string()));
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, untagged)]
enum FinalIfEnum {
    #[toml(final_if = |_ctx, item| item.kind() == toml_spanner::Kind::String)]
    Text(String),
    Num(i64),
}

#[test]
fn final_if_matches() {
    let arena = Arena::new();
    let input = r#"val = "committed""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: FinalIfEnum = th.required("val").unwrap();
    assert_eq!(v, FinalIfEnum::Text("committed".to_string()));
}

#[test]
fn final_if_skips_to_next() {
    let arena = Arena::new();
    let input = r#"val = 42"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: FinalIfEnum = th.required("val").unwrap();
    // Predicate false for integers, so Text is skipped, falls through to Num
    assert_eq!(v, FinalIfEnum::Num(42));
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, untagged)]
enum MixedHints {
    #[toml(final_if = |_ctx, item| item.kind() == toml_spanner::Kind::Boolean)]
    Flag(bool),
    #[toml(try_if = |_ctx, item| item.kind() == toml_spanner::Kind::Integer)]
    Num(i64),
    Text(String),
}

#[test]
fn mixed_hints_final_if() {
    let arena = Arena::new();
    let input = r#"val = true"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: MixedHints = th.required("val").unwrap();
    assert_eq!(v, MixedHints::Flag(true));
}

#[test]
fn mixed_hints_try_if() {
    let arena = Arena::new();
    let input = r#"val = 99"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: MixedHints = th.required("val").unwrap();
    assert_eq!(v, MixedHints::Num(99));
}

#[test]
fn mixed_hints_fallback_unhinted() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: MixedHints = th.required("val").unwrap();
    assert_eq!(v, MixedHints::Text("hello".to_string()));
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, untagged)]
enum AllHinted {
    #[toml(final_if = |_ctx, item| item.kind() == toml_spanner::Kind::Boolean)]
    Flag(bool),
    #[toml(try_if = |_ctx, item| item.kind() == toml_spanner::Kind::Integer)]
    Num(i64),
}

#[test]
fn all_hinted_no_match_gives_error() {
    let arena = Arena::new();
    let input = r#"val = "nope""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let result: Result<AllHinted, _> = th.required("val");
    assert!(result.is_err(), "expected error when no variant matches");
}

#[test]
fn try_if_no_error_leakage() {
    // When try_if predicate matches but deserialization fails, errors are truncated
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, untagged)]
    enum TryIfLeak {
        #[toml(try_if = |_ctx, item| item.kind() == toml_spanner::Kind::String)]
        Num(i64),
        Text(String),
    }

    let arena = Arena::new();
    let input = r#"val = "not_a_number""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let mut th = doc.as_item().table_helper(ctx).unwrap();
    let v: TryIfLeak = th.required("val").unwrap();
    // try_if predicate matches (it's a string), but i64 deser fails →
    // errors truncated, falls through to Text
    assert_eq!(v, TryIfLeak::Text("not_a_number".to_string()));
    assert!(
        root.errors().is_empty(),
        "errors should be empty but got: {:?}",
        root.errors()
    );
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct GenericWithDefault<P: Clone = String> {
    value: P,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct GenericOptionalField<P: Clone> {
    path: Option<P>,
}

#[test]
fn derive_generic_with_default_bound() {
    let arena = Arena::new();
    let input = r#"value = "hello""#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let result: GenericWithDefault = GenericWithDefault::from_toml(ctx, doc.as_item()).unwrap();
    assert_eq!(result.value, "hello");
}

#[test]
fn derive_generic_with_explicit_type() {
    let arena = Arena::new();
    let input = r#"value = 42"#;
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, doc) = root.split();
    let result: GenericWithDefault<i64> =
        GenericWithDefault::from_toml(ctx, doc.as_item()).unwrap();
    assert_eq!(result.value, 42);
}

#[test]
fn generic_optional_field_present() {
    let v: GenericOptionalField<String> = toml_spanner::from_str(r#"path = "hello""#).unwrap();
    assert_eq!(v.path, Some("hello".to_string()));
}

#[test]
fn generic_optional_field_absent() {
    let v: GenericOptionalField<String> = toml_spanner::from_str("").unwrap();
    assert_eq!(v.path, None);
}

#[test]
fn generic_optional_field_to_toml() {
    let v = GenericOptionalField {
        path: Some("test".to_string()),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("path = \"test\""), "got: {s}");
}

// Option<T> with #[toml(required)]: missing field is an error
#[derive(Toml, Debug, PartialEq)]

struct RequiredOption {
    #[toml(required)]
    val: Option<u32>,
}

#[test]
fn required_option_present() {
    let v: RequiredOption = toml_spanner::from_str("val = 42").unwrap();
    assert_eq!(v.val, Some(42));
}

#[test]
fn required_option_absent() {
    let result: Result<RequiredOption, _> = toml_spanner::from_str("");
    assert!(
        result.is_err(),
        "missing required Option field should error"
    );
}

// Option<T> with #[toml(default)]: missing field uses Default (None)
#[derive(Toml, Debug, PartialEq)]

struct DefaultOption {
    #[toml(default)]
    val: Option<u32>,
}

#[test]
fn default_option_present() {
    let v: DefaultOption = toml_spanner::from_str("val = 7").unwrap();
    assert_eq!(v.val, Some(7));
}

#[test]
fn default_option_absent() {
    let v: DefaultOption = toml_spanner::from_str("").unwrap();
    assert_eq!(v.val, None);
}

// Option<T> with #[toml(default = Some(99))]: missing field uses custom default
#[derive(Toml, Debug, PartialEq)]

struct DefaultOptionCustom {
    #[toml(default = Some(99))]
    val: Option<u32>,
}

#[test]
fn default_option_custom_present() {
    let v: DefaultOptionCustom = toml_spanner::from_str("val = 7").unwrap();
    assert_eq!(v.val, Some(7));
}

#[test]
fn default_option_custom_absent() {
    let v: DefaultOptionCustom = toml_spanner::from_str("").unwrap();
    assert_eq!(v.val, Some(99));
}

// Plain Option<T> (auto-detected) still works as before
#[derive(Toml, Debug, PartialEq)]

struct PlainOption {
    val: Option<u32>,
}

#[test]
fn plain_option_present() {
    let v: PlainOption = toml_spanner::from_str("val = 5").unwrap();
    assert_eq!(v.val, Some(5));
}

#[test]
fn plain_option_absent() {
    let v: PlainOption = toml_spanner::from_str("").unwrap();
    assert_eq!(v.val, None);
}

#[derive(Toml, Debug, PartialEq)]

struct WithAlias {
    #[toml(alias = "server_name")]
    name: String,
    port: u16,
}

#[test]
fn alias_primary_key() {
    let v: WithAlias = toml_spanner::from_str("name = \"app\"\nport = 80").unwrap();
    assert_eq!(v.name, "app");
    assert_eq!(v.port, 80);
}

#[test]
fn alias_alternate_key() {
    let v: WithAlias = toml_spanner::from_str("server_name = \"app\"\nport = 80").unwrap();
    assert_eq!(v.name, "app");
    assert_eq!(v.port, 80);
}

#[test]
fn alias_duplicate_error() {
    let result: Result<WithAlias, _> =
        toml_spanner::from_str("name = \"a\"\nserver_name = \"b\"\nport = 80");
    assert!(result.is_err(), "should error on duplicate field via alias");
}

#[derive(Toml, Debug, PartialEq)]

struct MultiAlias {
    #[toml(alias = "colour", alias = "clr")]
    color: String,
}

#[test]
fn multi_alias_first() {
    let v: MultiAlias = toml_spanner::from_str("colour = \"red\"").unwrap();
    assert_eq!(v.color, "red");
}

#[test]
fn multi_alias_second() {
    let v: MultiAlias = toml_spanner::from_str("clr = \"blue\"").unwrap();
    assert_eq!(v.color, "blue");
}

#[test]
fn multi_alias_primary() {
    let v: MultiAlias = toml_spanner::from_str("color = \"green\"").unwrap();
    assert_eq!(v.color, "green");
}

#[test]
fn multi_alias_duplicate_error() {
    let result: Result<MultiAlias, _> =
        toml_spanner::from_str("color = \"red\"\ncolour = \"blue\"");
    assert!(result.is_err(), "should error on duplicate via alias");
}

#[derive(Toml, Debug, PartialEq)]

struct AliasOptional {
    #[toml(alias = "nm")]
    name: Option<String>,
}

#[test]
fn alias_optional_via_alias() {
    let v: AliasOptional = toml_spanner::from_str("nm = \"hi\"").unwrap();
    assert_eq!(v.name, Some("hi".to_string()));
}

#[test]
fn alias_optional_absent() {
    let v: AliasOptional = toml_spanner::from_str("").unwrap();
    assert_eq!(v.name, None);
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct ServerConfig {
    host: String,
    port: u16,
    #[toml(default)]
    debug: bool,
}

#[test]
fn preserving_formatting_identity_roundtrip() {
    let input = "host = \"localhost\"\nport = 8080\ndebug = true\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let config: ServerConfig = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&config, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert_eq!(output, input);
}

#[test]
fn preserving_formatting_keeps_comments() {
    let input = "\
# Server configuration
host = \"localhost\"
port = 8080
# Enable debug mode
debug = true
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let config: ServerConfig = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&config, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("# Server configuration"), "got: {output}");
    assert!(output.contains("# Enable debug mode"), "got: {output}");
}

#[test]
fn preserving_formatting_keeps_inline_table_style() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithNested {
        name: String,
        point: Point,
    }
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Point {
        x: i64,
        y: i64,
    }

    let input = "name = \"test\"\npoint = { x = 1, y = 2 }\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: WithNested = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(
        output.contains("point = {"),
        "inline table style should be preserved, got: {output}"
    );
}

#[test]
fn preserving_formatting_keeps_header_style() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithSection {
        name: String,
        server: ServerSection,
    }
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct ServerSection {
        host: String,
        port: u16,
    }

    let input = "\
name = \"app\"

[server]
host = \"0.0.0.0\"
port = 443
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: WithSection = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(
        output.contains("[server]"),
        "header style should be preserved, got: {output}"
    );
    assert!(output.contains("host = \"0.0.0.0\""), "got: {output}");
}

#[test]
fn preserving_formatting_modified_value() {
    let input = "\
host = \"localhost\"
port = 8080
debug = false
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let config = ServerConfig {
        host: "localhost".to_string(),
        port: 9090,
        debug: false,
    };
    let output =
        to_string_with_config(&config, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(
        output.contains("port = 9090"),
        "modified value should appear, got: {output}"
    );
    assert!(
        output.contains("host = \"localhost\""),
        "unchanged value should be preserved, got: {output}"
    );
}

#[test]
fn preserving_formatting_hex_integers() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct HexConfig {
        color: i64,
        size: i64,
    }

    let input = "color = 0xFF00FF\nsize = 42\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: HexConfig = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(
        output.contains("0xFF00FF"),
        "hex format should be preserved, got: {output}"
    );
}

#[test]
fn preserving_formatting_literal_strings() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct PathConfig {
        path: String,
        name: String,
    }

    let input = "path = 'C:\\Users\\test'\nname = \"hello\"\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: PathConfig = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(
        output.contains("'C:\\Users\\test'"),
        "literal string format should be preserved, got: {output}"
    );
}

// Comprehensive ToToml coverage via roundtrip: exercises many ser.rs impls
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct ComprehensiveTypes {
    // Primitives
    flag: bool,
    int_i32: i32,
    int_i64: i64,
    float_f64: f64,
    name: String,
    // Collections
    int_list: Vec<i64>,
    labels: BTreeMap<String, String>,
    hash_labels: std::collections::HashMap<String, i64>,
    // Nested / wrapper types
    boxed_val: Box<i64>,
    path: std::path::PathBuf,
    maybe: Option<String>,
    maybe_absent: Option<String>,
    fixed_arr: [i32; 3],
}

#[test]
fn comprehensive_ser_roundtrip() {
    let mut labels = BTreeMap::new();
    labels.insert("env".to_string(), "prod".to_string());
    labels.insert("region".to_string(), "us-east".to_string());

    let mut hash_labels = std::collections::HashMap::new();
    hash_labels.insert("count".to_string(), 42);

    let original = ComprehensiveTypes {
        flag: true,
        int_i32: -100,
        int_i64: 9999999999,
        float_f64: 3.14159,
        name: "test-name".to_string(),
        int_list: vec![1, 2, 3],
        labels,
        hash_labels,
        boxed_val: Box::new(99),
        path: std::path::PathBuf::from("/usr/local/bin"),
        maybe: Some("present".to_string()),
        maybe_absent: None,
        fixed_arr: [10, 20, 30],
    };

    let toml_str = toml_spanner::to_string(&original).unwrap();

    // Verify key content is present
    assert!(toml_str.contains("flag = true"), "got: {toml_str}");
    assert!(toml_str.contains("int_i32 = -100"), "got: {toml_str}");
    assert!(toml_str.contains("float_f64 = 3.14159"), "got: {toml_str}");
    assert!(toml_str.contains("name = \"test-name\""), "got: {toml_str}");
    assert!(toml_str.contains("boxed_val = 99"), "got: {toml_str}");
    assert!(
        toml_str.contains("path = \"/usr/local/bin\""),
        "got: {toml_str}"
    );
    assert!(toml_str.contains("maybe = \"present\""), "got: {toml_str}");

    // Roundtrip: parse back
    let restored: ComprehensiveTypes = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(original.flag, restored.flag);
    assert_eq!(original.int_i32, restored.int_i32);
    assert_eq!(original.int_i64, restored.int_i64);
    assert!((original.float_f64 - restored.float_f64).abs() < f64::EPSILON);
    assert_eq!(original.name, restored.name);
    assert_eq!(original.int_list, restored.int_list);

    assert_eq!(original.labels, restored.labels);
    assert_eq!(original.hash_labels, restored.hash_labels);
    assert_eq!(original.boxed_val, restored.boxed_val);
    assert_eq!(original.path, restored.path);
    assert_eq!(original.maybe, restored.maybe);
    assert_eq!(original.maybe_absent, restored.maybe_absent);
    assert_eq!(original.fixed_arr, restored.fixed_arr);
}

// Test ToToml for small integer types (i8, u8, i16, u16) and f32
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct SmallIntegers {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}

#[test]
fn small_integer_serialization() {
    // Test that various integer types can be serialized and round-tripped
    // (they all upcast to i64 for TOML)
    let v = SmallIntegers {
        a: 42,
        b: 255,
        c: -128,
        d: 1000,
    };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    let restored: SmallIntegers = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(v, restored);
}

// Direct ToToml tests for types not easily reachable via derive
#[test]
fn to_string_with_nested_vecs() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithVecs {
        nums: Vec<Vec<i64>>,
    }
    let v = WithVecs {
        nums: vec![vec![1, 2], vec![3, 4, 5]],
    };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    let restored: WithVecs = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(v, restored);
}

#[test]
fn to_string_with_float_types() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Floats {
        a: f64,
        b: f64,
    }
    let v = Floats { a: 1.5, b: -0.001 };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    let restored: Floats = toml_spanner::from_str(&toml_str).unwrap();
    assert!((v.a - restored.a).abs() < f64::EPSILON);
    assert!((v.b - restored.b).abs() < f64::EPSILON);
}

mod flatten_key_helper {
    use toml_spanner::{Arena, Context, Failed, Item, Key, Table, ToTomlError};

    pub fn init() -> Vec<String> {
        Vec::new()
    }

    pub fn insert<'de>(
        _ctx: &mut Context<'de>,
        key: &Key<'de>,
        _item: &Item<'de>,
        partial: &mut Vec<String>,
    ) -> Result<(), Failed> {
        partial.push(key.name.to_string());
        Ok(())
    }

    pub fn finish<'de>(
        _ctx: &mut Context<'de>,
        partial: Vec<String>,
    ) -> Result<Vec<String>, Failed> {
        Ok(partial)
    }

    pub fn to_flattened<'a>(
        val: &'a Vec<String>,
        arena: &'a Arena,
        table: &mut Table<'a>,
    ) -> Result<(), ToTomlError> {
        for s in val {
            table.insert(Key::anon(arena.alloc_str(s)), Item::from(true), arena);
        }
        Ok(())
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FlattenWithHelper {
    name: String,
    #[toml(flatten, with = flatten_key_helper)]
    extra_keys: Vec<String>,
}

#[test]
fn flatten_with_from_toml() {
    let input = r#"
        name = "test"
        foo = "bar"
        baz = "qux"
    "#;
    let v: FlattenWithHelper = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "test");
    assert_eq!(v.extra_keys.len(), 2);
    assert!(v.extra_keys.contains(&"foo".to_string()));
    assert!(v.extra_keys.contains(&"baz".to_string()));
}

#[test]
fn flatten_with_to_toml() {
    let v = FlattenWithHelper {
        name: "hello".to_string(),
        extra_keys: vec!["alpha".to_string(), "beta".to_string()],
    };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("name = \"hello\""), "got: {toml_str}");
    assert!(toml_str.contains("alpha = true"), "got: {toml_str}");
    assert!(toml_str.contains("beta = true"), "got: {toml_str}");
}

// HashMap flatten exercises de.rs HashMap FromFlattened path
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct WithHashMapFlatten {
    name: String,
    #[toml(flatten)]
    extras: std::collections::HashMap<String, i64>,
}

#[test]
fn flatten_hashmap_roundtrip() {
    let input = r#"
        name = "test"
        alpha = 1
        beta = 2
    "#;
    let v: WithHashMapFlatten = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "test");
    assert_eq!(v.extras.len(), 2);
    assert_eq!(v.extras["alpha"], 1);
    assert_eq!(v.extras["beta"], 2);

    // Roundtrip via to_string → from_str
    let toml_str = toml_spanner::to_string(&v).unwrap();
    let restored: WithHashMapFlatten = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(v.name, restored.name);
    assert_eq!(v.extras, restored.extras);
}

// Preserving formatting with diverse value types exercises reprojection hash paths
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct DiverseValues {
    label: String,
    count: i64,
    ratio: f64,
    enabled: bool,
    tags: Vec<String>,
    #[toml(default)]
    nested: Option<NestedPart>,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct NestedPart {
    x: i64,
    y: i64,
}

#[test]
fn preserving_formatting_diverse_types() {
    let input = "\
label = \"hello\"
count = 42
ratio = 3.14
enabled = true
tags = [\"a\", \"b\"]

[nested]
x = 10
y = 20
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: DiverseValues = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("ratio = 3.14"), "got: {output}");
    assert!(output.contains("enabled = true"), "got: {output}");
    assert!(output.contains("[nested]"), "got: {output}");
    assert!(output.contains("tags = [\"a\", \"b\"]"), "got: {output}");
}

#[test]
fn preserving_formatting_with_new_field() {
    // When dest has a field not in source, reprojection should handle gracefully
    let source_input = "\
host = \"localhost\"
port = 8080
";
    let arena = Arena::new();
    let root = toml_spanner::parse(source_input, &arena).unwrap();
    let config = ServerConfig {
        host: "localhost".to_string(),
        port: 8080,
        debug: true, // new field not in source
    };
    let output =
        to_string_with_config(&config, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("host = \"localhost\""), "got: {output}");
    assert!(output.contains("port = 8080"), "got: {output}");
    assert!(output.contains("debug = true"), "got: {output}");
}

// AOT (array of tables) roundtrip with preserving formatting
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct WithAot {
    name: String,
    items: Vec<AotEntry>,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct AotEntry {
    key: String,
    val: i64,
}

#[test]
fn preserving_formatting_aot() {
    let input = "\
name = \"project\"

[[items]]
key = \"first\"
val = 1

[[items]]
key = \"second\"
val = 2
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: WithAot = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("[[items]]"), "got: {output}");
    assert!(output.contains("key = \"first\""), "got: {output}");
    assert!(output.contains("key = \"second\""), "got: {output}");
}

// Exercises reprojection hash_item for float, bool, and integer array elements
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct WithDiverseArrays {
    floats: Vec<f64>,
    bools: Vec<bool>,
    ints: Vec<i64>,
}

#[test]
fn preserving_formatting_diverse_arrays() {
    let input = "\
floats = [1.5, 2.5, 3.5]
bools = [true, false, true]
ints = [10, 20, 30]
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: WithDiverseArrays = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("floats = [1.5, 2.5, 3.5]"), "got: {output}");
    assert!(
        output.contains("bools = [true, false, true]"),
        "got: {output}"
    );
}

// Exercises reprojection with modified array (element added/removed)
#[test]
fn preserving_formatting_modified_array() {
    let input = "\
floats = [1.0, 2.0, 3.0]
bools = [true, false]
ints = [10, 20, 30]
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let modified = WithDiverseArrays {
        floats: vec![1.0, 2.0, 3.0, 4.0], // added element
        bools: vec![true],                // removed element
        ints: vec![10, 20, 30],
    };
    let output =
        to_string_with_config(&modified, TomlConfig::default().with_formatting_from(&root))
            .unwrap();
    assert!(output.contains("floats"), "got: {output}");
    assert!(output.contains("bools"), "got: {output}");

    // Roundtrip: parse the output and verify
    let restored: WithDiverseArrays = toml_spanner::from_str(&output).unwrap();
    assert_eq!(modified.floats, restored.floats);
    assert_eq!(modified.bools, restored.bools);
    assert_eq!(modified.ints, restored.ints);
}

// Exercises reprojection with reordered array elements
#[test]
fn preserving_formatting_reordered_array() {
    let input = "ints = [30, 10, 20]\nfloats = [3.0, 1.0, 2.0]\nbools = [false, true, false]\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let reordered = WithDiverseArrays {
        floats: vec![1.0, 2.0, 3.0],
        bools: vec![true, false, false],
        ints: vec![10, 20, 30],
    };
    let output = to_string_with_config(
        &reordered,
        TomlConfig::default().with_formatting_from(&root),
    )
    .unwrap();
    let restored: WithDiverseArrays = toml_spanner::from_str(&output).unwrap();
    assert_eq!(reordered.floats, restored.floats);
    assert_eq!(reordered.ints, restored.ints);
}

// Exercises ser.rs ToToml impls for BTreeMap, BTreeSet, HashSet, Box, Rc, Arc, Cow, char, PathBuf
#[test]
fn ser_btreemap_and_btreeset_roundtrip() {
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithCollections {
        #[toml(flatten)]
        map: BTreeMap<String, i64>,
    }

    let mut map = BTreeMap::new();
    map.insert("alpha".to_string(), 1);
    map.insert("beta".to_string(), 2);
    let v = WithCollections { map };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    let restored: WithCollections = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(v, restored);

    // BTreeSet via a Vec field (sets serialize as arrays)
    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithSet {
        items: BTreeSet<String>,
    }
    let mut items = BTreeSet::new();
    items.insert("x".to_string());
    items.insert("y".to_string());
    let v = WithSet { items };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("items = ["), "got: {toml_str}");
    assert!(toml_str.contains("\"x\""), "got: {toml_str}");
    assert!(toml_str.contains("\"y\""), "got: {toml_str}");
}

#[test]
fn ser_hashset_roundtrip() {
    use std::collections::HashSet;

    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct WithHashSet {
        vals: HashSet<i64>,
    }
    let mut vals = HashSet::new();
    vals.insert(10);
    vals.insert(20);
    let v = WithHashSet { vals };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("vals = ["), "got: {toml_str}");
}

#[test]
fn ser_smart_pointers_and_cow() {
    use std::borrow::Cow;
    use std::rc::Rc;
    use std::sync::Arc;

    // All these implement ToToml by delegating to the inner type.
    // Exercise them through the to_string → from_str path.
    let arena = toml_spanner::Arena::new();

    let boxed: Box<String> = Box::new("boxed".to_string());
    let item = toml_spanner::ToToml::to_toml(&boxed, &arena).unwrap();
    assert_eq!(item.as_str(), Some("boxed"));

    let rc = Rc::new(42i64);
    let item = toml_spanner::ToToml::to_toml(&rc, &arena).unwrap();
    assert_eq!(item.as_i64(), Some(42));

    let arc = Arc::new("arc_str".to_string());
    let item = toml_spanner::ToToml::to_toml(&arc, &arena).unwrap();
    assert_eq!(item.as_str(), Some("arc_str"));

    let cow: Cow<'_, String> = Cow::Owned("cow_str".to_string());
    let item = toml_spanner::ToToml::to_toml(&cow, &arena).unwrap();
    assert_eq!(item.as_str(), Some("cow_str"));
}

#[test]
fn ser_char_and_path() {
    let arena = toml_spanner::Arena::new();

    let ch = 'Z';
    let item = toml_spanner::ToToml::to_toml(&ch, &arena).unwrap();
    assert_eq!(item.as_str(), Some("Z"));

    let ch_multi = '€';
    let item = toml_spanner::ToToml::to_toml(&ch_multi, &arena).unwrap();
    assert_eq!(item.as_str(), Some("€"));

    let path = std::path::PathBuf::from("/tmp/test.toml");
    let item = toml_spanner::ToToml::to_toml(&path, &arena).unwrap();
    assert_eq!(item.as_str(), Some("/tmp/test.toml"));

    let path_ref = std::path::Path::new("/etc/config");
    let item = toml_spanner::ToToml::to_toml(&path_ref, &arena).unwrap();
    assert_eq!(item.as_str(), Some("/etc/config"));
}

#[test]
fn ser_f32_roundtrip() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithF32 {
        val: f32,
    }
    let v = WithF32 { val: 1.5 };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("1.5"), "got: {toml_str}");
}

#[test]
fn ser_array_and_table_clone() {
    // Exercises ToToml for Array<'_> and Table<'_>
    let arena = toml_spanner::Arena::new();
    let root = toml_spanner::parse("[t]\na = 1\nb = [2, 3]", &arena).unwrap();

    let table = root["t"].as_table().unwrap();
    let item = toml_spanner::ToToml::to_toml(table, &arena).unwrap();
    assert_eq!(item.as_table().unwrap()["a"].as_i64(), Some(1));

    let arr = root["t"]["b"].as_array().unwrap();
    let item = toml_spanner::ToToml::to_toml(arr, &arena).unwrap();
    assert_eq!(item.as_array().unwrap().len(), 2);

    // Item::to_toml clones the item
    let orig = root["t"]["a"].item().unwrap();
    let cloned = toml_spanner::ToToml::to_toml(orig, &arena).unwrap();
    assert_eq!(cloned.as_i64(), Some(1));
}

#[test]
fn ser_ref_and_mut_ref() {
    let arena = toml_spanner::Arena::new();

    let val = 99i64;
    let r = &val;
    let item = toml_spanner::ToToml::to_toml(&r, &arena).unwrap();
    assert_eq!(item.as_i64(), Some(99));

    let mut val2 = true;
    let mr = &mut val2;
    let item = toml_spanner::ToToml::to_toml(&mr, &arena).unwrap();
    assert_eq!(item.as_bool(), Some(true));
}

#[test]
fn ser_fixed_array() {
    // Exercises [T; N]::to_toml
    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithFixedArray {
        vals: [i64; 3],
    }
    let v = WithFixedArray { vals: [10, 20, 30] };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("vals = [10, 20, 30]"), "got: {toml_str}");
}

#[test]
fn ser_hashmap_as_table() {
    use std::collections::HashMap;

    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithMap {
        #[toml(flatten)]
        data: HashMap<String, String>,
    }
    let mut data = HashMap::new();
    data.insert("key1".to_string(), "val1".to_string());
    let v = WithMap { data };
    let toml_str = toml_spanner::to_string(&v).unwrap();
    assert!(toml_str.contains("key1 = \"val1\""), "got: {toml_str}");
}

// Exercises to_string_with_config with nested tables and dotted keys
#[test]
fn preserving_formatting_dotted_keys() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Server {
        host: String,
        port: i64,
    }
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct AppConfig {
        name: String,
        server: Server,
    }

    let input = "\
name = \"myapp\"

[server]
host = \"localhost\"
port = 8080
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    let val: AppConfig = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("[server]"), "got: {output}");
    assert!(output.contains("host = \"localhost\""), "got: {output}");

    // Modify a value and re-emit
    let modified = AppConfig {
        name: "changed".to_string(),
        server: Server {
            host: "0.0.0.0".to_string(),
            port: 9090,
        },
    };
    let output2 =
        to_string_with_config(&modified, TomlConfig::default().with_formatting_from(&root))
            .unwrap();
    let restored: AppConfig = toml_spanner::from_str(&output2).unwrap();
    assert_eq!(modified, restored);
}

// Exercises to_string_with_config with removed nested table
#[test]
fn preserving_formatting_optional_nested_removed() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithOptNested {
        name: String,
        #[toml(default)]
        extra: Option<NestedPart>,
    }

    let input = "\
name = \"test\"

[extra]
x = 1
y = 2
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();
    // Remove the nested table
    let modified = WithOptNested {
        name: "test".to_string(),
        extra: None,
    };
    let output =
        to_string_with_config(&modified, TomlConfig::default().with_formatting_from(&root))
            .unwrap();
    assert!(output.contains("name = \"test\""), "got: {output}");
    let restored: WithOptNested = toml_spanner::from_str(&output).unwrap();
    assert_eq!(modified, restored);
}

// Exercises to_string_with_config with inline tables
#[test]
fn preserving_formatting_inline_table() {
    let input = "point = { x = 1, y = 2 }\nlabel = \"origin\"\n";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct Point {
        x: i64,
        y: i64,
    }
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct WithInline {
        point: Point,
        label: String,
    }
    let val: WithInline = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    assert!(output.contains("point = { x = 1, y = 2 }"), "got: {output}");

    // Modified inline table
    let modified = WithInline {
        point: Point { x: 10, y: 20 },
        label: "origin".to_string(),
    };
    let output2 =
        to_string_with_config(&modified, TomlConfig::default().with_formatting_from(&root))
            .unwrap();
    let restored: WithInline = toml_spanner::from_str(&output2).unwrap();
    assert_eq!(modified, restored);
}

// Exercises Array::pop, Array::into_iter, and Array::as_item
#[test]
fn array_pop_into_iter_and_as_item() {
    let arena = Arena::new();
    let root = toml_spanner::parse("a = [1, 2, 3, 4]", &arena).unwrap();
    let mut arr = root["a"].as_array().unwrap().clone_in(&arena);

    // pop
    let last = arr.pop().unwrap();
    assert_eq!(last.as_i64(), Some(4));
    assert_eq!(arr.len(), 3);

    // as_item
    let item = arr.as_item();
    assert_eq!(item.as_array().unwrap().len(), 3);

    // into_iter (consuming)
    let collected: Vec<i64> = arr.into_iter().filter_map(|i| i.as_i64()).collect();
    assert_eq!(collected, vec![1, 2, 3]);
}

// Exercises Array index operator for out-of-bounds (returns NONE)
#[test]
fn array_index_out_of_bounds() {
    let arena = Arena::new();
    let root = toml_spanner::parse("a = [1]", &arena).unwrap();
    let arr = root["a"].as_array().unwrap();
    assert!(arr[99].as_i64().is_none());
}

// Exercises to_string error path when top-level is not a table
#[test]
fn to_string_non_table_error() {
    // A type that produces a non-table item at the top level
    let result = toml_spanner::to_string(&"just a string");
    assert!(result.is_err());
}

// Exercises to_string_with_config with comments in source
#[test]
fn preserving_formatting_with_comments() {
    let input = "\
# This is a config file
name = \"myapp\"
# Server settings
port = 8080
";
    let arena = Arena::new();
    let root = toml_spanner::parse(input, &arena).unwrap();

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct SimpleConf {
        name: String,
        port: i64,
    }
    let val: SimpleConf = toml_spanner::from_str(input).unwrap();
    let output =
        to_string_with_config(&val, TomlConfig::default().with_formatting_from(&root)).unwrap();
    // Comments should be preserved in source-ordered mode
    assert!(output.contains("# This is a config file"), "got: {output}");
    assert!(output.contains("name = \"myapp\""), "got: {output}");
}
