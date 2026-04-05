<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased]

## [1.0.1] - 2026-04-04

### Added

- `ignore_source_formatting_recursively` option for emit, for opting out of format preservation for
  a particular item.

### Fixed

- Improve spacing around array-of-tables elements in emit
- Require span equality for format preservation when `span_projection_identity` enabled.

## [1.0.0] - 2026-03-29

### Added

- `#[derive(Toml)]` macro for deriving `FromToml` and `ToToml`
  - Struct and enum support with `rename`, `rename_all`, `default`, `skip`, `skip_if`, `flatten`, `with`, `alias`, `transparent`, `deny_unknown_fields`
  - Tagged, content, and untagged enum representations
  - Trait-scoped attributes (e.g. `#[toml(From with = parse_string, To with = display)]`)
  - `#[toml(style)]` attribute for controlling table and array style
  - `#[toml(other)]` catch-all enum variant
  - `#[toml(TryFrom = "Type")]` and `#[toml(From = "Type")]` container attributes
  - Combined `flatten` + `with` attribute support
- `ToToml` trait and serialization to TOML text via `to_string()` or `Formatting` builder
- `FromFlattened` / `ToFlattened` traits for map-like types that consume or emit remaining table keys
- `Formatting` builder API for serialization with indentation, style, and format preservation
  - `Formatting::preserved_from(&doc)` preserves comments, whitespace, and ordering from a parsed document
  - Order-independent array reprojection for format preservation
- `flatten_any` helper module for use with the `with` derive attribute
- Full TOML path tracking in `FromToml` errors
- `DateTime` now implements `PartialEq`
- `Item::table_helper()` convenience method
- `Item::parse()` and `parse_string()` helpers
- Encoder in `toml-test-harness` for official TOML test suite validation

### Changed

- **Renamed**: `Root` to `Document`, `Deserialize` to `FromToml`, `Context`
- **Features**: `deserialization` renamed to `from-toml`, new `to-toml` and `derive` features.
- **Removed** `reporting` feature (replaced by Error methods: `message`, `primary_label`, `secondary_label`)
- Improved parser error messages all around

### Fixed

- Format preservation no longer conflates child and self order
- Empty `0..0` spans treated uniformly as placeholders
- Generic bounds forwarded correctly when the bound has a default

## [0.4.0] - 2026-02-21

### Added

- Full TOML 1.1.0 datetime support: `DateTime`, `Date`, `Time`, `TimeOffset` types,
  `Value::DateTime` and `Kind::DateTime` variants
- `Root` wrapper type returned by `parse()`, bundling the table with a deserialization `Context`
- `TableHelper` for field extraction with bitset tracking, multi-error accumulation
- `Kind` enum and `Item::kind()` made public
- `Error::custom` constructor

### Changed

- Immutable deserialization: trait signature changed to
  `fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed>`,
  fields tracked via bitset instead of table mutation
- `Deserialize` trait and helpers moved to public `de` module
- `Table::required`/`optional`/`expect_empty` replaced by `TableHelper` methods
- `Table::remove` replaced by `Table::remove_entry`; `values_mut` removed
- `parse()` returns `Root` instead of `Table`
- Stricter arena lifetime bounds (`&'de Arena`) on `Table::insert` and `grow`
- Reject bare `\r` not followed by `\n`
- File size limit corrected to 512 MiB
- Added `Table::as_item()` (zero-cost `&Item` view)
- Parser micro-optimizations reducing generated MIR/LLVM IR
- Checked multiplication in `InnerTable::grow_to` on 32-bit targets
- `integ-tests` renamed to `snapshot-tests`

## [0.3.0] - 2026-02-16

### Changed

- Replace `Str<'de>` with `&'de str` in `Key`, `Value` and `ValueMut`

## [0.2.0] - 2026-02-15

### Added

- Arena-based bump allocation for all parsed data; `parse(input, &arena) -> Result<Table, Error>`
- In-place arena realloc at the tip
- Compact 24-byte `Item<'de>` with bit-packed spans, replacing the old `Value` type.
  `Value` repurposed as a borrowed enum view, `ValueMut` for mutable access
- `MaybeItem` with null-coalescing index operators for chained lookups
- Automatic hash index (foldhash) for tables with 6+ entries
- `Str<'de>`: 16-byte `Copy` string borrowing from input or arena
- Fuzz targets cross-validating against the `toml` crate
- Miri test suite
- Recursion depth limit, UTF-8 BOM handling, strict float parsing
- Benchmark suite

### Changed

- ~10x faster parsing via complete rewrite (pointer traversal, batched string reads, arena allocation);
  5-8x faster than the standard `toml` crate
- ~1/3 compile time through LLVM IR reduction
- `parse()` returns `Table` directly instead of wrapped `Value`
- `TableHelper` merged into `Table`; `Value` renamed to `Item`
- Single `Error` type with `ErrorKind` enum and `Span`; removed `Clone` from errors
- Flattened module structure; all types exported from crate root
- Table iterators yield `&(Key, Item)`; `IntoIterator` for `&Table`, `&mut Table`, `Table`
- CRLF normalization removed (matches `toml` crate behavior)
- Blanket `Deserialize` impl for `Spanned<T>`

## [0.1.0] - 2026-01-18

Initial release of `toml-spanner`, forked from [`toml-span`](https://github.com/EmbarkStudios/toml-span).

### Added

- Partial TOML 1.1.0 support:
  - Newlines in inline tables
  - Trailing commas in inline tables
  - `\e` escape sequence
  - `\xHH` hex escape sequences

<!-- next-url -->

[Unreleased]: https://github.com/exrok/toml-spanner/compare/1.0.1...HEAD
[1.0.1]: https://github.com/exrok/toml-spanner/compare/1.0.0...1.0.1
[1.0.0]: https://github.com/exrok/toml-spanner/compare/0.4.0...1.0.0
[0.4.0]: https://github.com/exrok/toml-spanner/compare/0.3.0...0.4.0
[0.3.0]: https://github.com/exrok/toml-spanner/compare/0.2.0...0.3.0
[0.2.0]: https://github.com/exrok/toml-spanner/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/exrok/toml-spanner/releases/tag/0.1.0
