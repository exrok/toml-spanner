# Fuzz Tests

Fuzz targets for toml-spanner using `libfuzzer-sys`. Requires nightly.

## Running

```bash
# run a target (runs until stopped or crash)
cargo +nightly fuzz run --fuzz-dir crates/fuzz <target> -- -max_len=128

# minimize a crash artifact
cargo +nightly fuzz tmin --fuzz-dir crates/fuzz <target> --runs 100000 <artifact>

# reproduce a crash with diagnostics
cd crates/fuzz && cargo run -- <target> <artifact_path>
```

The `fuzz` default binary is a CLI reproducer that prints detailed diagnostics for crash
artifacts (tree dumps, invariant checks, diffs).

## Targets

**Parsing**

- `parse_value` - throws arbitrary strings at the parser
- `parse_compare_toml` - parses with both toml-spanner and the `toml` crate, compares results
- `parse_recoverable` - tests recoverable/partial parsing
- `datetime` - roundtrips `DateTime` through parse/format/parse

**Emit (normalization and formatting)**

- `emit` - parse, emit, reparse, check semantic equality + idempotency
- `normalize` - generates random Item trees, emits them, reparses, checks idempotency

**Reprojection (format-preserving rewrite)**

- `reproject` - erases structural kinds, reprojects from source, checks output matches reference
- `emit_roundtrip` - generates TOML, erases kinds, reprojects, checks exact text preservation
- `emit_reproject_identity` - same text as source and dest, checks output matches input exactly
- `emit_reproject_edit` - generates two different TOML docs, reprojects dest through source formatting
- `emit_reproject_reorder` - like edit but also checks that source key ordering is preserved
- `emit_reproject_reorder_span_identity` - reprojects with span identity after mutating arrays
- `emit_reproject_exact` - tests single-entry edits/removes/inserts, checks surrounding text is preserved byte-for-byte

## Structure

- `fuzz_targets/` - libfuzzer entry points
- `src/lib.rs` - `Gen` (byte-driven PRNG for structured generation)
- `src/gen_toml.rs` - generates valid TOML text from random bytes
- `src/gen_tree.rs` - generates random Item trees directly, plus tree comparison utilities
- `src/exact.rs` - helpers for the exact preservation target (entry collection, surgical edits)
- `src/parse_compare.rs` - cross-crate parse comparison logic
- `src/recoverable.rs` - recoverable parse checking
- `corpus/` - saved inputs that exercise interesting paths
