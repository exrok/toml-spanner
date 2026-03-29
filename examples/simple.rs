#![allow(dead_code)]
use toml_spanner::{Arena, Context, Failed, FromToml, Item, Value};

const TOML_DOCUMENT: &str = r#"
enabled = false
number = 37

[[nested]]
number = 43

[[nested]]
enabled = true
number = 12
"#;

#[derive(Debug)]
struct Config {
    enabled: bool,
    nested: Vec<Config>,
    number: u32,
}

impl<'de> FromToml<'de> for Config {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let config = Config {
            enabled: th.optional("enabled").unwrap_or(false),
            number: th.required("number")?,
            nested: th.optional("nested").unwrap_or_default(),
        };
        th.require_empty()?;
        Ok(config)
    }
}

fn main() {
    let arena = Arena::new();

    let mut doc = toml_spanner::parse(TOML_DOCUMENT, &arena).unwrap();

    assert_eq!(doc["nested"][1]["enabled"].as_bool(), Some(true));

    match doc["nested"].value() {
        Some(Value::Array(array)) => assert_eq!(array.len(), 2),
        Some(other) => panic!("Expected Array but found: {:#?}", other),
        None => panic!("Expected value but found nothing"),
    }

    match doc.to::<Config>() {
        Ok(config) => println!("parsed: {:#?}", config),
        Err(e) => {
            println!("Deserialization Failure");
            for error in &e.errors {
                println!("error: {}", error);
            }
        }
    }
}
