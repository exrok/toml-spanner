use super::array::Array;
use super::table::Table;
use super::{FLAG_MASK, HINTS_BIT, Item, ItemMetadata, NOT_PROJECTED, TAG_MASK, TAG_SHIFT};

/// Bit 30 of `end_and_flag`: marks a full match during reprojection.
pub(crate) const FULL_MATCH_BIT: u32 = 1 << 30;
/// Bit 29 of `end_and_flag`: when set in format-hints mode, disables
/// source-position reordering for this table's immediate entries.
pub(crate) const IGNORE_SOURCE_ORDER_BIT: u32 = 1 << 29;
/// Bit 28 of `end_and_flag`: when set in format-hints mode, disables
/// copying structural styles from source during reprojection.
pub(crate) const IGNORE_SOURCE_STYLE_BIT: u32 = 1 << 28;
/// Bit 27 of `end_and_flag`: marks an array element as reordered during
/// reprojection. Prevents the emitter from sorting this element back to
/// its original source position in the parent array, without affecting
/// source-ordering of the element's own children.
pub(crate) const ARRAY_REORDERED_BIT: u32 = 1 << 27;

impl ItemMetadata {
    /// Returns the projected index (bits 3-31 of `start_and_tag`).
    #[inline]
    pub(crate) fn projected_index(&self) -> u32 {
        self.start_and_tag >> TAG_SHIFT
    }

    /// Branchless mask: span mode -> FLAG_MASK (clears stale span data),
    /// hints mode -> 0xFFFFFFFF (preserves existing hint bits).
    #[inline]
    fn hints_preserve_mask(&self) -> u32 {
        ((self.end_and_flag as i32) >> 31) as u32 | FLAG_MASK
    }

    /// Stores a reprojected index, preserving user-set hint bits when
    /// already in hints mode. Returns `false` if the index doesn't fit.
    #[inline]
    pub(crate) fn set_reprojected_index(&mut self, index: usize) -> bool {
        if index <= (u32::MAX >> TAG_SHIFT) as usize {
            self.start_and_tag = (self.start_and_tag & TAG_MASK) | ((index as u32) << TAG_SHIFT);
            self.end_and_flag = (self.end_and_flag & self.hints_preserve_mask()) | HINTS_BIT;
            true
        } else {
            false
        }
    }

    /// Marks as not projected, preserving user-set hint bits when already
    /// in hints mode and clearing full-match.
    #[inline]
    pub(crate) fn set_reprojected_to_none(&mut self) {
        self.start_and_tag |= NOT_PROJECTED;
        self.end_and_flag =
            (self.end_and_flag & (self.hints_preserve_mask() & !FULL_MATCH_BIT)) | HINTS_BIT;
    }

    #[inline]
    pub(crate) fn set_reprojected_full_match(&mut self) {
        self.end_and_flag |= FULL_MATCH_BIT;
    }

    #[inline]
    pub(crate) fn is_reprojected_full_match(&self) -> bool {
        self.end_and_flag & FULL_MATCH_BIT != 0
    }

    /// Disables source-position reordering for this table's entries.
    #[inline]
    pub(crate) fn set_ignore_source_order(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_ORDER_BIT;
    }

    /// Returns `true` if source-position reordering is disabled.
    /// Gates on `HINTS_BIT` so stale span-end bits cannot false-positive.
    #[inline]
    pub(crate) fn ignore_source_order(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
    }

    /// Marks an array element as reordered during reprojection.
    #[inline]
    pub(crate) fn set_array_reordered(&mut self) {
        self.end_and_flag |= HINTS_BIT | ARRAY_REORDERED_BIT;
    }

    /// Returns `true` if this element was reordered during array reprojection.
    #[inline]
    pub(crate) fn array_reordered(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | ARRAY_REORDERED_BIT) == (HINTS_BIT | ARRAY_REORDERED_BIT)
    }

    /// Disables copying structural styles from source during reprojection.
    #[inline]
    pub(crate) fn set_ignore_source_style(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_STYLE_BIT;
    }

    /// Returns `true` if source-style copying is disabled for this table.
    #[inline]
    pub(crate) fn ignore_source_style(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
    }
}

impl<'de> Item<'de> {
    /// Access projected item from source computed in reprojection.
    pub(crate) fn projected<'a>(&self, inputs: &[&'a Item<'a>]) -> Option<&'a Item<'a>> {
        let index = self.meta.projected_index();
        inputs.get(index as usize).copied()
    }
    pub(crate) fn set_reprojected_to_none(&mut self) {
        self.meta.set_reprojected_to_none();
    }
    pub(crate) fn set_reprojected_index(&mut self, index: usize) -> bool {
        self.meta.set_reprojected_index(index)
    }
    /// Marks this item as a full match during reprojection.
    pub(crate) fn set_reprojected_full_match(&mut self) {
        self.meta.set_reprojected_full_match();
    }
    /// Returns whether this item was marked as a full match during reprojection.
    pub(crate) fn is_reprojected_full_match(&self) -> bool {
        self.meta.is_reprojected_full_match()
    }

    /// Returns `true` if this item is emitted as a subsection rather than
    /// as part of the body: `[header]` tables, implicit tables, and
    /// `[[array-of-tables]]`.
    #[inline]
    pub(crate) fn is_subsection(&self) -> bool {
        self.has_header_bit() || self.is_implicit_table() || self.is_aot()
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn set_flag(&mut self, flag: u32) {
        self.meta.set_flag(flag);
    }
}

impl<'de> Table<'de> {
    /// Disables source-position reordering for this table's immediate entries
    /// during emission. Non-recursive: child tables are unaffected.
    pub fn set_ignore_source_order(&mut self) {
        self.meta.set_ignore_source_order();
    }

    /// Returns `true` if source-position reordering is disabled for this table.
    #[must_use]
    pub fn ignore_source_order(&self) -> bool {
        self.meta.ignore_source_order()
    }

    /// Disables copying structural styles (TableStyle/ArrayStyle) from source
    /// during reprojection for this table's immediate entries. Key spans and
    /// reprojection indices are still copied. Non-recursive.
    pub fn set_ignore_source_style(&mut self) {
        self.meta.set_ignore_source_style();
    }

    /// Returns `true` if source-style copying is disabled for this table.
    #[must_use]
    pub fn ignore_source_style(&self) -> bool {
        self.meta.ignore_source_style()
    }

    /// Returns `true` if this table has automatic style resolution pending.
    #[must_use]
    pub fn is_auto_style(&self) -> bool {
        self.meta.is_auto_style()
    }
}

impl<'de> Array<'de> {
    /// Returns `true` if this array has automatic style resolution pending.
    #[must_use]
    pub fn is_auto_style(&self) -> bool {
        self.meta.is_auto_style()
    }
}
