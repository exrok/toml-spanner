#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};

// Fuzzes `Formatting::preserved_from` with reprojection between TWO different documents.
//
// Generates two related TOML documents (shared key space, different values
// and structure) via structured generation. Reprojects the first document's
// formatting onto the second, normalizes, and emits. This exercises the
// partial-modification paths: `try_emit_array_partial`,
// `try_emit_inline_table_partial`, body gap preservation, and
// `try_emit_entry_from_source` with mixed projected/unmatched entries.
//
// Invariants:
//   1. Output is valid UTF-8
//   2. Output parses as valid TOML
//   3. Output is semantically equal to dest (values match, ignoring flags)
//   4. Output is idempotent (re-emit through the same pipeline)
fuzz_target!(|data: &[u8]| -> Corpus {
    // Generate two related TOML documents from random bytes.
    let mut buffer = String::new();
    let split = fuzz::gen_toml::random_double_toml(&mut buffer, data);
    let src_text = &buffer[..split];
    let dest_text = &buffer[split..];

    // Parse source.
    let arena = toml_spanner::Arena::new();
    let Ok(src_root) = toml_spanner::parse(src_text, &arena) else {
        return Corpus::Keep;
    };

    // Parse dest.
    let Ok(dest_root) = toml_spanner::parse(dest_text, &arena) else {
        return Corpus::Keep;
    };
    let dest_table = dest_root.into_table();

    // Reproject, normalize, and emit via Formatting API.
    let buf = toml_spanner::Formatting::preserved_from(&src_root)
        .format_table_to_bytes(dest_table, &arena);

    // Invariant 1: valid UTF-8.
    let output = std::str::from_utf8(&buf).expect("emit must produce valid UTF-8");

    // Invariant 2: parses as valid TOML.
    let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
        panic!(
            "emit output must be valid TOML!\n\
             src:\n{src_text}\n\
             dest:\n{dest_text}\n\
             output:\n{output}\n\
             error: {e:?}"
        )
    });

    // Invariant 3: semantically equal to dest (values, ignoring structural flags
    // which may differ due to reprojection from src).
    let dest_ref = toml_spanner::parse(dest_text, &arena).unwrap();
    assert!(
        dest_ref.table().as_item() == out_root.table().as_item(),
        "emit output differs semantically from dest!\n\
         src:\n{src_text}\n\
         dest:\n{dest_text}\n\
         output:\n{output}"
    );

    // Invariant 4: idempotent — re-emit the output with self-reprojection.
    {
        let src2 = toml_spanner::parse(output, &arena).unwrap();
        let dest2 = src2.table().clone_in(&arena);
        let buf2 = toml_spanner::Formatting::preserved_from(&src2)
            .format_table_to_bytes(dest2, &arena);
        assert!(
            buf == buf2,
            "emit is not idempotent!\n\
             src:\n{src_text}\n\
             dest:\n{dest_text}\n\
             first:\n{output}\n\
             second:\n{}",
            String::from_utf8_lossy(&buf2),
        );
    }

    Corpus::Keep
});
