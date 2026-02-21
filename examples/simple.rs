#![allow(dead_code)]
use toml_spanner::{Arena, Context, Deserialize, Failed, Item};

#[derive(Debug)]
struct Config {
    nested: Vec<Config>,
    enable: bool,
    number: u32,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let config = Config {
            enable: th.optional("enabled").unwrap_or(false),
            number: th.required("number")?,
            nested: th.required("nested")?,
        };
        th.expect_empty()?;
        Ok(config)
    }
}

const TOML_DOCUMENT: &str = r#"
enabled = false
number = 37

[[nested]]
number = 43

[[nested]]
enabled = true
number = 12
"#;

fn main() {
    let arena = Arena::new();

    let mut root = toml_spanner::parse(TOML_DOCUMENT, &arena).unwrap();
    if let Ok(config) = root.deserialize::<Config>() {
        println!("parsed: {:?}", config);
    } else {
        println!("Deserialization Failure");
        for error in root.errors() {
            println!("error: {}", error);
        }
    }
}
