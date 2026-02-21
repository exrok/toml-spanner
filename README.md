# toml-spanner

High-performance, fast compiling, span preserving toml parsing for rust.
Orginally forked from `toml-span` to add TOML 1.1.0 support, `toml-spanner`
has received significant performance improvements and reductions in compile time.

[![Crates.io](https://img.shields.io/crates/v/toml-spanner?style=flat-square)](https://crates.io/crates/toml-spanner)
[![Docs.rs](https://img.shields.io/docsrs/toml-spanner?style=flat-square)](https://docs.rs/toml-spanner/latest/toml_spanner/)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

Like the original `toml-span` temporal values such as timestamps or local times are not supported.

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

## Benchmarks

Measured on AMD Ryzen 9 5950X, 64GB RAM, Linux 6.18, rustc 1.93.0.
Relative parse time across real-world TOML files (lower is better):

![bench](https://github.com/user-attachments/assets/a762a25f-379f-4d9b-8901-5d2d25ec06c5)

Crate Versions: `toml-spanner = 0.3.0`, `toml = 1.0.2+spec-1.1.0`, `toml-span = 0.7.0`

```
                  time(μs)  cycles(K)   instr(K)  branch(K)
zed/Cargo.toml
  toml-spanner        25.1        119        441         92
  toml               257.2       1220       3084        607
  toml-span          381.6       1816       5048       1046
extask.toml
  toml-spanner         8.9         42        148         29
  toml                78.7        376       1002        192
  toml-span          105.0        500       1335        263
devsm.toml
  toml-spanner         3.6         17         68         15
  toml                32.3        155        422         80
  toml-span           55.0        262        713        141
```

### Compile Time

Extra `cargo build --release` time for binaries using the respective crates (lower is better):

![compile_bench](https://github.com/user-attachments/assets/acff6d37-5df5-4e79-a219-c9cc60c52c22)

```
                 median(ms)    added(ms)
null                     99
toml-spanner            655         +556
toml-span              1375        +1276
toml                   3027        +2928
toml+serde             5037        +4938
```

Checkout `./benchmark` for more details, but numbers should simulate the additional
time added users would experience during source based installs such as via `cargo install`.

## Divergence from `toml-span`

While `toml-spanner` star
ted as a fork of `toml-span`, it has since undergone
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

The `unsafe` in this crate demands thorough testing. The full suite includes
[Miri](https://github.com/rust-lang/miri) for detecting undefined behavior,
fuzzing against the reference `toml` crate, and snapshot-based integration
tests.

```bash
cargo test --workspace                          # all tests
cargo test -p snapshot-tests                       # integration tests only
cargo +nightly miri nextest run                 # undefined behavior checks
cargo +nightly fuzz run parse_compare_toml      # fuzz against the toml crate
cargo +nightly fuzz run parse_value             # fuzz the parser directly

# Test 32bit support under MIRI
cargo +nightly miri nextes -p toml-spanner --target i686-unknown-linux-gnu
```

Integration tests use [insta](https://insta.rs/) for snapshot assertions.
Run `cargo insta test -p snapshot-tests` and `cargo insta review` to review
changes.

Code coverage:

```bash
cargo +nightly llvm-cov --branch --show-missing-lines -- -q
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
