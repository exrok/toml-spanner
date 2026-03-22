use jsony::Jsony;
use std::{collections::HashMap, io::Read};
use toml_spanner::{Arena, Array, DateTime, Formatting, Item, Key, Table};

#[derive(Jsony)]
#[jsony(untagged)]
enum Toml {
    Array(Vec<Toml>),
    Scalar(Scalar),
    Object(HashMap<String, Toml>),
}

mod parsed_string {
    use std::{fmt::Display, str::FromStr};

    use jsony::json::DecodeError;
    pub fn decode_json<T: FromStr>(
        parser: &mut jsony::parser::Parser<'_>,
    ) -> Result<T, &'static DecodeError>
    where
        T::Err: Display,
    {
        let text = parser.at.take_string(&mut parser.scratch)?;
        match text.parse() {
            Ok(value) => Ok(value),
            Err(err) => {
                parser.report_error(err.to_string());
                Err(&DecodeError {
                    message: "Failed to parse string",
                })
            }
        }
    }
}

#[derive(Jsony)]
#[jsony(tag = "type", content = "value", rename_all = "kebab-case")]
enum Scalar {
    String(String),
    Integer(#[jsony(with = parsed_string)] i64),
    Float(#[jsony(with = parsed_string)] f64),
    Bool(#[jsony(with = parsed_string)] bool),
    Datetime(#[jsony(with = parsed_string)] DateTime),
    DatetimeLocal(#[jsony(with = parsed_string)] DateTime),
    DateLocal(#[jsony(with = parsed_string)] DateTime),
    TimeLocal(#[jsony(with = parsed_string)] DateTime),
}

fn convert<'a>(toml: &Toml, arena: &'a Arena) -> Item<'a> {
    match toml {
        Toml::Object(map) => {
            let mut table = Table::new();
            for (key, value) in map {
                let key = Key::new(arena.alloc_str(key));
                table.insert(key, convert(value, arena), arena);
            }
            table.into_item()
        }
        Toml::Array(arr) => {
            let mut array = Array::new();
            for value in arr {
                array.push(convert(value, arena), arena);
            }
            array.into_item()
        }
        Toml::Scalar(scalar) => match scalar {
            Scalar::String(s) => Item::string(arena.alloc_str(s)),
            Scalar::Integer(i) => Item::from(*i),
            Scalar::Float(f) => Item::from(*f),
            Scalar::Bool(b) => Item::from(*b),
            Scalar::Datetime(dt)
            | Scalar::DatetimeLocal(dt)
            | Scalar::DateLocal(dt)
            | Scalar::TimeLocal(dt) => Item::from(*dt),
        },
    }
}

fn main() {
    let mut input = std::io::stdin();
    let _ = input.lock();
    let mut buffer = Vec::new();
    let _ = input.read_to_end(&mut buffer);
    let toml = match jsony::from_json_bytes::<Toml>(&buffer) {
        Ok(toml) => toml,
        Err(err) => {
            eprintln!("Parsing JSON input failed: {}", err);
            std::process::exit(1);
        }
    };
    let arena = Arena::new();
    let table = match toml {
        Toml::Object(map) => {
            let mut table = Table::new();
            for (key, value) in &map {
                let key = Key::new(arena.alloc_str(key));
                table.insert(key, convert(value, &arena), &arena);
            }
            table
        }
        _ => {
            eprintln!("Expected top-level JSON object");
            std::process::exit(1);
        }
    };
    let output = Formatting::default().format_table_to_bytes(table, &arena);
    // SAFETY: emit produces valid UTF-8 TOML text
    let text = unsafe { String::from_utf8_unchecked(output) };
    print!("{text}");
}
