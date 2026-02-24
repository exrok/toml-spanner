#[cfg(test)]
#[path = "reprojection_tests.rs"]
mod tests;

use crate::array::Array;
use crate::parser::Root;
use crate::span::Span;
use crate::table::{Table, TableIndex};
use crate::value::{ArrayStyle, Item, Key, TableStyle, Value, ValueMut};

/// Reprojects structural kinds from a parsed source onto a destination table.
///
/// Takes a [`Root`] to statically enforce that the source was produced by
/// [`parse`](crate::parse). Walks the dest tree, matching entries against
/// src by key name (tables) or position (arrays). For each matched pair,
/// copies the src item's structural kind onto dest and records the src item
/// in `items` so that `emit_with_reprojection` can later retrieve original
/// formatting.
///
/// Within each table, unmatched entries before the first match inherit the
/// first match's kind; unmatched entries after a match inherit the previous
/// match's kind.
///
/// Note: You will still have to normalize the result after reprojection.
///
/// Note: This invalidates span information in dest, but is fine because this
/// is only used for serialization where we ignore dest spans.
pub fn reproject<'de>(src: &'de Root<'de>, dest: &mut Table<'_>, items: &mut Vec<&'de Item<'de>>) {
    let index = src.table_index();
    let src_table = src.table();
    reproject_table(index, src_table, dest, items);
}

/// Returns `true` when the entire subtree is fully matched (same structure,
/// same values, same lengths).
///
/// Scalars are only given a reprojected index when fully matched.
/// Containers always get a reprojected index when types match (even
/// partial match), so emit can access the source item's span for
/// cursor-based partial emission (e.g. appending to an array while
/// preserving comments on existing elements).
fn reproject_item<'de>(
    index: &TableIndex<'_>,
    src: &'de Item<'de>,
    dest: &mut Item<'_>,
    items: &mut Vec<&'de Item<'de>>,
) -> bool {
    let mut container_match = false;
    let full_match = match (src.value(), dest.value_mut()) {
        (Value::String(a), ValueMut::String(b)) => *a == *b,
        (Value::Integer(a), ValueMut::Integer(b)) => *a == *b,
        (Value::Float(a), ValueMut::Float(b)) => a.to_bits() == b.to_bits(),
        (Value::Boolean(a), ValueMut::Boolean(b)) => *a == *b,
        (Value::DateTime(a), ValueMut::DateTime(b)) => {
            a.date() == b.date() && a.time() == b.time() && a.offset() == b.offset()
        }
        (Value::Table(src_table), ValueMut::Table(dest_table)) => {
            container_match = true;
            reproject_table(index, src_table, dest_table, items)
        }
        (Value::Array(src_array), ValueMut::Array(dest_array)) => {
            container_match = true;
            reproject_array(index, src_array, dest_array, items)
        }
        _ => false,
    };
    if full_match || container_match {
        if dest.set_reprojected_index(items.len()) {
            items.push(src);
        }
        if full_match {
            dest.set_reprojected_full_match();
        }
    } else {
        // Type mismatch: children weren't visited by recursive reprojection,
        // so their spans still point into dest text. Clear them to prevent
        // emit from indexing into source text at wrong positions.
        clear_stale_item(dest);
    }
    full_match
}

/// Returns `true` if the entry is an empty body-level table whose source
/// was a header/implicit section (eligible for promotion to Header).
fn is_promotable(entry: &(Key<'_>, Item<'_>), src: &Table<'_>, index: &TableIndex<'_>) -> bool {
    let Some(dt) = entry.1.as_table() else {
        return false;
    };
    if !dt.is_empty() || !matches!(dt.style(), TableStyle::Dotted | TableStyle::Inline) {
        return false;
    }
    let Some(e) = src.value.get_entry_with_index(entry.0.name, index) else {
        return false;
    };
    let Some(st) = e.1.as_table() else {
        return false;
    };
    matches!(st.style(), TableStyle::Header | TableStyle::Implicit)
}

/// Returns `true` when every entry in dest matched a src entry in the same
/// order and every `reproject_item` returned `true` (full structural match).
fn reproject_table<'de>(
    index: &TableIndex<'_>,
    src: &'de Table<'de>,
    dest: &mut Table<'_>,
    items: &mut Vec<&'de Item<'de>>,
) -> bool {
    let is_body_parent = matches!(dest.style(), TableStyle::Dotted | TableStyle::Inline);
    let ignore_style = dest.meta.ignore_source_style();
    let entries = dest.entries_mut();

    let mut last_table_kind: Option<TableStyle> = None;
    let mut last_array_kind: Option<ArrayStyle> = None;
    let mut first_table_matched = false;
    let mut first_array_matched = false;
    let mut all_matched = src.len() == entries.len();

    // Source-ordering state for reprojected_order mode.
    let mut max_stuck_src_pos: u32 = 0;
    let mut has_stuck = false;

    // Pass 1: Match entries, assign structural kinds, execute localized backfills,
    // and detect any "stuck" entries (subsections forced into body-level).
    for i in 0..entries.len() {
        let key_name = entries[i].0.name;

        if let Some(src_entry) = src.value.get_entry_with_index(key_name, index) {
            entries[i].0.span = src_entry.0.span;

            let item_full = reproject_item(index, &src_entry.1, &mut entries[i].1, items);
            if !item_full {
                all_matched = false;
            }

            if !ignore_style {
                let mut src_is_sub = false;

                if let Some(st) = src_entry.1.as_table() {
                    let mut kind = st.style();
                    src_is_sub = matches!(kind, TableStyle::Header | TableStyle::Implicit);

                    if let Some(dt) = entries[i].1.as_table_mut() {
                        let dest_kind = dt.style();
                        let dest_is_body =
                            dest_kind == TableStyle::Dotted || dest_kind == TableStyle::Inline;
                        if dest_is_body && src_is_sub {
                            kind = dest_kind;
                        }
                        dt.set_style(kind);
                    }

                    if !first_table_matched {
                        first_table_matched = true;
                        // Backfill: apply first matched table kind to preceding unmatched tables
                        for j in 0..i {
                            if entries[j].0.span.is_empty() {
                                if let Some(dt) = entries[j].1.as_table_mut() {
                                    dt.set_style(kind);
                                }
                            }
                        }
                    }
                    last_table_kind = Some(kind);
                }

                if let Some(sa) = src_entry.1.as_array() {
                    let kind = sa.style();
                    if kind == ArrayStyle::Header {
                        src_is_sub = true;
                    }

                    if let Some(da) = entries[i].1.as_array_mut() {
                        if kind == ArrayStyle::Header && da.style() == ArrayStyle::Inline {
                            let mut has_non_frozen = false;
                            for e in da.iter() {
                                if let Some(t) = e.as_table() {
                                    if t.style() != TableStyle::Inline {
                                        has_non_frozen = true;
                                        break;
                                    }
                                }
                            }
                            if has_non_frozen {
                                da.set_style(kind);
                            }
                        } else {
                            da.set_style(kind);
                        }
                    }

                    if !first_array_matched {
                        first_array_matched = true;
                        // Backfill: apply first matched array kind to preceding unmatched arrays
                        for j in 0..i {
                            if entries[j].0.span.is_empty() {
                                if let Some(da) = entries[j].1.as_array_mut() {
                                    da.set_style(kind);
                                }
                            }
                        }
                    }
                    last_array_kind = Some(kind);
                }

                if src_entry.1.as_table().is_none() {
                    if let Some(dt) = entries[i].1.as_table_mut() {
                        dt.set_style(if dt.is_empty() {
                            TableStyle::Inline
                        } else {
                            TableStyle::Dotted
                        });
                    }
                }
                if src_entry.1.as_array().is_none() {
                    if let Some(da) = entries[i].1.as_array_mut() {
                        da.set_style(ArrayStyle::Inline);
                    }
                }

                if src_is_sub {
                    let is_stuck = if let Some(dt) = entries[i].1.as_table() {
                        let body = matches!(dt.style(), TableStyle::Dotted | TableStyle::Inline);
                        // Stuck if it is a non-empty body table, or an empty body table with non-table source
                        body && !(dt.is_empty() && src_entry.1.as_table().is_some())
                    } else if let Some(da) = entries[i].1.as_array() {
                        da.style() == ArrayStyle::Inline
                    } else {
                        true // Scalar type mismatch
                    };

                    if is_stuck {
                        has_stuck = true;
                        max_stuck_src_pos = max_stuck_src_pos.max(entries[i].0.span.start);
                    }
                }
            }
        } else {
            // Unmatched Entry
            all_matched = false;
            entries[i].0.span = Span::default();
            clear_stale_item(&mut entries[i].1);

            if !ignore_style {
                if let Some(kind) = last_table_kind {
                    if let Some(dt) = entries[i].1.as_table_mut() {
                        dt.set_style(kind);
                    }
                }
                if let Some(kind) = last_array_kind {
                    if let Some(da) = entries[i].1.as_array_mut() {
                        da.set_style(kind);
                    }
                }
            }
        }
    }

    // Pass 2: Source-ordering fixes (Demotions + Promotions).
    // Skip in body-level parents: promoting creates subsections that sort
    // after all body items at the parent level, breaking source order.
    if !ignore_style {
        for i in 0..entries.len() {
            if entries[i].0.span.is_empty() {
                continue;
            }

            if has_stuck {
                let src_pos = entries[i].0.span.start;
                // Demote subsections before the stuck point to body-level.
                if src_pos < max_stuck_src_pos {
                    if let Some(dt) = entries[i].1.as_table_mut() {
                        if dt.style() == TableStyle::Header {
                            dt.set_style(TableStyle::Inline);
                        }
                    }
                    if let Some(da) = entries[i].1.as_array_mut() {
                        if da.style() == ArrayStyle::Header {
                            da.set_style(ArrayStyle::Inline);
                            for elem in da.as_mut_slice() {
                                if let Some(t) = elem.as_table_mut() {
                                    t.set_style(TableStyle::Inline);
                                }
                            }
                        }
                    }
                    continue;
                }
            }

            // Promote empty body-level tables to Header.
            if !is_body_parent && is_promotable(&entries[i], src, index) {
                if let Some(dt) = entries[i].1.as_table_mut() {
                    dt.set_style(TableStyle::Header);
                }
            }
        }
    }

    all_matched
}

/// Clears reprojection data from an unmatched item and all its descendants.
fn clear_stale_item(item: &mut Item<'_>) {
    item.set_reprojected_to_none();
    if let Some(sub) = item.as_table_mut() {
        clear_stale_spans(sub);
    } else if let Some(arr) = item.as_array_mut() {
        for elem in arr.as_mut_slice() {
            clear_stale_item(elem);
        }
    }
}

/// Recursively clears key spans and reprojection data from an unmatched
/// subtree so emit doesn't index into source text at dest-text positions.
fn clear_stale_spans(table: &mut Table<'_>) {
    for (key, item) in table.entries_mut() {
        key.span = Span::default();
        clear_stale_item(item);
    }
}

/// Returns `true` when `src.len() == dest.len()` and every element's
/// `reproject_item` returned `true`.
fn reproject_array<'de>(
    index: &TableIndex<'_>,
    src: &'de Array<'de>,
    dest: &mut Array<'_>,
    items: &mut Vec<&'de Item<'de>>,
) -> bool {
    let mut all_matched = src.len() == dest.len();
    let mut i = 0;
    for dest_item in dest.as_mut_slice() {
        if let Some(src_item) = src.get(i) {
            if !reproject_item(index, src_item, dest_item, items) {
                all_matched = false;
            }
        } else {
            all_matched = false;
            clear_stale_item(dest_item);
        }
        i += 1;
    }
    all_matched
}
