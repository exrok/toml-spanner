#![allow(clippy::manual_map)]
#[cfg(test)]
#[path = "./value_tests.rs"]
mod tests;

pub(crate) mod array;
pub(crate) mod owned;
pub(crate) mod table;
use crate::arena::Arena;
use crate::{DateTime, Error, ErrorKind, Span, Table};
use std::fmt;
use std::mem::ManuallyDrop;

pub use array::Array;
pub(crate) use array::InternalArray;
use table::InnerTable;

pub(crate) const TAG_MASK: u32 = 0x7;
pub(crate) const TAG_SHIFT: u32 = 3;

pub(crate) const TAG_STRING: u32 = 0;
pub(crate) const TAG_INTEGER: u32 = 1;
pub(crate) const TAG_FLOAT: u32 = 2;
pub(crate) const TAG_BOOLEAN: u32 = 3;
pub(crate) const TAG_DATETIME: u32 = 4;
pub(crate) const TAG_TABLE: u32 = 5;
pub(crate) const TAG_ARRAY: u32 = 6;

// Only set in maybe item
pub(crate) const TAG_NONE: u32 = 7;

/// 3-bit state field in `end_and_flag` encoding container kind and sub-state.
/// Bit 2 set → table, bits 1:0 == 01 → array. Allows dispatch without
/// reading `start_and_tag`.
pub(crate) const FLAG_MASK: u32 = 0x7;
pub(crate) const FLAG_SHIFT: u32 = 3;

pub(crate) const FLAG_NONE: u32 = 0;
pub(crate) const FLAG_ARRAY: u32 = 2;
pub(crate) const FLAG_AOT: u32 = 3;
pub(crate) const FLAG_TABLE: u32 = 4;
pub(crate) const FLAG_DOTTED: u32 = 5;
pub(crate) const FLAG_HEADER: u32 = 6;
pub(crate) const FLAG_FROZEN: u32 = 7;

/// Bit 31 of `end_and_flag`: when set, the metadata is in format-hints mode
/// (constructed programmatically); when clear, it is in span mode (from parser).
pub(crate) const HINTS_BIT: u32 = 1 << 31;
/// Bit 30 of `end_and_flag`: marks a full match during reprojection.
#[cfg(feature = "to-toml")]
pub(crate) const FULL_MATCH_BIT: u32 = 1 << 30;
/// Bit 29 of `end_and_flag`: when set in format-hints mode, disables
/// source-position reordering for this table's immediate entries.
#[cfg(feature = "to-toml")]
pub(crate) const IGNORE_SOURCE_ORDER_BIT: u32 = 1 << 29;
/// Bit 28 of `end_and_flag`: when set in format-hints mode, disables
/// copying structural styles from source during reprojection.
#[cfg(feature = "to-toml")]
pub(crate) const IGNORE_SOURCE_STYLE_BIT: u32 = 1 << 28;
/// Bit 27 of `end_and_flag`: marks an array element as reordered during
/// reprojection. Prevents the emitter from sorting this element back to
/// its original source position in the parent array, without affecting
/// source-ordering of the element's own children.
#[cfg(feature = "to-toml")]
pub(crate) const ARRAY_REORDERED_BIT: u32 = 1 << 27;
/// Value bits (above TAG_SHIFT) all set = "not projected".
const NOT_PROJECTED: u32 = !(TAG_MASK); // 0xFFFF_FFF8

/// Packed 8-byte metadata for `Item`, `Table`, and `Array`.
///
/// Two variants discriminated by bit 31 of `end_and_flag`:
///
/// **Span variant** (bit 31 = 0): items produced by the parser.
/// - `start_and_tag`: bits 0-2 = tag, bits 3-30 = span start (28 bits, max 256 MiB)
/// - `end_and_flag`: bits 0-2 = flag, bits 3-30 = span end (28 bits), bit 31 = 0
///
/// **Format hints variant** (bit 31 = 1): items constructed programmatically.
/// - `start_and_tag`: bits 0-2 = tag, bits 3-31 = projected index (all 1's = not projected)
/// - `end_and_flag`: bit 31 = 1, bits 0-2 = flag, bits 3-30 = format hint bits
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ItemMetadata {
    pub(crate) start_and_tag: u32,
    pub(crate) end_and_flag: u32,
}

impl ItemMetadata {
    /// Creates metadata in span mode (parser-produced items).
    #[inline]
    pub(crate) fn spanned(tag: u32, flag: u32, start: u32, end: u32) -> Self {
        Self {
            start_and_tag: (start << TAG_SHIFT) | tag,
            end_and_flag: (end << FLAG_SHIFT) | flag,
        }
    }

    /// Creates metadata in format-hints mode (programmatically constructed items).
    #[inline]
    pub(crate) fn hints(tag: u32, flag: u32) -> Self {
        Self {
            start_and_tag: NOT_PROJECTED | tag,
            end_and_flag: HINTS_BIT | flag,
        }
    }

    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.start_and_tag & TAG_MASK
    }

    #[inline]
    pub(crate) fn flag(&self) -> u32 {
        self.end_and_flag & FLAG_MASK
    }

    #[inline]
    pub(crate) fn set_flag(&mut self, flag: u32) {
        self.end_and_flag = (self.end_and_flag & !FLAG_MASK) | flag;
    }

    /// Returns `true` if this metadata carries a source span (parser-produced).
    #[inline]
    pub(crate) fn is_span_mode(&self) -> bool {
        self.end_and_flag & HINTS_BIT == 0
    }

    /// Returns the source span, or `0..0` if in format-hints mode.
    #[inline]
    pub fn span(&self) -> Span {
        if self.is_span_mode() {
            self.span_unchecked()
        } else {
            Span::default()
        }
    }

    /// Returns the source span without checking the variant.
    /// Valid only during deserialization on parser-produced items.
    /// In span mode, bit 31 is always 0, so all bits above FLAG_SHIFT are span data.
    #[inline]
    pub(crate) fn span_unchecked(&self) -> Span {
        debug_assert!(self.is_span_mode());
        Span::new(
            self.start_and_tag >> TAG_SHIFT,
            self.end_and_flag >> FLAG_SHIFT,
        )
    }

    #[inline]
    pub(crate) fn span_start(&self) -> u32 {
        debug_assert!(self.is_span_mode());
        self.start_and_tag >> TAG_SHIFT
    }

    #[inline]
    pub(crate) fn set_span_start(&mut self, v: u32) {
        debug_assert!(self.is_span_mode());
        self.start_and_tag = (v << TAG_SHIFT) | (self.start_and_tag & TAG_MASK);
    }

    #[inline]
    pub(crate) fn set_span_end(&mut self, v: u32) {
        debug_assert!(self.is_span_mode());
        self.end_and_flag = (v << FLAG_SHIFT) | (self.end_and_flag & FLAG_MASK);
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        debug_assert!(self.is_span_mode());
        let old = self.end_and_flag;
        let current = old >> FLAG_SHIFT;
        self.end_and_flag = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_MASK);
    }

    #[cfg(feature = "to-toml")]
    /// Returns the projected index (bits 3-31 of `start_and_tag`).
    #[inline]
    pub(crate) fn projected_index(&self) -> u32 {
        self.start_and_tag >> TAG_SHIFT
    }

    #[cfg(feature = "to-toml")]
    /// Branchless mask: span mode -> FLAG_MASK (clears stale span data),
    /// hints mode -> 0xFFFFFFFF (preserves existing hint bits).
    #[inline]
    fn hints_preserve_mask(&self) -> u32 {
        ((self.end_and_flag as i32) >> 31) as u32 | FLAG_MASK
    }

    #[cfg(feature = "to-toml")]
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

    #[cfg(feature = "to-toml")]
    /// Marks as not projected, preserving user-set hint bits when already
    /// in hints mode and clearing full-match.
    #[inline]
    pub(crate) fn set_reprojected_to_none(&mut self) {
        self.start_and_tag |= NOT_PROJECTED;
        self.end_and_flag =
            (self.end_and_flag & (self.hints_preserve_mask() & !FULL_MATCH_BIT)) | HINTS_BIT;
    }

    #[cfg(feature = "to-toml")]
    #[inline]
    pub(crate) fn set_reprojected_full_match(&mut self) {
        self.end_and_flag |= FULL_MATCH_BIT;
    }

    #[cfg(feature = "to-toml")]
    #[inline]
    pub(crate) fn is_reprojected_full_match(&self) -> bool {
        self.end_and_flag & FULL_MATCH_BIT != 0
    }

    #[cfg(feature = "to-toml")]
    /// Disables source-position reordering for this table's entries.
    #[inline]
    pub(crate) fn set_ignore_source_order(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_ORDER_BIT;
    }

    #[cfg(feature = "to-toml")]
    /// Returns `true` if source-position reordering is disabled.
    /// Gates on `HINTS_BIT` so stale span-end bits cannot false-positive.
    #[inline]
    pub(crate) fn ignore_source_order(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_ORDER_BIT)
    }

    #[cfg(feature = "to-toml")]
    /// Marks an array element as reordered during reprojection.
    #[inline]
    pub(crate) fn set_array_reordered(&mut self) {
        self.end_and_flag |= HINTS_BIT | ARRAY_REORDERED_BIT;
    }

    #[cfg(feature = "to-toml")]
    /// Returns `true` if this element was reordered during array reprojection.
    #[inline]
    pub(crate) fn array_reordered(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | ARRAY_REORDERED_BIT) == (HINTS_BIT | ARRAY_REORDERED_BIT)
    }

    #[cfg(feature = "to-toml")]
    /// Disables copying structural styles from source during reprojection.
    #[inline]
    pub(crate) fn set_ignore_source_style(&mut self) {
        self.end_and_flag |= HINTS_BIT | IGNORE_SOURCE_STYLE_BIT;
    }

    #[cfg(feature = "to-toml")]
    /// Returns `true` if source-style copying is disabled for this table.
    #[inline]
    pub(crate) fn ignore_source_style(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
            == (HINTS_BIT | IGNORE_SOURCE_STYLE_BIT)
    }
}

/// The kind of a TOML table, distinguishing how it was defined in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableStyle {
    /// Structural parent with no explicit header (`FLAG_TABLE`).
    Implicit,
    /// Created by dotted keys, e.g. `a.b.c = 1` (`FLAG_DOTTED`).
    Dotted,
    /// Explicit `[section]` header (`FLAG_HEADER`).
    Header,
    /// Inline `{ }` table (`FLAG_FROZEN`).
    Inline,
}

/// The kind of a TOML array, distinguishing inline arrays from arrays of tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayStyle {
    /// Inline `[1, 2, 3]` array (`FLAG_ARRAY`).
    Inline,
    /// Array of tables `[[section]]` (`FLAG_AOT`).
    Header,
}

#[repr(C, align(8))]
union Payload<'de> {
    string: &'de str,
    integer: i64,
    float: f64,
    boolean: bool,
    array: ManuallyDrop<InternalArray<'de>>,
    table: ManuallyDrop<InnerTable<'de>>,
    datetime: DateTime,
}

/// A parsed TOML value with span information.
///
/// Use the `as_*` methods ([`as_str`](Self::as_str),
/// [`as_i64`](Self::as_i64), [`as_table`](Self::as_table), etc.) to
/// extract the value, or call [`value`](Self::value) /
/// [`value_mut`](Self::value_mut) to pattern match via the [`Value`] /
/// [`ValueMut`] enums.
///
/// Items support indexing with `&str` (table lookup) and `usize` (array
/// access). These operators return [`MaybeItem`] and never panic — missing
/// keys or out-of-bounds indices produce a `None` variant instead.
///
/// # Lookup performance
///
/// String-key lookups (`item["key"]`, [`as_table`](Self::as_table) +
/// [`Table::get`]) perform a linear scan over the table entries — O(n) in
/// the number of keys. For small tables or a handful of lookups, as is
/// typical in TOML, this is well fast enough.
///
/// For structured deserialization of larger tables, use
/// [`TableHelper`](crate::de::TableHelper) with the [`Context`](crate::de::Context)
/// returned by [`parse`](crate::parse).
///
/// # Examples
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let table = toml_spanner::parse("x = 42", &arena)?;
/// assert_eq!(table["x"].as_i64(), Some(42));
/// assert_eq!(table["missing"].as_i64(), None);
/// # Ok::<(), toml_spanner::Error>(())
/// ```
#[repr(C)]
pub struct Item<'de> {
    payload: Payload<'de>,
    pub(crate) meta: ItemMetadata,
}

const _: () = assert!(std::mem::size_of::<Item<'_>>() == 24);
const _: () = assert!(std::mem::align_of::<Item<'_>>() == 8);

impl<'de> From<i64> for Item<'de> {
    fn from(value: i64) -> Self {
        Self::raw_hints(TAG_INTEGER, FLAG_NONE, Payload { integer: value })
    }
}
impl<'de> From<&'de str> for Item<'de> {
    fn from(value: &'de str) -> Self {
        Self::raw_hints(TAG_STRING, FLAG_NONE, Payload { string: value })
    }
}

impl<'de> From<f64> for Item<'de> {
    fn from(value: f64) -> Self {
        Self::raw_hints(TAG_FLOAT, FLAG_NONE, Payload { float: value })
    }
}

impl<'de> From<bool> for Item<'de> {
    fn from(value: bool) -> Self {
        Self::raw_hints(TAG_BOOLEAN, FLAG_NONE, Payload { boolean: value })
    }
}

impl<'de> From<DateTime> for Item<'de> {
    fn from(value: DateTime) -> Self {
        Self::raw_hints(TAG_DATETIME, FLAG_NONE, Payload { datetime: value })
    }
}

#[cfg(feature = "to-toml")]
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
}

impl<'de> Item<'de> {
    #[inline]
    fn raw(tag: u32, flag: u32, start: u32, end: u32, payload: Payload<'de>) -> Self {
        Self {
            meta: ItemMetadata::spanned(tag, flag, start, end),
            payload,
        }
    }

    #[inline]
    fn raw_hints(tag: u32, flag: u32, payload: Payload<'de>) -> Self {
        Self {
            meta: ItemMetadata::hints(tag, flag),
            payload,
        }
    }

    #[inline]
    pub fn string(s: &'de str) -> Self {
        Self::raw_hints(TAG_STRING, FLAG_NONE, Payload { string: s })
    }

    #[inline]
    pub(crate) fn string_spanned(s: &'de str, span: Span) -> Self {
        Self::raw(
            TAG_STRING,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { string: s },
        )
    }

    #[inline]
    pub(crate) fn integer_spanned(i: i64, span: Span) -> Self {
        Self::raw(
            TAG_INTEGER,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { integer: i },
        )
    }

    #[inline]
    pub(crate) fn float_spanned(f: f64, span: Span) -> Self {
        Self::raw(
            TAG_FLOAT,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { float: f },
        )
    }

    #[inline]
    pub(crate) fn boolean(b: bool, span: Span) -> Self {
        Self::raw(
            TAG_BOOLEAN,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { boolean: b },
        )
    }

    #[inline]
    pub(crate) fn array(a: InternalArray<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
            FLAG_ARRAY,
            span.start,
            span.end,
            Payload {
                array: ManuallyDrop::new(a),
            },
        )
    }

    #[inline]
    pub(crate) fn table(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_TABLE,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates an array-of-tables value.
    #[inline]
    pub(crate) fn array_aot(a: InternalArray<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
            FLAG_AOT,
            span.start,
            span.end,
            Payload {
                array: ManuallyDrop::new(a),
            },
        )
    }

    /// Creates a frozen (inline) table value.
    #[inline]
    pub(crate) fn table_frozen(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_FROZEN,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with HEADER state (explicitly opened by `[header]`).
    #[inline]
    pub(crate) fn table_header(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_HEADER,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with DOTTED state (created by dotted-key navigation).
    #[inline]
    pub(crate) fn table_dotted(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            FLAG_DOTTED,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    #[inline]
    pub(crate) fn moment(m: DateTime, span: Span) -> Self {
        Self::raw(
            TAG_DATETIME,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { datetime: m },
        )
    }
}
/// Discriminant for the TOML value types stored in an [`Item`].
///
/// Obtained via [`Item::kind`].
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(unused)]
pub enum Kind {
    String = 0,
    Integer = 1,
    Float = 2,
    Boolean = 3,
    DateTime = 4,
    Table = 5,
    Array = 6,
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Kind {
    /// Returns the TOML type name as a lowercase string (e.g. `"string"`, `"table"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::String => "string",
            Kind::Integer => "integer",
            Kind::Float => "float",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
            Kind::Table => "table",
            Kind::DateTime => "datetime",
        }
    }
}

impl<'de> Item<'de> {
    #[inline]
    pub fn kind(&self) -> Kind {
        debug_assert!((self.meta.start_and_tag & TAG_MASK) as u8 <= Kind::Array as u8);
        // SAFETY: tag bits 0-2 are always in 0..=6 (set only by pub(crate)
        // constructors). All values are valid Kind discriminants.
        unsafe { std::mem::transmute::<u8, Kind>(self.meta.start_and_tag as u8 & 0x7) }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.meta.tag()
    }

    /// Returns `true` for leaf values (string, integer, float, boolean,
    /// datetime) that contain no arena-allocated children.
    #[inline]
    pub(crate) fn is_scalar(&self) -> bool {
        self.tag() < TAG_TABLE
    }

    #[inline]
    pub fn flag(&self) -> u32 {
        self.meta.flag()
    }

    #[inline]
    #[cfg(all(test, feature = "to-toml"))]
    pub(crate) fn set_flag(&mut self, flag: u32) {
        self.meta.set_flag(flag);
    }

    /// Returns the byte-offset span of this value in the source document.
    /// Only valid on parser-produced items (span mode).
    #[inline]
    pub fn span_unchecked(&self) -> Span {
        self.meta.span_unchecked()
    }

    /// Returns the source span, or `0..0` if this item was constructed
    /// programmatically (format-hints mode).
    #[inline]
    pub fn span(&self) -> Span {
        self.meta.span()
    }

    /// Returns the TOML type name (e.g. `"string"`, `"integer"`, `"table"`).
    #[inline]
    pub fn type_str(&self) -> &'static str {
        match self.kind() {
            Kind::String => "string",
            Kind::Integer => "integer",
            Kind::Float => "float",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
            Kind::Table => "table",
            Kind::DateTime => "datetime",
        }
    }

    #[inline]
    pub(crate) fn is_table(&self) -> bool {
        self.flag() >= FLAG_TABLE
    }

    #[inline]
    pub(crate) fn is_array(&self) -> bool {
        self.flag() & 6 == 2
    }

    #[inline]
    pub(crate) fn is_frozen(&self) -> bool {
        self.flag() == FLAG_FROZEN
    }

    #[inline]
    pub(crate) fn is_aot(&self) -> bool {
        self.flag() == FLAG_AOT
    }

    #[inline]
    pub(crate) fn has_header_bit(&self) -> bool {
        self.flag() == FLAG_HEADER
    }

    #[inline]
    pub(crate) fn has_dotted_bit(&self) -> bool {
        self.flag() == FLAG_DOTTED
    }

    /// Returns `true` if this is an implicit intermediate table — a plain
    /// table that is neither a `[header]` section, a dotted-key intermediate,
    /// nor a frozen inline `{ }` table.  These entries act as structural
    /// parents for header sections and have no text of their own.
    #[inline]
    pub(crate) fn is_implicit_table(&self) -> bool {
        self.flag() == FLAG_TABLE
    }

    /// Returns `true` if this item is emitted as a subsection rather than
    /// as part of the body: `[header]` tables, implicit tables, and
    /// `[[array-of-tables]]`.
    #[inline]
    #[cfg(feature = "to-toml")]
    pub(crate) fn is_subsection(&self) -> bool {
        self.has_header_bit() || self.is_implicit_table() || self.is_aot()
    }

    /// Splits this array item into disjoint borrows of the span field and array payload.
    ///
    /// # Safety
    ///
    /// The caller must ensure `self.is_array()` is true.
    #[inline]
    pub(crate) unsafe fn split_array_end_flag(&mut self) -> (&mut u32, &mut InternalArray<'de>) {
        debug_assert!(self.is_array());
        let ptr = self as *mut Item<'de>;
        // SAFETY: meta.end_and_flag and payload.array are at disjoint offsets within
        // the repr(C) layout. addr_of_mut! creates derived pointers without
        // intermediate references, avoiding aliasing violations.
        unsafe {
            let end_flag = &mut *std::ptr::addr_of_mut!((*ptr).meta.end_and_flag);
            let array =
                &mut *std::ptr::addr_of_mut!((*ptr).payload.array).cast::<InternalArray<'de>>();
            (end_flag, array)
        }
    }
}

/// Borrowed view into an [`Item`] for pattern matching.
///
/// Obtained via [`Item::value`].
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, Value};
///
/// let arena = Arena::new();
/// let table = toml_spanner::parse("n = 10", &arena)?;
/// match table["n"].item().unwrap().value() {
///     Value::Integer(i) => assert_eq!(*i, 10),
///     _ => panic!("expected integer"),
/// }
/// # Ok::<(), toml_spanner::Error>(())
/// ```
#[derive(Debug)]
pub enum Value<'a, 'de> {
    /// A string value.
    String(&'a &'de str),
    /// An integer value.
    Integer(&'a i64),
    /// A floating-point value.
    Float(&'a f64),
    /// A boolean value.
    Boolean(&'a bool),
    /// A datetime value.
    DateTime(&'a DateTime),
    /// A table value.
    Table(&'a Table<'de>),
    /// An array value.
    Array(&'a Array<'de>),
}

/// Mutable view into an [`Item`] for pattern matching.
///
/// Obtained via [`Item::value_mut`].
pub enum ValueMut<'a, 'de> {
    /// A string value.
    String(&'a mut &'de str),
    /// An integer value.
    Integer(&'a mut i64),
    /// A floating-point value.
    Float(&'a mut f64),
    /// A boolean value.
    Boolean(&'a mut bool),
    /// A datetime value (read-only; datetime fields are not mutable).
    DateTime(&'a DateTime),
    /// A table value.
    Table(&'a mut Table<'de>),
    /// An array value.
    Array(&'a mut Array<'de>),
}

impl<'de> Item<'de> {
    /// Returns a borrowed view for pattern matching.
    pub fn value(&self) -> Value<'_, 'de> {
        // SAFETY: kind() returns the discriminant set at construction. Each
        // match arm reads the union field that was written for that discriminant.
        unsafe {
            match self.kind() {
                Kind::String => Value::String(&self.payload.string),
                Kind::Integer => Value::Integer(&self.payload.integer),
                Kind::Float => Value::Float(&self.payload.float),
                Kind::Boolean => Value::Boolean(&self.payload.boolean),
                Kind::Array => Value::Array(self.as_array_unchecked()),
                Kind::Table => Value::Table(self.as_table_unchecked()),
                Kind::DateTime => Value::DateTime(&self.payload.datetime),
            }
        }
    }

    /// Returns a mutable view for pattern matching.
    pub fn value_mut(&mut self) -> ValueMut<'_, 'de> {
        // SAFETY: kind() returns the discriminant set at construction. Each
        // match arm accesses the union field that was written for that discriminant.
        unsafe {
            match self.kind() {
                Kind::String => ValueMut::String(&mut self.payload.string),
                Kind::Integer => ValueMut::Integer(&mut self.payload.integer),
                Kind::Float => ValueMut::Float(&mut self.payload.float),
                Kind::Boolean => ValueMut::Boolean(&mut self.payload.boolean),
                Kind::Array => ValueMut::Array(self.as_array_mut_unchecked()),
                Kind::Table => ValueMut::Table(self.as_table_mut_unchecked()),
                Kind::DateTime => ValueMut::DateTime(&self.payload.datetime),
            }
        }
    }
}

impl<'de> Item<'de> {
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            // SAFETY: tag check guarantees the payload is a string.
            Some(unsafe { self.payload.string })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer })
        } else {
            None
        }
    }

    /// Returns an `f64` if this is a float or integer value.
    ///
    /// Integer values are converted to `f64` via `as` cast (lossy for large
    /// values outside the 2^53 exact-integer range).
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self.value() {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Returns a `bool` if this is a boolean value.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            // SAFETY: tag check guarantees the payload is a boolean.
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag check guarantees this item is an array variant.
            Some(unsafe { self.as_array_unchecked() })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.is_table() {
            // SAFETY: is_table() check guarantees this item is a table variant.
            Some(unsafe { self.as_table_unchecked() })
        } else {
            None
        }
    }

    /// Returns a borrowed [`DateTime`] if this is a datetime value.
    #[inline]
    pub fn as_datetime(&self) -> Option<&DateTime> {
        if self.tag() == TAG_DATETIME {
            // SAFETY: tag check guarantees the payload is a moment.
            Some(unsafe { &self.payload.datetime })
        } else {
            None
        }
    }

    /// Returns a mutable array reference.
    #[inline]
    pub fn as_array_mut(&mut self) -> Option<&mut Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag check guarantees this item is an array variant.
            Some(unsafe { self.as_array_mut_unchecked() })
        } else {
            None
        }
    }

    /// Returns a mutable table reference.
    #[inline]
    pub fn as_table_mut(&mut self) -> Option<&mut Table<'de>> {
        if self.is_table() {
            // SAFETY: is_table() check guarantees this item is a table variant.
            Some(unsafe { self.as_table_mut_unchecked() })
        } else {
            None
        }
    }

    /// Reinterprets this [`Item`] as an [`Array`] (shared reference).
    ///
    /// # Safety
    ///
    /// The caller must ensure `self.tag() == TAG_ARRAY`.
    #[inline]
    pub(crate) unsafe fn as_array_unchecked(&self) -> &Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        // SAFETY: Both types are `#[repr(C)]` with identical layout when the
        // payload is an array.
        unsafe { &*(self as *const Item<'de>).cast::<Array<'de>>() }
    }

    /// Reinterprets this [`Item`] as an [`Array`] (mutable reference).
    ///
    /// # Safety
    ///
    /// The caller must ensure `self.tag() == TAG_ARRAY`.
    #[inline]
    pub(crate) unsafe fn as_array_mut_unchecked(&mut self) -> &mut Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        unsafe { &mut *(self as *mut Item<'de>).cast::<Array<'de>>() }
    }

    /// Returns a mutable table pointer (parser-internal).
    #[inline]
    pub(crate) unsafe fn as_inner_table_mut_unchecked(&mut self) -> &mut InnerTable<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut self.payload.table }
    }

    /// Reinterprets this [`Item`] as a [`Table`].
    ///
    /// SAFETY: The caller must ensure `self.is_table()` is true. Both types
    /// are `#[repr(C)]` with identical layout when the payload is a table.
    #[inline]
    pub(crate) unsafe fn as_table_mut_unchecked(&mut self) -> &mut Table<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut *(self as *mut Item<'de>).cast::<Table<'de>>() }
    }

    /// Reinterprets this [`Item`] as a [`Table`] (shared reference).
    ///
    /// # Safety
    ///
    /// The caller must ensure `self.is_table()` is true.
    #[inline]
    pub(crate) unsafe fn as_table_unchecked(&self) -> &Table<'de> {
        debug_assert!(self.is_table());
        // SAFETY: Both types are `#[repr(C)]` with identical layout when the payload is a table.
        unsafe { &*(self as *const Item<'de>).cast::<Table<'de>>() }
    }

    /// Returns true if the value is a table and is non-empty.
    #[inline]
    pub fn has_keys(&self) -> bool {
        self.as_table().is_some_and(|t| !t.is_empty())
    }

    /// Returns true if the value is a table and has the specified key.
    #[inline]
    pub fn has_key(&self, key: &str) -> bool {
        self.as_table().is_some_and(|t| t.contains_key(key))
    }

    /// Deep-clones this item into `arena`. Keys and strings are shared
    /// with the source.
    pub fn clone_in(&self, arena: &'de Arena) -> Item<'de> {
        if self.is_scalar() {
            // SAFETY: Scalar items (string, integer, float, boolean, datetime)
            // contain no arena-allocated pointers. Bitwise copy is correct.
            // Item is non-Drop.
            unsafe { std::ptr::read(self) }
        } else if self.tag() == TAG_ARRAY {
            // SAFETY: tag == TAG_ARRAY guarantees the payload is an array.
            let cloned = unsafe { self.payload.array.clone_in(arena) };
            Item {
                payload: Payload {
                    array: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        } else {
            // SAFETY: !is_scalar() && tag != TAG_ARRAY → tag == TAG_TABLE.
            let cloned = unsafe { self.payload.table.clone_in(arena) };
            Item {
                payload: Payload {
                    table: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        }
    }
}

impl<'de> Item<'de> {
    /// Creates an "expected X, found Y" error using this value's type and span.
    #[inline]
    pub fn expected(&self, expected: &'static str) -> Error {
        Error {
            kind: ErrorKind::Wanted {
                expected,
                found: self.type_str(),
            },
            span: self.span_unchecked(),
        }
    }

    /// Takes a string value and parses it via [`std::str::FromStr`].
    ///
    /// Returns an error if the value is not a string or parsing fails.
    #[inline]
    pub fn parse<T>(&self) -> Result<T, Error>
    where
        T: std::str::FromStr,
        <T as std::str::FromStr>::Err: std::fmt::Display,
    {
        let Some(s) = self.as_str() else {
            return Err(self.expected("a string"));
        };
        match s.parse() {
            Ok(v) => Ok(v),
            Err(err) => Err(Error {
                kind: ErrorKind::Custom(format!("failed to parse string: {err}").into()),
                span: self.span_unchecked(),
            }),
        }
    }
}

impl fmt::Debug for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value() {
            Value::String(s) => s.fmt(f),
            Value::Integer(i) => i.fmt(f),
            Value::Float(v) => v.fmt(f),
            Value::Boolean(b) => b.fmt(f),
            Value::Array(a) => a.fmt(f),
            Value::Table(t) => t.fmt(f),
            Value::DateTime(m) => {
                let mut buf = std::mem::MaybeUninit::uninit();
                f.write_str(m.format(&mut buf))
            }
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Item<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.value() {
            Value::String(s) => ser.serialize_str(s),
            Value::Integer(i) => ser.serialize_i64(*i),
            Value::Float(f) => ser.serialize_f64(*f),
            Value::Boolean(b) => ser.serialize_bool(*b),
            Value::Array(arr) => {
                use serde::ser::SerializeSeq;
                let mut seq = ser.serialize_seq(Some(arr.len()))?;
                for ele in arr {
                    seq.serialize_element(ele)?;
                }
                seq.end()
            }
            Value::Table(tab) => {
                use serde::ser::SerializeMap;
                let mut map = ser.serialize_map(Some(tab.len()))?;
                for (k, v) in tab {
                    map.serialize_entry(&*k.name, v)?;
                }
                map.end()
            }
            Value::DateTime(m) => {
                let mut buf = std::mem::MaybeUninit::uninit();
                ser.serialize_str(m.format(&mut buf))
            }
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for InnerTable<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = ser.serialize_map(Some(self.len()))?;
        for (k, v) in self.entries() {
            map.serialize_entry(&*k.name, v)?;
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Table<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(ser)
    }
}

/// A TOML table key with its source span.
///
/// Keys appear as the first element in `(`[`Key`]`, `[`Item`]`)` entry pairs
/// when iterating over a [`Table`].
#[derive(Copy, Clone)]
pub struct Key<'de> {
    /// The key name.
    pub name: &'de str,
    /// The byte-offset span of the key in the source document.
    pub span: Span,
}

impl<'de> Key<'de> {
    pub fn anon(value: &'de str) -> Self {
        Self {
            name: value,
            span: Span::default(),
        }
    }
    /// Returns the key name as a string slice.
    pub fn as_str(&self) -> &'de str {
        self.name
    }
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Key<'_>>() == 24);

impl std::borrow::Borrow<str> for Key<'_> {
    fn borrow(&self) -> &str {
        self.name
    }
}

impl fmt::Debug for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl fmt::Display for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(other.name)
    }
}

impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(other.name)
    }
}

impl Eq for Key<'_> {}

impl<'de> PartialEq for Item<'de> {
    fn eq(&self, other: &Self) -> bool {
        let a = self;
        let b = other;
        if a.kind() != b.kind() {
            return false;
        }
        // SAFETY: kind check above guarantees both payloads hold the same union
        // variant. Each arm reads only the field that corresponds to that variant.
        // Righting this way generates the minimal LLVM lines and the fastest impl
        unsafe {
            match a.kind() {
                Kind::String => a.payload.string == b.payload.string,
                Kind::Integer => a.payload.integer == b.payload.integer,
                Kind::Float => {
                    let af = a.payload.float;
                    let bf = b.payload.float;
                    if af.is_nan() && bf.is_nan() {
                        af.is_sign_negative() == bf.is_sign_negative()
                    } else {
                        af.to_bits() == bf.to_bits()
                    }
                }
                Kind::Boolean => a.payload.boolean == b.payload.boolean,
                Kind::DateTime => {
                    let da = &a.payload.datetime;
                    let db = &b.payload.datetime;
                    da.date() == db.date() && da.time() == db.time() && da.offset() == db.offset()
                }
                Kind::Array => a.payload.array.as_slice() == a.payload.array.as_slice(),
                Kind::Table => {
                    let tab_a = a.as_table_unchecked();
                    let tab_b = b.as_table_unchecked();
                    if tab_a.len() != tab_b.len() {
                        return false;
                    }
                    for (key, val_a) in tab_a {
                        let Some(val_b) = tab_b.get(key.name) else {
                            return false;
                        };
                        if !items_equal(val_a, val_b) {
                            return false;
                        }
                    }
                    true
                }
            }
        }
    }
}
/// Compares two [`Item`] values for semantic equality, ignoring spans.
pub fn items_equal(a: &Item<'_>, b: &Item<'_>) -> bool {
    a == b
}

impl<'de> std::ops::Index<&str> for Item<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(table) = self.as_table()
            && let Some(item) = table.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<usize> for Item<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(arr) = self.as_array()
            && let Some(item) = arr.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<&str> for MaybeItem<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(table) = self.as_table()
            && let Some(item) = table.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

impl<'de> std::ops::Index<usize> for MaybeItem<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(arr) = self.as_array()
            && let Some(item) = arr.get(index)
        {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}

/// A nullable reference to a parsed TOML value.
///
/// `MaybeItem` is returned by the index operators (`[]`) on [`Item`],
/// [`Table`], [`Array`], and `MaybeItem` itself. It acts like an
/// [`Option<&Item>`] that can be further indexed without panicking — chained
/// lookups on missing keys simply propagate the `None` state.
///
/// Use the `as_*` accessors to extract a value, or call [`item`](Self::item)
/// to get back an `Option<&Item>`.
///
/// # Examples
///
/// ```
/// use toml_spanner::Arena;
///
/// let arena = Arena::new();
/// let table = toml_spanner::parse(r#"
/// [server]
/// host = "localhost"
/// port = 8080
/// "#, &arena)?;
///
/// // Successful nested lookup.
/// assert_eq!(table["server"]["host"].as_str(), Some("localhost"));
/// assert_eq!(table["server"]["port"].as_i64(), Some(8080));
///
/// // Missing keys propagate through chained indexing without panicking.
/// assert_eq!(table["server"]["missing"].as_str(), None);
/// assert_eq!(table["nonexistent"]["deep"]["path"].as_str(), None);
///
/// // Convert back to an Option<&Item> when needed.
/// assert!(table["server"]["host"].item().is_some());
/// assert!(table["nope"].item().is_none());
/// # Ok::<(), toml_spanner::Error>(())
/// ```
#[repr(C)]
pub struct MaybeItem<'de> {
    payload: Payload<'de>,
    meta: ItemMetadata,
}

// SAFETY: MaybeItem is only constructed as either (a) the static NONE sentinel
// (payload is zeroed, tag is TAG_NONE — no pointers are dereferenced), or
// (b) a reinterpretation of &Item via from_ref. In both cases the data is
// accessed only through shared references. All payload variants (integers,
// floats, bools, &str, Array, InnerTable) are safe to share across threads
// when behind a shared reference.
unsafe impl Sync for MaybeItem<'_> {}

pub(crate) static NONE: MaybeItem<'static> = MaybeItem {
    payload: Payload { integer: 0 },
    meta: ItemMetadata {
        start_and_tag: TAG_NONE,
        end_and_flag: FLAG_NONE,
    },
};

impl<'de> MaybeItem<'de> {
    /// Views an [`Item`] reference as a `MaybeItem`.
    pub fn from_ref<'a>(item: &'a Item<'de>) -> &'a Self {
        // SAFETY: Item and MaybeItem are both #[repr(C)] with identical field
        // layout (Payload, ItemMetadata). Size and alignment equality is verified
        // by const assertions.
        unsafe { &*(item as *const Item<'de>).cast::<MaybeItem<'de>>() }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.meta.tag()
    }
    /// Returns the underlying [`Item`], or [`None`] if this is a missing value.
    pub fn item(&self) -> Option<&Item<'de>> {
        if self.tag() != TAG_NONE {
            // SAFETY: tag != TAG_NONE means this was created via from_ref from
            // a valid Item. Item and MaybeItem have identical repr(C) layout.
            Some(unsafe { &*(self as *const MaybeItem<'de>).cast::<Item<'de>>() })
        } else {
            None
        }
    }
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            // SAFETY: tag check guarantees the payload is a string.
            Some(unsafe { self.payload.string })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer })
        } else {
            None
        }
    }

    /// Returns an `f64` if this is a float or integer value.
    ///
    /// Integer values are converted to `f64` via `as` cast (lossy for large
    /// values outside the 2^53 exact-integer range).
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        self.item()?.as_f64()
    }

    /// Returns a `bool` if this is a boolean value.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            // SAFETY: tag check guarantees the payload is a boolean.
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            // SAFETY: tag == TAG_ARRAY guarantees the payload is an array.
            // MaybeItem and Array have identical repr(C) layout (verified by
            // const size/align assertions on Item and Array).
            Some(unsafe { &*(self as *const Self).cast::<Array<'de>>() })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.tag() == TAG_TABLE {
            // SAFETY: tag == TAG_TABLE guarantees the payload is a table.
            // MaybeItem and Table have identical repr(C) layout (verified by
            // const size/align assertions on Item and Table).
            Some(unsafe { &*(self as *const Self).cast::<Table<'de>>() })
        } else {
            None
        }
    }

    /// Returns a borrowed [`DateTime`] if this is a datetime value.
    #[inline]
    pub fn as_datetime(&self) -> Option<&DateTime> {
        if self.tag() == TAG_DATETIME {
            // SAFETY: tag check guarantees the payload is a moment.
            Some(unsafe { &self.payload.datetime })
        } else {
            None
        }
    }

    /// Returns the source span, or `0..0` if this is a missing value.
    pub fn span(&self) -> Span {
        if let Some(item) = self.item() {
            item.span_unchecked()
        } else {
            Span::default()
        }
    }

    /// Returns a borrowed [`Value`] for pattern matching, or [`None`] if missing.
    pub fn value(&self) -> Option<Value<'_, 'de>> {
        if let Some(item) = self.item() {
            Some(item.value())
        } else {
            None
        }
    }
}
