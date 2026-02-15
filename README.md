# toml-spanner

High-performance, fast compiling, span preserving toml parsing for rust.
Orginally forked from `toml-span` to add TOML 1.1.0 support, `toml-spanner`
has received significant performance improvements and reductions in compile time.

[![Crates.io](https://img.shields.io/crates/v/toml-spanner?style=flat-square)](https://crates.io/crates/toml-spanner)
[![Docs.rs](https://img.shields.io/docsrs/toml-spanner?style=flat-square)](https://docs.rs/jsony/latest/toml-spanner/)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

Like the orginal `toml-span` temporal values such as timestamps or local times are not supported.

## Example

```rust
use toml_spanner::{Deserialize, Error, Item};

#[derive(Debug)]
struct Things {
    name: String,
    value: u32,
    color: Option<String>,
}

impl<'de> Deserialize<'de> for Things {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        Ok(Things {
            name: table.required("name")?,
            value: table.required("value")?,
            color: table.optional("color")?,
        })
    }
}

struct Config {
    things: Vec<Things>,
    dev_mode: bool,
}

impl Config {
    pub fn parse(content: &str) -> Result<Config, Error> {
        let arena = toml_spanner::Arena::new();
        let mut table = toml_spanner::parse(content, &arena)?;
        let config = Config {
            things: table.required("things")?,
            dev_mode: table.optional("dev-mode")?.unwrap_or(false),
        };

        // Report unexpected fields
        table.expect_empty()?;

        Ok(config)
    }
}

fn main() {
    let content = r#"
    dev-mode = true

    [[things]]
    name = "hammer"
    value = 43

    [[things]]
    name = "drill"
    value = 300
    color = "green"
    "#;

    match Config::parse(content) {
        Ok(config) => {
            println!("dev_mode: {}", config.dev_mode);
            for thing in config.things {
                println!("thing: {:?}", thing);
            }
        }
        Err(e) => eprintln!("Error parsing config: {e}"),
    }
}
```

## Divergence from `toml-span`

While `toml-spanner` started as a fork of `toml-span`, it has since undergone
extensive changes:

- 10x faster than `toml-span`, and 5-8x faster than `toml` across
  real-world workloads.
- Preserved index order: tables retain their insertion order by default,
  unlike `toml_span` and the default mode of `toml`.

- Compact `Value` type (on 64bit platforms):

  | Crate                 | Value/Item | TableEntry |
  | --------------------- | ---------- | ---------- |
  | **toml-spanner**      | 24 bytes   | 48 bytes   |
  | toml-span             | 48 bytes   | 88 bytes   |
  | toml                  | 32 bytes   | 56 bytes   |
  | toml (preserve_order) | 80 bytes   | 104 bytes  |

  Note that the `toml` crate `Value` type doesn't contain any span information
  and that `toml-span` doesn't support table entry order preservation.

### Trade-offs

`toml-spanner` makes extensive use of `unsafe` code to achieve its performance
and size goals. This is mitigated by fuzzing and running the test suite under
Miri.

### Testing

Code coverage

```
cargo install cargo-llvm-cov
```

```
cargo +nightly llvm-cov  --branch --show-missing-lines -- -q
```

## Differences from `toml`

First off I just want to be up front and clear about the differences/limitations of this crate versus `toml`

1. No `serde` support for deserialization, there is a `serde` feature, but that only enables serialization of the `Value` and `Spanned` types.
1. No toml serialization. This crate is only intended to be a span preserving deserializer, there is no intention to provide serialization to toml, especially the advanced format preserving kind provided by `toml-edit`.
1. No datetime deserialization. It would be trivial to add support for this (behind an optional feature), I just have no use for it at the moment. PRs welcome.

### License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
