use std::collections::BTreeMap;
use toml_spanner::{
    Arena, EmitConfig, Failed, FromToml, Root, ToContext, ToToml, emit_with_config, parse,
    reproject,
};
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

pub fn srp<T: ToToml>(root: &Root<'_>, item: &T) -> Result<String, Failed> {
    let arena = Arena::new();
    let mut ctx = ToContext::new(&arena);
    let mut item = item.to_toml(&mut ctx)?;
    let Some(table) = item.as_table_mut() else {
        return Err(Failed);
    };
    let mut items = Vec::new();
    reproject(root, table, &mut items);
    let mut output = Vec::new();
    emit_with_config(
        table.normalize(),
        &EmitConfig {
            projected_source_items: &items,
            projected_source_text: root.ctx.source(),
            reprojected_order: true,
        },
        &mut output,
    );

    Ok(String::from_utf8(output).unwrap())
}
