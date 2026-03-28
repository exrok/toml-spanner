use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use toml_spanner::{Arena, Formatting, FromToml};
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();

    let (ctx, table) = doc.split();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: Untagged = th.required("val").unwrap();
    assert_eq!(v, Untagged::Num(42));
}

#[test]
fn untagged_tuple_string() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: UntaggedWithUnit = th.required("val").unwrap();
    // "Empty" matches Named(String) first since it comes before the unit variant
    assert_eq!(v, UntaggedWithUnit::Named("Empty".to_string()));
}

// Verify errors are properly cleaned up between attempts
#[test]
fn untagged_no_error_leakage() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: Untagged = th.required("val").unwrap();
    assert_eq!(v, Untagged::Text("hello".to_string()));
    // Num attempt failed but errors should have been truncated
    assert!(
        doc.errors().is_empty(),
        "errors should be empty but got: {:?}",
        doc.errors()
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: TryIfEnum = th.required("val").unwrap();
    assert_eq!(v, TryIfEnum::Arr(vec!["a".to_string(), "b".to_string()]));
}

#[test]
fn try_if_skips() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: FinalIfEnum = th.required("val").unwrap();
    assert_eq!(v, FinalIfEnum::Text("committed".to_string()));
}

#[test]
fn final_if_skips_to_next() {
    let arena = Arena::new();
    let input = r#"val = 42"#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: MixedHints = th.required("val").unwrap();
    assert_eq!(v, MixedHints::Flag(true));
}

#[test]
fn mixed_hints_try_if() {
    let arena = Arena::new();
    let input = r#"val = 99"#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: MixedHints = th.required("val").unwrap();
    assert_eq!(v, MixedHints::Num(99));
}

#[test]
fn mixed_hints_fallback_unhinted() {
    let arena = Arena::new();
    let input = r#"val = "hello""#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: TryIfLeak = th.required("val").unwrap();
    // try_if predicate matches (it's a string), but i64 deser fails →
    // errors truncated, falls through to Text
    assert_eq!(v, TryIfLeak::Text("not_a_number".to_string()));
    assert!(
        doc.errors().is_empty(),
        "errors should be empty but got: {:?}",
        doc.errors()
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
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let result: GenericWithDefault = GenericWithDefault::from_toml(ctx, table.as_item()).unwrap();
    assert_eq!(result.value, "hello");
}

#[test]
fn derive_generic_with_explicit_type() {
    let arena = Arena::new();
    let input = r#"value = 42"#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let (ctx, table) = doc.split();
    let result: GenericWithDefault<i64> =
        GenericWithDefault::from_toml(ctx, table.as_item()).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let config: ServerConfig = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&config).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let config: ServerConfig = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&config).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: WithNested = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: WithSection = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let config = ServerConfig {
        host: "localhost".to_string(),
        port: 9090,
        debug: false,
    };
    let output = Formatting::preserved_from(&doc).format(&config).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: HexConfig = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: PathConfig = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
        _parent: &Table<'de>,
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
            table.insert_unique(Key::new(arena.alloc_str(s)), Item::from(true), arena);
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: DiverseValues = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(source_input, &arena).unwrap();
    let config = ServerConfig {
        host: "localhost".to_string(),
        port: 8080,
        debug: true, // new field not in source
    };
    let output = Formatting::preserved_from(&doc).format(&config).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: WithAot = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: WithDiverseArrays = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let modified = WithDiverseArrays {
        floats: vec![1.0, 2.0, 3.0, 4.0], // added element
        bools: vec![true],                // removed element
        ints: vec![10, 20, 30],
    };
    let output = Formatting::preserved_from(&doc).format(&modified).unwrap();
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let reordered = WithDiverseArrays {
        floats: vec![1.0, 2.0, 3.0],
        bools: vec![true, false, false],
        ints: vec![10, 20, 30],
    };
    let output = Formatting::preserved_from(&doc).format(&reordered).unwrap();
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
    let doc = toml_spanner::parse("[t]\na = 1\nb = [2, 3]", &arena).unwrap();

    let table = doc["t"].as_table().unwrap();
    let item = toml_spanner::ToToml::to_toml(table, &arena).unwrap();
    assert_eq!(item.as_table().unwrap()["a"].as_i64(), Some(1));

    let arr = doc["t"]["b"].as_array().unwrap();
    let item = toml_spanner::ToToml::to_toml(arr, &arena).unwrap();
    assert_eq!(item.as_array().unwrap().len(), 2);

    // Item::to_toml clones the item
    let orig = doc["t"]["a"].item().unwrap();
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

// Exercises to_string_with with nested tables and dotted keys
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    let val: AppConfig = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
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
    let output2 = Formatting::preserved_from(&doc).format(&modified).unwrap();
    let restored: AppConfig = toml_spanner::from_str(&output2).unwrap();
    assert_eq!(modified, restored);
}

// Exercises to_string_with with removed nested table
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
    let doc = toml_spanner::parse(input, &arena).unwrap();
    // Remove the nested table
    let modified = WithOptNested {
        name: "test".to_string(),
        extra: None,
    };
    let output = Formatting::preserved_from(&doc).format(&modified).unwrap();
    assert!(output.contains("name = \"test\""), "got: {output}");
    let restored: WithOptNested = toml_spanner::from_str(&output).unwrap();
    assert_eq!(modified, restored);
}

// Exercises to_string_with with inline tables
#[test]
fn preserving_formatting_inline_table() {
    let input = "point = { x = 1, y = 2 }\nlabel = \"origin\"\n";
    let arena = Arena::new();
    let doc = toml_spanner::parse(input, &arena).unwrap();

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
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
    assert!(output.contains("point = { x = 1, y = 2 }"), "got: {output}");

    // Modified inline table
    let modified = WithInline {
        point: Point { x: 10, y: 20 },
        label: "origin".to_string(),
    };
    let output2 = Formatting::preserved_from(&doc).format(&modified).unwrap();
    let restored: WithInline = toml_spanner::from_str(&output2).unwrap();
    assert_eq!(modified, restored);
}

// Exercises Array::pop, Array::into_iter, and Array::as_item
#[test]
fn array_pop_into_iter_and_as_item() {
    let arena = Arena::new();
    let doc = toml_spanner::parse("a = [1, 2, 3, 4]", &arena).unwrap();
    let mut arr = doc["a"].as_array().unwrap().clone_in(&arena);

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
    let doc = toml_spanner::parse("a = [1]", &arena).unwrap();
    let arr = doc["a"].as_array().unwrap();
    assert!(arr[99].as_i64().is_none());
}

// Exercises to_string error path when top-level is not a table
#[test]
fn to_string_non_table_error() {
    // A type that produces a non-table item at the top level
    let result = toml_spanner::to_string(&"just a string");
    assert!(result.is_err());
}

// Exercises to_string_with with comments in source
#[test]
fn preserving_formatting_with_comments() {
    let input = "\
# This is a config file
name = \"myapp\"
# Server settings
port = 8080
";
    let arena = Arena::new();
    let doc = toml_spanner::parse(input, &arena).unwrap();

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ToToml)]
    struct SimpleConf {
        name: String,
        port: i64,
    }
    let val: SimpleConf = toml_spanner::from_str(input).unwrap();
    let output = Formatting::preserved_from(&doc).format(&val).unwrap();
    // Comments should be preserved in source-ordered mode
    assert!(output.contains("# This is a config file"), "got: {output}");
    assert!(output.contains("name = \"myapp\""), "got: {output}");
}

use toml_spanner::helper::flatten_any;

// Basic struct flatten (FromToml + ToToml)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaPoint {
    x: i64,
    y: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaBasic {
    name: String,
    #[toml(flatten, with = flatten_any)]
    point: FaPoint,
}

#[test]
fn flatten_any_basic() {
    let input = r#"
        name = "origin"
        x = 10
        y = 20
    "#;
    let v: FaBasic = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "origin");
    assert_eq!(v.point, FaPoint { x: 10, y: 20 });
}

#[test]
fn flatten_any_basic_to_toml() {
    let v = FaBasic {
        name: "origin".into(),
        point: FaPoint { x: 10, y: 20 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("name = \"origin\""), "got: {s}");
    assert!(s.contains("x = 10"), "got: {s}");
    assert!(s.contains("y = 20"), "got: {s}");
}

#[test]
fn flatten_any_basic_roundtrip() {
    let v = FaBasic {
        name: "rt".into(),
        point: FaPoint { x: -1, y: 99 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let v2: FaBasic = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, v2);
}

// Internally-tagged enum
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, tag = "kind")]
enum FaShape {
    Circle { radius: f64 },
    Rect { w: f64, h: f64 },
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaDrawing {
    label: String,
    #[toml(flatten, with = flatten_any)]
    shape: FaShape,
}

#[test]
fn flatten_any_tagged_enum() {
    let input = r#"
        label = "big circle"
        kind = "Circle"
        radius = 5.0
    "#;
    let v: FaDrawing = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.label, "big circle");
    assert_eq!(v.shape, FaShape::Circle { radius: 5.0 });
}

#[test]
fn flatten_any_tagged_enum_to_toml() {
    let v = FaDrawing {
        label: "circle".into(),
        shape: FaShape::Circle { radius: 2.5 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("label = \"circle\""), "got: {s}");
    assert!(s.contains("kind = \"Circle\""), "got: {s}");
    assert!(s.contains("radius = 2.5"), "got: {s}");
}

#[test]
fn flatten_any_tagged_enum_roundtrip() {
    let v = FaDrawing {
        label: "rect".into(),
        shape: FaShape::Rect { w: 1.0, h: 2.0 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let v2: FaDrawing = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, v2);
}

// Adjacently-tagged enum
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, tag = "type", content = "payload")]
enum FaAction {
    Log(String),
    Move { dx: i64, dy: i64 },
    Noop,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaStep {
    id: i64,
    #[toml(flatten, with = flatten_any)]
    action: FaAction,
}

#[test]
fn flatten_any_adjacent_enum() {
    let input = r#"
        id = 1
        type = "Move"
        payload = { dx = 5, dy = -3 }
    "#;
    let v: FaStep = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.id, 1);
    assert_eq!(v.action, FaAction::Move { dx: 5, dy: -3 });
}

#[test]
fn flatten_any_adjacent_enum_unit() {
    let input = r#"
        id = 99
        type = "Noop"
    "#;
    let v: FaStep = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.id, 99);
    assert_eq!(v.action, FaAction::Noop);
}

#[test]
fn flatten_any_adjacent_enum_to_toml() {
    let v = FaStep {
        id: 7,
        action: FaAction::Move { dx: 1, dy: -1 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("id = 7"), "got: {s}");
    assert!(s.contains("type = \"Move\""), "got: {s}");
}

#[test]
fn flatten_any_adjacent_enum_roundtrip() {
    for action in [
        FaAction::Log("hello".into()),
        FaAction::Move { dx: 3, dy: 4 },
        FaAction::Noop,
    ] {
        let v = FaStep { id: 1, action };
        let s = toml_spanner::to_string(&v).unwrap();
        let v2: FaStep = toml_spanner::from_str(&s)
            .unwrap_or_else(|e| panic!("failed to roundtrip:\n{s}\nerror: {e:?}"));
        assert_eq!(v, v2);
    }
}

// Borrowed lifetimes via &'de str
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaBorrowedInner<'de> {
    tag: &'de str,
    value: &'de str,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaBorrowedOuter<'de> {
    id: i64,
    #[toml(flatten, with = flatten_any)]
    inner: FaBorrowedInner<'de>,
}

#[test]
fn flatten_any_borrowed_lifetime() {
    let arena = Arena::new();
    let input = r#"
        id = 42
        tag = "hello"
        value = "world"
    "#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let v: FaBorrowedOuter<'_> = doc.to().unwrap();
    assert_eq!(v.id, 42);
    assert_eq!(v.inner.tag, "hello");
    assert_eq!(v.inner.value, "world");
}

#[test]
fn flatten_any_borrowed_roundtrip() {
    let arena = Arena::new();
    let v = FaBorrowedOuter {
        id: 1,
        inner: FaBorrowedInner {
            tag: "a",
            value: "b",
        },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let mut doc = toml_spanner::parse(&s, &arena).unwrap();
    let v2: FaBorrowedOuter<'_> = doc.to().unwrap();
    assert_eq!(v2.id, 1);
    assert_eq!(v2.inner.tag, "a");
    assert_eq!(v2.inner.value, "b");
}

// Large table exceeding INDEXED_TABLE_THRESHOLD (6) to exercise
// the is_span_mode guard in TableHelper::new.
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaBigInner {
    f1: i64,
    f2: i64,
    f3: i64,
    f4: i64,
    f5: i64,
    f6: i64,
    f7: i64,
    f8: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaBigOuter {
    name: String,
    #[toml(flatten, with = flatten_any)]
    big: FaBigInner,
}

#[test]
fn flatten_any_exceeds_index_threshold() {
    let input = r#"
        name = "large"
        f1 = 1
        f2 = 2
        f3 = 3
        f4 = 4
        f5 = 5
        f6 = 6
        f7 = 7
        f8 = 8
    "#;
    let v: FaBigOuter = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "large");
    assert_eq!(
        v.big,
        FaBigInner {
            f1: 1,
            f2: 2,
            f3: 3,
            f4: 4,
            f5: 5,
            f6: 6,
            f7: 7,
            f8: 8,
        }
    );
}

#[test]
fn flatten_any_big_roundtrip() {
    let v = FaBigOuter {
        name: "big".into(),
        big: FaBigInner {
            f1: 1,
            f2: 2,
            f3: 3,
            f4: 4,
            f5: 5,
            f6: 6,
            f7: 7,
            f8: 8,
        },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let v2: FaBigOuter = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, v2);
}

// Nested tables (shallow copy of table-valued Items)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaSubTable {
    a: i64,
    b: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaNestedInner {
    sub: FaSubTable,
    extra: String,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaNestedOuter {
    id: i64,
    #[toml(flatten, with = flatten_any)]
    nested: FaNestedInner,
}

#[test]
fn flatten_any_nested_tables() {
    let input = r#"
        id = 1
        extra = "stuff"
        [sub]
        a = 10
        b = 20
    "#;
    let v: FaNestedOuter = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.id, 1);
    assert_eq!(v.nested.extra, "stuff");
    assert_eq!(v.nested.sub, FaSubTable { a: 10, b: 20 });
}

#[test]
fn flatten_any_nested_roundtrip() {
    let v = FaNestedOuter {
        id: 42,
        nested: FaNestedInner {
            sub: FaSubTable { a: 10, b: 20 },
            extra: "yes".into(),
        },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let v2: FaNestedOuter = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, v2);
}

// Arrays (shallow copy of array-valued Items)
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaWithArrayInner {
    tags: Vec<String>,
    count: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaWithArrayOuter {
    name: String,
    #[toml(flatten, with = flatten_any)]
    data: FaWithArrayInner,
}

#[test]
fn flatten_any_with_arrays() {
    let input = r#"
        name = "tagged"
        tags = ["a", "b", "c"]
        count = 3
    "#;
    let v: FaWithArrayOuter = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.name, "tagged");
    assert_eq!(v.data.tags, vec!["a", "b", "c"]);
    assert_eq!(v.data.count, 3);
}

#[test]
fn flatten_any_arrays_roundtrip() {
    let v = FaWithArrayOuter {
        name: "arr".into(),
        data: FaWithArrayInner {
            tags: vec!["x".into(), "y".into()],
            count: 2,
        },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let v2: FaWithArrayOuter = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, v2);
}

// Kitchen sink: lifetimes + >6 fields + arrays + mixed types
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaKitchenSinkInner<'de> {
    f1: &'de str,
    f2: &'de str,
    f3: i64,
    f4: i64,
    f5: bool,
    f6: bool,
    f7: f64,
    f8: Vec<i64>,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct FaKitchenSinkOuter<'de> {
    id: i64,
    #[toml(flatten, with = flatten_any)]
    rest: FaKitchenSinkInner<'de>,
}

#[test]
fn flatten_any_kitchen_sink() {
    let arena = Arena::new();
    let input = r#"
        id = 0
        f1 = "alpha"
        f2 = "beta"
        f3 = 100
        f4 = 200
        f5 = true
        f6 = false
        f7 = 3.14
        f8 = [1, 2, 3]
    "#;
    let mut doc = toml_spanner::parse(input, &arena).unwrap();
    let v: FaKitchenSinkOuter<'_> = doc.to().unwrap();
    assert_eq!(v.id, 0);
    assert_eq!(v.rest.f1, "alpha");
    assert_eq!(v.rest.f2, "beta");
    assert_eq!(v.rest.f3, 100);
    assert_eq!(v.rest.f4, 200);
    assert!(v.rest.f5);
    assert!(!v.rest.f6);
    assert!((v.rest.f7 - 3.14).abs() < f64::EPSILON);
    assert_eq!(v.rest.f8, vec![1, 2, 3]);
}

#[test]
fn flatten_any_kitchen_sink_roundtrip() {
    let arena = Arena::new();
    let v = FaKitchenSinkOuter {
        id: 99,
        rest: FaKitchenSinkInner {
            f1: "one",
            f2: "two",
            f3: 3,
            f4: 4,
            f5: true,
            f6: false,
            f7: 1.5,
            f8: vec![10, 20],
        },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let mut doc = toml_spanner::parse(&s, &arena).unwrap();
    let v2: FaKitchenSinkOuter<'_> = doc.to().unwrap();
    assert_eq!(v2.id, 99);
    assert_eq!(v2.rest.f1, "one");
    assert_eq!(v2.rest.f2, "two");
    assert_eq!(v2.rest.f3, 3);
    assert_eq!(v2.rest.f8, vec![10, 20]);
}

use toml_spanner::helper::display;
use toml_spanner::helper::parse_string;

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct ParseStringOnly {
    host: String,
    #[toml(with = parse_string)]
    addr: IpAddr,
}

#[test]
fn parse_string_ipv4() {
    let v: ParseStringOnly = toml_spanner::from_str(
        r#"
        host = "example.com"
        addr = "127.0.0.1"
    "#,
    )
    .unwrap();
    assert_eq!(v.host, "example.com");
    assert_eq!(v.addr, IpAddr::from([127, 0, 0, 1]));
}

#[test]
fn parse_string_ipv6() {
    let v: ParseStringOnly = toml_spanner::from_str(
        r#"
        host = "example.com"
        addr = "::1"
    "#,
    )
    .unwrap();
    assert_eq!(v.addr, IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]));
}

#[test]
fn parse_string_invalid_value() {
    let result = toml_spanner::from_str::<ParseStringOnly>(
        r#"
        host = "example.com"
        addr = "not-an-ip"
    "#,
    );
    assert!(result.is_err());
}

#[test]
fn parse_string_wrong_type() {
    let result = toml_spanner::from_str::<ParseStringOnly>(
        r#"
        host = "example.com"
        addr = 42
    "#,
    );
    assert!(result.is_err());
}

#[derive(Toml, Debug, PartialEq)]
#[toml(ToToml)]
struct DisplayOnly {
    host: String,
    #[toml(with = display)]
    addr: IpAddr,
}

#[test]
fn display_to_toml() {
    let v = DisplayOnly {
        host: "example.com".into(),
        addr: IpAddr::from([10, 0, 0, 1]),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("addr = \"10.0.0.1\""));
}

#[test]
fn display_to_toml_ipv6() {
    let v = DisplayOnly {
        host: "example.com".into(),
        addr: IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("addr = \"::1\""));
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct ParseStringDisplay {
    host: String,
    #[toml(FromToml with = parse_string, ToToml with = display)]
    addr: IpAddr,
}

#[test]
fn parse_string_display_roundtrip() {
    let v = ParseStringDisplay {
        host: "example.com".into(),
        addr: IpAddr::from([192, 168, 1, 1]),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let restored: ParseStringDisplay = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, restored);
}

#[test]
fn parse_string_display_roundtrip_ipv6() {
    let v = ParseStringDisplay {
        host: "localhost".into(),
        addr: "fe80::1".parse().unwrap(),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    let restored: ParseStringDisplay = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, restored);
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct OptionalAddr {
    host: String,
    #[toml(FromToml with = parse_string, ToToml with = display)]
    addr: Option<IpAddr>,
}

#[test]
fn parse_string_display_optional_present() {
    let v: OptionalAddr = toml_spanner::from_str(
        r#"
        host = "example.com"
        addr = "10.0.0.1"
    "#,
    )
    .unwrap();
    assert_eq!(v.addr, Some(IpAddr::from([10, 0, 0, 1])));
    let s = toml_spanner::to_string(&v).unwrap();
    let restored: OptionalAddr = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, restored);
}

#[test]
fn parse_string_display_optional_absent() {
    let v: OptionalAddr = toml_spanner::from_str(
        r#"
        host = "example.com"
    "#,
    )
    .unwrap();
    assert_eq!(v.addr, None);
}

#[test]
fn from_attribute_roundtrip() {
    #[derive(Toml)]
    #[toml(FromToml)]
    struct RawName {
        name: String,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, from = RawName)]
    struct UpperName {
        name: String,
    }

    impl From<RawName> for UpperName {
        fn from(raw: RawName) -> Self {
            UpperName {
                name: raw.name.to_uppercase(),
            }
        }
    }

    let v: UpperName = toml_spanner::from_str(r#"name = "hello""#).unwrap();
    assert_eq!(
        v,
        UpperName {
            name: "HELLO".to_string()
        }
    );
}

#[test]
fn try_from_attribute_success() {
    #[derive(Toml)]
    #[toml(FromToml)]
    struct RawRange {
        value: i64,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, try_from = RawRange)]
    struct PositiveValue {
        value: i64,
    }

    impl TryFrom<RawRange> for PositiveValue {
        type Error = String;
        fn try_from(raw: RawRange) -> Result<Self, Self::Error> {
            if raw.value > 0 {
                Ok(PositiveValue { value: raw.value })
            } else {
                Err(format!("{} is not positive", raw.value))
            }
        }
    }

    let v: PositiveValue = toml_spanner::from_str("value = 42").unwrap();
    assert_eq!(v, PositiveValue { value: 42 });
}

#[test]
fn try_from_attribute_failure() {
    #[derive(Toml)]
    #[toml(FromToml)]
    struct RawRange {
        value: i64,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, try_from = RawRange)]
    struct PositiveValue {
        value: i64,
    }

    impl TryFrom<RawRange> for PositiveValue {
        type Error = String;
        fn try_from(raw: RawRange) -> Result<Self, Self::Error> {
            if raw.value > 0 {
                Ok(PositiveValue { value: raw.value })
            } else {
                Err(format!("{} is not positive", raw.value))
            }
        }
    }

    let result = toml_spanner::from_str::<PositiveValue>("value = -5");
    assert!(result.is_err());
}

#[test]
fn from_attribute_with_struct_proxy() {
    #[derive(Toml)]
    #[toml(FromToml)]
    struct RawConfig {
        name: String,
        port: u16,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, from = RawConfig)]
    struct AppConfig {
        label: String,
        port: u16,
    }

    impl From<RawConfig> for AppConfig {
        fn from(raw: RawConfig) -> Self {
            AppConfig {
                label: raw.name.to_uppercase(),
                port: raw.port,
            }
        }
    }

    let v: AppConfig = toml_spanner::from_str(
        r#"
        name = "my-app"
        port = 8080
    "#,
    )
    .unwrap();
    assert_eq!(v.label, "MY-APP");
    assert_eq!(v.port, 8080);
}

#[test]
fn try_from_attribute_with_struct_proxy() {
    #[derive(Toml)]
    #[toml(FromToml)]
    struct RawRange {
        min: i64,
        max: i64,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, try_from = RawRange)]
    struct ValidRange {
        min: i64,
        max: i64,
    }

    impl TryFrom<RawRange> for ValidRange {
        type Error = String;
        fn try_from(raw: RawRange) -> Result<Self, Self::Error> {
            if raw.min > raw.max {
                return Err(format!("min ({}) must be <= max ({})", raw.min, raw.max));
            }
            Ok(ValidRange {
                min: raw.min,
                max: raw.max,
            })
        }
    }

    let v: ValidRange = toml_spanner::from_str("min = 1\nmax = 10").unwrap();
    assert_eq!(v, ValidRange { min: 1, max: 10 });

    let result = toml_spanner::from_str::<ValidRange>("min = 10\nmax = 1");
    assert!(result.is_err());
}

#[test]
fn from_attribute_enum() {
    #[derive(Toml)]
    #[toml(FromToml)]
    enum RawColor {
        Red,
        Green,
        Blue,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, from = RawColor)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    impl From<RawColor> for Color {
        fn from(raw: RawColor) -> Self {
            match raw {
                RawColor::Red => Color::Red,
                RawColor::Green => Color::Green,
                RawColor::Blue => Color::Blue,
            }
        }
    }

    let arena = Arena::new();
    let mut doc = toml_spanner::parse(r#"val = "Red""#, &arena).unwrap();
    let (ctx, table) = doc.split();
    let mut th = table.as_item().table_helper(ctx).unwrap();
    let v: Color = th.required("val").unwrap();
    assert_eq!(v, Color::Red);
}

// String enum with other
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
enum OtherStatus {
    Active,
    Inactive,
    #[toml(other)]
    Unknown,
}

#[test]
fn other_string_enum_known() {
    let v = toml_spanner::from_str::<OtherStatusWrap>(r#"status = "Active""#)
        .unwrap()
        .status;
    assert_eq!(v, OtherStatus::Active);
}

#[test]
fn other_string_enum_unknown() {
    let v = toml_spanner::from_str::<OtherStatusWrap>(r#"status = "Pending""#)
        .unwrap()
        .status;
    assert_eq!(v, OtherStatus::Unknown);
}

#[test]
fn other_string_enum_roundtrip_known() {
    let w = OtherStatusWrap {
        status: OtherStatus::Active,
    };
    let s = toml_spanner::to_string(&w).unwrap();
    let w2: OtherStatusWrap = toml_spanner::from_str(&s).unwrap();
    assert_eq!(w, w2);
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
struct OtherStatusWrap {
    status: OtherStatus,
}

// Internally tagged enum with other
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, tag = "type")]
enum Command {
    Start,
    Stop,
    Restart {
        delay: i64,
    },
    #[toml(other)]
    Unknown,
}

#[test]
fn other_internal_known_unit() {
    let v: Command = toml_spanner::from_str(r#"type = "Start""#).unwrap();
    assert_eq!(v, Command::Start);
}

#[test]
fn other_internal_known_struct() {
    let v: Command = toml_spanner::from_str("type = \"Restart\"\ndelay = 5").unwrap();
    assert_eq!(v, Command::Restart { delay: 5 });
}

#[test]
fn other_internal_unknown() {
    let v: Command = toml_spanner::from_str(r#"type = "Pause""#).unwrap();
    assert_eq!(v, Command::Unknown);
}

// Adjacently tagged enum with other
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml, tag = "kind", content = "data")]
enum Action {
    Click(String),
    Close,
    #[toml(other)]
    Unrecognized,
}

#[test]
fn other_adjacent_known_tuple() {
    let v: Action = toml_spanner::from_str("kind = \"Click\"\ndata = \"btn\"").unwrap();
    assert_eq!(v, Action::Click("btn".to_string()));
}

#[test]
fn other_adjacent_known_unit() {
    let v: Action = toml_spanner::from_str(r#"kind = "Close""#).unwrap();
    assert_eq!(v, Action::Close);
}

#[test]
fn other_adjacent_unknown() {
    let v: Action = toml_spanner::from_str(r#"kind = "Hover""#).unwrap();
    assert_eq!(v, Action::Unrecognized);
}

// External tagging with other
#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml, ToToml)]
enum Animal {
    Cat,
    Dog,
    Fish {
        color: String,
    },
    #[toml(other)]
    Unknown,
}

#[test]
fn other_external_known_unit() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml)]
    struct W {
        animal: Animal,
    }
    let v: W = toml_spanner::from_str(r#"animal = "Cat""#).unwrap();
    assert_eq!(v.animal, Animal::Cat);
}

#[test]
fn other_external_unknown_string() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml)]
    struct W {
        animal: Animal,
    }
    let v: W = toml_spanner::from_str(r#"animal = "Parrot""#).unwrap();
    assert_eq!(v.animal, Animal::Unknown);
}

#[test]
fn ignore_unknown_fields_struct() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, ignore_unknown_fields)]
    struct Simple {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let result = doc.to::<Simple>();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, "hi");
    assert!(doc.errors().is_empty(), "should have no errors");
}

#[test]
fn warn_unknown_fields_by_default() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml)]
    struct Simple {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Simple::from_toml(ctx, table.as_item());
    assert!(
        result.is_ok(),
        "struct should still be constructed: {:?}",
        result
    );
    assert_eq!(result.unwrap().name, "hi");
    assert!(
        !doc.errors().is_empty(),
        "should have errors for unknown field"
    );
}

#[test]
fn warn_unknown_fields_multiple_errors() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml)]
    struct Simple {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\na = 1\nb = 2", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Simple::from_toml(ctx, table.as_item());
    assert!(
        result.is_ok(),
        "struct should still be constructed: {:?}",
        result
    );
    let errors = doc.errors();
    assert_eq!(errors.len(), 2, "should have two errors, got: {:?}", errors);
}

#[test]
fn deny_unknown_fields_struct() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields)]
    struct Strict {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Strict::from_toml(ctx, table.as_item());
    assert!(result.is_err(), "should fail with deny_unknown_fields");
    assert!(
        !doc.errors().is_empty(),
        "should have errors for unknown field"
    );
}

#[test]
fn deny_unknown_fields_no_false_positive() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields)]
    struct Strict {
        name: String,
        port: u16,
    }
    let v: Strict = toml_spanner::from_str("name = \"hi\"\nport = 80").unwrap();
    assert_eq!(v.name, "hi");
    assert_eq!(v.port, 80);
}

#[test]
fn deny_unknown_fields_internal_tag_unit() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields, tag = "type")]
    enum Action {
        Stop,
        Go { speed: i64 },
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("type = \"Stop\"\nextra = true", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Action::from_toml(ctx, table.as_item());
    assert!(result.is_err(), "should fail with deny_unknown_fields");
    assert!(
        !doc.errors().is_empty(),
        "should have error for unknown field"
    );
}

#[test]
fn deny_unknown_fields_internal_tag_struct() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields, tag = "type")]
    enum Action {
        Stop,
        Go { speed: i64 },
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("type = \"Go\"\nspeed = 5\nextra = true", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Action::from_toml(ctx, table.as_item());
    assert!(result.is_err(), "should fail with deny_unknown_fields");
    assert!(
        !doc.errors().is_empty(),
        "should have error for unknown field"
    );
}

#[test]
fn deny_unknown_fields_adjacent_tag() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields, tag = "t", content = "c")]
    enum Msg {
        Ping,
        Data(String),
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("t = \"Ping\"\nextra = 1", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Msg::from_toml(ctx, table.as_item());
    assert!(result.is_err(), "should fail with deny_unknown_fields");
    assert!(
        !doc.errors().is_empty(),
        "should have error for unknown field"
    );
}

#[test]
fn warn_unknown_fields_explicit() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, warn_unknown_fields)]
    struct Simple {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Simple::from_toml(ctx, table.as_item());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, "hi");
    assert!(!doc.errors().is_empty());
}

const UNKNOWN_FIELD_TAG: u32 = 99;

#[test]
fn warn_unknown_fields_with_tag() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, warn_unknown_fields[UNKNOWN_FIELD_TAG])]
    struct Simple {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Simple::from_toml(ctx, table.as_item());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, "hi");
    assert_eq!(doc.errors().len(), 1);
    assert!(matches!(
        doc.errors()[0].kind(),
        toml_spanner::ErrorKind::UnexpectedKey { tag: 99 }
    ));
}

#[test]
fn deny_unknown_fields_with_tag() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(FromToml, deny_unknown_fields[UNKNOWN_FIELD_TAG])]
    struct Strict {
        name: String,
    }
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("name = \"hi\"\nextra = 42", &arena).unwrap();
    let (ctx, table) = doc.split();
    let result = Strict::from_toml(ctx, table.as_item());
    assert!(result.is_err());
    assert_eq!(doc.errors().len(), 1);
    assert!(matches!(
        doc.errors()[0].kind(),
        toml_spanner::ErrorKind::UnexpectedKey { tag: 99 }
    ));
}

// --- deprecated_alias tests ---

#[derive(Toml, Debug, PartialEq)]
struct WithDeprecatedAlias {
    #[toml(deprecated_alias = "old_name")]
    new_name: String,
}

#[test]
fn deprecated_alias_primary_key() {
    let v: WithDeprecatedAlias = toml_spanner::from_str("new_name = \"val\"").unwrap();
    assert_eq!(v.new_name, "val");
}

#[test]
fn deprecated_alias_uses_old_key() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("old_name = \"val\"", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<WithDeprecatedAlias>()
        .expect("deprecated alias should still deserialize");
    assert_eq!(val.new_name, "val");
    assert!(!errors.errors.is_empty(), "should have a deprecation error");
    let err = &errors.errors[0];
    assert!(
        format!("{err}").contains("deprecated"),
        "error should mention deprecated: {err}"
    );
}

const TEST_TAG: u32 = 0x1000;

#[derive(Toml, Debug, PartialEq)]
struct WithTaggedDeprecatedAlias {
    #[toml(deprecated_alias[TEST_TAG] = "old_field")]
    new_field: u32,
}

#[test]
fn deprecated_alias_with_tag() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("old_field = 42", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<WithTaggedDeprecatedAlias>()
        .expect("deprecated alias should still deserialize");
    assert_eq!(val.new_field, 42);
    assert!(!errors.errors.is_empty());
    let kind = errors.errors[0].kind();
    match kind {
        toml_spanner::ErrorKind::Deprecated { tag, .. } => {
            assert_eq!(tag, TEST_TAG);
        }
        _ => panic!("expected Deprecated error, got {kind:?}"),
    }
}

#[derive(Toml, Debug, PartialEq)]
struct MultiDeprecatedAlias {
    #[toml(deprecated_alias = "v1_name", deprecated_alias[2] = "v2_name")]
    current_name: String,
}

#[test]
fn multi_deprecated_alias_v1() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("v1_name = \"a\"", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<MultiDeprecatedAlias>()
        .expect("should deserialize");
    assert_eq!(val.current_name, "a");
    assert!(!errors.errors.is_empty());
    match errors.errors[0].kind() {
        toml_spanner::ErrorKind::Deprecated { tag, .. } => assert_eq!(tag, 0),
        k => panic!("expected Deprecated, got {k:?}"),
    }
}

#[test]
fn multi_deprecated_alias_v2() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("v2_name = \"b\"", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<MultiDeprecatedAlias>()
        .expect("should deserialize");
    assert_eq!(val.current_name, "b");
    assert!(!errors.errors.is_empty());
    match errors.errors[0].kind() {
        toml_spanner::ErrorKind::Deprecated { tag, .. } => assert_eq!(tag, 2),
        k => panic!("expected Deprecated, got {k:?}"),
    }
}

#[derive(Toml, Debug, PartialEq)]
struct MixedAliasAndDeprecated {
    #[toml(alias = "alt", deprecated_alias = "old")]
    name: String,
}

#[test]
fn mixed_alias_no_error() {
    let v: MixedAliasAndDeprecated = toml_spanner::from_str("alt = \"val\"").unwrap();
    assert_eq!(v.name, "val");
}

#[test]
fn mixed_deprecated_alias_emits_error() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("old = \"val\"", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<MixedAliasAndDeprecated>()
        .expect("should deserialize");
    assert_eq!(val.name, "val");
    assert!(!errors.errors.is_empty());
}

#[test]
fn deprecated_alias_duplicate_error() {
    let result: Result<WithDeprecatedAlias, _> =
        toml_spanner::from_str("new_name = \"a\"\nold_name = \"b\"");
    assert!(
        result.is_err(),
        "should error on duplicate via deprecated alias"
    );
}

#[derive(Toml, Debug, PartialEq)]
#[toml(rename_all = "kebab-case")]
struct KebabWithDeprecated {
    #[toml(deprecated_alias[TEST_TAG] = "default_features")]
    default_features: bool,
}

#[test]
fn deprecated_alias_with_rename_all() {
    let v: KebabWithDeprecated = toml_spanner::from_str("default-features = true").unwrap();
    assert!(v.default_features);
}

#[test]
fn deprecated_alias_with_rename_all_old_key() {
    let arena = Arena::new();
    let mut doc = toml_spanner::parse("default_features = true", &arena).unwrap();
    let (val, errors) = doc
        .to_allowing_errors::<KebabWithDeprecated>()
        .expect("should deserialize via deprecated alias");
    assert!(val.default_features);
    assert!(!errors.errors.is_empty());
    match errors.errors[0].kind() {
        toml_spanner::ErrorKind::Deprecated { tag, .. } => {
            assert_eq!(tag, TEST_TAG);
        }
        k => panic!("expected Deprecated, got {k:?}"),
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(Toml)]
struct WithTuples {
    pair: (String, i64),
    triple: (bool, i64, String),
    single: (f64,),
}

#[test]
fn derive_tuple_fields_roundtrip() {
    let input = r#"
pair = ["hello", 42]
triple = [true, 7, "ok"]
single = [3.14]
"#;
    let v: WithTuples = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.pair, ("hello".to_string(), 42));
    assert_eq!(v.triple, (true, 7, "ok".to_string()));
    assert_eq!(v.single.0, 3.14);

    let toml_str = toml_spanner::to_string(&v).unwrap();
    let roundtrip: WithTuples = toml_spanner::from_str(&toml_str).unwrap();
    assert_eq!(v, roundtrip);
}

#[test]
fn derive_tuple_wrong_length() {
    let input = r#"
pair = ["too", "many", "items"]
triple = [true, 7, "ok"]
single = [1.0]
"#;
    let result = toml_spanner::from_str::<WithTuples>(input);
    assert!(result.is_err());
}

fn assert_errors_equivalent(
    flat_errs: &[toml_spanner::Error],
    nested_errs: &[toml_spanner::Error],
    label: &str,
) {
    assert_eq!(
        flat_errs.len(),
        nested_errs.len(),
        "{label}: error count mismatch\n  flat:    {flat_errs:?}\n  nested:  {nested_errs:?}"
    );
    for (i, (f, n)) in flat_errs.iter().zip(nested_errs.iter()).enumerate() {
        assert_eq!(
            f.span(),
            n.span(),
            "{label}[{i}]: span mismatch\n  flat:   {f:?}\n  nested: {n:?}"
        );
        assert_eq!(
            f.path().map(|p| p.to_string()),
            n.path().map(|p| p.to_string()),
            "{label}[{i}]: path mismatch\n  flat:   {f:?}\n  nested: {n:?}"
        );
        assert_eq!(
            f.kind().kind_name(),
            n.kind().kind_name(),
            "{label}[{i}]: kind mismatch\n  flat:   {f:?}\n  nested: {n:?}"
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrFlat1 {
    name: String,
    x: i64,
    y: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrNested1 {
    name: String,
    #[toml(flatten, with = flatten_any)]
    point: FaPoint,
}

#[test]
fn flatten_any_error_equivalence_l1() {
    let valid = "name = \"ok\"\nx = 1\ny = 2";
    let flat: FaErrFlat1 = toml_spanner::from_str(valid).unwrap();
    let nested: FaErrNested1 = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.name, nested.name);
    assert_eq!(flat.x, nested.point.x);
    assert_eq!(flat.y, nested.point.y);

    for (label, input) in [
        ("type_mismatch", "name = \"ok\"\nx = \"not_an_int\"\ny = 2"),
        ("second_field", "name = \"ok\"\nx = 1\ny = \"nope\""),
        ("missing_field", "name = \"ok\"\nx = 1"),
        ("both_wrong", "name = \"ok\"\nx = true\ny = \"nope\""),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaErrFlat1>().unwrap_err().errors,
            &d2.to::<FaErrNested1>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrFlat2 {
    top: String,
    mid: String,
    a: i64,
    b: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrInner2 {
    a: i64,
    b: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrMiddle2 {
    mid: String,
    #[toml(flatten, with = flatten_any)]
    inner: FaErrInner2,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrNested2 {
    top: String,
    #[toml(flatten, with = flatten_any)]
    middle: FaErrMiddle2,
}

#[test]
fn flatten_any_error_equivalence_l2() {
    let valid = "top = \"ok\"\nmid = \"ok\"\na = 1\nb = 2";
    let flat: FaErrFlat2 = toml_spanner::from_str(valid).unwrap();
    let nested: FaErrNested2 = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.top, nested.top);
    assert_eq!(flat.mid, nested.middle.mid);
    assert_eq!(flat.a, nested.middle.inner.a);
    assert_eq!(flat.b, nested.middle.inner.b);

    for (label, input) in [
        (
            "type_mismatch",
            "top = \"ok\"\nmid = \"ok\"\na = \"wrong\"\nb = 2",
        ),
        (
            "inner_field",
            "top = \"ok\"\nmid = \"ok\"\na = 1\nb = \"wrong\"",
        ),
        ("missing_inner", "top = \"ok\"\nmid = \"ok\"\na = 1"),
        ("missing_mid", "top = \"ok\"\na = 1\nb = 2"),
        ("mid_wrong_type", "top = \"ok\"\nmid = 999\na = 1\nb = 2"),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaErrFlat2>().unwrap_err().errors,
            &d2.to::<FaErrNested2>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrFlat3 {
    id: i64,
    label: String,
    flag: bool,
    val: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrDeep3 {
    val: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrMid3 {
    flag: bool,
    #[toml(flatten, with = flatten_any)]
    deep: FaErrDeep3,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrWrap3 {
    label: String,
    #[toml(flatten, with = flatten_any)]
    mid: FaErrMid3,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaErrNested3 {
    id: i64,
    #[toml(flatten, with = flatten_any)]
    wrap: FaErrWrap3,
}

#[test]
fn flatten_any_error_equivalence_l3() {
    let valid = "id = 1\nlabel = \"ok\"\nflag = true\nval = 42";
    let flat: FaErrFlat3 = toml_spanner::from_str(valid).unwrap();
    let nested: FaErrNested3 = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.id, nested.id);
    assert_eq!(flat.label, nested.wrap.label);
    assert_eq!(flat.flag, nested.wrap.mid.flag);
    assert_eq!(flat.val, nested.wrap.mid.deep.val);

    for (label, input) in [
        (
            "deepest",
            "id = 1\nlabel = \"ok\"\nflag = true\nval = \"oops\"",
        ),
        (
            "mid",
            "id = 1\nlabel = \"ok\"\nflag = \"not_bool\"\nval = 42",
        ),
        ("outer", "id = 1\nlabel = 42\nflag = true\nval = 10"),
        ("missing_deepest", "id = 1\nlabel = \"ok\"\nflag = true"),
        ("missing_all", "id = 1"),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaErrFlat3>().unwrap_err().errors,
            &d2.to::<FaErrNested3>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaSubPort {
    port: u16,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaSubFlat {
    name: String,
    host: String,
    server: FaSubPort,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaSubInner {
    host: String,
    server: FaSubPort,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaSubNested {
    name: String,
    #[toml(flatten, with = flatten_any)]
    cfg: FaSubInner,
}

#[test]
fn flatten_any_error_equivalence_subtable() {
    let valid = "name = \"app\"\nhost = \"localhost\"\n[server]\nport = 80";
    let flat: FaSubFlat = toml_spanner::from_str(valid).unwrap();
    let nested: FaSubNested = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.name, nested.name);
    assert_eq!(flat.host, nested.cfg.host);
    assert_eq!(flat.server, nested.cfg.server);

    for (label, input) in [
        (
            "type_mismatch",
            "name = \"app\"\nhost = \"localhost\"\n[server]\nport = \"bad\"",
        ),
        (
            "missing_field",
            "name = \"app\"\nhost = \"localhost\"\n[server]",
        ),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaSubFlat>().unwrap_err().errors,
            &d2.to::<FaSubNested>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaBigFlat {
    name: String,
    f1: i64,
    f2: i64,
    f3: i64,
    f4: i64,
    f5: i64,
    f6: i64,
    f7: i64,
    f8: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaBigInnerErr {
    f1: i64,
    f2: i64,
    f3: i64,
    f4: i64,
    f5: i64,
    f6: i64,
    f7: i64,
    f8: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaBigNested {
    name: String,
    #[toml(flatten, with = flatten_any)]
    big: FaBigInnerErr,
}

#[test]
fn flatten_any_error_equivalence_big_table() {
    let valid = "name = \"big\"\nf1 = 1\nf2 = 2\nf3 = 3\nf4 = 4\nf5 = 5\nf6 = 6\nf7 = 7\nf8 = 8";
    let flat: FaBigFlat = toml_spanner::from_str(valid).unwrap();
    let nested: FaBigNested = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.name, nested.name);
    assert_eq!(
        [
            flat.f1, flat.f2, flat.f3, flat.f4, flat.f5, flat.f6, flat.f7, flat.f8
        ],
        [
            nested.big.f1,
            nested.big.f2,
            nested.big.f3,
            nested.big.f4,
            nested.big.f5,
            nested.big.f6,
            nested.big.f7,
            nested.big.f8
        ],
    );

    for (label, input) in [
        (
            "first",
            "name = \"big\"\nf1 = true\nf2 = 2\nf3 = 3\nf4 = 4\nf5 = 5\nf6 = 6\nf7 = 7\nf8 = 8",
        ),
        (
            "middle",
            "name = \"big\"\nf1 = 1\nf2 = 2\nf3 = 3\nf4 = \"wrong\"\nf5 = 5\nf6 = 6\nf7 = 7\nf8 = 8",
        ),
        (
            "last",
            "name = \"big\"\nf1 = 1\nf2 = 2\nf3 = 3\nf4 = 4\nf5 = 5\nf6 = 6\nf7 = 7\nf8 = \"nope\"",
        ),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaBigFlat>().unwrap_err().errors,
            &d2.to::<FaBigNested>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDefaultFlat {
    name: String,
    #[toml(default)]
    x: i64,
    #[toml(default)]
    y: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDefaultInnerErr {
    #[toml(default)]
    x: i64,
    #[toml(default)]
    y: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDefaultNested {
    name: String,
    #[toml(flatten, with = flatten_any)]
    coords: FaDefaultInnerErr,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDeepDefaultFlat {
    name: String,
    #[toml(default)]
    flag: bool,
    #[toml(default)]
    val: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDeepDefaultInner {
    #[toml(default)]
    val: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDeepDefaultMid {
    #[toml(default)]
    flag: bool,
    #[toml(flatten, with = flatten_any)]
    deep: FaDeepDefaultInner,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaDeepDefaultNested {
    name: String,
    #[toml(flatten, with = flatten_any)]
    mid: FaDeepDefaultMid,
}

#[test]
fn flatten_any_error_equivalence_multiple_errors() {
    let valid = "name = \"ok\"\nx = 1\ny = 2";
    let flat: FaDefaultFlat = toml_spanner::from_str(valid).unwrap();
    let nested: FaDefaultNested = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.name, nested.name);
    assert_eq!(flat.x, nested.coords.x);
    assert_eq!(flat.y, nested.coords.y);

    for (label, input) in [
        ("both_wrong", "name = \"ok\"\nx = \"bad\"\ny = true"),
        ("one_wrong", "name = \"ok\"\nx = 1\ny = true"),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaDefaultFlat>().unwrap_err().errors,
            &d2.to::<FaDefaultNested>().unwrap_err().errors,
            label,
        );
    }

    let valid2 = "name = \"ok\"\nflag = true\nval = 5";
    let flat2: FaDeepDefaultFlat = toml_spanner::from_str(valid2).unwrap();
    let nested2: FaDeepDefaultNested = toml_spanner::from_str(valid2).unwrap();
    assert_eq!(flat2.name, nested2.name);
    assert_eq!(flat2.flag, nested2.mid.flag);
    assert_eq!(flat2.val, nested2.mid.deep.val);

    for (label, input) in [
        (
            "across_levels",
            "name = \"ok\"\nflag = \"not_bool\"\nval = \"not_int\"",
        ),
        (
            "inner_only",
            "name = \"ok\"\nflag = true\nval = \"not_int\"",
        ),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaDeepDefaultFlat>().unwrap_err().errors,
            &d2.to::<FaDeepDefaultNested>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaMixedFlat {
    #[toml(default)]
    top: i64,
    a: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaMixedInnerErr {
    a: i64,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaMixedNested {
    #[toml(default)]
    top: i64,
    #[toml(flatten, with = flatten_any)]
    inner: FaMixedInnerErr,
}

#[test]
fn flatten_any_error_equivalence_mixed() {
    let valid = "top = 10\na = 20";
    let flat: FaMixedFlat = toml_spanner::from_str(valid).unwrap();
    let nested: FaMixedNested = toml_spanner::from_str(valid).unwrap();
    assert_eq!(flat.top, nested.top);
    assert_eq!(flat.a, nested.inner.a);

    for (label, input) in [
        ("both_sites", "top = \"wrong\"\na = \"also_wrong\""),
        ("inner_only", "top = 1\na = \"wrong\""),
    ] {
        let a1 = Arena::new();
        let mut d1 = toml_spanner::parse(input, &a1).unwrap();
        let a2 = Arena::new();
        let mut d2 = toml_spanner::parse(input, &a2).unwrap();
        assert_errors_equivalent(
            &d1.to::<FaMixedFlat>().unwrap_err().errors,
            &d2.to::<FaMixedNested>().unwrap_err().errors,
            label,
        );
    }
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaScaleInner {
    c: i64,
    #[toml(flatten, with = flatten_any)]
    rest: HashMap<String, i64>,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(FromToml)]
struct FaScaleRoot {
    a: i64,
    #[toml(flatten, with = flatten_any)]
    inner: FaScaleInner,
}

#[test]
#[cfg_attr(miri, ignore)]
fn flatten_any_error_patching_large_scale() {
    const N: usize = 100_000;

    let mut valid = String::from("a = 1\nc = 2\n");
    for i in 0..4 {
        use std::fmt::Write;
        write!(valid, "k{i} = {i}\n").unwrap();
    }
    let v: FaScaleRoot = toml_spanner::from_str(&valid).unwrap();
    assert_eq!(v.a, 1);
    assert_eq!(v.inner.c, 2);
    assert_eq!(v.inner.rest.len(), 4);
    assert_eq!(v.inner.rest["k0"], 0);
    assert_eq!(v.inner.rest["k3"], 3);

    let mut input = String::with_capacity(N * 20);
    input.push_str("a = 1\nc = 2\n");
    for i in 0..N {
        use std::fmt::Write;
        write!(input, "k{i} = \"wrong\"\n").unwrap();
    }

    let arena = Arena::new();
    let mut doc = toml_spanner::parse(&input, &arena).unwrap();
    let err = doc.to::<FaScaleRoot>().unwrap_err();
    assert_eq!(
        err.errors.len(),
        N,
        "expected {N} errors, got {}",
        err.errors.len()
    );

    for error in &err.errors {
        let path = error
            .path()
            .expect("every error should have a resolved path");
        let path_str = path.to_string();
        assert!(path_str.starts_with("k"), "unexpected path: {path_str}");
        assert!(
            !error.span().is_empty(),
            "error for {path_str} should have a non-empty span",
        );
    }
}
