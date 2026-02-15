#![allow(unsafe_code)]
#![allow(clippy::manual_map)]
#[cfg(test)]
#[path = "./value_tests.rs"]
mod tests;
use crate::str::Str;
use crate::{Error, ErrorKind, Span, Table};
use std::fmt;
use std::mem::ManuallyDrop;

/// A toml array
pub use crate::array::Array;
/// A toml table: flat list of key-value pairs in insertion order
use crate::table::InnerTable;

pub(crate) const TAG_MASK: u32 = 0x7;
pub(crate) const TAG_SHIFT: u32 = 3;

pub(crate) const TAG_STRING: u32 = 0;
pub(crate) const TAG_INTEGER: u32 = 1;
pub(crate) const TAG_FLOAT: u32 = 2;
pub(crate) const TAG_BOOLEAN: u32 = 3;
pub(crate) const TAG_ARRAY: u32 = 4;
pub(crate) const TAG_TABLE: u32 = 5;

// Only set in maybe item
pub(crate) const TAG_NONE: u32 = 6;

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

#[repr(C)]
union Payload<'de> {
    string: Str<'de>,
    integer: i64,
    float: f64,
    boolean: bool,
    array: ManuallyDrop<Array<'de>>,
    table: ManuallyDrop<InnerTable<'de>>,
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
    start_and_tag: u32,
    end_and_flag: u32,
}

const _: () = assert!(std::mem::size_of::<Item<'_>>() == 24);
const _: () = assert!(std::mem::align_of::<Item<'_>>() == 8);

impl<'de> Item<'de> {
    #[inline]
    fn raw(tag: u32, flag: u32, start: u32, end: u32, payload: Payload<'de>) -> Self {
        Self {
            start_and_tag: (start << TAG_SHIFT) | tag,
            end_and_flag: (end << FLAG_SHIFT) | flag,
            payload,
        }
    }

    #[inline]
    pub(crate) fn string(s: Str<'de>, span: Span) -> Self {
        Self::raw(
            TAG_STRING,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { string: s },
        )
    }

    #[inline]
    pub(crate) fn integer(i: i64, span: Span) -> Self {
        Self::raw(
            TAG_INTEGER,
            FLAG_NONE,
            span.start,
            span.end,
            Payload { integer: i },
        )
    }

    #[inline]
    pub(crate) fn float(f: f64, span: Span) -> Self {
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
    pub(crate) fn array(a: Array<'de>, span: Span) -> Self {
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
    pub(crate) fn array_aot(a: Array<'de>, span: Span) -> Self {
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
}
#[derive(Clone, Copy)]
#[repr(u8)]
#[allow(unused)]
enum Kind {
    String = 0,
    Integer = 1,
    Float = 2,
    Boolean = 3,
    Array = 4,
    Table = 5,
}

impl<'de> Item<'de> {
    #[inline]
    fn kind(&self) -> Kind {
        unsafe { std::mem::transmute::<u8, Kind>(self.start_and_tag as u8 & 0x7) }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.start_and_tag & TAG_MASK
    }

    #[inline]
    pub(crate) fn flag(&self) -> u32 {
        self.end_and_flag & FLAG_MASK
    }

    /// Returns the byte-offset span of this value in the source document.
    #[inline]
    pub fn span(&self) -> Span {
        Span::new(
            self.start_and_tag >> TAG_SHIFT,
            self.end_and_flag >> FLAG_SHIFT,
        )
    }

    /// Returns the TOML type name (e.g. `"string"`, `"integer"`, `"table"`).
    #[inline]
    pub fn type_str(&self) -> &'static str {
        match self.tag() {
            TAG_STRING => "string",
            TAG_INTEGER => "integer",
            TAG_FLOAT => "float",
            TAG_BOOLEAN => "boolean",
            TAG_ARRAY => "array",
            _ => "table",
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

    /// Splits this array item into disjoint borrows of the span field and array payload.
    ///
    /// # Safety
    ///
    /// The caller must ensure `self.is_array()` is true.
    #[inline]
    pub(crate) unsafe fn split_array_end_flag(&mut self) -> (&mut u32, &mut Array<'de>) {
        debug_assert!(self.is_array());
        let ptr = self as *mut Item<'de>;
        unsafe {
            let end_flag = &mut *std::ptr::addr_of_mut!((*ptr).end_and_flag);
            let array = &mut *std::ptr::addr_of_mut!((*ptr).payload.array).cast::<Array<'de>>();
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
pub enum Value<'a, 'de> {
    /// A string value.
    String(&'a Str<'de>),
    /// An integer value.
    Integer(&'a i64),
    /// A floating-point value.
    Float(&'a f64),
    /// A boolean value.
    Boolean(&'a bool),
    /// An array value.
    Array(&'a Array<'de>),
    /// A table value.
    Table(&'a Table<'de>),
}

/// Mutable view into an [`Item`] for pattern matching.
///
/// Obtained via [`Item::value_mut`].
pub enum ValueMut<'a, 'de> {
    /// A string value.
    String(&'a mut Str<'de>),
    /// An integer value.
    Integer(&'a mut i64),
    /// A floating-point value.
    Float(&'a mut f64),
    /// A boolean value.
    Boolean(&'a mut bool),
    /// An array value.
    Array(&'a mut Array<'de>),
    /// A table value.
    Table(&'a mut Table<'de>),
}

impl<'de> Item<'de> {
    /// Returns a borrowed view for pattern matching.
    #[inline(never)]
    pub fn value(&self) -> Value<'_, 'de> {
        unsafe {
            match self.kind() {
                Kind::String => Value::String(&self.payload.string),
                Kind::Integer => Value::Integer(&self.payload.integer),
                Kind::Float => Value::Float(&self.payload.float),
                Kind::Boolean => Value::Boolean(&self.payload.boolean),
                Kind::Array => Value::Array(&self.payload.array),
                Kind::Table => Value::Table(self.as_spanned_table_unchecked()),
            }
        }
    }

    /// Returns a mutable view for pattern matching.
    #[inline(never)]
    pub fn value_mut(&mut self) -> ValueMut<'_, 'de> {
        unsafe {
            match self.kind() {
                Kind::String => ValueMut::String(&mut self.payload.string),
                Kind::Integer => ValueMut::Integer(&mut self.payload.integer),
                Kind::Float => ValueMut::Float(&mut self.payload.float),
                Kind::Boolean => ValueMut::Boolean(&mut self.payload.boolean),
                Kind::Array => ValueMut::Array(&mut self.payload.array),
                Kind::Table => ValueMut::Table(self.as_spanned_table_mut_unchecked()),
            }
        }
    }
}

impl<'de> Item<'de> {
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            Some(unsafe { &self.payload.string })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
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
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            Some(unsafe { &self.payload.array })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.is_table() {
            Some(unsafe { self.as_spanned_table_unchecked() })
        } else {
            None
        }
    }
    /// Returns a mutable array reference, or an error if this is not an array.
    pub fn expect_array(&mut self) -> Result<&mut Array<'de>, Error> {
        if self.is_array() {
            Ok(unsafe { &mut self.payload.array })
        } else {
            Err(self.expected("a array"))
        }
    }
    /// Returns a mutable table reference, or an error if this is not a table.
    ///
    /// This is the typical entry point for implementing [`Deserialize`](crate::Deserialize).
    pub fn expect_table(&mut self) -> Result<&mut Table<'de>, Error> {
        if self.is_table() {
            Ok(unsafe { self.as_spanned_table_mut_unchecked() })
        } else {
            Err(self.expected("a table"))
        }
    }

    /// Returns a mutable array reference.
    #[inline]
    pub fn as_array_mut(&mut self) -> Option<&mut Array<'de>> {
        if self.tag() == TAG_ARRAY {
            Some(unsafe { &mut self.payload.array })
        } else {
            None
        }
    }

    /// Returns a mutable table reference.
    #[inline]
    pub fn as_table_mut(&mut self) -> Option<&mut Table<'de>> {
        if self.is_table() {
            Some(unsafe { self.as_spanned_table_mut_unchecked() })
        } else {
            None
        }
    }

    /// Returns a mutable table pointer (parser-internal).
    #[inline]
    pub(crate) unsafe fn as_table_mut_unchecked(&mut self) -> &mut InnerTable<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut self.payload.table }
    }

    /// Reinterprets this [`Item`] as a [`Table`].
    ///
    /// SAFETY: The caller must ensure `self.is_table()` is true. Both types
    /// are `#[repr(C)]` with identical layout when the payload is a table.
    #[inline]
    pub(crate) unsafe fn as_spanned_table_mut_unchecked(&mut self) -> &mut Table<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut *(self as *mut Item<'de>).cast::<Table<'de>>() }
    }

    #[inline]
    pub(crate) unsafe fn as_spanned_table_unchecked(&self) -> &Table<'de> {
        debug_assert!(self.is_table());
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
            span: self.span(),
        }
    }

    /// Takes a string value and parses it via [`std::str::FromStr`].
    ///
    /// Returns an error if the value is not a string or parsing fails.
    #[inline]
    pub fn parse<T, E>(&mut self) -> Result<T, Error>
    where
        T: std::str::FromStr<Err = E>,
        E: std::fmt::Display,
    {
        let s = self.take_string(None)?;
        match s.parse() {
            Ok(v) => Ok(v),
            Err(err) => Err(Error {
                kind: ErrorKind::Custom(format!("failed to parse string: {err}").into()),
                span: self.span(),
            }),
        }
    }

    /// Takes the value as a string, returning an error if it is not a string.
    #[inline]
    pub fn take_string(&mut self, msg: Option<&'static str>) -> Result<Str<'de>, Error> {
        let span = self.span();
        match self.value() {
            Value::String(s) => Ok(*s),
            _ => Err(Error {
                kind: ErrorKind::Wanted {
                    expected: msg.unwrap_or("a string"),
                    found: self.type_str(),
                },
                span,
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
    pub name: Str<'de>,
    /// The byte-offset span of the key in the source document.
    pub span: Span,
}
impl<'de> Key<'de> {
    /// Returns the key name as a string slice.
    pub fn as_str(&self) -> &'de str {
        self.name.as_str()
    }
}

const _: () = assert!(std::mem::size_of::<Key<'_>>() == 24);

impl std::borrow::Borrow<str> for Key<'_> {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Display for Key<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Eq for Key<'_> {}

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
    start_and_tag: u32,
    end_and_flag: u32,
}

unsafe impl Sync for MaybeItem<'_> {}

pub(crate) static NONE: MaybeItem<'static> = MaybeItem {
    payload: Payload { integer: 0 },
    start_and_tag: TAG_NONE,
    end_and_flag: FLAG_NONE,
};

impl<'de> MaybeItem<'de> {
    /// Views an [`Item`] reference as a `MaybeItem`.
    pub fn from_ref<'a>(item: &'a Item<'de>) -> &'a Self {
        unsafe { &*(item as *const Item<'de>).cast::<MaybeItem<'de>>() }
    }
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.start_and_tag & TAG_MASK
    }
    /// Returns the underlying [`Item`], or [`None`] if this is a missing value.
    pub fn item(&self) -> Option<&Item<'de>> {
        if self.tag() != TAG_NONE {
            Some(unsafe { &*(self as *const MaybeItem<'de>).cast::<Item<'de>>() })
        } else {
            None
        }
    }
    /// Returns a borrowed string if this is a string value.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        if self.tag() == TAG_STRING {
            Some(unsafe { &self.payload.string })
        } else {
            None
        }
    }

    /// Returns an `i64` if this is an integer value.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
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
            Some(unsafe { self.payload.boolean })
        } else {
            None
        }
    }

    /// Returns a borrowed array if this is an array value.
    #[inline]
    pub fn as_array(&self) -> Option<&Array<'de>> {
        if self.tag() == TAG_ARRAY {
            Some(unsafe { &self.payload.array })
        } else {
            None
        }
    }

    /// Returns a borrowed table if this is a table value.
    #[inline]
    pub fn as_table(&self) -> Option<&Table<'de>> {
        if self.tag() == TAG_TABLE {
            Some(unsafe { &*(self as *const Self).cast::<Table<'de>>() })
        } else {
            None
        }
    }

    /// Returns the source span, or [`None`] if this is a missing value.
    pub fn span(&self) -> Option<Span> {
        if let Some(item) = self.item() {
            Some(item.span())
        } else {
            None
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
