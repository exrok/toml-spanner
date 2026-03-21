use crate::emit::partition::ensure_body_order;
use crate::{Array, ArrayStyle, Item, Kind, Table, TableStyle, Value, ValueMut};
impl<'de> Table<'de> {
    fn normalize_inner(&mut self) -> &'de NormalizedTable<'de> {
        normalize_entries(self.value.entries_mut(), true);
        ensure_body_order(self.value.entries_mut());
        // SAFETY: NormalizedTable is #[repr(transparent)] over Table, so the
        // cast preserves layout. The normalize_item calls above have ensured
        // every item in the tree is reachable by the emit algorithm. The
        // NormalizedTable wrapper is a marker type that encodes this guarantee
        // at the type level.
        unsafe { &*(self as *const Table<'de> as *const NormalizedTable<'de>) }
    }

    /// Recursively corrects table and array kinds so the tree is valid
    /// for emission.
    ///
    /// Promotes implicit tables to headers when they contain values that
    /// would otherwise be unreachable, downgrades invalid
    /// array-of-tables to inline arrays, and fixes kind mismatches in
    /// nested contexts (e.g. a header table inside an inline table).
    pub(crate) fn normalize(&mut self) -> &'de NormalizedTable<'de> {
        self.normalize_inner()
    }

    /// Checks whether this table tree is already valid for emission
    /// without modifying it.
    ///
    /// Returns `Some` if every item is reachable by the emit algorithm,
    /// `None` otherwise. Use [`normalize`](Self::normalize) to fix an
    /// invalid tree instead.
    #[allow(dead_code)]
    pub(crate) fn try_as_normalized(&self) -> Option<&NormalizedTable<'de>> {
        if is_valid(self, true) {
            // SAFETY: NormalizedTable is #[repr(transparent)] over Table.
            // The validation confirmed the tree is emit-safe.
            Some(unsafe { &*(self as *const Table<'de> as *const NormalizedTable<'de>) })
        } else {
            None
        }
    }
}

#[repr(transparent)]
pub(crate) struct NormalizedTable<'de>(Table<'de>);

impl<'de> NormalizedTable<'de> {
    pub(crate) fn table(&self) -> &Table<'de> {
        &self.0
    }
}

impl<'de> std::ops::Deref for NormalizedTable<'de> {
    type Target = Table<'de>;
    fn deref(&self) -> &Table<'de> {
        &self.0
    }
}

/// Normalizes an item inside a frozen (inline/dotted) context.
/// `allow_dotted` is true for named table entries (dotted keys like `{a.b = 1}`
/// are valid), false for positional items like array elements (no key to dot).
fn normalize_inline(item: &mut Item<'_>, allow_dotted: bool) {
    match item.value_mut() {
        ValueMut::Table(sub) => {
            if allow_dotted {
                match sub.style() {
                    TableStyle::Inline | TableStyle::Dotted => {}
                    _ => sub.set_style(TableStyle::Inline),
                }
            } else {
                sub.set_style(TableStyle::Inline);
            }
            let is_dotted = sub.style() == TableStyle::Dotted;
            for (_, child) in sub.value.entries_mut().iter_mut() {
                normalize_inline(child, true);
            }
            // Empty dotted tables emit nothing; promote to frozen so
            // they appear as `key = {}`.
            if is_dotted && sub.value.is_empty() {
                sub.set_style(TableStyle::Inline);
            }
        }
        ValueMut::Array(arr) => {
            arr.set_style(ArrayStyle::Inline);
            arr.clear_expanded();
            for elem in &mut *arr {
                normalize_inline(elem, false);
            }
        }
        _ => {}
    }
}

/// Returns `true` if every item in the table tree is reachable by the
/// emit algorithm. Mirror of the old `validate_table` but returns bool.
#[allow(dead_code)]
fn is_valid(table: &Table<'_>, body_emitted: bool) -> bool {
    for (_, item) in table {
        if item.meta.is_auto_style() {
            return false;
        }
        if item.has_dotted_bit() {
            let Some(sub) = item.as_table() else {
                return false;
            };
            if !is_valid(sub, body_emitted) {
                return false;
            }
        } else if item.has_header_bit() {
            let Some(sub) = item.as_table() else {
                return false;
            };
            if !is_valid(sub, true) {
                return false;
            }
        } else if item.is_implicit_table() {
            let Some(sub) = item.as_table() else {
                return false;
            };
            if !is_valid(sub, false) {
                return false;
            }
        } else if item.is_aot() {
            let Some(arr) = item.as_array() else {
                return false;
            };
            for elem in arr {
                let Some(sub) = elem.as_table() else {
                    return false;
                };
                if !is_valid(sub, true) {
                    return false;
                }
            }
        } else if !body_emitted {
            return false;
        }
    }
    true
}

fn is_small_value(item: &Item<'_>) -> bool {
    match item.value() {
        Value::String(text) => {
            if text.len() > 30 {
                return false;
            }
            for byte in text.as_bytes() {
                if byte.is_ascii_control() {
                    return false;
                }
            }
            true
        }
        Value::Array(array) => array.is_empty(),
        Value::Table(table) => table.is_empty(),
        _ => true,
    }
}

fn resolve_auto_table(sub: &mut Table<'_>, body_emitted: bool) {
    if !sub.meta.is_auto_style() {
        return;
    }
    sub.meta.clear_auto_style();
    if !body_emitted {
        return;
    }

    match sub.entries() {
        [(_, a), (_, b)] if is_small_value(a) && is_small_value(b) => (),
        [(_, a)] if is_small_value(a) => (),
        [] => (),
        _ => return,
    };

    sub.set_style(TableStyle::Inline);
}

/// Estimates the inline rendered width of a value in characters.
/// Returns `None` if the value cannot be reasonably inlined (e.g.
/// non-empty nested containers).
fn inline_width(item: &Item<'_>) -> Option<usize> {
    match item.value() {
        Value::String(s) => {
            if s.len() > 40 {
                return None;
            }
            for &b in s.as_bytes() {
                if b.is_ascii_control() {
                    return None;
                }
            }
            Some(s.len() + 2) // quotes
        }
        Value::Integer(_) => Some(6), // conservative estimate
        Value::Float(_) => Some(8),   // conservative estimate
        Value::Boolean(b) => Some(if *b { 4 } else { 5 }),
        Value::DateTime(_) => Some(25), // conservative estimate
        Value::Array(a) if a.is_empty() => Some(2),
        Value::Table(t) if t.is_empty() => Some(2),
        _ => None,
    }
}

fn resolve_auto_array(arr: &mut Array<'_>) {
    if !arr.meta.is_auto_style() {
        return;
    }
    arr.meta.clear_auto_style();

    if arr.is_empty() {
        arr.set_style(ArrayStyle::Inline);
        return;
    }

    let all_tables = arr.iter().all(|e| e.kind() == Kind::Table);
    if all_tables {
        arr.set_style(ArrayStyle::Header);
        return;
    }

    // Estimate inline width: `[` + elements + `, ` separators + `]`
    let mut width: usize = 2; // brackets
    let mut fits_inline = true;
    for (i, elem) in arr.iter().enumerate() {
        if i > 0 {
            width += 2; // ", "
        }
        let Some(w) = inline_width(elem) else {
            fits_inline = false;
            break;
        };
        width += w;
        if width > 40 {
            fits_inline = false;
            break;
        }
    }
    if fits_inline {
        arr.set_style(ArrayStyle::Inline);
    } else {
        arr.set_expanded();
        arr.set_style(ArrayStyle::Inline);
    }
}

fn normalize_entries(entries: &mut [(crate::Key<'_>, Item<'_>)], body_emitted: bool) -> bool {
    let mut has_body = false;
    for (_, item) in entries.iter_mut() {
        has_body |= normalize_item(item, body_emitted);
    }
    has_body
}

fn renormalize_promoted_header_children(entries: &mut [(crate::Key<'_>, Item<'_>)]) {
    for (_, item) in entries.iter_mut() {
        if item.as_table().is_some() {
            normalize_item(item, true);
        }
    }
}

pub(crate) fn normalize_item(item: &mut Item<'_>, body_emitted: bool) -> bool {
    match item.value_mut() {
        ValueMut::Table(sub) => {
            resolve_auto_table(sub, body_emitted);
            let has_body = normalize_table(sub, body_emitted);
            ensure_body_order(sub.value.entries_mut());
            has_body
        }
        ValueMut::Array(arr) => {
            resolve_auto_array(arr);
            normalize_array(arr)
        }
        _ => true,
    }
}

fn normalize_table(sub: &mut Table<'_>, body_emitted: bool) -> bool {
    let kind = sub.style();

    match kind {
        TableStyle::Inline => {
            for (_, item) in sub.value.entries_mut().iter_mut() {
                normalize_inline(item, true);
            }
            return true;
        }
        TableStyle::Header => {
            normalize_entries(sub.value.entries_mut(), true);
            return false;
        }
        _ => {}
    }

    // TableKind::Implicit or TableKind::Dotted.
    // Dotted tables inherit body_emitted from parent; implicit always false.
    let effective_body = kind == TableStyle::Dotted && body_emitted;

    // Empty implicit/dotted tables produce no emit output (no body items,
    // no subsections). Promote to header so they appear as `[section]`.
    // Exception: empty Dotted in a body context stays as Inline (`key = {}`)
    // to preserve body-level ordering.
    if sub.value.is_empty() {
        if effective_body {
            sub.set_style(TableStyle::Inline);
            return true;
        } else {
            sub.set_style(TableStyle::Header);
            return false;
        }
    }

    let mut has_body = normalize_entries(sub.value.entries_mut(), effective_body);

    if !effective_body && has_body {
        sub.set_style(TableStyle::Header);
        // Promotion to Header only changes context-sensitive descendants
        // reachable through table children; arrays and scalars are already
        // normalized identically in either context.
        renormalize_promoted_header_children(sub.value.entries_mut());
        return false;
    }

    // After normalizing children, a DOTTED table whose children were all
    // promoted to subsection items (HEADER/IMPLICIT/AOT) has no body to
    // emit through the dotted-key path. Try demoting children to
    // body-level first to preserve the DOTTED kind. Only promote to
    // IMPLICIT if demotion doesn't produce body items.
    let no_body_after_norm = if kind != TableStyle::Dotted {
        false
    } else if !effective_body {
        true // We didn't return from the Header promotion, so has_body is false
    } else {
        !has_body
    };

    if no_body_after_norm {
        let mut demoted_has_body = false;
        for (_, child) in sub.value.entries_mut().iter_mut() {
            match child.value_mut() {
                ValueMut::Table(ct) => {
                    if matches!(ct.style(), TableStyle::Header | TableStyle::Implicit) {
                        if ct.is_empty() {
                            ct.set_style(TableStyle::Inline);
                            demoted_has_body = true;
                        } else {
                            ct.set_style(TableStyle::Dotted);
                            demoted_has_body |= normalize_table(ct, body_emitted);
                        }
                    } else {
                        demoted_has_body = true;
                    }
                }
                ValueMut::Array(arr) => {
                    if arr.style() == ArrayStyle::Header {
                        arr.set_expanded();
                        arr.set_style(ArrayStyle::Inline);
                        demoted_has_body |= normalize_array(arr);
                    } else {
                        demoted_has_body = true;
                    }
                }
                _ => {
                    demoted_has_body = true;
                }
            }
        }
        if !demoted_has_body {
            sub.set_style(TableStyle::Implicit);
            has_body = false;
        } else {
            has_body = true;
        }
    }

    has_body
}

/// Normalizes an element inside an expanded array.
///
/// Like `normalize_inline` but allows nested arrays to stay expanded.
/// `allow_dotted` is true for named table entries, false for array elements.
fn normalize_expanded_child(item: &mut Item<'_>, allow_dotted: bool) {
    match item.value_mut() {
        ValueMut::Table(sub) => {
            if allow_dotted {
                match sub.style() {
                    TableStyle::Inline | TableStyle::Dotted => {}
                    _ => sub.set_style(TableStyle::Inline),
                }
            } else {
                sub.set_style(TableStyle::Inline);
            }
            for (_, child) in sub.value.entries_mut().iter_mut() {
                normalize_expanded_child(child, true);
            }
        }
        ValueMut::Array(arr) => {
            resolve_auto_array(arr);
            normalize_array(arr);
        }
        _ => {}
    }
}

fn normalize_array(arr: &mut Array<'_>) -> bool {
    let mut kind = arr.style();

    // AOT must be non-empty with all-table elements; otherwise downgrade.
    if let ArrayStyle::Header = kind {
        let mut all_tables = !arr.is_empty();
        for e in arr.iter() {
            if e.kind() != Kind::Table {
                all_tables = false;
                break;
            }
        }
        if !all_tables {
            kind = ArrayStyle::Inline;
            arr.set_expanded();
        }
    }
    arr.set_style(kind);

    match kind {
        ArrayStyle::Header => {
            for elem in &mut *arr {
                let ValueMut::Table(sub) = elem.value_mut() else {
                    continue;
                };
                sub.set_style(TableStyle::Header);
                normalize_entries(sub.value.entries_mut(), true);
                ensure_body_order(sub.value.entries_mut());
            }
            false
        }
        ArrayStyle::Inline if arr.is_expanded() => {
            for elem in &mut *arr {
                normalize_expanded_child(elem, false);
            }
            true
        }
        ArrayStyle::Inline => {
            for elem in &mut *arr {
                normalize_inline(elem, false);
            }
            true
        }
    }
}
