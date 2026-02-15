<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate

## [0.2.0] - 2026-02-15

### Added

- **Arena-based allocation** — All parsed data (`Str`, `Table`, `Array`) is now allocated in a
  caller-supplied `Arena` bump allocator. The parse signature is now
  `parse(input, &arena) -> Result<Table, Error>`. This eliminates per-value heap allocations
  and enables bulk deallocation.
- **Arena realloc** — The arena supports in-place `realloc` when the allocation is at the tip,
  avoiding unnecessary copies during table and array growth.
- **Compact `Item` type** — Introduced a 24-byte `Item<'de>` tagged union with bit-packed span
  information, replacing the previous `Value` type. The `Value` name is now used for the
  borrowed enum view (`Value<'a, 'de>`), and `ValueMut<'a, 'de>` provides mutable access.
- **`MaybeItem` and null coalescing index operators** — `Item`, `Table`, and `Array` implement
  `Index` traits that return `MaybeItem` instead of panicking. This enables chained lookups
  like `item["key"][2]["name"].as_str()` that propagate `None` through the entire chain.
- **Hash index for table lookups** — Tables with 6 or more entries automatically build a hash
  index (using `foldhash`) for O(1) key lookups, while small tables retain fast linear scan.
- **`Str<'de>` type** — A 16-byte `Copy` string type that borrows from either the input or the
  arena. No `Drop` required, and supports `Deref<Target=str>`, `Borrow<str>`, and conversions
  to `String`, `Box<str>`, and `Cow<'de, str>`.
- **Fuzzer** — Added `cargo-fuzz` targets that cross-validate parsing output against the
  standard `toml` crate.
- **Miri test suite** — Dedicated tests for detecting undefined behavior via `cargo +nightly miri test`.
- **Recursion limit** — The parser now enforces a recursion depth limit to prevent stack
  overflow on deeply nested input.
- **UTF-8 BOM handling** — The parser now correctly skips a leading UTF-8 BOM if present.
- **Strict float parsing** — Float parsing now strictly conforms to the TOML spec, rejecting
  cases like missing digits around `.`, signs on radix literals, and misplaced underscores.
- **Benchmark suite** — Added a benchmark workspace member with real-world TOML files
  (Cargo.toml, task configs) for performance tracking.

### Changed

- **~10x performance improvement** — Complete parser rewrite using raw pointer traversal,
  optimized string reading (batch instead of per-byte), optimized integer and float
  formatting, scratch buffer extraction, and arena allocation. Benchmarks show ~10x faster
  than the original `toml-span` and 5-8x faster than the standard `toml` crate.
- **~1/3 compile time** — Reduced LLVM IR output through code bloat reduction in formatting
  impls, integer formatting optimization, and careful avoidance of monomorphization bloat.
- **`parse()` now returns `Table`** — The top-level `parse` function returns `Table<'de>`
  directly instead of a wrapped `Value`, since a TOML document is always a table.
- **`TableHelper` merged into `Table`** — The separate `TableHelper` type is removed.
  Deserialization methods `required()`, `optional()`, and `expect_empty()` are now methods
  directly on `Table`. Helper methods like `take_string()`, `parse()`, and `expected()` are
  now methods on `Item`.
- **`Value` renamed to `Item`** — The primary parsed value type is now `Item<'de>`. The name
  `Value` is repurposed as a borrowed enum (`Value<'a, 'de>`) for pattern matching, obtained
  via `item.value()`.
- **Unified error type** — `DeserError` and error aggregation are removed. There is now a
  single `Error` type with an `ErrorKind` enum and a `Span`. Line/column info is no longer
  stored in the error; compute it on demand when displaying diagnostics.
- **Removed `Clone` from `Error`/`ErrorKind`** — Avoids generating unnecessary code for a
  large enum that should not typically be cloned.
- **Simplified module structure** — Flattened internal modules; all types are exported from
  the crate root. No more deep module paths like `toml_spanner::value::Value`.
- **Table iterators** — `Table` iterators now consistently yield references to `(Key, Item)`
  entries. `IntoIterator` is implemented for `&Table`, `&mut Table`, and `Table`.
- **CRLF normalization removed** — The parser no longer normalizes `\r\n` to `\n` in parsed
  strings, matching the behavior of the standard `toml` crate. The TOML spec permits but
  does not require normalization.
- **`Spanned<T>` deserialization** — `Spanned<T>` now has a blanket `Deserialize` impl,
  so `table.required::<Spanned<T>>()` works for any deserializable `T`.

## [0.1.0] - 2026-01-18

Initial release of `toml-spanner`, forked from [`toml-span`](https://github.com/EmbarkStudios/toml-span).

### Added

- Partial TOML 1.1.0 support:
  - Newlines in inline tables
  - Trailing commas in inline tables
  - `\e` escape sequence
  - `\xHH` hex escape sequences

<!-- next-url -->
[Unreleased]: https://github.com/exrok/toml-spanner/compare/0.2.0...HEAD
[0.2.0]: https://github.com/exrok/toml-spanner/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/exrok/toml-spanner/releases/tag/0.1.0
