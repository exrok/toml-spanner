mod normalization;
mod partition;
mod reprojection;
#[cfg(test)]
pub(crate) mod test_data;
#[cfg(test)]
#[path = "emit_tests.rs"]
mod tests;

pub(crate) use normalization::NormalizedTable;
pub(crate) use reprojection::reproject;
pub(crate) use reprojection::reproject_with_span_identity;

use crate::Array;
use crate::Table;
use crate::arena::Arena;
use crate::item::{ArrayStyle, Item, Key, TableStyle, Value};
use crate::span::Span;
use std::io::Write;
use std::mem::MaybeUninit;

/// Stack-allocated linked list node for building key-path prefixes
/// without heap allocation. Each node lives on the call stack.
#[derive(Clone, Copy)]
struct Prefix<'a, 'de> {
    name: &'de str,
    key_span: Span,
    parent: Option<&'a Prefix<'a, 'de>>,
}

/// Indentation unit for expanded inline arrays.
///
/// Each nesting level repeats this unit once.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Indent {
    /// N spaces per level (e.g. `Spaces(4)` for 4-space indent).
    Spaces(u8),
    /// One tab character per level.
    Tab,
}

impl Default for Indent {
    fn default() -> Self {
        Indent::Spaces(4)
    }
}

impl Indent {
    fn write(&self, out: &mut Vec<u8>, depth: u32) {
        match self {
            Indent::Spaces(n) => {
                let total = depth as usize * *n as usize;
                for _ in 0..total {
                    out.push(b' ');
                }
            }
            Indent::Tab => {
                for _ in 0..depth {
                    out.push(b'\t');
                }
            }
        }
    }

    fn width(&self) -> usize {
        match self {
            Indent::Spaces(n) => *n as usize,
            Indent::Tab => 1,
        }
    }
}

/// Configuration for [`emit_with_config`].
///
/// When both fields are non-empty, scalar values whose reprojected source
/// item has a valid span will be emitted verbatim from the original text,
/// preserving formatting such as literal strings, hex/octal/binary
/// integers, and underscored numbers.
pub(crate) struct EmitConfig<'a> {
    pub projected_source_text: &'a str,
    pub projected_source_items: &'a [&'a Item<'a>],
    pub indent: Indent,
}

struct Emitter<'a, 'b> {
    arena: &'b Arena,
    src: &'a [u8],
    src_items: &'a [&'a Item<'a>],
    indent: Indent,
}

impl Default for EmitConfig<'_> {
    fn default() -> Self {
        Self {
            projected_source_text: "",
            projected_source_items: &[],
            indent: Indent::default(),
        }
    }
}


fn trim_trailing_newline(buf: &mut Vec<u8>) {
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
}

/// Like [`emit`], but with an [`EmitConfig`] for scalar format preservation.
///
/// When the config carries reprojection data (from [`reproject`]), scalar
/// values that match their source are emitted verbatim from the original
/// text, preserving formatting like literal strings, hex integers, etc.
pub(crate) fn emit_with_config(
    table: &NormalizedTable<'_>,
    config: &EmitConfig<'_>,
    arena: &Arena,
    buf: &mut Vec<u8>,
) {
    let table = table.table();
    let emit = Emitter {
        arena,
        src: config.projected_source_text.as_bytes(),
        src_items: config.projected_source_items,
        indent: config.indent,
    };

    if !emit.src.is_empty() {
        let mut cursor = 0usize;
        emit_ordered(table, None, None, &emit, buf, &mut cursor);
        emit_gap(&emit, cursor, emit.src.len(), buf);
        if !emit.src.ends_with(b"\n") {
            trim_trailing_newline(buf);
        }
    } else {
        emit_formatted(table, None, &emit, buf);
    }
}

enum EmitOp<'a, 'b, 'de> {
    Body(&'a Key<'de>, &'a Item<'de>, Option<&'b Prefix<'b, 'de>>),
    Header(&'a Table<'de>, &'a Item<'de>, &'b Prefix<'b, 'de>),
    AotElement(&'a Table<'de>, &'a Item<'de>, &'b Prefix<'b, 'de>),
}

fn is_reordered_aot(op: &EmitOp<'_, '_, '_>) -> bool {
    if let EmitOp::AotElement(_, entry, _) = op {
        entry.meta.array_reordered()
    } else {
        false
    }
}

fn segment_index(sort_pos: u32, index: usize, is_body: bool) -> u64 {
    let sub_bit = if is_body { 0 } else { 1u64 << 63 };
    sub_bit | ((sort_pos as u64) << 32) | (index as u64)
}

/// A segment of emit output with its source-derived sort position.
struct Segment<'a, 'b, 'de> {
    /// Source position for sorting (or `last_projected` for unprojected items).
    sort_pos: u32,
    /// Source byte offset where this segment's content begins.
    source_start: u32,
    /// True for body entries, false for subsection entries.
    /// Body segments are emitted before subsection segments.
    is_body: bool,
    /// The operation to execute during the final output pass.
    op: EmitOp<'a, 'b, 'de>,
}

fn alloc_prefix<'b, 'de>(
    arena: &'b Arena,
    name: &'de str,
    key_span: Span,
    parent: Option<&'b Prefix<'b, 'de>>,
) -> &'b Prefix<'b, 'de> {
    // SAFETY:
    // - `arena.alloc(size_of::<Prefix>())` returns memory suitably sized.
    //   Prefix contains &str (align 8) + Span (align 4) + Option<&Prefix>
    //   (align 8), so max field align is 8 which matches ALLOC_ALIGN.
    // - `ptr::write` initializes the allocation with a valid Prefix.
    // - The resulting &'b reference is valid for the arena's lifetime 'b.
    unsafe {
        let ptr = arena
            .alloc(std::mem::size_of::<Prefix<'b, 'de>>())
            .cast::<Prefix<'b, 'de>>()
            .as_ptr();
        std::ptr::write(
            ptr,
            Prefix {
                name,
                key_span,
                parent,
            },
        );
        &*ptr
    }
}

fn build_segment_order(segments: &[Segment<'_, '_, '_>], ignore_source_order: bool) -> Vec<u64> {
    let mut order = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        order.push(segment_index(seg.sort_pos, i, seg.is_body));
    }
    if !ignore_source_order {
        sort_index(&mut order);
    }
    order
}

fn emit_segment_prefix(
    seg: &Segment<'_, '_, '_>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    let ss = seg.source_start;
    let is_header = matches!(seg.op, EmitOp::Header(..) | EmitOp::AotElement(..));

    if ss != u32::MAX {
        let target = ss as usize;
        if target >= *cursor {
            let pre_len = out.len();
            emit_gap(emit, *cursor, target, out);
            if out.len() == pre_len && is_header && !out.is_empty() && out.last() != Some(&b'\n') {
                out.push(b'\n');
            }
            *cursor = target;
        } else if is_header && !is_reordered_aot(&seg.op) {
            out.push(b'\n');
        }
    } else if is_header && !is_reordered_aot(&seg.op) {
        out.push(b'\n');
    }
}

/// Segmented path: collect format operations WITHOUT gap handling,
/// record segments with sort keys, sort, and execute with proper gaps.
fn emit_ordered<'a, 'b, 'de: 'b>(
    table: &'a Table<'de>,
    dotted_prefix: Option<&'b Prefix<'b, 'de>>,
    section_prefix: Option<&'b Prefix<'b, 'de>>,
    emit: &Emitter<'b, 'de>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    let mut segments: Vec<Segment<'a, 'b, 'de>> = Vec::new();
    let mut last_projected: u32 = 0;

    collect_segments(
        table,
        dotted_prefix,
        section_prefix,
        emit,
        &mut segments,
        &mut last_projected,
        0, // ALL
    );

    let order = build_segment_order(&segments, table.meta.ignore_source_order());

    // Reassemble into output with proper gap handling.
    for &entry in &order {
        let seg = &segments[(entry & 0xFFFF_FFFF) as usize];
        emit_segment_prefix(seg, emit, out, cursor);

        match &seg.op {
            EmitOp::Body(key, item, dotted) => {
                emit_body_entry(key, item, *dotted, emit, out, cursor);
            }
            EmitOp::Header(sub_table, item, node) => {
                emit_header_body(sub_table, item, node, emit, out, cursor);
            }
            EmitOp::AotElement(sub_table, arr_entry, node) => {
                let pre_cursor = *cursor;
                emit_aot_element(sub_table, arr_entry, node, emit, out, cursor);
                // Prevent cursor from going backwards when elements are
                // reordered — stops trailing emit_gap from re-emitting
                // already-covered source content.
                *cursor = (*cursor).max(pre_cursor);
            }
        }
    }
}

/// Recursively collects segments, flattening implicit and dotted
/// containers so their children can interleave in source order.
///
/// Segments are emitted WITHOUT gap handling — gaps are added during
/// reassembly based on source positions.
///
/// `mode`: 0 = all entries, 1 = body only (skip headers/AOTs),
///         2 = subsections only (skip body entries).
fn collect_segments<'a, 'b, 'de: 'b>(
    table: &'a Table<'de>,
    dotted_prefix: Option<&'b Prefix<'b, 'de>>,
    section_prefix: Option<&'b Prefix<'b, 'de>>,
    emit: &Emitter<'b, 'de>,
    segments: &mut Vec<Segment<'a, 'b, 'de>>,
    last_projected: &mut u32,
    mode: u8,
) {
    const BODY_ONLY: u8 = 1;
    const SUBS_ONLY: u8 = 2;
    // mode 0 = ALL (both body and subsection segments)

    for entry in table.entries() {
        let key = &entry.0;
        let item = &entry.1;

        if item.has_dotted_bit() {
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            let dotted_node = alloc_prefix(emit.arena, key.name, key.span, dotted_prefix);
            let sec_node = alloc_prefix(emit.arena, key.name, key.span, section_prefix);
            // Anchor unprojected children near this container's source position.
            if !key.span.is_empty() {
                *last_projected = key.span.start;
            }
            collect_segments(
                sub_table,
                Some(dotted_node),
                Some(sec_node),
                emit,
                segments,
                last_projected,
                mode,
            );
            continue;
        }

        if item.is_implicit_table() {
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            // Anchor unprojected children near this container's source position.
            if !key.span.is_empty() {
                *last_projected = key.span.start;
            }
            let sec_node = alloc_prefix(emit.arena, key.name, key.span, section_prefix);
            collect_segments(
                sub_table,
                None,
                Some(sec_node),
                emit,
                segments,
                last_projected,
                mode,
            );
            continue;
        }

        if item.has_header_bit() {
            if mode == BODY_ONLY {
                continue;
            }
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            let node = alloc_prefix(emit.arena, key.name, key.span, section_prefix);
            let sort_key_opt = projected_span(item, emit).map(|s| s.start);
            let source_start = sort_key_opt.unwrap_or(u32::MAX);

            let sp = pack_sort_pos(sort_key_opt, last_projected);
            segments.push(Segment {
                sort_pos: sp,
                source_start,
                is_body: false,
                op: EmitOp::Header(sub_table, item, node),
            });

            collect_segments(
                sub_table,
                None,
                Some(node),
                emit,
                segments,
                last_projected,
                SUBS_ONLY,
            );
            continue;
        }

        if item.is_aot() {
            if mode == BODY_ONLY {
                continue;
            }
            let Some(arr) = item.as_array() else {
                continue;
            };
            let node = alloc_prefix(emit.arena, key.name, key.span, section_prefix);
            // Anchor unprojected elements near this AOT's source position.
            if !key.span.is_empty() {
                *last_projected = key.span.start;
            }
            let mut prev_aot_pos: u32 = 0;
            for arr_entry in arr {
                let Some(sub_table) = arr_entry.as_table() else {
                    continue;
                };
                let elem_sort = if arr_entry.meta.array_reordered() {
                    None
                } else {
                    projected_span(arr_entry, emit).map(|s| s.start)
                };
                let elem_source_start = elem_sort.unwrap_or(u32::MAX);
                let mut sp = pack_sort_pos(elem_sort, last_projected);
                // AOT element order is semantic: ensure sort positions are
                // monotonically non-decreasing so that content-based array
                // matching doesn't reorder elements during source-ordered emit.
                if sp < prev_aot_pos {
                    sp = prev_aot_pos;
                }
                prev_aot_pos = sp;

                segments.push(Segment {
                    sort_pos: sp,
                    source_start: elem_source_start,
                    is_body: false,
                    op: EmitOp::AotElement(sub_table, arr_entry, node),
                });
            }
            continue;
        }

        // Body entry
        if mode == SUBS_ONLY {
            continue;
        }
        // Use line_start as source_start so that reassembly gap handling
        // doesn't duplicate indentation that emit_body_entry preserves.
        let sort_key_opt = if !key.span.is_empty() {
            Some(key.span.start)
        } else {
            None
        };
        let ls = if let Some(pos) = sort_key_opt {
            line_start_of(emit.src, pos as usize) as u32
        } else {
            u32::MAX
        };
        let sp = pack_sort_pos(sort_key_opt, last_projected);

        segments.push(Segment {
            sort_pos: sp,
            source_start: ls,
            is_body: true,
            op: EmitOp::Body(key, item, dotted_prefix),
        });
    }
}

/// Emits a header line and body entries only (no subsections) into a buffer.
/// Subsection entries (headers, AOTs) within the table are skipped so they
/// can be collected as independent segments for interleaved sorting.
fn emit_header_body<'a, 'b, 'de: 'b>(
    table: &'a Table<'de>,
    item: &'a Item<'de>,
    prefix: &'b Prefix<'b, 'de>,
    emit: &Emitter<'b, 'de>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    if emit_projected_header_line(item, false, emit, out, cursor) {
        emit_body_ordered(table, emit, out, cursor);
        return;
    }

    // Fallback: formatted header
    write_section_header(prefix, emit, out);
    emit_body_ordered(table, emit, out, cursor);
}

/// Emits body entries from a table in source order, skipping headers and AOTs.
/// Dotted and implicit containers are flattened via `collect_segments` with
/// BODY_ONLY mode. Segments are sorted by source position.
fn emit_body_ordered<'a, 'b, 'de: 'b>(
    table: &'a Table<'de>,
    emit: &Emitter<'b, 'de>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    let mut segments: Vec<Segment<'a, 'b, 'de>> = Vec::new();
    let mut last_projected: u32 = 0;

    collect_segments(
        table,
        None,
        None,
        emit,
        &mut segments,
        &mut last_projected,
        1, // BODY_ONLY
    );

    let order = build_segment_order(&segments, table.meta.ignore_source_order());

    for &entry in &order {
        let seg = &segments[(entry & 0xFFFF_FFFF) as usize];
        let ss = seg.source_start;
        if ss != u32::MAX {
            let target = ss as usize;
            if target >= *cursor {
                emit_gap(emit, *cursor, target, out);
                *cursor = target;
            }
        }
        if let EmitOp::Body(key, item, dotted) = &seg.op {
            emit_body_entry(key, item, *dotted, emit, out, cursor);
        }
    }
}

fn emit_projected_header_line(
    item: &Item<'_>,
    include_comment_prefix: bool,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) -> bool {
    let Some(src_span) = projected_span(item, emit) else {
        return false;
    };
    let hdr_start = src_span.start as usize;
    if hdr_start >= emit.src.len() || emit.src[hdr_start] != b'[' {
        return false;
    }
    if include_comment_prefix {
        let (pstart, pend) = find_comment_prefix(emit.src, hdr_start);
        if pstart != pend {
            out.extend_from_slice(&emit.src[pstart..pend]);
        }
    }
    let hdr_line_end = line_end_of(emit.src, hdr_start);
    let hdr_slice = &emit.src[hdr_start..hdr_line_end];
    out.extend_from_slice(hdr_slice);
    if !hdr_slice.ends_with(b"\n") {
        out.push(b'\n');
    }
    *cursor = hdr_line_end;
    true
}

/// Emits a single AOT element (header + body) without gap/separator handling.
fn emit_aot_element<'a, 'b, 'de: 'b>(
    sub_table: &'a Table<'de>,
    entry: &'a Item<'de>,
    prefix: &'b Prefix<'b, 'de>,
    emit: &Emitter<'b, 'de>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    if emit_projected_header_line(entry, entry.meta.array_reordered(), emit, out, cursor) {
        emit_ordered(sub_table, None, Some(prefix), emit, out, cursor);
        return;
    }

    // Fallback: formatted header
    write_aot_header(prefix, emit, out);
    emit_ordered(sub_table, None, Some(prefix), emit, out, cursor);
}

/// Returns a sort position from an optional source position.
/// Projected entries use their position directly.
/// Unprojected entries inherit the last projected sibling's position,
/// keeping them adjacent during sorting (collection order breaks ties).
fn pack_sort_pos(source_pos: Option<u32>, last_projected: &mut u32) -> u32 {
    if let Some(pos) = source_pos {
        *last_projected = pos;
        pos
    } else {
        *last_projected
    }
}

/// Emits a single body entry (scalar, inline array, frozen table) with
/// forward gap scanning from the cursor.
fn emit_body_entry(
    key: &Key<'_>,
    item: &Item<'_>,
    dotted_prefix: Option<&Prefix<'_, '_>>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
    cursor: &mut usize,
) {
    // Try source-based emission
    if !key.span.is_empty() && projected_span(item, emit).is_some() {
        let key_start = key.span.start as usize;
        let line_start = line_start_of(emit.src, key_start);
        let ahead = line_start >= *cursor;
        if ahead {
            emit_gap(emit, *cursor, line_start, out);
        }
        // Preserve leading whitespace (indentation) before the entry.
        // Only emit spaces/tabs at the start of the line; for dotted
        // keys the range line_start..key_start includes the prefix
        // (e.g. "  z." for key w in "  z.w = 2"), so we stop at the
        // first non-whitespace byte.
        let mut ws_len = 0;
        for &b in &emit.src[line_start..] {
            if b != b' ' && b != b'\t' {
                break;
            }
            ws_len += 1;
        }
        if ws_len > 0 {
            out.extend_from_slice(&emit.src[line_start..line_start + ws_len]);
        }
        if let Some(line_end) = try_emit_entry_from_source(key, item, dotted_prefix, emit, out) {
            *cursor = if ahead {
                line_end
            } else {
                (*cursor).max(line_end)
            };
            return;
        }
        // Source emit failed — fall through to formatted output.
    }

    // Fallback: formatted emission
    write_dotted_key(dotted_prefix, key, emit, out);
    out.extend_from_slice(b" = ");
    format_value(item, emit, out);
    out.push(b'\n');
}

/// Emits a table in formatted mode (no source text available).
/// Body entries first, then subsections — matching the old behavior.
///
/// `section_prefix` is the path used for `[header]` lines. Body entries
/// inside a section always have NO dotted prefix (the header establishes
/// the context).
fn emit_formatted(
    table: &Table<'_>,
    section_prefix: Option<&Prefix<'_, '_>>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) {
    emit_formatted_body(table, None, emit, out);
    emit_formatted_subsections(table, section_prefix, emit, out);
}

/// Emits body entries (scalars, inline arrays, frozen tables, dotted chains)
/// in formatted mode.
fn emit_formatted_body(
    table: &Table<'_>,
    dotted_prefix: Option<&Prefix<'_, '_>>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) {
    for (key, item) in table {
        if item.has_dotted_bit() {
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            let node = Prefix {
                name: key.name,
                key_span: key.span,
                parent: dotted_prefix,
            };
            emit_formatted_body(sub_table, Some(&node), emit, out);
            continue;
        }
        if item.has_header_bit() || item.is_implicit_table() || item.is_aot() {
            continue;
        }

        write_dotted_key(dotted_prefix, key, emit, out);
        out.extend_from_slice(b" = ");
        format_value(item, emit, out);
        out.push(b'\n');
    }
}

/// Emits subsections (headers, implicit tables, AOTs) in formatted mode.
fn emit_formatted_subsections(
    table: &Table<'_>,
    prefix: Option<&Prefix<'_, '_>>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) {
    for (key, item) in table {
        let node = Prefix {
            name: key.name,
            key_span: key.span,
            parent: prefix,
        };
        if item.has_header_bit() {
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            if !out.is_empty() {
                out.push(b'\n');
            }
            write_section_header(&node, emit, out);
            emit_formatted(sub_table, Some(&node), emit, out);
        } else if item.is_implicit_table() || item.has_dotted_bit() {
            let Some(sub_table) = item.as_table() else {
                continue;
            };
            emit_formatted_subsections(sub_table, Some(&node), emit, out);
        } else if item.is_aot() {
            let Some(arr) = item.as_array() else {
                continue;
            };
            for entry in arr {
                let Some(sub_table) = entry.as_table() else {
                    continue;
                };
                if !out.is_empty() {
                    out.push(b'\n');
                }
                write_aot_header(&node, emit, out);
                emit_formatted(sub_table, Some(&node), emit, out);
            }
        }
    }
}

fn write_section_header(prefix: &Prefix<'_, '_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    out.push(b'[');
    write_prefix_path(prefix, emit, out);
    out.extend_from_slice(b"]\n");
}

fn write_aot_header(prefix: &Prefix<'_, '_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    out.extend_from_slice(b"[[");
    write_prefix_path(prefix, emit, out);
    out.extend_from_slice(b"]]\n");
}

fn write_prefix_path(node: &Prefix<'_, '_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    if let Some(parent) = node.parent {
        write_prefix_path(parent, emit, out);
        out.push(b'.');
    }
    emit_key(node.name, node.key_span, emit, out);
}

/// Writes a dotted key with optional prefix: `prefix.key`.
fn write_dotted_key(
    prefix: Option<&Prefix<'_, '_>>,
    key: &Key<'_>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) {
    if let Some(node) = prefix {
        write_prefix_path(node, emit, out);
        out.push(b'.');
    }
    emit_key(key.name, key.span, emit, out);
}

/// Returns the byte offset of the start of the line containing `pos`.
fn line_start_of(src: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && src[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

/// Scans backwards from a header position to find preceding comment lines
/// and blank lines that belong to this element. Returns the byte range
/// `(prefix_start, header_line_start)`. Empty when there are no preceding
/// comment/blank lines.
fn find_comment_prefix(src: &[u8], header_pos: usize) -> (usize, usize) {
    let header_line_start = line_start_of(src, header_pos);
    let mut prefix_start = header_line_start;
    let mut cursor = header_line_start;
    while cursor > 0 {
        let prev_line_start = line_start_of(src, cursor - 1);
        let line = &src[prev_line_start..cursor];
        let mut first_non_ws = None;
        for &b in line {
            if b != b' ' && b != b'\t' {
                first_non_ws = Some(b);
                break;
            }
        }
        match first_non_ws {
            Some(b'#') | Some(b'\n') | Some(b'\r') | None => {
                prefix_start = prev_line_start;
                cursor = prev_line_start;
            }
            _ => break,
        }
    }
    (prefix_start, header_line_start)
}

fn contains_byte(slice: &[u8], needle: u8) -> bool {
    for &b in slice {
        if b == needle {
            return true;
        }
    }
    false
}

fn is_all_whitespace(slice: &[u8]) -> bool {
    for &b in slice {
        if b != b' ' && b != b'\t' {
            return false;
        }
    }
    true
}

/// Scans forward from `pos` past the value, any trailing whitespace and
/// comment, returning the offset just past the `\n` (or end of source).
fn line_end_of(src: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < src.len() && src[i] != b'\n' {
        i += 1;
    }
    if i < src.len() {
        i + 1 // past the \n
    } else {
        i
    }
}

/// Emits only blank lines and comment lines from `src[start..end]`.
///
/// Skips any line that looks like a TOML entry (key-value pair, header, etc.).
/// This prevents leaking source entries that aren't present in dest when the
/// cursor-based gap emission spans over removed entries.
fn emit_gap(emit: &Emitter<'_, '_>, start: usize, end: usize, out: &mut Vec<u8>) {
    let mut i = start;
    while i < end {
        let line_end = line_end_of(emit.src, i).min(end);
        // Check the first non-whitespace byte on this line.
        let mut first_non_ws = None;
        for &b in &emit.src[i..line_end] {
            if b != b' ' && b != b'\t' {
                first_non_ws = Some(b);
                break;
            }
        }
        match first_non_ws {
            // Comment line or blank line (only whitespace/newline) — emit it.
            Some(b'#') | Some(b'\n') | Some(b'\r') | None => {
                out.extend_from_slice(&emit.src[i..line_end]);
            }
            // Anything else is a TOML entry line — skip it.
            _ => {}
        }
        i = line_end;
    }
}

/// Checks if trailing text (after a value, before end-of-line) contains a
/// comma outside of a comment. Commas inside `# ...` comments don't count.
fn has_trailing_comma(trailing: &[u8]) -> bool {
    for &b in trailing {
        if b == b'#' {
            return false;
        }
        if b == b',' {
            return true;
        }
    }
    false
}

/// Returns the projected source item's span for an item, if available.
fn projected_span(item: &Item<'_>, emit: &Emitter<'_, '_>) -> Option<Span> {
    let span = projected_source(item, emit)?.span_unchecked();
    if span.is_empty() {
        return None;
    }
    Some(span)
}

/// Returns the projected source item, if available.
fn projected_source<'a>(item: &Item<'_>, emit: &Emitter<'a, '_>) -> Option<&'a Item<'a>> {
    if emit.src.is_empty() {
        return None;
    }
    item.projected(emit.src_items)
}

/// Tries to emit a body entry line from source text. Returns the line-end
/// offset on success, or `None` if the entry can't be emitted from source.
///
/// Emits: key from source, `source[key_end..val_start]` (preserving ` = `
/// whitespace), the value via `format_value` (which handles full/partial
/// container match correctly), then trailing whitespace/comment from source.
fn try_emit_entry_from_source(
    key: &Key<'_>,
    item: &Item<'_>,
    dotted_prefix: Option<&Prefix<'_, '_>>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) -> Option<usize> {
    if emit.src.is_empty() || key.span.is_empty() {
        return None;
    }
    let val_span = projected_span(item, emit)?;

    let key_end = key.span.end as usize;
    let val_start = val_span.start as usize;
    let val_end = val_span.end as usize;
    if key_end > val_start || val_end > emit.src.len() {
        return None;
    }

    // Emit the key path (prefix + leaf key) from source spans
    write_dotted_key(dotted_prefix, key, emit, out);
    // Emit source from after key to start of value (preserves ` = ` whitespace)
    out.extend_from_slice(&emit.src[key_end..val_start]);
    // Emit value via format_value (handles full/partial container match)
    format_value(item, emit, out);
    // Scan forward past trailing whitespace/comment to newline
    let line_end = line_end_of(emit.src, val_end);
    let trailing = &emit.src[val_end..line_end];
    out.extend_from_slice(trailing);
    // Ensure newline-terminated output for idempotency
    // (source entries at EOF may lack a trailing newline).
    if !trailing.ends_with(b"\n") {
        out.push(b'\n');
    }
    Some(line_end)
}

/// Tries to emit a partially-changed multiline array by preserving source
/// lines for unchanged elements and formatting new/changed elements.
///
/// Handles additions (anywhere), removals, and value changes. Unchanged
/// elements keep their original formatting, whitespace, and trailing comments.
/// Single-line arrays fall through to `format_array`.
/// Emits a preserved element from source, including trailing comma fixup.
/// `from` is the byte position to start copying (usually line_start_of the element).
/// `val_end` is the byte position past the end of the value.
fn emit_preserved_with_comma(
    emit: &Emitter<'_, '_>,
    from: usize,
    val_end: usize,
    out: &mut Vec<u8>,
) {
    out.extend_from_slice(&emit.src[from..val_end]);
    let le = line_end_of(emit.src, val_end);
    let trailing = &emit.src[val_end..le];
    if !has_trailing_comma(trailing) {
        out.push(b',');
    }
    out.extend_from_slice(trailing);
}

fn try_emit_array_partial(
    dest: &Array<'_>,
    arr_span: Span,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) -> bool {
    let arr_start = arr_span.start as usize;
    let arr_end = arr_span.end as usize;
    if arr_end == 0 || arr_end > emit.src.len() || emit.src[arr_end - 1] != b']' {
        return false;
    }

    // Only handle multiline; single-line falls through to format_array
    if !contains_byte(&emit.src[arr_start..arr_end], b'\n') {
        return false;
    }

    let dest_slice = dest.as_slice();
    if dest_slice.is_empty() {
        return false;
    }

    // Detect indentation from the first projected element's source line
    let indent = 'indent: {
        for elem in dest_slice {
            if let Some(sp) = projected_span(elem, emit) {
                let elem_start = sp.start as usize;
                let candidate = &emit.src[line_start_of(emit.src, elem_start)..elem_start];
                // Element on same line as opening bracket: indent contains
                // non-whitespace (e.g. key prefix). Bail out to format_array.
                if !is_all_whitespace(candidate) {
                    return false;
                }
                break 'indent candidate;
            }
        }
        return false; // No projected elements → can't detect formatting
    };

    // Emit opening: `[` to end of its line
    out.extend_from_slice(&emit.src[arr_start..line_end_of(emit.src, arr_start)]);

    for elem in dest_slice {
        if is_fully_projected(elem, emit) {
            let val_span = projected_span(elem, emit).unwrap();
            emit_preserved_with_comma(
                emit,
                line_start_of(emit.src, val_span.start as usize),
                val_span.end as usize,
                out,
            );
        } else {
            out.extend_from_slice(indent);
            let depth = (indent.len() / emit.indent.width()) as u32;
            format_value_at(elem, emit, out, depth);
            out.extend_from_slice(b",\n");
        }
    }

    // Emit closing `]` with source indentation
    let bracket = arr_end - 1;
    out.extend_from_slice(&emit.src[line_start_of(emit.src, bracket)..arr_end]);
    true
}

/// Tries to emit a partially-changed multiline inline table by preserving
/// source lines for unchanged entries and formatting new/changed entries.
///
/// Handles additions (anywhere), removals, and value changes. Unchanged
/// entries keep their original formatting, whitespace, and trailing comments.
/// Single-line inline tables fall through to `format_inline_table`.
fn try_emit_inline_table_partial(
    dest: &Table<'_>,
    tab_span: Span,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
) -> bool {
    let tab_start = tab_span.start as usize;
    let tab_end = tab_span.end as usize;
    if tab_end == 0 || tab_end > emit.src.len() || emit.src[tab_end - 1] != b'}' {
        return false;
    }

    // Only handle multiline; single-line falls through to format_inline_table
    if !contains_byte(&emit.src[tab_start..tab_end], b'\n') {
        return false;
    }

    let entries = dest.entries();
    if entries.is_empty() {
        return false;
    }

    // Detect indentation from the first projected entry's source line
    let indent = 'indent: {
        for (key, val) in entries {
            if !key.span.is_empty() && projected_span(val, emit).is_some() {
                let k = key.span.start as usize;
                let candidate = &emit.src[line_start_of(emit.src, k)..k];
                // Entry on same line as opening brace: indent contains
                // non-whitespace. Bail out to format_inline_table.
                if !is_all_whitespace(candidate) {
                    return false;
                }
                break 'indent candidate;
            }
        }
        return false; // No projected entries → can't detect formatting
    };

    // Collect all leaf entries, flattening dotted chains so that
    // `b.c.d = 4` and `b.f = [1, 2]` are emitted as separate lines
    // rather than collapsed into `b = { c.d = 4, f = [1, 2] }`.
    let (leaves, order) = collect_and_sort_leaves(dest, emit.arena);

    // Emit opening: `{` to end of its line
    out.extend_from_slice(&emit.src[tab_start..line_end_of(emit.src, tab_start)]);

    for &entry in &order {
        let leaf = &leaves[(entry & 0xFFFF_FFFF) as usize];
        let key = leaf.key;
        let val = leaf.item;
        if !key.span.is_empty() && is_fully_projected(val, emit) {
            let val_span = projected_span(val, emit).unwrap();
            emit_preserved_with_comma(
                emit,
                line_start_of(emit.src, key.span.start as usize),
                val_span.end as usize,
                out,
            );
        } else {
            out.extend_from_slice(indent);
            let depth = (indent.len() / emit.indent.width()) as u32;
            write_inline_leaf_key(leaf, emit, out);
            out.extend_from_slice(b" = ");
            format_value_at(val, emit, out, depth);
            out.extend_from_slice(b",\n");
        }
    }

    // Emit closing `}` with source indentation
    let brace = tab_end - 1;
    out.extend_from_slice(&emit.src[line_start_of(emit.src, brace)..tab_end]);
    true
}

/// Emits a key, using the original source text when the span is valid.
fn emit_key(name: &str, span: Span, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    if !span.is_empty() && !emit.src.is_empty() {
        out.extend_from_slice(&emit.src[span.range()]);
    } else {
        format_key(name, out);
    }
}

/// Returns the original source bytes for a projected scalar item,
/// or `None` if projection is unavailable or the item is unmatched.
fn projected_text<'a>(item: &Item<'_>, emit: &Emitter<'a, '_>) -> Option<&'a [u8]> {
    if emit.src.is_empty() {
        return None;
    }
    let src_item = item.projected(emit.src_items)?;
    let span = src_item.span_unchecked();
    if span.is_empty() {
        return None;
    }
    Some(&emit.src[span.range()])
}

/// Checks if an item is fully projected — not just container-matched.
///
/// O(1) full-match check using the flag set during reprojection.
fn is_fully_projected(item: &Item<'_>, emit: &Emitter<'_, '_>) -> bool {
    if emit.src_items.is_empty() {
        return false;
    }
    item.is_reprojected_full_match()
}

fn format_value(item: &Item<'_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    format_value_at(item, emit, out, 0);
}

fn format_value_at(item: &Item<'_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>, depth: u32) {
    match item.value() {
        Value::Array(arr) => {
            if let Some(src_item) = projected_source(item, emit) {
                if let Some(src_arr) = src_item.as_array() {
                    // AOT spans cover [[header]] lines, not [elem, ...] brackets.
                    // Skip source-based emit when source is AOT but dest is inline.
                    if src_arr.style() == ArrayStyle::Inline {
                        let span = src_item.span_unchecked();
                        if is_fully_projected(item, emit) {
                            out.extend_from_slice(&emit.src[span.range()]);
                            return;
                        }
                        // Partial match: try preserving unchanged elements from source
                        if try_emit_array_partial(arr, span, emit, out) {
                            return;
                        }
                    }
                }
            }
            if arr.is_expanded() {
                format_expanded_array(arr, emit, out, depth);
            } else {
                format_array(arr, emit, out);
            }
        }
        Value::Table(tab) => {
            if let Some(src_item) = projected_source(item, emit) {
                if let Some(src_tab) = src_item.as_table() {
                    // Only inline (frozen) table spans have {…} format.
                    // HEADER/IMPLICIT/DOTTED spans are incompatible.
                    if src_tab.style() == TableStyle::Inline {
                        let span = src_item.span_unchecked();
                        if is_fully_projected(item, emit) {
                            out.extend_from_slice(&emit.src[span.range()]);
                            return;
                        }
                        // Partial match: try preserving unchanged entries from source
                        if try_emit_inline_table_partial(tab, span, emit, out) {
                            return;
                        }
                    }
                }
            }
            format_inline_table(tab, emit, out, depth);
        }
        _ => {
            if let Some(text) = projected_text(item, emit) {
                out.extend_from_slice(text);
                return;
            }
            format_scalar(item, out);
        }
    }
}

fn format_scalar(item: &Item<'_>, out: &mut Vec<u8>) {
    match item.value() {
        Value::String(s) => format_string(s, out),
        Value::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => format_float(*f, out),
        Value::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::DateTime(dt) => {
            let mut buf = MaybeUninit::uninit();
            out.extend_from_slice(dt.format(&mut buf).as_bytes());
        }
        _ => {}
    }
}

/// Leaf entry collected from an inline table for reordering.
#[derive(Clone, Copy)]
struct InlineLeaf<'a, 'de> {
    key: &'a Key<'de>,
    item: &'a Item<'de>,
    /// Arena-allocated prefix chain from root to leaf's dotted parent.
    prefix: &'a [(&'de str, Span)],
    /// Source position for sorting (or inherited from last projected sibling).
    sort_pos: u32,
}

/// Arena-allocates a new prefix slice that extends `prefix` with one element.
fn arena_extend_prefix<'a, 'de>(
    arena: &'a Arena,
    prefix: &[(&'de str, Span)],
    name: &'de str,
    span: Span,
) -> &'a [(&'de str, Span)] {
    let new_len = prefix.len() + 1;
    let byte_size = new_len * std::mem::size_of::<(&str, Span)>();
    let ptr = arena.alloc(byte_size);
    let slice_ptr = ptr.as_ptr() as *mut (&'de str, Span);
    // SAFETY:
    // - `byte_size` is `new_len * size_of::<(&str, Span)>()`, so the arena
    //   allocation is large enough for `new_len` elements.
    // - `(&str, Span)` has align <= 8, matching ALLOC_ALIGN.
    // - The copy writes `prefix.len()` elements from the old slice (disjoint
    //   from the fresh arena allocation), then one more element at the end.
    // - After both writes, all `new_len` elements are initialized.
    unsafe {
        std::ptr::copy_nonoverlapping(prefix.as_ptr(), slice_ptr, prefix.len());
        std::ptr::write(slice_ptr.add(prefix.len()), (name, span));
        std::slice::from_raw_parts(slice_ptr as *const (&'de str, Span), new_len)
    }
}

/// Collects all leaf entries from an inline table, recursing through dotted chains.
fn collect_inline_leaves<'a, 'de>(
    table: &'a Table<'de>,
    prefix_chain: &'a [(&'de str, Span)],
    leaves: &mut Vec<InlineLeaf<'a, 'de>>,
    last_pos: &mut u32,
    arena: &'a Arena,
) {
    for (key, val) in table {
        if let Some(sub) = val.as_table() {
            if val.has_dotted_bit() || sub.style() == TableStyle::Dotted {
                let chain = arena_extend_prefix(arena, prefix_chain, key.name, key.span);
                if !key.span.is_empty() {
                    *last_pos = key.span.start;
                }
                collect_inline_leaves(sub, chain, leaves, last_pos, arena);
                continue;
            }
        }
        if !key.span.is_empty() {
            *last_pos = key.span.start;
        }
        leaves.push(InlineLeaf {
            key,
            item: val,
            prefix: prefix_chain,
            sort_pos: *last_pos,
        });
    }
}

/// Writes the prefix chain and leaf key for an inline leaf entry.
fn write_inline_leaf_key(leaf: &InlineLeaf<'_, '_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    let mut first = true;
    for &(name, span) in leaf.prefix {
        if !first {
            out.push(b'.');
        }
        first = false;
        emit_key(name, span, emit, out);
    }
    if !leaf.prefix.is_empty() {
        out.push(b'.');
    }
    emit_key(leaf.key.name, leaf.key.span, emit, out);
}

/// Single sort monomorphization for all emit ordering.
/// Each u64 packs sort-relevant bits in the high bits and the original
/// array index in the low 32 bits.
fn sort_index(order: &mut [u64]) {
    order.sort_unstable();
}

/// Collects inline leaves from a table, sorts by source position, returns
/// (leaves, sorted order indices). The arena must outlive the returned vecs.
fn collect_and_sort_leaves<'a, 'de>(
    table: &'a Table<'de>,
    arena: &'a Arena,
) -> (Vec<InlineLeaf<'a, 'de>>, Vec<u64>) {
    let mut leaves = Vec::new();
    let mut last_pos = 0u32;
    collect_inline_leaves(table, &[], &mut leaves, &mut last_pos, arena);
    let mut order: Vec<u64> = Vec::with_capacity(leaves.len());
    let mut i = 0u64;
    for leaf in &leaves {
        order.push(((leaf.sort_pos as u64) << 32) | i);
        i += 1;
    }
    sort_index(&mut order);
    (leaves, order)
}

fn format_inline_table(tab: &Table<'_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>, depth: u32) {
    if tab.is_empty() {
        out.extend_from_slice(b"{}");
        return;
    }

    if !emit.src_items.is_empty() && !tab.meta.ignore_source_order() {
        let mut needs_sort = false;
        for (k, v) in tab.entries() {
            if v.has_dotted_bit() || !k.span.is_empty() {
                needs_sort = true;
                break;
            }
            if let Some(t) = v.as_table() {
                if t.style() == TableStyle::Dotted {
                    needs_sort = true;
                    break;
                }
            }
        }
        if needs_sort {
            let (leaves, order) = collect_and_sort_leaves(tab, emit.arena);
            out.extend_from_slice(b"{ ");
            let mut first = true;
            for &entry in &order {
                let leaf = &leaves[(entry & 0xFFFF_FFFF) as usize];
                if !first {
                    out.extend_from_slice(b", ");
                }
                first = false;
                write_inline_leaf_key(leaf, emit, out);
                out.extend_from_slice(b" = ");
                format_value_at(leaf.item, emit, out, depth);
            }
            out.extend_from_slice(b" }");
            return;
        }
    }

    out.extend_from_slice(b"{ ");
    let mut first = true;
    for (key, val) in tab {
        if let Some(sub) = val.as_table() {
            if val.has_dotted_bit() || sub.style() == TableStyle::Dotted {
                let node = Prefix {
                    name: key.name,
                    key_span: key.span,
                    parent: None,
                };
                format_inline_dotted_kv(sub, &node, emit, out, &mut first, depth);
                continue;
            }
        }
        if !first {
            out.extend_from_slice(b", ");
        }
        first = false;
        emit_key(key.name, key.span, emit, out);
        out.extend_from_slice(b" = ");
        format_value_at(val, emit, out, depth);
    }
    out.extend_from_slice(b" }");
}

fn format_inline_dotted_kv(
    table: &Table<'_>,
    prefix: &Prefix<'_, '_>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
    first: &mut bool,
    depth: u32,
) {
    for (key, val) in table {
        if let Some(sub) = val.as_table() {
            if val.has_dotted_bit() || sub.style() == TableStyle::Dotted {
                let node = Prefix {
                    name: key.name,
                    key_span: key.span,
                    parent: Some(prefix),
                };
                format_inline_dotted_kv(sub, &node, emit, out, first, depth);
                continue;
            }
        }
        if !*first {
            out.extend_from_slice(b", ");
        }
        *first = false;
        write_prefix_path(prefix, emit, out);
        out.push(b'.');
        emit_key(key.name, key.span, emit, out);
        out.extend_from_slice(b" = ");
        format_value_at(val, emit, out, depth);
    }
}

fn format_expanded_array(
    arr: &Array<'_>,
    emit: &Emitter<'_, '_>,
    out: &mut Vec<u8>,
    depth: u32,
) {
    if arr.is_empty() {
        out.extend_from_slice(b"[]");
        return;
    }
    out.extend_from_slice(b"[\n");
    let child = depth + 1;
    for elem in arr {
        emit.indent.write(out, child);
        format_value_at(elem, emit, out, child);
        out.extend_from_slice(b",\n");
    }
    emit.indent.write(out, depth);
    out.push(b']');
}

fn format_array(arr: &Array<'_>, emit: &Emitter<'_, '_>, out: &mut Vec<u8>) {
    if arr.is_empty() {
        out.extend_from_slice(b"[]");
        return;
    }
    out.push(b'[');
    let mut first = true;
    for elem in arr {
        if !first {
            out.extend_from_slice(b", ");
        }
        first = false;
        format_value(elem, emit, out);
    }
    out.push(b']');
}

fn format_key(name: &str, out: &mut Vec<u8>) {
    if is_bare_key(name) {
        out.extend_from_slice(name.as_bytes());
    } else {
        format_string(name, out);
    }
}

fn is_bare_key(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for b in name.bytes() {
        if !b.is_ascii_alphanumeric() && b != b'-' && b != b'_' {
            return false;
        }
    }
    true
}

fn format_string(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\u{0008}' => out.extend_from_slice(b"\\b"),
            '\u{000C}' => out.extend_from_slice(b"\\f"),
            c if c < '\x20' || c == '\x7F' => {
                let val = c as u32;
                let hex = b"0123456789ABCDEF";
                out.extend_from_slice(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    hex[(val >> 4) as usize & 0xF],
                    hex[val as usize & 0xF],
                ]);
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

fn format_float(f: f64, out: &mut Vec<u8>) {
    if f.is_nan() {
        out.extend_from_slice(if f.is_sign_positive() {
            b"nan"
        } else {
            b"-nan"
        });
    } else if f.is_infinite() {
        out.extend_from_slice(if f > 0.0 { b"inf" } else { b"-inf" });
    } else {
        let mut buffer = zmij::Buffer::new();
        out.extend_from_slice(buffer.format(f).as_bytes());
    }
}
