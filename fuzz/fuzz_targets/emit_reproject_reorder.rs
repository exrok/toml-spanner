#![no_main]

use std::hash::{BuildHasher, Hasher};

use libfuzzer_sys::{Corpus, fuzz_target};

// Fuzzes `emit_with_config` with `reprojected_order: true` between TWO
// different documents.
//
// Generates two related TOML documents via structured generation. Reprojects
// the first document's formatting onto the second, normalizes, and emits
// with fragment-based reordering. This exercises the interleaving-preservation
// path for headers, dotted keys, and AOT entries.
//
// Invariants:
//   1. Output is valid UTF-8
//   2. Output parses as valid TOML
//   3. Output is semantically equal to dest (values match, ignoring flags)
//   4. Output is idempotent (re-emit through the same pipeline)
//   5. Projected entries preserve their source-relative ordering in the output
fuzz_target!(|data: &[u8]| -> Corpus {
    let mut buffer = String::new();
    let split = fuzz::gen_toml::random_double_toml(&mut buffer, data);
    let src_text = &buffer[..split];
    let dest_text = &buffer[split..];

    let arena = toml_spanner::Arena::new();
    let Ok(src_root) = toml_spanner::parse(src_text, &arena) else {
        return Corpus::Keep;
    };
    let Ok(dest_root) = toml_spanner::parse(dest_text, &arena) else {
        return Corpus::Keep;
    };
    let mut dest_table = dest_root.into_table();

    let mut items = Vec::new();
    toml_spanner::reproject(&src_root, &mut dest_table, &mut items);

    // Collect projected source key positions as (path_hash, key_span_start).
    let mut src_positions = Vec::new();
    collect_projected_positions(src_root.table(), 0, &mut src_positions);

    let norm = dest_table.normalize();
    let config = toml_spanner::EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::with_capacity(dest_text.len() + 64);
    toml_spanner::emit_with_config(norm, &config, &mut buf);

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

    // Invariant 3: semantically equal to dest (values, ignoring structural
    // flags which may differ due to reprojection from src).
    assert!(
        toml_spanner::items_equal(dest_table.as_item(), out_root.table().as_item()),
        "emit output differs semantically from dest!\n\
         src:\n{src_text}\n\
         dest:\n{dest_text}\n\
         output:\n{output}"
    );

    // Invariant 4: idempotent — re-emit the output with self-reprojection.
    {
        let src2 = toml_spanner::parse(output, &arena).unwrap();
        let mut dest2 = src2.table().clone_in(&arena);
        items.clear();
        toml_spanner::reproject(&src2, &mut dest2, &mut items);
        let norm2 = dest2.normalize();
        let cfg2 = toml_spanner::EmitConfig {
            projected_source_text: output,
            projected_source_items: &items,
            reprojected_order: true,
        };
        let mut buf2 = Vec::with_capacity(buf.len());
        toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);
        assert!(
            buf == buf2,
            "not idempotent!\n\
             src:\n{src_text}\n\
             dest:\n{dest_text}\n\
             first:\n{output}\n\
             second:\n{}",
            String::from_utf8_lossy(&buf2),
        );
    }

    // Invariant 5: projected entries preserve their source-relative ordering.
    let mut out_positions = Vec::new();
    collect_projected_positions(out_root.table(), 0, &mut out_positions);
    assert_order_preserved(&src_positions, &out_positions, src_text, dest_text, output);

    Corpus::Keep
});

static HASH_STATE: foldhash::fast::FixedState = foldhash::fast::FixedState::with_seed(0);

/// Hash a path segment into the running path hash using foldhash.
#[inline]
fn hash_segment(parent: u64, bytes: &[u8]) -> u64 {
    let mut h = HASH_STATE.build_hasher();
    h.write_u64(parent);
    h.write_u32(bytes.len() as u32);
    h.write(bytes);
    h.finish()
}

/// Collects (path_hash, key_span_start) for projected entries.
/// Uses incremental foldhash instead of `Vec<String>` paths,
/// eliminating all per-entry String allocations.
fn collect_projected_positions(
    table: &toml_spanner::Table<'_>,
    parent_hash: u64,
    out: &mut Vec<(u64, u32)>,
) {
    for (key, item) in table {
        if key.span.is_empty() {
            continue;
        }
        let h = hash_segment(parent_hash, key.name.as_bytes());
        out.push((h, key.span.start));
        match item.value() {
            toml_spanner::Value::Table(sub) => {
                collect_projected_positions(sub, h, out);
            }
            toml_spanner::Value::Array(arr) => {
                for (i, elem) in arr.iter().enumerate() {
                    if let Some(sub) = elem.as_table() {
                        let ah = hash_segment(h, &(i as u32).to_le_bytes());
                        collect_projected_positions(sub, ah, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Verifies source-relative ordering is preserved in output.
/// Linear scan avoids HashMap allocation for typical small entry counts.
fn assert_order_preserved(
    src_positions: &[(u64, u32)],
    out_positions: &[(u64, u32)],
    src_text: &str,
    dest_text: &str,
    output: &str,
) {
    let mut last_out_pos = 0u32;
    let mut found_any = false;
    for &(src_hash, src_pos) in src_positions {
        for &(out_hash, out_pos) in out_positions {
            if out_hash == src_hash {
                if found_any {
                    assert!(
                        last_out_pos <= out_pos,
                        "order violation at src_pos={src_pos} out_pos={out_pos} \
                         prev_out={last_out_pos}\n\
                         src:\n{src_text}\n\
                         dest:\n{dest_text}\n\
                         output:\n{output}",
                    );
                }
                last_out_pos = out_pos;
                found_any = true;
                break;
            }
        }
    }
}
