#![allow(unsafe_code)]

//! Contains the [`Item`] tagged union: a 24-byte TOML value with inline span.

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
pub(crate) const TAG_TABLE_HEADER: u32 = 6;
pub(crate) const TAG_TABLE_DOTTED: u32 = 7;

pub(crate) const FLAG_BIT: u32 = 1;
pub(crate) const FLAG_SHIFT: u32 = 1;

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
#[repr(C)]
pub struct Item<'de> {
    /// Bits 2-0: tag, bits 31-3: span.start
    start_and_tag: u32,
    /// Bit 0: flag bit (parser-internal), bits 31-1: span.end
    end_and_flag: u32,
    payload: Payload<'de>,
}

const _: () = assert!(std::mem::size_of::<Item<'_>>() == 24);
const _: () = assert!(std::mem::align_of::<Item<'_>>() == 8);

impl<'de> Item<'de> {
    #[inline]
    fn raw(tag: u32, start: u32, end: u32, payload: Payload<'de>) -> Self {
        Self {
            start_and_tag: (start << TAG_SHIFT) | tag,
            end_and_flag: end << FLAG_SHIFT,
            payload,
        }
    }

    #[inline]
    pub(crate) fn string(s: Str<'de>, span: Span) -> Self {
        Self::raw(TAG_STRING, span.start, span.end, Payload { string: s })
    }

    #[inline]
    pub(crate) fn integer(i: i64, span: Span) -> Self {
        Self::raw(TAG_INTEGER, span.start, span.end, Payload { integer: i })
    }

    #[inline]
    pub(crate) fn float(f: f64, span: Span) -> Self {
        Self::raw(TAG_FLOAT, span.start, span.end, Payload { float: f })
    }

    #[inline]
    pub(crate) fn boolean(b: bool, span: Span) -> Self {
        Self::raw(TAG_BOOLEAN, span.start, span.end, Payload { boolean: b })
    }

    #[inline]
    pub(crate) fn array(a: Array<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
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
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates an array-of-tables value (flag bit set).
    #[inline]
    pub(crate) fn array_aot(a: Array<'de>, span: Span) -> Self {
        let mut v = Self::array(a, span);
        v.end_and_flag |= FLAG_BIT;
        v
    }

    /// Creates a frozen (inline) table value (flag bit set).
    #[inline]
    pub(crate) fn table_frozen(t: InnerTable<'de>, span: Span) -> Self {
        let mut v = Self::table(t, span);
        v.end_and_flag |= FLAG_BIT;
        v
    }

    /// Creates a table with HEADER tag (explicitly opened by `[header]`).
    #[inline]
    pub(crate) fn table_header(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE_HEADER,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with DOTTED tag (created by dotted-key navigation).
    #[inline]
    pub(crate) fn table_dotted(t: InnerTable<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE_DOTTED,
            span.start,
            span.end,
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }
}

impl<'de> Item<'de> {
    #[inline]
    pub(crate) fn tag(&self) -> u32 {
        self.start_and_tag & TAG_MASK
    }

    /// Returns the source span (tag and flag bits masked out).
    #[inline]
    pub fn span(&self) -> Span {
        Span::new(
            self.start_and_tag >> TAG_SHIFT,
            self.end_and_flag >> FLAG_SHIFT,
        )
    }

    /// Gets the type of the value as a string.
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
        self.tag() >= TAG_TABLE
    }

    #[inline]
    pub(crate) fn is_array(&self) -> bool {
        self.tag() == TAG_ARRAY
    }

    #[inline]
    pub(crate) fn is_frozen(&self) -> bool {
        self.end_and_flag & FLAG_BIT != 0
    }

    #[inline]
    pub(crate) fn is_aot(&self) -> bool {
        self.tag() == TAG_ARRAY && self.end_and_flag & FLAG_BIT != 0
    }

    #[inline]
    pub(crate) fn has_header_bit(&self) -> bool {
        self.tag() == TAG_TABLE_HEADER
    }

    #[inline]
    pub(crate) fn has_dotted_bit(&self) -> bool {
        self.tag() == TAG_TABLE_DOTTED
    }

    /// Splits this array [`Item`] into disjoint borrows: `&mut u32` for the
    /// `end_and_flag` span field (bytes \[4..8\)) and `&mut Array` for the
    /// payload (bytes \[8..24\)).
    ///
    /// SAFETY: The caller must ensure `self.is_array()` is true.
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
pub enum ValueRef<'a, 'de> {
    String(&'a Str<'de>),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(&'a Array<'de>),
    Table(&'a Table<'de>),
}

/// Mutable view into an [`Item`] for pattern matching.
pub enum ValueMut<'a, 'de> {
    String(&'a mut Str<'de>),
    Integer(&'a mut i64),
    Float(&'a mut f64),
    Boolean(&'a mut bool),
    Array(&'a mut Array<'de>),
    Table(&'a mut Table<'de>),
}

impl<'de> Item<'de> {
    /// Returns a borrowed view for pattern matching.
    #[inline]
    pub fn as_ref(&self) -> ValueRef<'_, 'de> {
        unsafe {
            match self.tag() {
                TAG_STRING => ValueRef::String(&self.payload.string),
                TAG_INTEGER => ValueRef::Integer(self.payload.integer),
                TAG_FLOAT => ValueRef::Float(self.payload.float),
                TAG_BOOLEAN => ValueRef::Boolean(self.payload.boolean),
                TAG_ARRAY => ValueRef::Array(&self.payload.array),
                _ => ValueRef::Table(self.as_spanned_table_unchecked()),
            }
        }
    }

    /// Returns a mutable view for pattern matching.
    #[inline]
    pub fn as_mut(&mut self) -> ValueMut<'_, 'de> {
        unsafe {
            match self.tag() {
                TAG_STRING => ValueMut::String(&mut self.payload.string),
                TAG_INTEGER => ValueMut::Integer(&mut self.payload.integer),
                TAG_FLOAT => ValueMut::Float(&mut self.payload.float),
                TAG_BOOLEAN => ValueMut::Boolean(&mut self.payload.boolean),
                TAG_ARRAY => ValueMut::Array(&mut self.payload.array),
                _ => ValueMut::Table(self.as_spanned_table_mut_unchecked()),
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
    pub fn as_integer(&self) -> Option<i64> {
        if self.tag() == TAG_INTEGER {
            Some(unsafe { self.payload.integer })
        } else {
            None
        }
    }

    /// Returns an `f64` if this is a float value.
    #[inline]
    pub fn as_float(&self) -> Option<f64> {
        if self.tag() == TAG_FLOAT {
            Some(unsafe { self.payload.float })
        } else {
            None
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
    pub fn expect_array(&mut self) -> Result<&mut Array<'de>, Error> {
        if self.is_array() {
            Ok(unsafe { &mut self.payload.array })
        } else {
            Err(self.expected("a array"))
        }
    }
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
    /// Constructs an [`ErrorKind::Wanted`] error from this value.
    ///
    /// Uses `self.type_str()` as the `found` field and `self.span()` as the span.
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
        match self.as_ref() {
            ValueRef::String(s) => Ok(*s),
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
        unsafe {
            match self.tag() {
                TAG_STRING => self.payload.string.fmt(f),
                TAG_INTEGER => self.payload.integer.fmt(f),
                TAG_FLOAT => self.payload.float.fmt(f),
                TAG_BOOLEAN => self.payload.boolean.fmt(f),
                TAG_ARRAY => self.payload.array.fmt(f),
                _ => self.payload.table.fmt(f),
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
        match self.as_ref() {
            ValueRef::String(s) => ser.serialize_str(s),
            ValueRef::Integer(i) => ser.serialize_i64(i),
            ValueRef::Float(f) => ser.serialize_f64(f),
            ValueRef::Boolean(b) => ser.serialize_bool(b),
            ValueRef::Array(arr) => {
                use serde::ser::SerializeSeq;
                let mut seq = ser.serialize_seq(Some(arr.len()))?;
                for ele in arr {
                    seq.serialize_element(ele)?;
                }
                seq.end()
            }
            ValueRef::Table(tab) => {
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
        for (k, v) in self {
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

/// A TOML table key with span information.
#[derive(Copy, Clone)]
pub struct Key<'de> {
    /// The key name, borrowed from the TOML source or the parser arena.
    pub name: Str<'de>,
    /// The span for the key in the original document.
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

#[cfg(test)]
#[path = "./value_tests.rs"]
mod tests;
