<div align="center">

# `toml-spanner`

This create is fork of `toml-span` adding more support for the toml 1.1.0 spec,
along the way,

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
