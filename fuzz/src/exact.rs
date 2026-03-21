use std::io::Write;
use toml_spanner::{Item, Table, TableStyle, Value};

#[derive(Debug, Clone)]
pub struct BodyEntry<'de> {
    pub path: Vec<&'de str>,
    pub key_start: usize,
    pub key_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub in_inline: bool,
    pub in_dotted: bool,
    pub kind: ScalarKind,
}

#[derive(Debug, Clone)]
pub enum ScalarKind {
    Integer(i64),
    Boolean(bool),
    NonEditable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKind {
    EditScalar,
    Remove,
    Insert,
}

pub fn line_start_of(src: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && src[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

pub fn line_end_of(src: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < src.len() && src[i] != b'\n' {
        i += 1;
    }
    if i < src.len() { i + 1 } else { i }
}

pub fn collect_body_entries<'de>(
    table: &Table<'de>,
    source: &[u8],
    path: &mut Vec<&'de str>,
    out: &mut Vec<BodyEntry<'de>>,
    in_inline: bool,
    in_dotted: bool,
) {
    for (key, item) in table {
        if key.span.is_empty() {
            continue;
        }
        path.push(key.name);
        let key_start = key.span.start as usize;
        let key_end = key.span.end as usize;
        let val_span = item.span();

        match item.value() {
            Value::Table(sub) => {
                match sub.style() {
                    TableStyle::Inline => {
                        // Body entry containing an inline table.
                        let ls = line_start_of(source, key_start);
                        let le = line_end_of(source, val_span.end as usize);
                        out.push(BodyEntry {
                            path: path.clone(),
                            key_start,
                            key_end,
                            line_start: ls,
                            line_end: le,
                            in_inline: true,
                            in_dotted,
                            kind: ScalarKind::NonEditable,
                        });
                        // Recurse into inline table for leaf scalars.
                        collect_body_entries(sub, source, path, out, true, in_dotted);
                    }
                    TableStyle::Header | TableStyle::Implicit => {
                        collect_body_entries(sub, source, path, out, in_inline, false);
                    }
                    TableStyle::Dotted => {
                        collect_body_entries(sub, source, path, out, in_inline, true);
                    }
                }
            }
            Value::Array(arr) => {
                if arr.style() == toml_spanner::ArrayStyle::Header {
                    path.pop();
                    continue;
                }
                let ls = if in_inline {
                    0
                } else {
                    line_start_of(source, key_start)
                };
                let le = if in_inline {
                    source.len()
                } else {
                    line_end_of(source, val_span.end as usize)
                };
                out.push(BodyEntry {
                    path: path.clone(),
                    key_start,
                    key_end,
                    line_start: ls,
                    line_end: le,
                    in_inline,
                    in_dotted,
                    kind: ScalarKind::NonEditable,
                });
            }
            _ => {
                let kind = match item.value() {
                    Value::Integer(v) => ScalarKind::Integer(*v),
                    Value::Boolean(v) => ScalarKind::Boolean(*v),
                    _ => ScalarKind::NonEditable,
                };
                let (ls, le) = if in_inline {
                    (0, source.len())
                } else {
                    (
                        line_start_of(source, key_start),
                        line_end_of(source, val_span.end as usize),
                    )
                };
                out.push(BodyEntry {
                    path: path.clone(),
                    key_start,
                    key_end,
                    line_start: ls,
                    line_end: le,
                    in_inline,
                    in_dotted,
                    kind,
                });
            }
        }
        path.pop();
    }
}

pub fn set_at_path<'de>(table: &mut Table<'de>, path: &[&str], item: Item<'de>) {
    if path.len() == 1 {
        let val = table.get_mut(path[0]).expect("set_at_path: key not found");
        *val = item;
        return;
    }
    let sub = table.get_mut(path[0]).expect("set_at_path: key not found");
    let sub_table = sub.as_table_mut().expect("set_at_path: not a table");
    set_at_path(sub_table, &path[1..], item);
}

pub fn remove_at_path(table: &mut Table<'_>, path: &[&str]) {
    if path.len() == 1 {
        table
            .remove_entry(path[0])
            .expect("remove_at_path: key not found");
        return;
    }
    let sub = table
        .get_mut(path[0])
        .expect("remove_at_path: key not found");
    let sub_table = sub.as_table_mut().expect("remove_at_path: not a table");
    remove_at_path(sub_table, &path[1..]);
}

pub fn format_canonical_integer(v: i64) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = write!(buf, "{v}");
    buf
}

pub fn format_canonical_bool(v: bool) -> Vec<u8> {
    if v {
        b"true".to_vec()
    } else {
        b"false".to_vec()
    }
}

/// Finds editable body entries: body-level (not in_inline, not in_dotted)
/// scalars that are Integer or Boolean.
pub fn editable_entries(entries: &[BodyEntry<'_>]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            !e.in_inline
                && !e.in_dotted
                && matches!(e.kind, ScalarKind::Integer(_) | ScalarKind::Boolean(_))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Finds removable body entries: body-level entries not inside inline tables,
/// not inside dotted tables, and not spanning multiple lines (multiline
/// inline arrays/tables may contain embedded comments that the emitter
/// preserves as gap text).
pub fn removable_entries(entries: &[BodyEntry<'_>], source: &[u8]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            if e.in_inline || e.in_dotted {
                return false;
            }
            // Exclude multiline entries (value spans multiple lines).
            let region = &source[e.line_start..e.line_end];
            let newlines = region.iter().filter(|&&b| b == b'\n').count();
            newlines <= 1
        })
        .map(|(i, _)| i)
        .collect()
}

/// Returns a fresh key name that doesn't conflict with existing table entries.
pub fn fresh_key_name<'a>(table: &Table<'_>, candidates: &[&'a str]) -> Option<&'a str> {
    for &name in candidates {
        if table.get(name).is_none() {
            return Some(name);
        }
    }
    None
}

/// Finds insertable tables: root + header/implicit tables where we can add a new
/// key without conflicting. Returns (path_to_table, fresh_key_name).
pub fn insertable_targets<'de>(
    table: &Table<'de>,
    path: &mut Vec<&'de str>,
    out: &mut Vec<(Vec<&'de str>, &'static str)>,
) {
    const CANDIDATES: &[&str] = &["zz_new", "zz_ins", "zz_add", "zz_x", "zz_y", "zz_z"];
    if let Some(name) = fresh_key_name(table, CANDIDATES) {
        out.push((path.clone(), name));
    }
    for (key, item) in table {
        if let Some(sub) = item.as_table() {
            match sub.style() {
                TableStyle::Header | TableStyle::Implicit => {
                    path.push(key.name);
                    insertable_targets(sub, path, out);
                    path.pop();
                }
                _ => {}
            }
        }
    }
}

/// Navigate to a table by path, returning a mutable reference.
pub fn table_at_path_mut<'a, 'de>(table: &'a mut Table<'de>, path: &[&str]) -> &'a mut Table<'de> {
    if path.is_empty() {
        return table;
    }
    let sub = table
        .get_mut(path[0])
        .expect("table_at_path_mut: key not found");
    let sub_table = sub.as_table_mut().expect("table_at_path_mut: not a table");
    table_at_path_mut(sub_table, &path[1..])
}

/// Reconstruct the `key = value` line the emitter would produce for a
/// body-level scalar edit. Uses the source key text verbatim.
pub fn predict_edit_line(source: &[u8], entry: &BodyEntry<'_>, new_value_bytes: &[u8]) -> Vec<u8> {
    // The reprojected entry inherits the source key span, so the emitter will
    // output: source[key_span] + " = " + format_scalar(new_value) + "\n"
    let key_text = &source[entry.key_start..entry.key_end];
    let mut predicted = Vec::with_capacity(key_text.len() + 3 + new_value_bytes.len() + 1);
    predicted.extend_from_slice(key_text);
    predicted.extend_from_slice(b" = ");
    predicted.extend_from_slice(new_value_bytes);
    predicted.push(b'\n');
    predicted
}

/// Run the exact preservation check for a scalar edit.
/// Returns Ok(()) on pass, Err(message) on failure.
pub fn check_edit_preservation(
    source: &[u8],
    actual: &[u8],
    entry: &BodyEntry<'_>,
    new_value_bytes: &[u8],
) -> Result<(), String> {
    let before_len = entry.line_start;
    let after_len = source.len() - entry.line_end;

    // Check before region.
    if actual.len() < before_len {
        return Err(format!(
            "output too short for before region: {} < {}",
            actual.len(),
            before_len
        ));
    }
    if actual[..before_len] != source[..before_len] {
        let first_diff = actual[..before_len]
            .iter()
            .zip(source[..before_len].iter())
            .position(|(a, b)| a != b)
            .unwrap_or(before_len.min(actual.len()));
        return Err(format!(
            "before region differs at byte {first_diff} (line_start={before_len})"
        ));
    }

    // Check after region.
    if actual.len() < after_len {
        return Err(format!(
            "output too short for after region: {} < {}",
            actual.len(),
            after_len
        ));
    }
    let actual_after_start = actual.len() - after_len;
    let source_after_start = source.len() - after_len;
    if actual[actual_after_start..] != source[source_after_start..] {
        let first_diff = actual[actual_after_start..]
            .iter()
            .zip(source[source_after_start..].iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "after region differs at byte offset {first_diff} from actual[{}..] vs source[{}..] \
             (after_len={after_len})",
            actual_after_start, source_after_start
        ));
    }

    // For body-level (non-inline) edits, check predicted middle.
    if !entry.in_inline {
        let predicted = predict_edit_line(source, entry, new_value_bytes);
        let middle_start = before_len;
        let middle_end = actual.len() - after_len;
        let actual_middle = &actual[middle_start..middle_end];
        if actual_middle != predicted.as_slice() {
            return Err(format!(
                "middle region mismatch:\n  actual:    {:?}\n  predicted: {:?}",
                String::from_utf8_lossy(actual_middle),
                String::from_utf8_lossy(&predicted),
            ));
        }
    }

    Ok(())
}

/// Run the exact preservation check for a removal.
pub fn check_remove_preservation(
    source: &[u8],
    actual: &[u8],
    entry: &BodyEntry<'_>,
) -> Result<(), String> {
    let before_len = entry.line_start;
    let after = &source[entry.line_end..];
    let expected_len = before_len + after.len();
    if actual.len() != expected_len {
        return Err(format!(
            "length mismatch: actual={} expected={}",
            actual.len(),
            expected_len
        ));
    }
    if actual[..before_len] != source[..before_len] {
        let first_diff = actual[..before_len]
            .iter()
            .zip(source[..before_len].iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!("before region differs at byte {first_diff}"));
    }
    if actual[before_len..] != *after {
        let first_diff = actual[before_len..]
            .iter()
            .zip(after.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(format!(
            "after region differs at byte {first_diff} (splice point={before_len})"
        ));
    }
    Ok(())
}

/// Run the exact preservation check for an insertion (round-trip approach).
/// Parses the output, removes the inserted key, re-emits, and checks exact match
/// with the original source.
pub fn check_insert_preservation(
    source_text: &str,
    actual: &[u8],
    insert_table_path: &[&str],
    insert_key: &str,
) -> Result<(), String> {
    let output_text =
        std::str::from_utf8(actual).map_err(|e| format!("output is not valid UTF-8: {e}"))?;

    let arena = toml_spanner::Arena::new();

    // Parse original source as the projection source.
    let src_root = toml_spanner::parse(source_text, &arena)
        .map_err(|e| format!("source re-parse failed: {e:?}"))?;

    // Parse the output (with insertion) as dest, then remove the inserted key.
    let mut dest_table = toml_spanner::parse(output_text, &arena)
        .map_err(|e| format!("output re-parse failed: {e:?}"))?
        .into_table();

    // Navigate to the table and remove the inserted entry.
    let target = table_at_path_mut(&mut dest_table, insert_table_path);
    if target.remove_entry(insert_key).is_none() {
        return Err(format!(
            "inserted key {:?} not found in output at path {:?}",
            insert_key, insert_table_path
        ));
    }

    // Reproject original source onto the (now-reverted) dest and emit.
    let buf2 = toml_spanner::Formatting::preserved_from(&src_root)
        .format_table_to_bytes(dest_table, &arena);

    let source_bytes = source_text.as_bytes().trim_ascii();
    let buf2_trimmed = buf2.trim_ascii();
    if buf2_trimmed != source_bytes {
        return Err(format!(
            "insert round-trip mismatch:\n  expected: {:?}\n  got:      {:?}",
            String::from_utf8_lossy(source_bytes),
            String::from_utf8_lossy(buf2_trimmed),
        ));
    }
    Ok(())
}
