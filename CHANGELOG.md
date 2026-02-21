<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.4.0] - 2026-02-21

### Added

- **DateTime support** — The parser now handles all TOML 1.1.0 temporal values:
  offset date-times, local date-times, local dates, and local times. New types
  `DateTime`, `Date`, `Time`, and `TimeOffset` are exported from the crate root.
  `Value::DateTime` and `Kind::DateTime` variants are added to the value enums.
  This removes the last spec-compliance gap — toml-spanner now fully implements
  TOML 1.1.0.
- **`Root` wrapper type** — `parse()` now returns `Root<'de>` instead of a bare
  `Table`. `Root` bundles the parsed table with a deserialization `Context` that
  carries the parser's hash index, enabling O(1) key lookups during
  deserialization. Access the table via `Root::table()`, `Root::into_table()`,
  or index operators directly on `Root`.
- **`TableHelper` deserialization helper** — New `de::TableHelper` provides
  `required()` and `optional()` field extraction with arena-allocated bitset
  tracking of consumed fields. `expect_empty()` reports all unexpected keys at
  once. `into_remaining()` iterates over unconsumed entries for catch-all
  deserialization patterns.
- **Multi-error accumulation** — The `de::Context` collects all deserialization
  errors rather than failing on the first one. Call `Root::into_result()` or
  inspect `ctx.errors` to retrieve accumulated diagnostics.
- **`Kind` enum made public** — `Item::kind()` and the `Kind` enum are now
  public, with `Debug` and `Display` impls for type-name formatting.
- **`Error::custom` constructor** — Convenience method for creating errors with
  a custom message and span.

### Changed

- **Immutable deserialization model** — The `Deserialize` trait signature changed
  from `fn deserialize(item: &mut Item<'de>) -> Result<Self, Error>` to
  `fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed>`.
  Deserialization is now immutable over the parsed tree — fields are tracked via
  a bitset rather than removed from the table. This preserves the hash index
  built during parsing and simplifies borrow checking.
- **`Deserialize` trait moved to `de` module** — The trait and its companion
  `DeserializeOwned` are now in the public `de` module, along with `Context`,
  `Failed`, and `TableHelper`. The `de` module is re-exported at the crate root
  for convenience.
- **`Table::required` / `optional` / `expect_empty` removed** — These methods
  are replaced by `TableHelper::required`, `TableHelper::optional`, and
  `TableHelper::expect_empty`. Use `Root::helper()` or
  `Item::table_helper(ctx)` to obtain a `TableHelper`.
- **`Table::remove` and `Table::values_mut` removed** — Use
  `Table::remove_entry` instead of `remove`. Mutable value iteration is no
  longer exposed.
- **`parse()` returns `Root` instead of `Table`** — Callers that only need the
  table can call `.into_table()` or `.table()`.
- **Stricter arena lifetime bounds** — `Table::insert` and internal `grow`
  methods now require `&'de Arena` instead of `&Arena`, preventing a potential
  use-after-free when a shorter-lived arena was used for collection growth.
- **Reject stray carriage returns** — The parser now rejects `\r` not followed
  by `\n`, matching the TOML spec and the reference `toml` crate behavior.
- **File size limit corrected** — Maximum input size is 512 MiB (exclusive),
  corrected from the previously documented 4 GiB.
- **`Table::as_item` replaces consuming conversion** — New `Table::as_item()`
  returns `&Item<'de>` via zero-cost transmute, complementing the existing
  `into_item()`.
- **Micro parser optimizations** — Reduced redundant byte peeks, restructured
  pattern matching to avoid matching on `u8` and `Option` simultaneously,
  lowering generated MIR/LLVM IR.
- **32-bit overflow protection** — `InnerTable::grow_to` uses checked
  multiplication on 32-bit targets to prevent capacity overflow.
- **Integration tests renamed** — `integ-tests` workspace member renamed to
  `snapshot-tests` to better reflect its purpose.

## [0.3.0] - 2026-02-16

### Changed

- Replace `Str<'de>` with `&'de str` in `Key`, `Value` and `ValueMut`

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

[Unreleased]: https://github.com/exrok/toml-spanner/compare/0.4.0...HEAD
[0.4.0]: https://github.com/exrok/toml-spanner/compare/0.3.0...0.4.0
[0.3.0]: https://github.com/exrok/toml-spanner/compare/0.2.0...0.3.0
[0.2.0]: https://github.com/exrok/toml-spanner/compare/0.1.0...0.2.0
[0.1.0]: https://github.com/exrok/toml-spanner/releases/tag/0.1.0
