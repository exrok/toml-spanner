#![allow(clippy::manual_map)]
#[cfg(test)]
#[path = "./value_tests.rs"]
mod tests;

pub(crate) mod array;
pub(crate) mod table;
#[cfg(feature = "to-toml")]
mod to_toml;
use crate::arena::Arena;
use crate::error::{Error, ErrorKind};
use crate::item::table::TableIndex;
use crate::{DateTime, Span, Table};
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
/// Bit 26 of `end_and_flag`: when set in hints mode, defers style decisions
/// to normalization time. Resolved based on content heuristics.
pub(crate) const AUTO_STYLE_BIT: u32 = 1 << 26;
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

    #[inline]
    pub(crate) fn set_auto_style(&mut self) {
        self.end_and_flag |= AUTO_STYLE_BIT;
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn is_auto_style(&self) -> bool {
        self.end_and_flag & (HINTS_BIT | AUTO_STYLE_BIT) == (HINTS_BIT | AUTO_STYLE_BIT)
    }

    #[inline]
    pub(crate) fn clear_auto_style(&mut self) {
        self.end_and_flag &= !AUTO_STYLE_BIT;
    }

    /// Returns `true` if this metadata carries a source span (parser-produced).
    #[inline]
    pub(crate) fn is_span_mode(&self) -> bool {
        (self.end_and_flag as i32) >= 0
    }

    /// Returns the source span, or `0..0` if in format-hints mode.
    #[inline]
    pub fn span(&self) -> Span {
        if (self.end_and_flag as i32) >= 0 {
            self.span_unchecked()
        } else {
            Span { start: 0, end: 0 }
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

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PackedI128 {
    value: i128,
}

/// A TOML integer value.
///
/// This is a storage type that supports the full range of `i128`.
/// Convert to a primitive with [`as_i128`](Self::as_i128),
/// [`as_i64`](Self::as_i64), or [`as_u64`](Self::as_u64), perform
/// your arithmetic there, then convert back with `From`.
#[repr(align(8))]
#[derive(Clone, Copy)]
pub struct Integer {
    value: PackedI128,
}

impl Integer {
    /// Returns the value as an `i128`.
    #[inline]
    pub fn as_i128(&self) -> i128 {
        let copy = *self;
        copy.value.value
    }

    /// Returns the value as an `f64`, which may be lossy for large integers.
    #[inline]
    pub fn as_f64(&self) -> f64 {
        let copy = *self;
        copy.value.value as f64
    }

    /// Returns the value as an `i64`, or [`None`] if it does not fit.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        i64::try_from(self.as_i128()).ok()
    }

    /// Returns the value as a `u64`, or [`None`] if it does not fit.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        u64::try_from(self.as_i128()).ok()
    }
}

impl From<i128> for Integer {
    #[inline]
    fn from(v: i128) -> Self {
        Self {
            value: PackedI128 { value: v },
        }
    }
}

impl From<i64> for Integer {
    #[inline]
    fn from(v: i64) -> Self {
        Self::from(v as i128)
    }
}

impl From<u64> for Integer {
    #[inline]
    fn from(v: u64) -> Self {
        Self::from(v as i128)
    }
}

impl From<i32> for Integer {
    #[inline]
    fn from(v: i32) -> Self {
        Self::from(v as i128)
    }
}

impl From<u32> for Integer {
    #[inline]
    fn from(v: u32) -> Self {
        Self::from(v as i128)
    }
}

impl PartialEq for Integer {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_i128() == other.as_i128()
    }
}

impl Eq for Integer {}

impl fmt::Debug for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_i128().fmt(f)
    }
}

impl fmt::Display for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_i128().fmt(f)
    }
}

#[repr(C, align(8))]
union Payload<'de> {
    string: &'de str,
    integer: Integer,
    float: f64,
    boolean: bool,
    array: ManuallyDrop<InternalArray<'de>>,
    table: ManuallyDrop<InnerTable<'de>>,
    datetime: DateTime,
}

/// A parsed TOML value with span information.
///
/// Extract values with the `as_*` methods ([`as_str`](Self::as_str),
/// [`as_i64`](Self::as_i64), [`as_table`](Self::as_table), etc.) or
/// pattern match via [`value`](Self::value) and [`value_mut`](Self::value_mut).
///
/// Items support indexing with `&str` (table lookup) and `usize` (array
/// access). These operators return [`MaybeItem`] and never panic. Missing
/// keys or out-of-bounds indices produce a `None` variant instead.
///
/// # Lookup performance
///
/// String-key lookups (`item["key"]`, [`as_table`](Self::as_table) +
/// [`Table::get`]) perform a linear scan over the table entries, O(n) in
/// the number of keys. For small tables or a handful of lookups, as is
/// typical in TOML, this is fast enough.
///
/// For structured conversion of larger tables, use
/// [`TableHelper`](crate::de::TableHelper) with the [`Context`](crate::de::Context)
/// from [`parse`](crate::parse), which internally uses an index for O(1) lookups.
///
/// # Examples
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let table = toml_spanner::parse("x = 42", &arena).unwrap();
/// assert_eq!(table["x"].as_i64(), Some(42));
/// assert_eq!(table["missing"].as_i64(), None);
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
        Self::raw_hints(
            TAG_INTEGER,
            FLAG_NONE,
            Payload {
                integer: Integer::from(value),
            },
        )
    }
}
impl<'de> From<i128> for Item<'de> {
    fn from(value: i128) -> Self {
        Self::raw_hints(
            TAG_INTEGER,
            FLAG_NONE,
            Payload {
                integer: Integer::from(value),
            },
        )
    }
}
impl<'de> From<i32> for Item<'de> {
    fn from(value: i32) -> Self {
        Self::from(value as i64)
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

    /// Creates a string [`Item`] in format-hints mode (no source span).
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
    pub(crate) fn integer_spanned(i: i128, span: Span) -> Self {
        Self::raw(
            TAG_INTEGER,
            FLAG_NONE,
            span.start,
            span.end,
            Payload {
                integer: Integer::from(i),
            },
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
    pub fn as_str(&self) -> &'static str {
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
    /// Returns the type discriminant of this value.
    #[inline]
    pub fn kind(&self) -> Kind {
        debug_assert!((self.meta.start_and_tag & TAG_MASK) as u8 <= Kind::Array as u8);
        // SAFETY: Kind is #[repr(u8)] with discriminants 0..=6. The tag bits
        // (bits 0 to 2 of start_and_tag) are set exclusively by pub(crate)
        // constructors which only use TAG_STRING(0)..TAG_ARRAY(6). TAG_NONE(7)
        // is only used for MaybeItem, which has its own tag() method returning
        // u32. kind() is never called on a NONE tagged value. Therefore the
        // masked value is always a valid Kind discriminant.
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

    /// Returns the raw 3-bit flag encoding the container sub-kind.
    ///
    /// Prefer [`Table::style`] or [`Array::style`] for a typed alternative.
    #[inline]
    pub fn flag(&self) -> u32 {
        self.meta.flag()
    }

    /// Returns the byte-offset span of this value in the source document.
    /// Only valid on parser-produced items (span mode).
    #[inline]
    pub(crate) fn span_unchecked(&self) -> Span {
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
    pub fn type_str(&self) -> &'static &'static str {
        match self.kind() {
            Kind::String => &"string",
            Kind::Integer => &"integer",
            Kind::Float => &"float",
            Kind::Boolean => &"boolean",
            Kind::Array => &"array",
            Kind::Table => &"table",
            Kind::DateTime => &"datetime",
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

    /// Returns `true` if this is an implicit intermediate table, a plain
    /// table that is neither a `[header]` section, a dotted-key intermediate,
    /// nor a frozen inline `{ }` table. These entries act as structural
    /// parents for header sections and have no text of their own.
    #[inline]
    pub(crate) fn is_implicit_table(&self) -> bool {
        self.flag() == FLAG_TABLE
    }

    /// Splits this array item into disjoint borrows of the span field and array payload.
    ///
    /// # Safety
    ///
    /// - `self.is_array()` must be true (i.e. the payload union holds `array`).
    #[inline]
    pub(crate) unsafe fn split_array_end_flag(&mut self) -> (&mut u32, &mut InternalArray<'de>) {
        debug_assert!(self.is_array());
        let ptr = self as *mut Item<'de>;
        // SAFETY:
        // - Caller guarantees this is an array item, so `payload.array` is the
        //   active union field.
        // - `payload.array` occupies bytes 0..16 (ManuallyDrop<InternalArray>).
        //   `meta.end_and_flag` occupies bytes 20..24. These do not overlap.
        // - `addr_of_mut!` derives raw pointers without creating intermediate
        //   references, avoiding aliasing violations.
        // - The `.cast::<InternalArray>()` strips ManuallyDrop, which is
        //   #[repr(transparent)] and therefore has identical layout.
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
/// let table = toml_spanner::parse("n = 10", &arena).unwrap();
/// match table["n"].item().unwrap().value() {
///     Value::Integer(i) => assert_eq!(i.as_i128(), 10),
///     _ => panic!("expected integer"),
/// }
/// ```
#[derive(Debug)]
pub enum Value<'a, 'de> {
    /// A string value.
    String(&'a &'de str),
    /// An integer value.
    Integer(&'a Integer),
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
    Integer(&'a mut Integer),
    /// A floating-point value.
    Float(&'a mut f64),
    /// A boolean value.
    Boolean(&'a mut bool),
    /// A datetime value (read-only, datetime fields are not mutable).
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

    #[doc(hidden)]
    /// Used in derive macro for style attributes
    pub fn with_style_of_array_or_table(mut self, style: TableStyle) -> Item<'de> {
        match self.value_mut() {
            ValueMut::Table(table) => table.set_style(style),
            ValueMut::Array(array) => match style {
                TableStyle::Header => array.set_style(ArrayStyle::Header),
                TableStyle::Inline => array.set_style(ArrayStyle::Inline),
                _ => (),
            },
            _ => (),
        }
        self
    }

    /// Returns an `i128` if this is an integer value.
    #[inline]
    pub fn as_i128(&self) -> Option<i128> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer.as_i128() })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value that fits in the `i64` range.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_i64() }
        } else {
            None
        }
    }

    /// Returns a `u64` if this is an integer value that fits in the `u64` range.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_u64() }
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
            Value::Integer(i) => Some(i.as_i128() as f64),
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

    /// Consumes this item, returning the table if it is one.
    #[inline]
    pub fn into_table(self) -> Option<Table<'de>> {
        if self.is_table() {
            // SAFETY: is_table() guarantees the active union field is `table`.
            // Item and Table have identical size, alignment, and repr(C) layout
            // (verified by const assertions on Table). Item has no Drop impl.
            Some(unsafe { std::mem::transmute::<Item<'de>, Table<'de>>(self) })
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
    /// - `self.tag()` must be `TAG_ARRAY`.
    #[inline]
    pub(crate) unsafe fn as_array_unchecked(&self) -> &Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        // SAFETY: Item is #[repr(C)] { payload: Payload, meta: ItemMetadata }.
        // Array is #[repr(C)] { value: InternalArray, meta: ItemMetadata }.
        // Payload is a union whose `array` field is ManuallyDrop<InternalArray>
        // (#[repr(transparent)]). Both types are 24 bytes, align 8 (verified by
        // const assertions). Field offsets match: data at 0..16, metadata at 16..24.
        // Caller guarantees the active union field is `array`.
        unsafe { &*(self as *const Item<'de>).cast::<Array<'de>>() }
    }

    /// Reinterprets this [`Item`] as an [`Array`] (mutable reference).
    ///
    /// # Safety
    ///
    /// - `self.tag()` must be `TAG_ARRAY`.
    #[inline]
    pub(crate) unsafe fn as_array_mut_unchecked(&mut self) -> &mut Array<'de> {
        debug_assert!(self.tag() == TAG_ARRAY);
        // SAFETY: Same layout argument as as_array_unchecked.
        unsafe { &mut *(self as *mut Item<'de>).cast::<Array<'de>>() }
    }

    /// Returns a mutable reference to the inner table payload (parser-internal).
    ///
    /// # Safety
    ///
    /// - `self.is_table()` must be true.
    #[inline]
    pub(crate) unsafe fn as_inner_table_mut_unchecked(&mut self) -> &mut InnerTable<'de> {
        debug_assert!(self.is_table());
        // SAFETY: Caller guarantees the active union field is `table`.
        // ManuallyDrop<InnerTable> dereferences to &mut InnerTable.
        unsafe { &mut self.payload.table }
    }

    /// Reinterprets this [`Item`] as a [`Table`] (mutable reference).
    ///
    /// # Safety
    ///
    /// - `self.is_table()` must be true.
    #[inline]
    pub(crate) unsafe fn as_table_mut_unchecked(&mut self) -> &mut Table<'de> {
        debug_assert!(self.is_table());
        // SAFETY: Item is #[repr(C)] { payload: Payload, meta: ItemMetadata }.
        // Table is #[repr(C)] { value: InnerTable, meta: ItemMetadata }.
        // Payload's `table` field is ManuallyDrop<InnerTable> (#[repr(transparent)]).
        // Both are 24 bytes, align 8 (const assertions). Field offsets match.
        // Caller guarantees the active union field is `table`.
        unsafe { &mut *(self as *mut Item<'de>).cast::<Table<'de>>() }
    }

    /// Reinterprets this [`Item`] as a [`Table`] (shared reference).
    ///
    /// # Safety
    ///
    /// - `self.is_table()` must be true.
    #[inline]
    pub(crate) unsafe fn as_table_unchecked(&self) -> &Table<'de> {
        debug_assert!(self.is_table());
        // SAFETY: Same layout argument as as_table_mut_unchecked.
        unsafe { &*(self as *const Item<'de>).cast::<Table<'de>>() }
    }

    /// Returns `true` if the value is a non-empty table.
    #[inline]
    pub fn has_keys(&self) -> bool {
        self.as_table().is_some_and(|t| !t.is_empty())
    }

    /// Returns `true` if the value is a table containing `key`.
    #[inline]
    pub fn has_key(&self, key: &str) -> bool {
        self.as_table().is_some_and(|t| t.contains_key(key))
    }
    /// Clones this item into `arena`, sharing existing strings.
    ///
    /// Scalar values are copied directly. Tables and arrays are
    /// recursively cloned with new arena-allocated storage. String
    /// values and table key names continue to reference their original
    /// memory, so the source arena (or input string) must remain alive.
    pub fn clone_in(&self, arena: &'de Arena) -> Item<'de> {
        if self.is_scalar() {
            // SAFETY: Scalar items have tags 0..=4 (STRING, INTEGER, FLOAT,
            // BOOLEAN, DATETIME). None of these own arena-allocated children.
            // STRING contains a &'de str (shared reference to input/arena data)
            // which is safe to duplicate. Item has no Drop impl, so ptr::read
            // is a plain bitwise copy with no double-free risk.
            unsafe { std::ptr::read(self) }
        } else if self.tag() == TAG_ARRAY {
            // SAFETY: tag == TAG_ARRAY guarantees payload.array is the active
            // union field.
            let cloned = unsafe { self.payload.array.clone_in(arena) };
            Item {
                payload: Payload {
                    array: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        } else {
            // SAFETY: Tags are 0..=6. is_scalar() is false (tag >= 5) and
            // tag != TAG_ARRAY (6), so tag must be TAG_TABLE (5). Therefore
            // payload.table is the active union field.
            let cloned = unsafe { self.payload.table.clone_in(arena) };
            Item {
                payload: Payload {
                    table: ManuallyDrop::new(cloned),
                },
                meta: self.meta,
            }
        }
    }

    /// Copies this item into `target`, returning a copy with `'static` lifetime.
    ///
    /// # Safety
    ///
    /// `target` must have sufficient space as computed by
    /// [`compute_size`](crate::owned_item).
    pub(crate) unsafe fn emplace_in(
        &self,
        target: &mut crate::owned_item::ItemCopyTarget,
    ) -> Item<'static> {
        match self.tag() {
            TAG_STRING => {
                // SAFETY: tag == TAG_STRING guarantees payload.string is active.
                let s = unsafe { self.payload.string };
                // SAFETY: Caller guarantees sufficient string space.
                let new_s = unsafe { target.copy_str(s) };
                Item {
                    payload: Payload { string: new_s },
                    meta: self.meta,
                }
            }
            TAG_TABLE => {
                // SAFETY: tag == TAG_TABLE guarantees payload.table is active.
                // Caller guarantees sufficient space for recursive emplace.
                let new_table = unsafe { self.payload.table.emplace_in(target) };
                Item {
                    payload: Payload {
                        table: ManuallyDrop::new(new_table),
                    },
                    meta: self.meta,
                }
            }
            TAG_ARRAY => {
                // SAFETY: tag == TAG_ARRAY guarantees payload.array is active.
                // Caller guarantees sufficient space for recursive emplace.
                let new_array = unsafe { self.payload.array.emplace_in(target) };
                Item {
                    payload: Payload {
                        array: ManuallyDrop::new(new_array),
                    },
                    meta: self.meta,
                }
            }
            _ => {
                // SAFETY: Non-string scalars (INTEGER, FLOAT, BOOLEAN, DATETIME)
                // contain no borrowed data. Item<'de> and Item<'static> have
                // identical layout. Item has no Drop impl.
                unsafe { std::mem::transmute_copy(self) }
            }
        }
    }
}

impl<'de> Item<'de> {
    /// Creates an "expected X, found Y" error using this value's type and span.
    #[inline]
    pub fn expected(&self, expected: &'static &'static str) -> Error {
        Error::new(
            ErrorKind::Wanted {
                expected,
                found: self.type_str(),
            },
            self.span_unchecked(),
        )
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
            return Err(self.expected(&"a string"));
        };
        match s.parse() {
            Ok(v) => Ok(v),
            Err(err) => Err(Error::custom(
                format!("failed to parse string: {err}"),
                self.span_unchecked(),
            )),
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
    /// Creates a key with no source span.
    ///
    /// Use this when constructing tables programmatically via
    /// [`Table::insert`].
    pub fn new(value: &'de str) -> Self {
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

impl<'de> From<&'de str> for Key<'de> {
    fn from(value: &'de str) -> Self {
        Self::new(value)
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

impl std::hash::Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

pub(crate) fn equal_items(a: &Item<'_>, b: &Item<'_>, index: Option<&TableIndex<'_>>) -> bool {
    if a.kind() != b.kind() {
        return false;
    }
    // SAFETY: The kind() equality check above guarantees both items hold the
    // same union variant. Each match arm reads only the union field that
    // corresponds to the matched Kind discriminant. Since both a and b have
    // the same kind, both payload accesses read the active field.
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
            Kind::DateTime => a.payload.datetime == b.payload.datetime,
            Kind::Array => {
                let a = a.payload.array.as_slice();
                let b = b.payload.array.as_slice();
                if a.len() != b.len() {
                    return false;
                }
                for i in 0..a.len() {
                    if !equal_items(&*a.as_ptr().add(i), &*b.as_ptr().add(i), index) {
                        return false;
                    }
                }
                true
            }
            Kind::Table => {
                let tab_a = a.as_table_unchecked();
                let tab_b = b.as_table_unchecked();
                if tab_a.len() != tab_b.len() {
                    return false;
                }
                for (key, val_a) in tab_a {
                    let Some((_, val_b)) = tab_b.value.get_entry_with_maybe_index(key.name, index)
                    else {
                        return false;
                    };
                    if !equal_items(val_a, val_b, index) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl<'de> PartialEq for Item<'de> {
    fn eq(&self, other: &Self) -> bool {
        equal_items(self, other, None)
    }
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

impl fmt::Debug for MaybeItem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.item() {
            Some(item) => item.fmt(f),
            None => f.write_str("None"),
        }
    }
}

/// A nullable reference to a parsed TOML value.
///
/// `MaybeItem` is returned by the index operators (`[]`) on [`Item`],
/// [`Table`], [`Array`], and `MaybeItem` itself. It acts like an
/// [`Option<&Item>`] that can be further indexed without panicking. Chained
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
/// "#, &arena).unwrap();
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
    payload: Payload {
        integer: Integer {
            value: PackedI128 { value: 0 },
        },
    },
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

    /// Returns an `i128` if this is an integer value.
    #[inline]
    pub fn as_i128(&self) -> Option<i128> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            Some(unsafe { self.payload.integer.as_i128() })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value that fits in the `i64` range.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_i64() }
        } else {
            None
        }
    }

    /// Returns a `u64` if this is an integer value that fits in the `u64` range.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        if self.tag() == TAG_INTEGER {
            // SAFETY: tag check guarantees the payload is an integer.
            unsafe { self.payload.integer.as_u64() }
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
