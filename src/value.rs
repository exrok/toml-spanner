#![allow(unsafe_code)]

//! Contains the [`Value`] tagged union: a 24-byte TOML value with inline span.

use crate::str::Str;
use crate::{Error, ErrorKind, Span};
use std::mem::ManuallyDrop;
use std::{fmt, ptr};

/// A toml array
pub use crate::array::Array;
/// A toml table: flat list of key-value pairs in insertion order
pub use crate::table::Table;

const TAG_MASK: u32 = 0x7;
const TAG_SHIFT: u32 = 3;

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
    string: ManuallyDrop<Str<'de>>,
    integer: i64,
    float: f64,
    boolean: bool,
    array: ManuallyDrop<Array<'de>>,
    table: ManuallyDrop<Table<'de>>,
}

#[repr(C)]
pub(crate) struct SpannedTable<'de> {
    /// Bits 2-0: tag, bits 31-3: span.start
    start_and_tag: u32,
    /// Bit 0: flag bit (parser-internal), bits 31-1: span.end
    end_and_flag: u32,
    pub(crate) value: ManuallyDrop<Table<'de>>,
}

const _: () = assert!(std::mem::size_of::<SpannedTable<'_>>() == std::mem::size_of::<Value<'_>>());
const _: () =
    assert!(std::mem::align_of::<SpannedTable<'_>>() == std::mem::align_of::<Value<'_>>());

impl<'de> SpannedTable<'de> {
    #[inline]
    pub(crate) fn span_start(&self) -> u32 {
        self.start_and_tag >> TAG_SHIFT
    }

    #[inline]
    pub(crate) fn set_span_start(&mut self, v: u32) {
        self.start_and_tag = (v << TAG_SHIFT) | (self.start_and_tag & TAG_MASK);
    }

    #[inline]
    pub(crate) fn set_span_end(&mut self, v: u32) {
        self.end_and_flag = (v << FLAG_SHIFT) | (self.end_and_flag & FLAG_BIT);
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        let old = self.end_and_flag;
        let current = old >> FLAG_SHIFT;
        self.end_and_flag = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_BIT);
    }

    #[inline]
    pub(crate) fn set_header_tag(&mut self) {
        self.start_and_tag = (self.start_and_tag & !TAG_MASK) | TAG_TABLE_HEADER;
    }
}

/// A parsed TOML value with inline span information.
///
/// This is a 24-byte `#[repr(C)]` tagged union. The tag and span are packed
/// into two `u32` fields; the payload is a 16-byte union.
#[repr(C)]
pub struct Value<'de> {
    /// Bits 2-0: tag, bits 31-3: span.start
    start_and_tag: u32,
    /// Bit 0: flag bit (parser-internal), bits 31-1: span.end
    end_and_flag: u32,
    payload: Payload<'de>,
}

const _: () = assert!(std::mem::size_of::<Value<'_>>() == 24);
const _: () = assert!(std::mem::align_of::<Value<'_>>() == 8);

impl<'de> Value<'de> {
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
        Self::raw(
            TAG_STRING,
            span.start(),
            span.end(),
            Payload {
                string: ManuallyDrop::new(s),
            },
        )
    }

    #[inline]
    pub(crate) fn integer(i: i64, span: Span) -> Self {
        Self::raw(
            TAG_INTEGER,
            span.start(),
            span.end(),
            Payload { integer: i },
        )
    }

    #[inline]
    pub(crate) fn float(f: f64, span: Span) -> Self {
        Self::raw(TAG_FLOAT, span.start(), span.end(), Payload { float: f })
    }

    #[inline]
    pub(crate) fn boolean(b: bool, span: Span) -> Self {
        Self::raw(
            TAG_BOOLEAN,
            span.start(),
            span.end(),
            Payload { boolean: b },
        )
    }

    #[inline]
    pub(crate) fn array(a: Array<'de>, span: Span) -> Self {
        Self::raw(
            TAG_ARRAY,
            span.start(),
            span.end(),
            Payload {
                array: ManuallyDrop::new(a),
            },
        )
    }

    #[inline]
    pub(crate) fn table(t: Table<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE,
            span.start(),
            span.end(),
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
    pub(crate) fn table_frozen(t: Table<'de>, span: Span) -> Self {
        let mut v = Self::table(t, span);
        v.end_and_flag |= FLAG_BIT;
        v
    }

    /// Creates a table with HEADER tag (explicitly opened by `[header]`).
    #[inline]
    pub(crate) fn table_header(t: Table<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE_HEADER,
            span.start(),
            span.end(),
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }

    /// Creates a table with DOTTED tag (created by dotted-key navigation).
    #[inline]
    pub(crate) fn table_dotted(t: Table<'de>, span: Span) -> Self {
        Self::raw(
            TAG_TABLE_DOTTED,
            span.start(),
            span.end(),
            Payload {
                table: ManuallyDrop::new(t),
            },
        )
    }
}

impl<'de> Value<'de> {
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

    /// Split this array `Value` into disjoint borrows: `&mut u32` for the
    /// `end_and_flag` span field (bytes \[4..8\)) and `&mut Array` for the
    /// payload (bytes \[8..24\)).
    ///
    /// SAFETY: The caller must ensure `self.is_array()` is true.
    #[inline]
    pub(crate) unsafe fn split_array_end_flag(&mut self) -> (&mut u32, &mut Array<'de>) {
        debug_assert!(self.is_array());
        let ptr = self as *mut Value<'de>;
        unsafe {
            let end_flag = &mut *std::ptr::addr_of_mut!((*ptr).end_and_flag);
            let array = &mut *std::ptr::addr_of_mut!((*ptr).payload.array).cast::<Array<'de>>();
            (end_flag, array)
        }
    }
}

/// Borrowed view into a [`Value`] for pattern matching.
pub enum ValueRef<'a, 'de> {
    /// A string
    String(&'a Str<'de>),
    /// An integer
    Integer(i64),
    /// A float
    Float(f64),
    /// A boolean
    Boolean(bool),
    /// An array
    Array(&'a Array<'de>),
    /// A table
    Table(&'a Table<'de>),
}

/// Mutable view into a [`Value`] for pattern matching.
pub enum ValueMut<'a, 'de> {
    /// A string
    String(&'a mut Str<'de>),
    /// An integer
    Integer(&'a mut i64),
    /// A float
    Float(&'a mut f64),
    /// A boolean
    Boolean(&'a mut bool),
    /// An array
    Array(&'a mut Array<'de>),
    /// A table
    Table(&'a mut Table<'de>),
}

/// Owned value extracted from a [`Value`] for pattern matching.
pub enum ValueOwned<'de> {
    /// A string
    String(Str<'de>),
    /// An integer
    Integer(i64),
    /// A float
    Float(f64),
    /// A boolean
    Boolean(bool),
    /// An array
    Array(Array<'de>),
    /// A table
    Table(Table<'de>),
}

impl ValueOwned<'_> {
    /// Gets the type of the value as a string.
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::String(..) => "string",
            Self::Integer(..) => "integer",
            Self::Float(..) => "float",
            Self::Boolean(..) => "boolean",
            Self::Array(..) => "array",
            Self::Table(..) => "table",
        }
    }
}

impl fmt::Debug for ValueOwned<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => s.fmt(f),
            Self::Integer(i) => i.fmt(f),
            Self::Float(v) => v.fmt(f),
            Self::Boolean(b) => b.fmt(f),
            Self::Array(a) => a.fmt(f),
            Self::Table(t) => t.fmt(f),
        }
    }
}

impl<'de> Value<'de> {
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
                _ => ValueRef::Table(&self.payload.table),
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
                _ => ValueMut::Table(&mut self.payload.table),
            }
        }
    }

    /// Consumes the value and returns an owned kind for pattern matching.
    ///
    /// The span information is lost; call [`Self::span()`] before this if needed.
    #[inline]
    pub fn into_kind(self) -> ValueOwned<'de> {
        let tag = self.tag();
        let me = ManuallyDrop::new(self);
        unsafe {
            match tag {
                TAG_STRING => ValueOwned::String(ptr::read(&*me.payload.string)),
                TAG_INTEGER => ValueOwned::Integer(me.payload.integer),
                TAG_FLOAT => ValueOwned::Float(me.payload.float),
                TAG_BOOLEAN => ValueOwned::Boolean(me.payload.boolean),
                TAG_ARRAY => ValueOwned::Array(ptr::read(&*me.payload.array)),
                _ => ValueOwned::Table(ptr::read(&*me.payload.table)),
            }
        }
    }
}

impl<'de> Value<'de> {
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
            Some(unsafe { &self.payload.table })
        } else {
            None
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
            Some(unsafe { &mut self.payload.table })
        } else {
            None
        }
    }

    /// Returns a mutable table pointer (parser-internal).
    #[inline]
    pub(crate) unsafe fn as_table_mut_unchecked(&mut self) -> &mut Table<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut self.payload.table }
    }

    /// Reinterpret this `Value` as a `SpannedTable`.
    ///
    /// SAFETY: The caller must ensure `self.is_table()` is true. Both types
    /// are `#[repr(C)]` with identical layout when the payload is a table.
    #[inline]
    pub(crate) unsafe fn as_spanned_table_mut_unchecked(&mut self) -> &mut SpannedTable<'de> {
        debug_assert!(self.is_table());
        unsafe { &mut *(self as *mut Value<'de>).cast::<SpannedTable<'de>>() }
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

impl<'de> Value<'de> {
    /// Takes the payload, replacing self with `Boolean(false)`.
    /// The span is preserved.
    #[inline]
    pub fn take(&mut self) -> ValueOwned<'de> {
        let span = self.span();
        let old = std::mem::replace(self, Value::boolean(false, span));
        old.into_kind()
    }

    /// Takes the value as a string, returning an error if it is not a string.
    #[inline]
    pub fn take_string(&mut self, msg: Option<&'static str>) -> Result<Str<'de>, Error> {
        let span = self.span();
        match self.take() {
            ValueOwned::String(s) => Ok(s),
            other => Err(Error {
                kind: ErrorKind::Wanted {
                    expected: msg.unwrap_or("a string"),
                    found: other.type_str(),
                },
                span,
                line_info: None,
            }),
        }
    }

    /// Replace payload with a table, preserving the span.
    pub fn set_table(&mut self, table: Table<'de>) {
        let span = self.span();
        let old = std::mem::replace(self, Value::table(table, span));
        drop(old);
    }
}

impl Drop for Value<'_> {
    fn drop(&mut self) {
        match self.tag() {
            TAG_STRING => unsafe { ManuallyDrop::drop(&mut self.payload.string) },
            TAG_ARRAY => unsafe { ManuallyDrop::drop(&mut self.payload.array) },
            TAG_TABLE | TAG_TABLE_HEADER | TAG_TABLE_DOTTED => unsafe {
                ManuallyDrop::drop(&mut self.payload.table);
            },
            _ => {}
        }
    }
}

impl fmt::Debug for Value<'_> {
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
impl serde::Serialize for Value<'_> {
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

/// A toml table key
#[derive(Clone)]
pub struct Key<'de> {
    /// The key itself, in most cases it will be borrowed, but may be owned
    /// if escape characters are present in the original source
    pub name: Str<'de>,
    /// The span for the key in the original document
    pub span: Span,
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
