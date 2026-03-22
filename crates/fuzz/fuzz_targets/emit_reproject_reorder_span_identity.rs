#![no_main]

use std::hash::{BuildHasher, Hasher};
use libfuzzer_sys::{Corpus, fuzz_target};

// Fuzzes `Formatting::preserved_from(...).with_span_projection_identity()` with
// direct table mutations on a parsed document.
//
// Unlike emit_reproject_reorder which generates two separate TOML documents,
// this target parses one document, clones the table, applies radical
// in-place edits (sort, reverse, swap, remove, and deep scalar mutation
// on arrays), then reprojects with span identity enabled.
//
// Invariants:
//   1. Output is valid UTF-8
//   2. Output parses as valid TOML
//   3. Output is semantically equal to the mutated table
//   4. Output is idempotent (re-emit through the same pipeline)
//   5. Projected entries preserve their source-relative ordering in the output
fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 4 {
        return Corpus::Reject;
    }

    // Split: first bytes for TOML generation, last bytes for mutation control.
    let split = data.len() * 3 / 4;
    let gen_data = &data[..split];
    let mut_data = &data[split..];

    let mut buffer = String::new();
    fuzz::gen_toml::random_roundtrip_toml(&mut buffer, gen_data);
    if buffer.is_empty() {
        return Corpus::Reject;
    }
    let text = &buffer;

    let arena = toml_spanner::Arena::new();
    let Ok(doc) = toml_spanner::parse(text, &arena) else {
        return Corpus::Reject;
    };

    let mut table = doc.table().clone_in(&arena);

    // Apply mutations guided by mut_data.
    let mut g = fuzz::Gen::new(mut_data);
    let mutated = fuzz::gen_toml::mutate_arrays(&mut table, &mut g);
    if !mutated {
        return Corpus::Reject;
    }

    // Snapshot the mutated table for semantic comparison.
    let ref_table = table.clone_in(&arena);

    // Collect projected source key positions.
    let mut src_positions = Vec::new();
    collect_projected_positions(doc.table(), 0, &mut src_positions);

    // Reproject with span identity, normalize, and emit.
    let buf = toml_spanner::Formatting::preserved_from(&doc)
        .with_span_projection_identity()
        .format_table_to_bytes(table, &arena);

    // Invariant 1: valid UTF-8.
    let output = std::str::from_utf8(&buf).expect("emit must produce valid UTF-8");

    // Invariant 2: parses as valid TOML.
    let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
        panic!(
            "emit output must be valid TOML!\n\
             source:\n{text}\n\
             output:\n{output}\n\
             error: {e:?}"
        )
    });

    // Invariant 3: semantically equal to mutated table.
    assert!(
        ref_table.as_item() == out_root.table().as_item(),
        "emit output differs semantically from mutated table!\n\
         source:\n{text}\n\
         output:\n{output}"
    );

    // Invariant 4: idempotent.
    {
        let src2 = toml_spanner::parse(output, &arena).unwrap();
        let dest2 = src2.table().clone_in(&arena);
        let buf2 = toml_spanner::Formatting::preserved_from(&src2)
            .with_span_projection_identity()
            .format_table_to_bytes(dest2, &arena);
        assert!(
            buf == buf2,
            "not idempotent!\n\
             source:\n{text}\n\
             first:\n{output}\n\
             second:\n{}",
            String::from_utf8_lossy(&buf2),
        );
    }

    // Invariant 5: projected entries preserve their source-relative ordering.
    let mut out_positions = Vec::new();
    collect_projected_positions(out_root.table(), 0, &mut out_positions);
    assert_order_preserved(&src_positions, &out_positions, text, output);

    Corpus::Keep
});

static HASH_STATE: foldhash::fast::FixedState = foldhash::fast::FixedState::with_seed(0);

#[inline]
fn hash_segment(parent: u64, bytes: &[u8]) -> u64 {
    let mut h = HASH_STATE.build_hasher();
    h.write_u64(parent);
    h.write_u32(bytes.len() as u32);
    h.write(bytes);
    h.finish()
}

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
            toml_spanner::Value::Array(arr) if arr.len() == 1 => {
                if let Some(sub) = arr.iter().next().unwrap().as_table() {
                    let ah = hash_segment(h, &0u32.to_le_bytes());
                    collect_projected_positions(sub, ah, out);
                }
            }
            _ => {}
        }
    }
}

fn assert_order_preserved(
    src_positions: &[(u64, u32)],
    out_positions: &[(u64, u32)],
    source: &str,
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
                         source:\n{source}\n\
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
