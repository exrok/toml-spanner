#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};

// Fuzzes `emit_with_config` with reprojection, checking byte-exact
// preservation of unmodified regions.
//
// Generates a single TOML document via structured generation, applies ONE
// modification (scalar edit, entry removal, or entry insertion), reprojects
// the original formatting, and verifies that every byte outside the affected
// region is preserved identically.
//
// Invariants:
//   1. Output is valid UTF-8 and parses as valid TOML
//   2. Region preservation: bytes before/after the affected line are identical
//   3. For body-level scalar edits: the modified line matches prediction
//   4. For insertions: removing the inserted key and re-emitting recovers source
//   5. Semantic correctness: parsed output matches the modified tree
//   6. Idempotency: re-emit through the same pipeline produces identical bytes
fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 4 {
        return Corpus::Reject;
    }

    // Reserve last 2 bytes for mod selection.
    let gen_data = &data[..data.len() - 2];
    let mod_selector = data[data.len() - 2];
    let entry_selector = data[data.len() - 1];

    // Generate source TOML.
    let mut buffer = String::new();
    fuzz::gen_toml::random_roundtrip_toml(&mut buffer, gen_data);
    let source_text = &buffer;
    if source_text.is_empty() {
        return Corpus::Reject;
    }

    let arena = toml_spanner::Arena::new();

    // Parse source.
    let src_root = match toml_spanner::parse(source_text, &arena) {
        Ok(r) => r,
        Err(_) => return Corpus::Reject,
    };

    // Must be normalizable.
    if src_root.table().try_as_normalized().is_none() {
        return Corpus::Reject;
    }

    let source_bytes = source_text.as_bytes();

    // Collect body entries.
    let mut entries = Vec::new();
    fuzz::exact::collect_body_entries(
        src_root.table(),
        source_bytes,
        &mut Vec::new(),
        &mut entries,
        false,
        false,
    );

    if entries.is_empty() {
        return Corpus::Reject;
    }

    // Choose modification type.
    let mod_kind = match mod_selector % 3 {
        0 => fuzz::exact::ModKind::EditScalar,
        1 => fuzz::exact::ModKind::Remove,
        _ => fuzz::exact::ModKind::Insert,
    };

    match mod_kind {
        fuzz::exact::ModKind::EditScalar => {
            let editable = fuzz::exact::editable_entries(&entries);
            if editable.is_empty() {
                return Corpus::Reject;
            }
            let idx = editable[entry_selector as usize % editable.len()];
            let entry = &entries[idx];

            // Compute new value.
            let (new_item, new_value_bytes) = match &entry.kind {
                fuzz::exact::ScalarKind::Integer(v) => {
                    let new_v = v ^ 1;
                    (
                        toml_spanner::Item::from(new_v),
                        fuzz::exact::format_canonical_integer(new_v),
                    )
                }
                fuzz::exact::ScalarKind::Boolean(v) => {
                    let new_v = !v;
                    (
                        toml_spanner::Item::from(new_v),
                        fuzz::exact::format_canonical_bool(new_v),
                    )
                }
                _ => return Corpus::Reject,
            };

            // Clone tree and apply edit.
            let mut dest_table = src_root.table().clone_in(&arena);
            fuzz::exact::set_at_path(&mut dest_table, &entry.path, new_item);

            // Reproject and emit.
            let mut items = Vec::new();
            toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
            let norm = dest_table.normalize();
            let config = toml_spanner::EmitConfig {
                projected_source_text: source_text,
                projected_source_items: &items,
                reprojected_order: false,
        ..Default::default()
            };
            let mut buf = Vec::new();
            toml_spanner::emit_with_config(norm, &config, &mut buf);

            // Invariant 1: valid UTF-8 + valid TOML.
            let output = std::str::from_utf8(&buf).expect("must be valid UTF-8");
            let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
                panic!(
                    "output must be valid TOML!\nsrc:\n{source_text}\noutput:\n{output}\nerror: {e:?}"
                )
            });

            // Invariant 2+3: region preservation.
            if let Err(msg) =
                fuzz::exact::check_edit_preservation(source_bytes, &buf, entry, &new_value_bytes)
            {
                panic!(
                    "edit preservation failed: {msg}\npath: {:?}\nsrc:\n{source_text}\noutput:\n{output}",
                    entry.path
                );
            }

            // Invariant 5: semantic correctness.
            assert!(
                dest_table.as_item() == out_root.table().as_item(),
                "semantic mismatch after edit!\nsrc:\n{source_text}\noutput:\n{output}"
            );

            // Invariant 6: idempotency.
            check_idempotency(output, &buf, &arena);
        }

        fuzz::exact::ModKind::Remove => {
            let removable = fuzz::exact::removable_entries(&entries, source_bytes);
            if removable.is_empty() {
                return Corpus::Reject;
            }
            let idx = removable[entry_selector as usize % removable.len()];
            let entry = &entries[idx];

            // Clone tree and remove entry.
            let mut dest_table = src_root.table().clone_in(&arena);
            fuzz::exact::remove_at_path(&mut dest_table, &entry.path);

            // Reproject and emit.
            let mut items = Vec::new();
            toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
            let norm = dest_table.normalize();
            let config = toml_spanner::EmitConfig {
                projected_source_text: source_text,
                projected_source_items: &items,
                reprojected_order: false,
        ..Default::default()
            };
            let mut buf = Vec::new();
            toml_spanner::emit_with_config(norm, &config, &mut buf);

            // Invariant 1: valid UTF-8 + valid TOML.
            let output = std::str::from_utf8(&buf).expect("must be valid UTF-8");
            let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
                panic!(
                    "output must be valid TOML!\nsrc:\n{source_text}\noutput:\n{output}\nerror: {e:?}"
                )
            });

            // Invariant 2: region preservation.
            if let Err(msg) = fuzz::exact::check_remove_preservation(source_bytes, &buf, entry) {
                panic!(
                    "remove preservation failed: {msg}\npath: {:?}\nsrc:\n{source_text}\noutput:\n{output}",
                    entry.path
                );
            }

            // Invariant 5: semantic correctness.
            assert!(
                dest_table.as_item() == out_root.table().as_item(),
                "semantic mismatch after remove!\nsrc:\n{source_text}\noutput:\n{output}"
            );

            // Invariant 6: idempotency.
            check_idempotency(output, &buf, &arena);
        }

        fuzz::exact::ModKind::Insert => {
            // Find insertable tables.
            let mut targets = Vec::new();
            fuzz::exact::insertable_targets(src_root.table(), &mut Vec::new(), &mut targets);
            if targets.is_empty() {
                return Corpus::Reject;
            }
            let (table_path, fresh_key) = &targets[entry_selector as usize % targets.len()];

            // Clone tree and insert a new entry.
            let mut dest_table = src_root.table().clone_in(&arena);
            let target = fuzz::exact::table_at_path_mut(&mut dest_table, table_path);
            let new_item = toml_spanner::Item::from(42i64);
            target.insert(toml_spanner::Key::anon(fresh_key), new_item, &arena);

            // Reproject and emit.
            let mut items = Vec::new();
            toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
            let norm = dest_table.normalize();
            let config = toml_spanner::EmitConfig {
                projected_source_text: source_text,
                projected_source_items: &items,
                reprojected_order: false,
        ..Default::default()
            };
            let mut buf = Vec::new();
            toml_spanner::emit_with_config(norm, &config, &mut buf);

            // Invariant 1: valid UTF-8 + valid TOML.
            let output = std::str::from_utf8(&buf).expect("must be valid UTF-8");
            let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
                panic!(
                    "output must be valid TOML!\nsrc:\n{source_text}\noutput:\n{output}\nerror: {e:?}"
                )
            });

            // Invariant 4: insert round-trip.
            if let Err(msg) =
                fuzz::exact::check_insert_preservation(source_text, &buf, table_path, fresh_key)
            {
                panic!(
                    "insert preservation failed: {msg}\ntable_path: {table_path:?}\n\
                     key: {fresh_key}\nsrc:\n{source_text}\noutput:\n{output}"
                );
            }

            // Invariant 5: semantic correctness.
            assert!(
                dest_table.as_item() == out_root.table().as_item(),
                "semantic mismatch after insert!\nsrc:\n{source_text}\noutput:\n{output}"
            );

            // Invariant 6: idempotency.
            check_idempotency(output, &buf, &arena);
        }
    }

    Corpus::Keep
});

fn check_idempotency(output: &str, buf: &[u8], arena: &toml_spanner::Arena) {
    let src2 = toml_spanner::parse(output, arena).unwrap();
    let mut dest2 = src2.table().clone_in(arena);
    let mut items2 = Vec::new();
    toml_spanner::reproject(&src2, &mut dest2, &mut items2);
    let norm2 = dest2.normalize();
    let cfg2 = toml_spanner::EmitConfig {
        projected_source_text: output,
        projected_source_items: &items2,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf2 = Vec::with_capacity(buf.len());
    toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);
    assert!(
        buf == buf2.as_slice(),
        "not idempotent!\nfirst:\n{output}\nsecond:\n{}",
        String::from_utf8_lossy(&buf2),
    );
}
