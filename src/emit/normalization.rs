use crate::emit::partition::ensure_body_order;
use crate::{Array, ArrayStyle, Item, Kind, Table, TableStyle, ValueMut};
impl<'de> Table<'de> {
    fn normalize_inner(&mut self) -> &'de NormalizedTable<'de> {
        for (_, item) in self.value.entries_mut().iter_mut() {
            normalize_item(item, true);
        }
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
    #[cfg(not(fuzzing))]
    pub(crate) fn normalize(&mut self) -> &'de NormalizedTable<'de> {
        self.normalize_inner()
    }

    /// Recursively corrects table and array kinds so the tree is valid
    /// for emission.
    ///
    /// Promotes implicit tables to headers when they contain values that
    /// would otherwise be unreachable, downgrades invalid
    /// array-of-tables to inline arrays, and fixes kind mismatches in
    /// nested contexts (e.g. a header table inside an inline table).
    #[cfg(fuzzing)]
    pub fn normalize(&mut self) -> &'de NormalizedTable<'de> {
        self.normalize_inner()
    }

    /// Checks whether this table tree is already valid for emission
    /// without modifying it.
    ///
    /// Returns `Some` if every item is reachable by the emit algorithm,
    /// `None` otherwise. Use [`normalize`](Self::normalize) to fix an
    /// invalid tree instead.
    #[cfg(not(fuzzing))]
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

    /// Checks whether this table tree is already valid for emission
    /// without modifying it.
    ///
    /// Returns `Some` if every item is reachable by the emit algorithm,
    /// `None` otherwise. Use [`normalize`](Self::normalize) to fix an
    /// invalid tree instead.
    #[cfg(fuzzing)]
    pub fn try_as_normalized(&self) -> Option<&NormalizedTable<'de>> {
        if is_valid(self, true) {
            // SAFETY: NormalizedTable is #[repr(transparent)] over Table.
            // The validation confirmed the tree is emit-safe.
            Some(unsafe { &*(self as *const Table<'de> as *const NormalizedTable<'de>) })
        } else {
            None
        }
    }
}

/// A [`Table`] that has been validated or normalized for emission.
///
/// Obtained via [`Table::normalize`] (mutating fix-up) or
/// [`Table::try_as_normalized`] (read-only validation). Pass to
/// [`emit`](crate::emit) to produce TOML text.
#[repr(transparent)]
pub struct NormalizedTable<'de>(Table<'de>);

impl<'de> NormalizedTable<'de> {
    /// Returns a reference to the inner table.
    pub fn table(&self) -> &Table<'de> {
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

pub(crate) fn normalize_item(item: &mut Item<'_>, body_emitted: bool) -> bool {
    match item.value_mut() {
        ValueMut::Table(sub) => {
            let has_body = normalize_table(sub, body_emitted);
            ensure_body_order(sub.value.entries_mut());
            has_body
        }
        ValueMut::Array(arr) => normalize_array(arr),
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
            for (_, child) in sub.value.entries_mut().iter_mut() {
                normalize_item(child, true);
            }
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

    let mut has_body = false;

    for (_, child) in sub.value.entries_mut().iter_mut() {
        has_body |= normalize_item(child, effective_body);
    }

    if !effective_body && has_body {
        sub.set_style(TableStyle::Header);
        // We initially processed children with `effective_body = false`.
        // Now that we've become a Header, the correct context is `true`.
        // Re-normalize to ensure any nested items adopt the correct kind
        // (e.g., staying Dotted instead of becoming Header).
        for (_, child) in sub.value.entries_mut().iter_mut() {
            normalize_item(child, true);
        }
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
                for (_, child) in sub.value.entries_mut().iter_mut() {
                    normalize_item(child, true);
                }
                ensure_body_order(sub.value.entries_mut());
            }
            false
        }
        ArrayStyle::Inline => {
            for elem in &mut *arr {
                normalize_inline(elem, false);
            }
            true
        }
    }
}
