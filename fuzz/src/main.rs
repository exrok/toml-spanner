fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: fuzz <target> <artifact_path>");
        eprintln!(
            "targets: normalize, emit_roundtrip, emit_reproject_identity, emit_reproject_edit, emit_reproject_reorder, emit_reproject_exact, parse_recoverable"
        );
        std::process::exit(1);
    }
    let target = &args[1];
    let path = &args[2];

    match target.as_str() {
        "normalize" => run_normalize(path),
        "emit_roundtrip" => run_emit_roundtrip(path),
        "emit_reproject_identity" => run_reproject_identity(path),
        "emit_reproject_edit" => run_reproject_edit(path),
        "emit_reproject_reorder" => run_reproject_reorder(path),
        "emit_reproject_exact" => run_reproject_exact(path),
        "parse_recoverable" => fuzz::recoverable::run_cli(path),
        _ => {
            eprintln!("unknown target: {target}");
            eprintln!(
                "targets: normalize, emit_roundtrip, emit_reproject_identity, emit_reproject_edit, emit_reproject_reorder, emit_reproject_exact, parse_recoverable"
            );
            std::process::exit(1);
        }
    }
}

// ── normalize ──

fn run_normalize(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}", len = data.len());
    println!();

    if data.len() < 4 {
        println!("artifact too short (< 4 bytes), fuzzer would reject");
        return;
    }

    let mut g = fuzz::Gen::new(&data);
    let arena = toml_spanner::Arena::new();
    let mut root = fuzz::gen_tree::gen_root_table(&mut g, &arena);

    fuzz::gen_tree::print_table(&root, "constructed tree (before normalize)");
    println!();

    let normalized = root.normalize();

    fuzz::gen_tree::print_table(normalized.table(), "normalized tree");
    println!();

    // Emit.
    let mut buf1 = Vec::new();
    toml_spanner::emit(normalized, &mut buf1);
    let emitted = String::from_utf8(buf1.clone()).expect("emit must produce valid UTF-8");
    println!("── emitted ({} bytes) ──\n{emitted:?}\n", emitted.len());

    // Parse the emitted output.
    let arena2 = toml_spanner::Arena::new();
    let root2 = match toml_spanner::parse(&emitted, &arena2) {
        Ok(r) => r,
        Err(e) => {
            println!("FAILURE: emitted output failed to parse: {e:?}");
            std::process::exit(1);
        }
    };

    fuzz::gen_tree::print_table(root2.table(), "re-parsed tree");
    println!();

    // Compare normalized vs parsed.
    match fuzz::gen_tree::items_eq(
        normalized.table().as_item(),
        root2.table().as_item(),
        &mut Vec::new(),
    ) {
        Ok(()) => println!("── items_eq: OK ──"),
        Err(msg) => {
            println!("FAILURE: {msg}");
            std::process::exit(1);
        }
    }

    // Idempotency.
    let normalized2 = root2
        .table()
        .try_as_normalized()
        .expect("round-tripped table should be valid");
    let mut buf2 = Vec::new();
    toml_spanner::emit(normalized2, &mut buf2);
    if buf1 == buf2 {
        println!("── idempotency: OK ──");
    } else {
        let emitted2 = String::from_utf8_lossy(&buf2);
        println!("FAILURE: emit is not idempotent!");
        println!("  first:  {emitted:?}");
        println!("  second: {emitted2:?}");
        std::process::exit(1);
    }
}

// ── emit_roundtrip ──

fn run_emit_roundtrip(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}\n", len = data.len());

    // Generate TOML from random bytes (same generator as the fuzz target).
    let mut buffer = String::new();
    fuzz::gen_toml::random_roundtrip_toml(&mut buffer, &data);
    let text = &buffer;

    println!("── generated text ({} bytes) ──\n{text:?}\n", text.len());

    let arena = toml_spanner::Arena::new();
    let doc = match toml_spanner::parse(text, &arena) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("generated text does not parse: {e:?}");
            std::process::exit(1);
        }
    };

    fuzz::gen_tree::print_table(doc.table(), "parsed input");
    println!();

    if doc.table().try_as_normalized().is_none() {
        println!("table is not normalizable, skipping");
        return;
    }

    // Clone and erase all structural kinds.
    let mut dest = doc.table().clone_in(&arena);
    fuzz::gen_tree::erase_kinds_table(&mut dest);

    fuzz::gen_tree::print_table(&dest, "erased kinds");
    println!();

    // Reproject original structure onto erased dest.
    let mut items = Vec::new();
    toml_spanner::reproject(&doc, &mut dest, &mut items);
    println!("── reprojected ({} items) ──", items.len());

    // Normalize and emit with reprojection config.
    let norm = dest.normalize();
    fuzz::gen_tree::print_table(norm.table(), "normalized tree");
    println!();

    let config = toml_spanner::EmitConfig {
        projected_source_text: text,
        projected_source_items: &items,
        reprojected_order: false,
        ..Default::default()
    };
    let mut out_buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut out_buf);

    let output = String::from_utf8_lossy(&out_buf);
    println!(
        "── roundtrip output ({} bytes) ──\n{output:?}\n",
        out_buf.len()
    );

    let input = text.as_bytes().trim_ascii();
    // Exact text match.
    if input == out_buf.trim_ascii() {
        println!("── exact text match: OK ──");
    } else {
        eprintln!(
            "FAILURE: roundtrip did not preserve input text!\n\
             input:\n{text:?}\n\
             output:\n{output:?}"
        );
        std::process::exit(1);
    }
}

// ── reproject_identity ──

fn run_reproject_identity(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    let text = match std::str::from_utf8(&data) {
        Ok(s) => s.to_owned(),
        Err(e) => {
            eprintln!("artifact is not valid UTF-8: {e}");
            std::process::exit(1);
        }
    };

    println!("artifact: {path}");
    println!("input ({} bytes): {text:?}\n", text.len());

    // Parse as source.
    let arena_src = toml_spanner::Arena::new();
    let src_root = match toml_spanner::parse(&text, &arena_src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("input does not parse: {e:?}");
            std::process::exit(1);
        }
    };

    println!("── parsed input ──");
    fuzz::gen_tree::print_table(src_root.table(), "source tree");
    println!();

    // Parse as dest (identity — same text).
    let arena_dest = toml_spanner::Arena::new();
    let mut dest_table = match toml_spanner::parse(&text, &arena_dest) {
        Ok(r) => r.into_table(),
        Err(e) => {
            eprintln!("input does not parse as dest: {e:?}");
            std::process::exit(1);
        }
    };

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
    println!("── reprojected ({} items) ──", items.len());

    // Normalize and emit with reprojection config.
    let norm = dest_table.normalize();
    fuzz::gen_tree::print_table(norm.table(), "normalized tree");
    println!();

    let config = toml_spanner::EmitConfig {
        projected_source_text: &text,
        projected_source_items: &items,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut buf);

    // Invariant 1: valid UTF-8.
    let output = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_owned(),
        Err(e) => {
            eprintln!(
                "FAILURE: emit_with_config produced invalid UTF-8: {e}\n\
                 raw bytes: {buf:?}"
            );
            std::process::exit(1);
        }
    };

    println!("── emit output ({} bytes) ──\n{output:?}\n", output.len());

    // Invariant 2: parses as valid TOML.
    let arena_out = toml_spanner::Arena::new();
    let out_root = match toml_spanner::parse(&output, &arena_out) {
        Ok(r) => {
            fuzz::gen_tree::print_table(r.table(), "re-parsed output");
            println!();
            r
        }
        Err(e) => {
            eprintln!(
                "FAILURE: emit output does not parse: {e:?}\n\
                 input:\n{text:?}\n\
                 output:\n{output:?}"
            );
            std::process::exit(1);
        }
    };

    // Invariant 3: semantically equal with matching flags.
    match fuzz::gen_tree::items_eq(
        src_root.table().as_item(),
        out_root.table().as_item(),
        &mut Vec::new(),
    ) {
        Ok(()) => println!("── items_eq: OK ──"),
        Err(msg) => {
            eprintln!(
                "FAILURE: {msg}\n\
                 input:\n{text:?}\n\
                 output:\n{output:?}"
            );
            std::process::exit(1);
        }
    }

    // Also check flags match (gen_tree::items_eq checks values but not flags).
    check_flags_match(
        src_root.table().as_item(),
        out_root.table().as_item(),
        &mut Vec::new(),
        &text,
        &output,
    );
    println!("── flags_match: OK ──");

    // Invariant 4: idempotent — re-emit through the same pipeline.
    let arena_s2 = toml_spanner::Arena::new();
    let src2 = toml_spanner::parse(&output, &arena_s2).unwrap();
    let arena_d2 = toml_spanner::Arena::new();
    let mut dest2 = toml_spanner::parse(&output, &arena_d2)
        .unwrap()
        .into_table();
    let mut items2 = Vec::new();
    toml_spanner::reproject(&src2, &mut dest2, &mut items2);
    let norm2 = dest2.normalize();
    let cfg2 = toml_spanner::EmitConfig {
        projected_source_text: &output,
        projected_source_items: &items2,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf2 = Vec::new();
    toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);

    if buf == buf2 {
        println!("── idempotency: OK ──");
    } else {
        let output2 = String::from_utf8_lossy(&buf2);
        eprintln!(
            "FAILURE: emit_with_config is not idempotent!\n\
             input:\n{text:?}\n\
             first:\n{output:?}\n\
             second:\n{output2:?}"
        );
        std::process::exit(1);
    }
}

fn check_flags_match(
    a: &toml_spanner::Item<'_>,
    b: &toml_spanner::Item<'_>,
    path: &mut Vec<String>,
    input: &str,
    emitted: &str,
) {
    let p = || {
        if path.is_empty() {
            "<root>".to_string()
        } else {
            path.join(".")
        }
    };

    if a.kind() as u8 != b.kind() as u8 {
        eprintln!(
            "FAILURE: kind mismatch at {}\n\
             input:\n{input:?}\nemitted:\n{emitted:?}",
            p(),
        );
        std::process::exit(1);
    }

    if a.flag() != b.flag() {
        eprintln!(
            "FAILURE: flag mismatch at {}: {} vs {}\n\
             input:\n{input:?}\nemitted:\n{emitted:?}",
            p(),
            a.flag(),
            b.flag(),
        );
        std::process::exit(1);
    }

    match a.value() {
        toml_spanner::Value::Table(tab_a) => {
            let tab_b = b.as_table().unwrap();
            for (key, val_a) in tab_a {
                path.push(key.name.to_string());
                let val_b = tab_b.get(key.name).unwrap();
                check_flags_match(val_a, val_b, path, input, emitted);
                path.pop();
            }
        }
        toml_spanner::Value::Array(arr_a) => {
            let arr_b = b.as_array().unwrap();
            for i in 0..arr_a.len() {
                path.push(format!("[{i}]"));
                check_flags_match(
                    &arr_a.as_slice()[i],
                    &arr_b.as_slice()[i],
                    path,
                    input,
                    emitted,
                );
                path.pop();
            }
        }
        _ => {}
    }
}

// ── reproject_edit ──

fn run_reproject_edit(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}\n", len = data.len());

    let mut buffer = String::new();
    let split = fuzz::gen_toml::random_double_toml(&mut buffer, &data);
    let src_text = buffer[..split].to_owned();
    let dest_text = buffer[split..].to_owned();

    println!(
        "── source text ({} bytes) ──\n{src_text:?}\n",
        src_text.len()
    );
    println!(
        "── dest text ({} bytes) ──\n{dest_text:?}\n",
        dest_text.len()
    );

    // Parse source.
    let arena_src = toml_spanner::Arena::new();
    let src_root = match toml_spanner::parse(&src_text, &arena_src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("source does not parse: {e:?}");
            std::process::exit(1);
        }
    };
    fuzz::gen_tree::print_table(src_root.table(), "parsed source");
    println!();

    // Parse dest (reference copy — not modified).
    let arena_ref = toml_spanner::Arena::new();
    let ref_root = match toml_spanner::parse(&dest_text, &arena_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dest does not parse: {e:?}");
            std::process::exit(1);
        }
    };
    fuzz::gen_tree::print_table(ref_root.table(), "parsed dest (reference)");
    println!();

    // Parse dest (working copy for reproject + normalize).
    let arena_dest = toml_spanner::Arena::new();
    let mut dest_table = match toml_spanner::parse(&dest_text, &arena_dest) {
        Ok(r) => r.into_table(),
        Err(e) => {
            eprintln!("dest does not parse (working copy): {e:?}");
            std::process::exit(1);
        }
    };

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
    println!("── reprojected ({} items) ──", items.len());

    // Normalize and emit with reprojection config.
    let norm = dest_table.normalize();
    fuzz::gen_tree::print_table(norm.table(), "normalized dest tree");
    println!();

    let config = toml_spanner::EmitConfig {
        projected_source_text: &src_text,
        projected_source_items: &items,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut buf);

    // Invariant 1: valid UTF-8.
    let output = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_owned(),
        Err(e) => {
            eprintln!(
                "FAILURE: emit_with_config produced invalid UTF-8: {e}\n\
                 raw bytes: {buf:?}"
            );
            std::process::exit(1);
        }
    };

    println!("── emit output ({} bytes) ──\n{output:?}\n", output.len());

    // Invariant 2: parses as valid TOML.
    let arena_out = toml_spanner::Arena::new();
    let out_root = match toml_spanner::parse(&output, &arena_out) {
        Ok(r) => {
            fuzz::gen_tree::print_table(r.table(), "re-parsed output");
            println!();
            r
        }
        Err(e) => {
            eprintln!(
                "FAILURE: emit output does not parse: {e:?}\n\
                 src:\n{src_text:?}\n\
                 dest:\n{dest_text:?}\n\
                 output:\n{output:?}"
            );
            std::process::exit(1);
        }
    };

    // Invariant 3: semantically equal to dest (values, ignoring flags).
    if ref_root.table().as_item() != out_root.table().as_item() {
        eprintln!(
            "FAILURE: emit output differs semantically from dest!\n\
             src:\n{src_text:?}\n\
             dest:\n{dest_text:?}\n\
             output:\n{output:?}"
        );
        std::process::exit(1);
    }
    println!("── items equal: OK ──");

    // Invariant 4: idempotent — re-emit the output with self-reprojection.
    let arena_s2 = toml_spanner::Arena::new();
    let src2 = toml_spanner::parse(&output, &arena_s2).unwrap();
    let arena_d2 = toml_spanner::Arena::new();
    let mut dest2 = toml_spanner::parse(&output, &arena_d2)
        .unwrap()
        .into_table();
    let mut items2 = Vec::new();
    toml_spanner::reproject(&src2, &mut dest2, &mut items2);
    let norm2 = dest2.normalize();
    let cfg2 = toml_spanner::EmitConfig {
        projected_source_text: &output,
        projected_source_items: &items2,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf2 = Vec::new();
    toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);

    if buf == buf2 {
        println!("── idempotency: OK ──");
    } else {
        let output2 = String::from_utf8_lossy(&buf2);
        eprintln!(
            "FAILURE: emit_with_config is not idempotent!\n\
             src:\n{src_text:?}\n\
             dest:\n{dest_text:?}\n\
             first:\n{output:?}\n\
             second:\n{output2:?}"
        );
        std::process::exit(1);
    }
}

// ── reproject_reorder ──

fn run_reproject_reorder(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}\n", len = data.len());

    let mut buffer = String::new();
    let split = fuzz::gen_toml::random_reorder_pair(&mut buffer, &data);
    let src_text = buffer[..split].to_owned();
    let dest_text = buffer[split..].to_owned();

    println!(
        "── source text ({} bytes) ──\n{src_text:?}\n",
        src_text.len()
    );
    println!(
        "── dest text ({} bytes) ──\n{dest_text:?}\n",
        dest_text.len()
    );

    // Parse source.
    let arena_src = toml_spanner::Arena::new();
    let src_root = match toml_spanner::parse(&src_text, &arena_src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("source does not parse: {e:?}");
            std::process::exit(1);
        }
    };
    fuzz::gen_tree::print_table(src_root.table(), "parsed source");
    println!();

    // Parse dest (reference copy — not modified).
    let arena_ref = toml_spanner::Arena::new();
    let ref_root = match toml_spanner::parse(&dest_text, &arena_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dest does not parse: {e:?}");
            std::process::exit(1);
        }
    };
    fuzz::gen_tree::print_table(ref_root.table(), "parsed dest (reference)");
    println!();

    // Parse dest (working copy for reproject + normalize).
    let arena_dest = toml_spanner::Arena::new();
    let mut dest_table = match toml_spanner::parse(&dest_text, &arena_dest) {
        Ok(r) => r.into_table(),
        Err(e) => {
            eprintln!("dest does not parse (working copy): {e:?}");
            std::process::exit(1);
        }
    };

    // Collect projected source key positions before reproject mutates things.
    let mut src_positions: Vec<(Vec<String>, u32)> = Vec::new();
    collect_table_key_positions(src_root.table(), &mut Vec::new(), &mut src_positions);
    println!("── source key positions ({}) ──", src_positions.len());
    for (path, pos) in &src_positions {
        println!("  {} @ {pos}", path.join("."));
    }
    println!();

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    toml_spanner::reproject(&src_root, &mut dest_table, &mut items);
    println!("── reprojected ({} items) ──", items.len());

    // Normalize and emit with reprojection config + reordering.
    let norm = dest_table.normalize();
    fuzz::gen_tree::print_table(norm.table(), "normalized dest tree");
    println!();

    let config = toml_spanner::EmitConfig {
        projected_source_text: &src_text,
        projected_source_items: &items,
        reprojected_order: true,
        ..Default::default()
    };
    let mut buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut buf);

    // Invariant 1: valid UTF-8.
    let output = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_owned(),
        Err(e) => {
            eprintln!(
                "FAILURE: emit_with_config produced invalid UTF-8: {e}\n\
                 raw bytes: {buf:?}"
            );
            std::process::exit(1);
        }
    };

    println!("── emit output ({} bytes) ──\n{output:?}\n", output.len());

    // Invariant 2: parses as valid TOML.
    let arena_out = toml_spanner::Arena::new();
    let out_root = match toml_spanner::parse(&output, &arena_out) {
        Ok(r) => {
            fuzz::gen_tree::print_table(r.table(), "re-parsed output");
            println!();
            r
        }
        Err(e) => {
            eprintln!(
                "FAILURE: emit output does not parse: {e:?}\n\
                 src:\n{src_text:?}\n\
                 dest:\n{dest_text:?}\n\
                 output:\n{output:?}"
            );
            std::process::exit(1);
        }
    };

    // Invariant 3: semantically equal to dest (values, ignoring flags).
    if ref_root.table().as_item() != out_root.table().as_item() {
        eprintln!(
            "FAILURE: emit output differs semantically from dest!\n\
             src:\n{src_text:?}\n\
             dest:\n{dest_text:?}\n\
             output:\n{output:?}"
        );
        std::process::exit(1);
    }
    println!("── items equal: OK ──");

    // Invariant 4: idempotent — re-emit the output with self-reprojection.
    let arena_s2 = toml_spanner::Arena::new();
    let src2 = toml_spanner::parse(&output, &arena_s2).unwrap();
    let arena_d2 = toml_spanner::Arena::new();
    let mut dest2 = toml_spanner::parse(&output, &arena_d2)
        .unwrap()
        .into_table();
    let mut items2 = Vec::new();
    toml_spanner::reproject(&src2, &mut dest2, &mut items2);
    let norm2 = dest2.normalize();
    let cfg2 = toml_spanner::EmitConfig {
        projected_source_text: &output,
        projected_source_items: &items2,
        reprojected_order: true,
        ..Default::default()
    };
    let mut buf2 = Vec::new();
    toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);

    if buf == buf2 {
        println!("── idempotency: OK ──");
    } else {
        let output2 = String::from_utf8_lossy(&buf2);
        eprintln!(
            "FAILURE: emit_with_config is not idempotent!\n\
             src:\n{src_text:?}\n\
             dest:\n{dest_text:?}\n\
             first:\n{output:?}\n\
             second:\n{output2:?}"
        );
        std::process::exit(1);
    }

    // Invariant 5: projected entries preserve their source-relative ordering.
    let mut out_positions: Vec<(Vec<String>, u32)> = Vec::new();
    collect_table_key_positions(out_root.table(), &mut Vec::new(), &mut out_positions);
    println!("── output key positions ({}) ──", out_positions.len());
    for (path, pos) in &out_positions {
        println!("  {} @ {pos}", path.join("."));
    }
    println!();

    check_order_preserved(
        &src_positions,
        &out_positions,
        &src_text,
        &dest_text,
        &output,
    );
    println!("── order_preserved: OK ──");
}

/// Collects (key_path, key_span_start) for table entries with non-empty key spans.
/// Recurses into nested tables and single-element arrays (where the
/// src→dest element mapping is unambiguous). Multi-element arrays are
/// skipped — positional fallback makes cross-document identity arbitrary.
fn collect_table_key_positions(
    table: &toml_spanner::Table<'_>,
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, u32)>,
) {
    for (key, item) in table {
        path.push(key.name.to_string());

        if !key.span.is_empty() {
            out.push((path.clone(), key.span.start));
        }

        match item.value() {
            toml_spanner::Value::Table(sub) => {
                collect_table_key_positions(sub, path, out);
            }
            toml_spanner::Value::Array(arr) if arr.len() == 1 => {
                if let Some(sub) = arr.iter().next().unwrap().as_table() {
                    path.push("[0]".to_string());
                    collect_table_key_positions(sub, path, out);
                    path.pop();
                }
            }
            _ => {}
        }

        path.pop();
    }
}

/// Verifies that for every pair of entries (A, B) present in both source and
/// output, if src_pos(A) < src_pos(B) then out_pos(A) < out_pos(B).
fn check_order_preserved(
    src_positions: &[(Vec<String>, u32)],
    out_positions: &[(Vec<String>, u32)],
    src_text: &str,
    dest_text: &str,
    output: &str,
) {
    use std::collections::HashMap;
    let out_map: HashMap<&[String], u32> = out_positions
        .iter()
        .map(|(path, pos)| (path.as_slice(), *pos))
        .collect();

    // Collect entries that appear in both source and output, in source order.
    let mut matched: Vec<(&[String], u32, u32)> = Vec::new();
    for (path, src_pos) in src_positions {
        if let Some(&out_pos) = out_map.get(path.as_slice()) {
            matched.push((path.as_slice(), *src_pos, out_pos));
        }
    }

    // Verify output positions are monotonically ordered (matching source order).
    for i in 1..matched.len() {
        let (path_a, src_a, out_a) = &matched[i - 1];
        let (path_b, src_b, out_b) = &matched[i];
        if src_a < src_b && out_a >= out_b {
            eprintln!(
                "FAILURE: order violation!\n\
                 {:?} (src={src_a}, out={out_a}) should appear before {:?} (src={src_b}, out={out_b})\n\
                 src:\n{src_text:?}\n\
                 dest:\n{dest_text:?}\n\
                 output:\n{output:?}",
                path_a, path_b,
            );
            std::process::exit(1);
        }
    }
}

// ── reproject_exact ──

fn run_reproject_exact(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}\n", len = data.len());

    if data.len() < 4 {
        println!("artifact too short (< 4 bytes), fuzzer would reject");
        return;
    }

    let gen_data = &data[..data.len() - 2];
    let mod_selector = data[data.len() - 2];
    let entry_selector = data[data.len() - 1];

    // Generate source TOML.
    let mut buffer = String::new();
    fuzz::gen_toml::random_roundtrip_toml(&mut buffer, gen_data);
    let source_text = &buffer;

    println!(
        "── source text ({} bytes) ──\n{source_text:?}\n",
        source_text.len()
    );

    if source_text.is_empty() {
        println!("empty source, fuzzer would reject");
        return;
    }

    let arena = toml_spanner::Arena::new();
    let src_root = match toml_spanner::parse(source_text, &arena) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("source does not parse: {e:?}");
            std::process::exit(1);
        }
    };

    fuzz::gen_tree::print_table(src_root.table(), "parsed source");
    println!();

    if src_root.table().try_as_normalized().is_none() {
        println!("table is not normalizable, skipping");
        return;
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

    println!("── body entries ({}) ──", entries.len());
    for (i, e) in entries.iter().enumerate() {
        println!(
            "  [{i}] path={:?} key={}..{} line={}..{} in_inline={} in_dotted={} kind={:?}",
            e.path,
            e.key_start,
            e.key_end,
            e.line_start,
            e.line_end,
            e.in_inline,
            e.in_dotted,
            e.kind
        );
    }
    println!();

    if entries.is_empty() {
        println!("no body entries, fuzzer would reject");
        return;
    }

    let mod_kind = match mod_selector % 3 {
        0 => fuzz::exact::ModKind::EditScalar,
        1 => fuzz::exact::ModKind::Remove,
        _ => fuzz::exact::ModKind::Insert,
    };
    println!("── modification: {mod_kind:?} ──\n");

    match mod_kind {
        fuzz::exact::ModKind::EditScalar => {
            let editable = fuzz::exact::editable_entries(&entries);
            if editable.is_empty() {
                println!("no editable entries, fuzzer would reject");
                return;
            }
            let idx = editable[entry_selector as usize % editable.len()];
            let entry = &entries[idx];
            println!("editing entry [{idx}]: path={:?}", entry.path);

            let (new_item, new_value_bytes) = match &entry.kind {
                fuzz::exact::ScalarKind::Integer(v) => {
                    let new_v = v ^ 1;
                    println!("  integer {v} -> {new_v}");
                    (
                        toml_spanner::Item::from(new_v),
                        fuzz::exact::format_canonical_integer(new_v),
                    )
                }
                fuzz::exact::ScalarKind::Boolean(v) => {
                    let new_v = !v;
                    println!("  boolean {v} -> {new_v}");
                    (
                        toml_spanner::Item::from(new_v),
                        fuzz::exact::format_canonical_bool(new_v),
                    )
                }
                _ => {
                    println!("  non-editable kind, fuzzer would reject");
                    return;
                }
            };

            let mut dest_table = src_root.table().clone_in(&arena);
            fuzz::exact::set_at_path(&mut dest_table, &entry.path, new_item);

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

            let output = String::from_utf8_lossy(&buf);
            println!("── output ({} bytes) ──\n{output:?}\n", buf.len());

            match fuzz::exact::check_edit_preservation(source_bytes, &buf, entry, &new_value_bytes)
            {
                Ok(()) => println!("── edit preservation: OK ──"),
                Err(msg) => {
                    eprintln!("FAILURE: edit preservation: {msg}");
                    std::process::exit(1);
                }
            }

            // Semantic check.
            let out_root = toml_spanner::parse(&output, &arena).unwrap();
            if dest_table.as_item() != out_root.table().as_item() {
                eprintln!("FAILURE: semantic mismatch after edit");
                std::process::exit(1);
            }
            println!("── semantic: OK ──");

            // Idempotency.
            check_idempotency_verbose(&output, &buf, source_text);
        }

        fuzz::exact::ModKind::Remove => {
            let removable = fuzz::exact::removable_entries(&entries, source_bytes);
            if removable.is_empty() {
                println!("no removable entries, fuzzer would reject");
                return;
            }
            let idx = removable[entry_selector as usize % removable.len()];
            let entry = &entries[idx];
            println!(
                "removing entry [{idx}]: path={:?} line={}..{}",
                entry.path, entry.line_start, entry.line_end
            );

            let mut dest_table = src_root.table().clone_in(&arena);
            fuzz::exact::remove_at_path(&mut dest_table, &entry.path);

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

            let output = String::from_utf8_lossy(&buf);
            println!("── output ({} bytes) ──\n{output:?}\n", buf.len());

            match fuzz::exact::check_remove_preservation(source_bytes, &buf, entry) {
                Ok(()) => println!("── remove preservation: OK ──"),
                Err(msg) => {
                    eprintln!("FAILURE: remove preservation: {msg}");
                    std::process::exit(1);
                }
            }

            // Semantic check.
            let out_root = toml_spanner::parse(&output, &arena).unwrap();
            if dest_table.as_item() != out_root.table().as_item() {
                eprintln!("FAILURE: semantic mismatch after remove");
                std::process::exit(1);
            }
            println!("── semantic: OK ──");

            check_idempotency_verbose(&output, &buf, source_text);
        }

        fuzz::exact::ModKind::Insert => {
            let mut targets = Vec::new();
            fuzz::exact::insertable_targets(src_root.table(), &mut Vec::new(), &mut targets);
            if targets.is_empty() {
                println!("no insertable targets, fuzzer would reject");
                return;
            }
            let (table_path, fresh_key) = &targets[entry_selector as usize % targets.len()];
            println!(
                "inserting key {:?} into table at path {:?}",
                fresh_key, table_path
            );

            let mut dest_table = src_root.table().clone_in(&arena);
            let target = fuzz::exact::table_at_path_mut(&mut dest_table, table_path);
            let new_item = toml_spanner::Item::from(42i64);
            target.insert(toml_spanner::Key::anon(fresh_key), new_item, &arena);

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

            let output = String::from_utf8_lossy(&buf);
            println!("── output ({} bytes) ──\n{output:?}\n", buf.len());

            match fuzz::exact::check_insert_preservation(source_text, &buf, table_path, fresh_key) {
                Ok(()) => println!("── insert preservation: OK ──"),
                Err(msg) => {
                    eprintln!("FAILURE: insert preservation: {msg}");
                    std::process::exit(1);
                }
            }

            // Semantic check.
            let out_root = toml_spanner::parse(&output, &arena).unwrap();
            if dest_table.as_item() != out_root.table().as_item() {
                eprintln!("FAILURE: semantic mismatch after insert");
                std::process::exit(1);
            }
            println!("── semantic: OK ──");

            check_idempotency_verbose(&output, &buf, source_text);
        }
    }
}

fn check_idempotency_verbose(output: &str, buf: &[u8], source_text: &str) {
    let arena = toml_spanner::Arena::new();
    let src2 = toml_spanner::parse(output, &arena).unwrap();
    let mut dest2 = src2.table().clone_in(&arena);
    let mut items2 = Vec::new();
    toml_spanner::reproject(&src2, &mut dest2, &mut items2);
    let norm2 = dest2.normalize();
    let cfg2 = toml_spanner::EmitConfig {
        projected_source_text: output,
        projected_source_items: &items2,
        reprojected_order: false,
        ..Default::default()
    };
    let mut buf2 = Vec::new();
    toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);
    if buf == buf2.as_slice() {
        println!("── idempotency: OK ──");
    } else {
        let output2 = String::from_utf8_lossy(&buf2);
        eprintln!(
            "FAILURE: not idempotent!\n\
             source:\n{source_text:?}\n\
             first:\n{output:?}\n\
             second:\n{output2:?}"
        );
        std::process::exit(1);
    }
}
