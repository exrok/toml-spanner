# toml-spanner

High-performance, fast compiling, TOML serialization and deserialization library for rust with full compliance with the TOML 1.1 spec.

[![Crates.io](https://img.shields.io/crates/v/toml-spanner?style=flat-square)](https://crates.io/crates/toml-spanner)
[![Docs.rs](https://img.shields.io/docsrs/toml-spanner?style=flat-square)](https://docs.rs/toml-spanner/latest/toml_spanner/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square)](LICENSE-MIT)

toml-spanner is a complete TOML library featuring:

- High Performance: [See Benchmarks](#benchmarks)
- Fast (Increment & Clean) Compilation: [See Compile Time Benchmarks](https://github.com/exrok/rust-serialization-build-time-benchmarks/blob/main/README.md)
- Compact Span Preserving Tree
- Derive macros: optional, powerful, zero-dependency: [See Derive Documentation](https://docs.rs/toml-spanner/latest/toml_spanner/derive.Toml.html)
- Format Preserving Serialization, even through mutation on your own data types.
- Full TOML 1.1, including date-time support, passing 100% of official TOML test-suite
- Tiny Binary Size: [See Binary Size Benchmarks](https://github.com/exrok/rust-serialization-build-time-benchmarks/blob/main/report/BENCH-cargo-toml.md#binary-size)
- Extensively tested with miri and fuzzing under memory sanitizers and debug assertions.
- High quality error messages: [See Error Examples](#error-examples)

## Example

Suppose you have some TOML document declared in `TOML_DOCUMENT` as a `&str`:

```toml
enabled = false
number = 37

[[nested]]
number = 43

[[nested]]
enabled = true
number = 12
```

Parse the TOML document into an `Item` tree:

```rust
let arena = toml_spanner::Arena::new();
let doc = toml_spanner::parse(TOML_DOCUMENT, &arena).unwrap();
```

Traverse the tree and inspect values:

```rust
assert_eq!(doc["nested"][1]["enabled"].as_bool(), Some(true));

match doc["nested"].value() {
    Some(Value::Array(array)) => assert_eq!(array.len(), 2),
    Some(other) => panic!("Expected Array but found: {:#?}", other),
    None => panic!("Expected value but found nothing"),
}
```

### Derive Macros

The `Toml` derive macro generates `FromToml` and/or `ToToml` implementations.

```rust
use toml_spanner::{Arena, Toml};

#[derive(Debug, Toml)]
#[toml(From, To)] // By default only `FromToml` is derived.
struct Config {
    name: String,
    port: u16,
    #[toml(default)]
    debug: bool,
}

let arena = Arena::new();
let mut doc = toml_spanner::parse("name = 'app'\nport = 8080", &arena).unwrap();
let config = doc.to::<Config>().unwrap();
```

See the [`Toml` derive docs](https://docs.rs/toml-spanner/latest/toml_spanner/derive.Toml.html)
for the full set of attributes (`rename`, `default`, `flatten`, `skip`, tagged enums, etc.).

### Manual `FromToml`

Implement `FromToml` directly using `TableHelper` for type-safe field extraction.

```rust
use toml_spanner::{Arena, Context, FromToml, Failed, Item};

#[derive(Debug)]
struct Config {
    enabled: bool,
    nested: Vec<Config>,
    number: u32,
}

impl<'de> FromToml<'de> for Config {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let config = Config {
            enabled: th.optional("enabled").unwrap_or(false),
            number: th.required("number")?,
            nested: th.optional("nested").unwrap_or_default(),
        };
        th.expect_empty()?;
        Ok(config)
    }
}

let arena = Arena::new();
let mut doc = toml_spanner::parse(TOML_DOCUMENT, &arena).unwrap();

match doc.to::<Config>() {
    Ok(config) => println!("parsed: {config:?}"),
    Err(errors) => {
        for error in &errors {
            println!("error: {error}");
        }
    }
}
```

### Serialization

Any type implementing `ToToml` (including via derive) can be written to TOML
with `to_string` or the `Formatting` builder for format-preserving output.

```rust
// Use default formatting.
let output = toml_spanner::to_string(&config).unwrap();

// Preserve formatting from a parsed document
let output = toml_spanner::Formatting::preserved_from(&doc).format(&config).unwrap();
```

See [`Formatting` docs](https://docs.rs/toml-spanner/latest/toml_spanner/struct.Formatting.html)
for indentation, format preservation, and other options.

Please consult the [API documentation](https://docs.rs/toml-spanner/latest/toml_spanner/) for more details.

## Benchmarks

Measured on AMD Ryzen 9 5950X, 64GB RAM, Linux 6.18, rustc 1.94.1.
Relative parse time across real-world TOML files (lower is better):

![bench](https://github.com/user-attachments/assets/6a0d460d-a6e4-4b52-9849-03d65cac4998)

Crate versions: `toml-spanner 1.0.0`, `toml 1.0.7+spec-1.1.0`, `toml_edit 0.25.5+spec-1.1.0`, `toml-span 0.7.1`

```
                  time(μs)  cycles(K)   instr(K)  branch(K)
zed/Cargo.toml
  toml-spanner        24.5        115        440         93
  toml               228.5       1088       2912        523
  toml_edit          306.6       1460       4252        861
  toml-span          393.8       1866       5024       1045
extask.toml
  toml-spanner         8.9         43        149         29
  toml                78.5        374       1031        177
  toml_edit          106.7        505       1470        290
  toml-span          105.8        500       1331        263
devsm.toml
  toml-spanner         3.7         17         70         15
  toml                35.8        171        459         79
  toml_edit           48.7        232        650        127
  toml-span           56.4        269        708        140
```

This runtime benchmark is pretty simple and focuses just on the parsing step. In practice,
if you also deserialize into your own data types (where toml-spanner has only made marginal
improvements), the total runtime improvement is less, but it is highly dependent on the content
and target data types. Switching devsm from `toml-span` to `toml-spanner` saw a total 8x reduction
in runtime measured from the actual application when including both parsing and deserialization.

### Deserialization and Parsing

Usually, you don't just parse TOML, `toml-spanner` derive macros for for full deserialization.

The following benchmarks have taken the exact data structures and deserialization code (originally
using toml and serde), and added support for `toml-spanner` and `toml-span` based parsing and
deserialization. (I haven't added `toml-span` support for Cargo.toml due to its complexity.)

![bench_cargo](https://github.com/user-attachments/assets/4d606902-05c1-4db5-ab08-0d06e8b4f00f)

Crate versions: `toml-spanner = 1.0.1`, `toml = 1.0.7+spec-1.1.0`, `toml-span = 0.7.1`

Commit `3ca292befbc3585084922c1592ea3d17e423f035` was used from `rust-lang/cargo` as reference.

```
                  time(μs)  cycles(K)   instr(K)  branch(K)
zed/Cargo.lock (parse + deserialize)
  toml-spanner      1023.5       4803      16135       3514
  toml              2977.4      14248      37270       7296
  toml-span         5643.2      26831      74584      15460
zed/Cargo.toml (parse + deserialize)
  toml-spanner        92.5        439       1405        283
  toml               309.4       1475       3622        662
```

### Compile Time

For a crate serializing and deserialization a simiplifed cargo manifest using the derive macro respect each crate. With unrestricted parallelism we get the following:

<img width="1971" height="725" alt="cargo-toml_aggregate" src="https://github.com/user-attachments/assets/d311f26d-0815-4758-9cee-de520390e329" />

See [Compile Time Benchmarks](https://github.com/exrok/rust-serialization-build-time-benchmarks/blob/main/README.md) for more details.

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

### Error Examples

Toml-spanner provides specific errors with spans and paths pointing directly to the problem, multi-error accumulation,
and methods for easy use with [annotate-snippets](https://crates.io/crates/annotate-snippets)
and [codespan-reporting](https://crates.io/crates/codespan-reporting).

Here are some parsing examples using the annotated-snippets feature:

![unterminated string](https://raw.githubusercontent.com/exrok/toml-spanner/f04adac57a998c24361b1acaf39950c4287d4562/crates/error-examples/output/unterminated_string.svg)

![duplicate key](https://raw.githubusercontent.com/exrok/toml-spanner/f04adac57a998c24361b1acaf39950c4287d4562/crates/error-examples/output/duplicate_key.svg)

Here are some conversion errors, note how multiple errors are reported instead of
bailing out after the first error.

![deserialization errors](https://raw.githubusercontent.com/exrok/toml-spanner/f04adac57a998c24361b1acaf39950c4287d4562/crates/error-examples/output/deserialization_errors.svg)

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
cargo +nightly miri nextest run -p toml-spanner --target i686-unknown-linux-gnu
```

Integration tests use [insta](https://insta.rs/) for snapshot assertions.
Run `cargo insta test -p snapshot-tests` and `cargo insta review` to review
changes.

Code coverage:

```bash
cargo +nightly llvm-cov --branch --show-missing-lines -- -q
```

Note: See the `devsm.toml` file in the root for typical commands that are run during development.

### Acknowledgements

toml-spanner started off as fork of toml-span and though it's been pretty muchcompletely rewritten at this point, the original test suite and some of the API patterns remain.

Thanks to both the toml and toml-edit crates inspired the API as well as the error messages as
well as serving targets to fuzz against.

### License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
