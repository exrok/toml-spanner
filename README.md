# toml-spanner

This crate is a fork of `toml-span` that has diverged significantly from the
original, with major performance improvements, a much smaller memory footprint,
and preserved insertion order for tables.

## Divergence from `toml-span`

While `toml-spanner` started as a fork of `toml-span`, it has since undergone
extensive changes:

- **Up to 10x faster** than `toml-span`, and 4-7x faster than `toml` across
  real-world workloads.
- **Compact `Value` type** — `size_of::<Value>` is 24 bytes, compared to 48 in
  `toml_span`, 32 in `toml`, and 80 in `toml` with `preserve_order`.
- **Small map entries** — table entry size is 48 bytes, versus 88 in
  `toml_span`, 56 in `toml`, and 104 in `toml` with `preserve_order`.
- **Preserved index order** — tables retain their insertion order by default,
  unlike `toml_span` and the default mode of `toml`.

### Trade-offs

`toml-spanner` makes extensive use of `unsafe` code to achieve its performance
and size goals. This is mitigated by fuzzing and running the test suite under
Miri.

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
