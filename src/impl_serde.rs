//! Serde serialization support for [`Spanned<T>`](crate::Spanned) and
//! [`Item`](crate::Item).
//!
//! Enabled by the `serde` feature flag. This provides [`serde::Serialize`]
//! implementations only — deserialization uses the [`FromToml`](crate::FromToml)
//! trait instead.

use crate::Spanned;
use crate::item::Value;
use crate::item::table::InnerTable;
use crate::parser::Document;
use crate::{Item, Table};

impl<T> serde::Serialize for Spanned<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl serde::Serialize for Item<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.value() {
            Value::String(s) => ser.serialize_str(s),
            Value::Integer(i) => ser.serialize_i128(i.as_i128()),
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
                    map.serialize_entry(k.name, v)?;
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

impl serde::Serialize for InnerTable<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = ser.serialize_map(Some(self.len()))?;
        for (k, v) in self.entries() {
            map.serialize_entry(k.name, v)?;
        }
        map.end()
    }
}

impl serde::Serialize for Table<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(ser)
    }
}

impl serde::Serialize for Document<'_> {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.table.serialize(ser)
    }
}
