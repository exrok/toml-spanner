# toml-spanner-macros

Derive macros for [toml-spanner](https://crates.io/crates/toml-spanner).

[![Crates.io](https://img.shields.io/crates/v/toml-spanner-macros?style=flat-square)](https://crates.io/crates/toml-spanner-macros)
[![Docs.rs](https://img.shields.io/docsrs/toml-spanner?style=flat-square)](https://docs.rs/toml-spanner/latest/toml_spanner/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square)](LICENSE-MIT)

This crate provides `#[derive(Toml)]` for automatically generating `FromToml` and `ToToml` implementations. It is designed to be used with [toml-spanner](https://crates.io/crates/toml-spanner).

The easiest way to use this crate is by enabling the `derive` feature on `toml-spanner`, which re-exports everything automatically:

```toml
[dependencies]
toml-spanner = { version = "1", features = ["derive"] }
```

You can also depend on `toml-spanner-macros` directly alongside `toml-spanner` as a compile time reduction technique, since it allows Cargo to begin compiling the macros in parallel with `toml-spanner` itself. That said, `toml-spanner-macros` is dependency free and compiles quickly, so in most non-trivial projects the difference is negligible and using the `derive` feature is equally fast.

For documentation on the derive macro and its attributes, see the [toml-spanner docs](https://docs.rs/toml-spanner/latest/toml_spanner/derive.Toml.html).

## License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
