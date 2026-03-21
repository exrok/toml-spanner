#[cfg(test)]
#[path = "reprojection_tests.rs"]
mod tests;

use crate::item::table::TableIndex;
use crate::item::{ArrayStyle, Item, TableStyle, Value, ValueMut};
use crate::parser::Document;
use crate::span::Span;
use crate::{Array, Table};
use std::hash::{BuildHasher, Hasher};

#[cfg(test)]
std::thread_local! {
    static FORCE_HASH_COLLISIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[inline]
fn maybe_force_collision(hash: u64) -> u64 {
    #[cfg(test)]
    if FORCE_HASH_COLLISIONS.get() {
        return 42;
    }
    hash
}

/// Reprojects structural kinds from a parsed source onto a destination table.
///
/// Takes a [`Document`] to statically enforce that the source was produced by
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
pub fn reproject<'de>(
    src: &'de Document<'de>,
    dest: &mut Table<'_>,
    items: &mut Vec<&'de Item<'de>>,
) {
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
        (Value::DateTime(a), ValueMut::DateTime(b)) => a == b,
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

    // Match entries, assign structural kinds, execute localized backfills,
    // and detect any "stuck" entries (subsections forced into body-level).
    for i in 0..entries.len() {
        let (dst_key, dst_item) = &mut entries[i];
        let mut dst_item = dst_item;

        let Some((src_key, src_item)) = src.value.get_entry_with_index(dst_key.name, index) else {
            // Unmatched Entry
            all_matched = false;
            dst_key.span = Span::default();
            clear_stale_item(&mut dst_item);

            if !ignore_style {
                match dst_item.value_mut() {
                    ValueMut::Table(t) => {
                        if let Some(style) = last_table_kind {
                            t.set_style(style);
                        }
                    }
                    ValueMut::Array(a) => {
                        if let Some(style) = last_array_kind {
                            a.set_style(style);
                        }
                    }
                    _ => {}
                }
            }
            continue;
        };
        dst_key.span = src_key.span;

        let item_full = reproject_item(index, &src_item, dst_item, items);
        if !item_full {
            all_matched = false;
        }
        if ignore_style {
            continue;
        }
        let mut src_is_sub = false;

        if let Some(st) = src_item.as_table() {
            let mut kind = st.style();
            src_is_sub = matches!(kind, TableStyle::Header | TableStyle::Implicit);

            if let Some(dt) = dst_item.as_table_mut() {
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
                for (key, item) in &mut entries[..i] {
                    if key.span.is_empty() {
                        if let Some(dt) = item.as_table_mut() {
                            dt.set_style(kind);
                        }
                    }
                }
                // Make borrow check happy
                dst_item = &mut entries[i].1;
            }
            last_table_kind = Some(kind);
        } else if let Some(sa) = src_item.as_array() {
            let kind = sa.style();
            if kind == ArrayStyle::Header {
                src_is_sub = true;
            }

            if let Some(da) = dst_item.as_array_mut() {
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
                } else if kind == ArrayStyle::Inline && da.is_auto_style() {
                    // Progressive: preserve auto-style so normalization can
                    // set EXPANDED_BIT if content warrants it
                } else {
                    da.set_style(kind);
                }
            }

            if !first_array_matched {
                first_array_matched = true;
                for (key, item) in &mut entries[..i] {
                    if key.span.is_empty() {
                        if let Some(dt) = item.as_array_mut() {
                            if !dt.is_auto_style() {
                                dt.set_style(kind);
                            }
                        }
                    }
                }
                // Make borrow check happy, this is the same as
                // what dst_item already was but the the above loop,
                // invalidated it entries.
                dst_item = &mut entries[i].1;
            }
            last_array_kind = Some(kind);
        }

        // When the source is not a table but dest is: the source
        // was a body entry, so demote subsection-style dest tables
        // to body-level to preserve ordering. But if the dest is
        // already body-level (Inline/Dotted), keep its style —
        // it reflects user intent and the source has no structural
        // opinion about the new container.
        if src_item.as_table().is_none() {
            if let Some(dt) = dst_item.as_table_mut() {
                let dest_kind = dt.style();
                if dest_kind != TableStyle::Inline && dest_kind != TableStyle::Dotted {
                    dt.set_style(if dt.is_empty() {
                        TableStyle::Inline
                    } else {
                        TableStyle::Dotted
                    });
                }
            }
        }
        if src_item.as_array().is_none() {
            if let Some(da) = dst_item.as_array_mut() {
                da.set_style(ArrayStyle::Inline);
            }
        }

        if src_is_sub {
            let is_stuck = if let Some(dt) = dst_item.as_table() {
                let body = matches!(dt.style(), TableStyle::Dotted | TableStyle::Inline);
                // Stuck if it is a non-empty body table, or an empty body table with non-table source
                body && !(dt.is_empty() && src_item.as_table().is_some())
            } else if let Some(da) = dst_item.as_array() {
                da.style() == ArrayStyle::Inline
            } else {
                true // Scalar type mismatch
            };

            if is_stuck {
                has_stuck = true;
                max_stuck_src_pos = max_stuck_src_pos.max(src_key.span.start);
            }
        }
    }

    if !ignore_style {
        ensure_valid_subsection_ordering(
            index,
            src,
            is_body_parent,
            entries,
            max_stuck_src_pos,
            has_stuck,
        );
    }

    all_matched
}

fn ensure_valid_subsection_ordering(
    index: &TableIndex<'_>,
    src: &'_ Table<'_>,
    is_body_parent: bool,
    entries: &mut [(crate::Key<'_>, Item<'_>)],
    max_stuck_src_pos: u32,
    has_stuck: bool,
) {
    for (dst_key, dst_item) in entries {
        // Ignore unprojected items
        if dst_key.span.is_empty() {
            continue;
        }

        if has_stuck {
            let src_pos = dst_key.span.start;
            // Demote subsections before the stuck point to body-level.
            if src_pos < max_stuck_src_pos {
                match dst_item.value_mut() {
                    ValueMut::Array(da) => {
                        if da.style() == ArrayStyle::Header {
                            da.set_style(ArrayStyle::Inline);
                            for elem in da.as_mut_slice() {
                                if let Some(t) = elem.as_table_mut() {
                                    t.set_style(TableStyle::Inline);
                                }
                            }
                        }
                    }
                    ValueMut::Table(dt) => {
                        if dt.style() == TableStyle::Header {
                            dt.set_style(TableStyle::Inline);
                        }
                    }
                    _ => (),
                }
                continue;
            }
        }

        // Promote empty body-level tables to Header.
        if is_body_parent {
            continue;
        }

        let Some(dt) = dst_item.as_table_mut() else {
            continue;
        };
        if !dt.is_empty() || !matches!(dt.style(), TableStyle::Dotted | TableStyle::Inline) {
            continue;
        }
        let Some((_, src_item)) = src.value.get_entry_with_index(dst_key.name, index) else {
            continue;
        };
        let Some(st) = src_item.as_table() else {
            continue;
        };
        if matches!(st.style(), TableStyle::Header | TableStyle::Implicit) {
            dt.set_style(TableStyle::Header);
        }
    }
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

/// Hashes an item's content into `h`. Tables use order-independent XOR
/// bucketing so that key ordering doesn't affect the hash.
fn hash_item(item: &Item<'_>, h: &mut impl Hasher, build: &impl BuildHasher) {
    match item.value() {
        Value::String(s) => {
            h.write_u8(0);
            h.write_u64(s.len() as u64);
            h.write(s.as_bytes());
        }
        Value::Integer(i) => {
            h.write_u8(1);
            h.write_i64(*i);
        }
        Value::Float(f) => {
            h.write_u8(2);
            h.write_u64(f.to_bits());
        }
        Value::Boolean(b) => {
            h.write_u8(3);
            h.write_u8(*b as u8);
        }
        Value::DateTime(d) => {
            h.write_u8(4);
            if let Some(date) = d.date() {
                h.write_u16(date.year);
                h.write_u8(date.month);
                h.write_u8(date.day);
            }
            if let Some(time) = d.time() {
                h.write_u8(time.hour);
                h.write_u8(time.minute);
                h.write_u8(time.second);
                h.write_u32(time.nanosecond);
            }
            if let Some(offset) = d.offset() {
                match offset {
                    crate::TimeOffset::Z => h.write_i16(i16::MAX),
                    crate::TimeOffset::Custom { minutes } => h.write_i16(minutes),
                }
            }
        }
        Value::Table(t) => {
            h.write_u8(5);
            // Order-independent: XOR entry hashes into 8 buckets keyed by key hash.
            let mut buckets = [0u64; 8];
            for (key, val) in t.entries() {
                let mut kh = build.build_hasher();
                kh.write(key.name.as_bytes());
                let key_hash = kh.finish();

                hash_item(val, &mut kh, build);
                let val_hash = kh.finish();

                buckets[(key_hash as usize) & 7] ^= val_hash;
            }
            for b in &buckets {
                h.write_u64(*b);
            }
        }
        Value::Array(a) => {
            h.write_u8(6);
            for elem in a.iter() {
                hash_item(elem, h, build);
            }
        }
    }
}

const INDEX_MASK: u64 = 0xFFFF;
const HASH_SHIFT: u32 = 16;
/// Bit 63: reserved flag to mark consumed src / matched dest entries.
const MATCHED_BIT: u64 = 1 << 63;
/// Hash mask: bits 62-16 (47 bits of hash, bit 63 reserved).
const HASH_MASK: u64 = !INDEX_MASK & !MATCHED_BIT;
/// Max collision-group cross-product before we skip matching.
#[cfg(not(test))]
const COLLISION_CAP: usize = 256;
#[cfg(test)]
const COLLISION_CAP: usize = 16;

/// Max array length for content-based matching; beyond this, fall back to
/// positional matching.
#[cfg(not(test))]
const INDEX_LIMIT: usize = u16::MAX as usize;
#[cfg(test)]
const INDEX_LIMIT: usize = 32;

/// Content-based array matching using hash + sort + merge-join.
///
/// Hashes each element with random state, packs `(hash47, index16)` into
/// u64 (bit 63 reserved as matched flag), sorts both arrays, then
/// merge-joins to find content matches. Match results are written directly
/// into the sorting buffer — no separate match map or used-set needed.
///
/// Collision groups use a `u32` bitset (capped at 32 src entries) to track
/// consumed elements within each group.
///
/// Returns `true` when every dest element fully matched a src element.
fn reproject_array<'de>(
    index: &TableIndex<'_>,
    src: &'de Array<'de>,
    dest: &mut Array<'_>,
    items: &mut Vec<&'de Item<'de>>,
) -> bool {
    let n = src.len();
    let m = dest.len();

    if n == 0 || m == 0 {
        if m > 0 {
            for dest_item in dest.as_mut_slice() {
                clear_stale_item(dest_item);
            }
        }
        return n == 0 && m == 0;
    }

    // Fall back to positional for arrays exceeding the index space.
    if n > INDEX_LIMIT || m > INDEX_LIMIT {
        return reproject_array_positional(index, src, dest, items);
    }

    let mut prefix = 0;

    while let (Some(src_head), Some(dst_head)) = (src.get(prefix), dest.get_mut(prefix)) {
        if crate::item::equal_items(src_head, dst_head, Some(index)) {
            reproject_item(index, src_head, dst_head, items);
            prefix += 1;
        } else {
            break;
        }
    }
    let src = &src.as_slice()[prefix..];
    let dest = &mut dest.as_mut_slice()[prefix..];
    let n = src.len();
    let m = dest.len();

    let build = foldhash::quality::RandomState::default();

    // Pack (hash47, index16) into u64. Bit 63 reserved as matched flag.
    let mut buf: Vec<u64> = Vec::with_capacity(n + m);
    for (i, item) in src.iter().enumerate() {
        let mut h = build.build_hasher();
        hash_item(item, &mut h, &build);
        buf.push(((maybe_force_collision(h.finish()) << HASH_SHIFT) & !MATCHED_BIT) | (i as u64));
    }
    for (i, item) in dest.iter().enumerate() {
        let mut h = build.build_hasher();
        hash_item(item, &mut h, &build);
        buf.push(((maybe_force_collision(h.finish()) << HASH_SHIFT) & !MATCHED_BIT) | (i as u64));
    }

    let (src_sorted, dest_sorted) = buf.split_at_mut(n);
    src_sorted.sort_unstable();
    dest_sorted.sort_unstable();

    // Merge-join on hash bits. Matched entries are tagged in-place:
    //   src_sorted:  MATCHED_BIT is set on consumed entries.
    //   dest_sorted: rewritten to MATCHED_BIT | (src_idx << 16) | dest_idx.
    let mut matched_count: usize = 0;
    let mut si = 0;
    let mut di = 0;
    while si < n && di < m {
        let sh = src_sorted[si] & HASH_MASK;
        let dh = dest_sorted[di] & HASH_MASK;
        if sh < dh {
            si += 1;
        } else if sh > dh {
            di += 1;
        } else {
            // Collect the collision group with this hash.
            let group_hash = sh;
            let si_start = si;
            let di_start = di;
            while si < n && (src_sorted[si] & HASH_MASK) == group_hash {
                si += 1;
            }
            while di < m && (dest_sorted[di] & HASH_MASK) == group_hash {
                di += 1;
            }

            let group_n = si - si_start;
            let group_m = di - di_start;

            // Fast prefix: within a collision group entries are sorted by
            // original index, so leading elements that are equal 1:1 can be
            // matched linearly before the quadratic cross-product remainder.
            let prefix_len = group_n.min(group_m);
            let mut prefix_matched = 0;
            for k in 0..prefix_len {
                let src_idx = (src_sorted[si_start + k] & INDEX_MASK) as usize;
                let dest_idx = (dest_sorted[di_start + k] & INDEX_MASK) as usize;
                if crate::item::equal_items(&src[src_idx], &dest[dest_idx], Some(index)) {
                    dest_sorted[di_start + k] =
                        MATCHED_BIT | ((src_idx as u64) << HASH_SHIFT) | (dest_idx as u64);
                    src_sorted[si_start + k] |= MATCHED_BIT;
                    matched_count += 1;
                    prefix_matched += 1;
                } else {
                    break;
                }
            }

            let si_rem = si_start + prefix_matched;
            let di_rem = di_start + prefix_matched;

            if (si - si_rem) * (di - di_rem) > COLLISION_CAP {
                continue;
            }

            for d in di_rem..di {
                let dest_idx = (dest_sorted[d] & INDEX_MASK) as usize;
                for s in si_rem..si {
                    let src_entry = &mut src_sorted[s];
                    if (*src_entry as i64) < 0 {
                        continue;
                    }
                    let src_idx = (*src_entry & INDEX_MASK) as usize;
                    if crate::item::equal_items(&src[src_idx], &dest[dest_idx], Some(index)) {
                        dest_sorted[d] =
                            MATCHED_BIT | ((src_idx as u64) << HASH_SHIFT) | (dest_idx as u64);
                        *src_entry |= MATCHED_BIT;
                        matched_count += 1;
                        break;
                    }
                }
            }
        }
    }

    // Restore dest_sorted to original dest order via cycle sort (O(m)).
    for i in 0..m {
        while (dest_sorted[i] & INDEX_MASK) as usize != i {
            let target = (dest_sorted[i] & INDEX_MASK) as usize;
            dest_sorted.swap(i, target);
        }
    }

    // Build fallback list: compact unconsumed src indices, sort by index.
    // Reuses the src_sorted buffer in-place.
    let mut fc = 0;
    for i in 0..n {
        if src_sorted[i] & MATCHED_BIT == 0 {
            src_sorted[fc] = src_sorted[i] & INDEX_MASK;
            fc += 1;
        }
    }
    src_sorted[..fc].sort_unstable();

    // Detect reordering across ALL assignments (content matches + fallbacks).
    // Array order is semantic, so any reordering must preserve dest order.
    let mut reordered = false;
    let mut prev_src = 0u64;
    let mut fbi = 0usize;
    for entry in dest_sorted.iter() {
        let si = if *entry & MATCHED_BIT != 0 {
            (*entry >> HASH_SHIFT) & INDEX_MASK
        } else if fbi < fc {
            let s = src_sorted[fbi];
            fbi += 1;
            s
        } else {
            continue;
        };
        if si < prev_src {
            reordered = true;
            break;
        }
        prev_src = si + 1;
    }

    // Apply content matches and fallback.
    // Reordered arrays are not fully matched: verbatim source copy would
    // restore source order, changing semantic meaning.
    let mut all_matched = matched_count == n && n == m && !reordered;
    let mut fi = 0;
    for (di, entry) in dest_sorted.iter().enumerate() {
        let dest_entry = &mut dest[di];
        if *entry & MATCHED_BIT != 0 {
            let src_idx = ((*entry >> HASH_SHIFT) & INDEX_MASK) as usize;
            if !reproject_item(index, &src[src_idx], dest_entry, items) {
                // Currently unreachable: content-matched elements (verified
                // by equal_items) always produce a full match in reproject_item.
                // Kept as a defensive guard.
                all_matched = false;
            }
        } else if fi < fc {
            // Fallback: pair with next unconsumed src for partial match.
            let src_idx = src_sorted[fi] as usize;
            reproject_item(index, &src[src_idx], dest_entry, items);
            fi += 1;
            all_matched = false;
        } else {
            clear_stale_item(dest_entry);
            all_matched = false;
        }

        if reordered {
            dest_entry.meta.set_array_reordered();
        }
    }

    all_matched
}

/// Simple positional fallback for arrays exceeding u16 index space.
/// Only reachable for arrays with >65535 elements; see the u16 guard
/// in [`reproject_array`].
fn reproject_array_positional<'de>(
    index: &TableIndex<'_>,
    src: &'de Array<'de>,
    dest: &mut Array<'_>,
    items: &mut Vec<&'de Item<'de>>,
) -> bool {
    let mut all_matched = src.len() == dest.len();
    let src = src.as_slice();
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
