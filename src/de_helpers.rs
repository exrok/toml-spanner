//! Provides helpers for deserializing [`Value`]/[`ValueKindOwned`] into Rust types

use crate::{
    DeserError, Deserialize, Error, ErrorKind, Span,
    span::Spanned,
    str::Str,
    value::{self, Item, Table},
};
use std::{fmt::Display, str::FromStr};

/// Helper for constructing an [`ErrorKind::Wanted`]
#[inline]
pub fn expected(expected: &'static str, found: &'static str, span: Span) -> Error {
    Error {
        kind: ErrorKind::Wanted { expected, found },
        span,
        line_info: None,
    }
}

/// Attempts to acquire a string and parse it, returning an error
/// if the value is not a string, or the parse implementation fails
#[inline]
pub fn parse<T, E>(value: &mut Item<'_>) -> Result<T, Error>
where
    T: FromStr<Err = E>,
    E: Display,
{
    let s = value.take_string(None)?;
    match s.parse() {
        Ok(v) => Ok(v),
        Err(err) => Err(Error {
            kind: ErrorKind::Custom(format!("failed to parse string: {err}").into()),
            span: value.span(),
            line_info: None,
        }),
    }
}

/// A helper for dealing with tables
pub struct TableHelper<'de> {
    /// The table the helper is operating upon
    pub table: Table<'de>,
    /// The span for the table location
    span: Span,
}

impl<'de> From<(Table<'de>, Span)> for TableHelper<'de> {
    fn from((table, span): (Table<'de>, Span)) -> Self {
        Self { table, span }
    }
}

impl<'de> TableHelper<'de> {
    /// Creates a helper for the value, failing if it is not a table
    pub fn new(value: &mut Item<'de>) -> Result<Self, DeserError> {
        let span = value.span();
        let value::ValueMut::Table(t) = value.as_mut() else {
            return Err(expected("a table", value.type_str(), span).into());
        };
        let table = std::mem::take(t);

        Ok(Self { table, span })
    }

    /// Returns true if the table contains the specified key
    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.table.contains_key(name)
    }

    /// Takes the specified key and its value if it exists
    #[inline]
    pub fn take(&mut self, name: &str) -> Option<(value::Key<'de>, Item<'de>)> {
        self.table.remove_entry(name)
    }

    /// Attempts to deserialize the specified key
    ///
    /// # Errors
    /// - The key does not exist
    /// - The [`Deserialize`] implementation for the type returns an error
    #[inline]
    pub fn required<T: Deserialize<'de>>(
        &mut self,
        name: &'static str,
    ) -> Result<T, DeserError> {
        Ok(self.required_s(name)?.value)
    }

    /// The same as [`Self::required`], except it returns a [`Spanned`]
    pub fn required_s<T: Deserialize<'de>>(
        &mut self,
        name: &'static str,
    ) -> Result<Spanned<T>, DeserError> {
        let Some(mut val) = self.table.remove(name) else {
            return Err(Error {
                kind: ErrorKind::MissingField(name),
                span: self.span,
                line_info: None,
            }
            .into());
        };

        Spanned::<T>::deserialize(&mut val)
    }

    /// Attempts to deserialize the specified key, if it exists
    #[inline]
    pub fn optional<T: Deserialize<'de>>(
        &mut self,
        name: &str,
    ) -> Result<Option<T>, DeserError> {
        Ok(self.optional_s(name)?.map(|v| v.value))
    }

    /// The same as [`Self::optional`], except it returns a [`Spanned`]
    pub fn optional_s<T: Deserialize<'de>>(
        &mut self,
        name: &str,
    ) -> Result<Option<Spanned<T>>, DeserError> {
        let Some(mut val) = self.table.remove(name) else {
            return Ok(None);
        };

        Spanned::<T>::deserialize(&mut val).map(Some)
    }

    /// Called when you are finished with this [`TableHelper`]
    ///
    /// If [`Option::None`] is passed, any keys that still exist in the table
    /// will produce an [`ErrorKind::UnexpectedKeys`] error, equivalent to
    /// [`#[serde(deny_unknown_fields)]`](https://serde.rs/container-attrs.html#deny_unknown_fields)
    ///
    /// If you want to simulate [`#[serde(flatten)]`](https://serde.rs/field-attrs.html#flatten)
    /// you can instead put that table back in its original value during this step
    pub fn finalize(self, original: Option<&mut Item<'de>>) -> Result<(), DeserError> {
        if let Some(original) = original {
            original.set_table(self.table);
        } else if !self.table.is_empty() {
            let keys = self
                .table
                .into_keys()
                .map(|key| (key.name.into(), key.span))
                .collect();

            return Err(Error::from((ErrorKind::UnexpectedKeys { keys }, self.span)).into());
        }

        Ok(())
    }
}

impl<'de> Deserialize<'de> for String {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        value
            .take_string(None)
            .map(String::from)
            .map_err(DeserError::from)
    }
}

impl<'de> Deserialize<'de> for Str<'de> {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        value.take_string(None).map_err(DeserError::from)
    }
}

impl<'de> Deserialize<'de> for std::borrow::Cow<'de, str> {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        value
            .take_string(None)
            .map(std::borrow::Cow::from)
            .map_err(DeserError::from)
    }
}

impl<'de> Deserialize<'de> for bool {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        match value.as_bool() {
            Some(b) => Ok(b),
            None => Err(expected("a bool", value.type_str(), value.span()).into()),
        }
    }
}

fn deser_integer(
    value: &mut Item<'_>,
    min: i64,
    max: i64,
    name: &'static str,
) -> Result<i64, DeserError> {
    let span = value.span();
    match value.as_integer() {
        Some(i) if i >= min && i <= max => Ok(i),
        Some(_) => Err(DeserError::from(Error {
            kind: ErrorKind::OutOfRange(name),
            span,
            line_info: None,
        })),
        None => Err(expected("an integer", value.type_str(), span).into()),
    }
}

macro_rules! integer {
    ($($num:ty),+) => {$(
        impl<'de> Deserialize<'de> for $num {
            fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
                match deser_integer(value, <$num>::MIN as i64, <$num>::MAX as i64, stringify!($num)) {
                    Ok(i) => Ok(i as $num),
                    Err(e) => Err(e),
                }
            }
        }
    )+};
}

integer!(i8, i16, i32, isize, u8, u16, u32);

impl<'de> Deserialize<'de> for i64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        deser_integer(value, i64::MIN, i64::MAX, "i64")
    }
}

impl<'de> Deserialize<'de> for u64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        match deser_integer(value, 0, i64::MAX, "u64") {
            Ok(i) => Ok(i as u64),
            Err(e) => Err(e),
        }
    }
}

impl<'de> Deserialize<'de> for usize {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        const MAX: i64 = if usize::BITS < 64 {
            usize::MAX as i64
        } else {
            i64::MAX
        };
        match deser_integer(value, 0, MAX, "usize") {
            Ok(i) => Ok(i as usize),
            Err(e) => Err(e),
        }
    }
}

impl<'de> Deserialize<'de> for f32 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        match value.as_float() {
            Some(f) => Ok(f as f32),
            None => Err(expected("a float", value.type_str(), value.span()).into()),
        }
    }
}

impl<'de> Deserialize<'de> for f64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, DeserError> {
        match value.as_float() {
            Some(f) => Ok(f),
            None => Err(expected("a float", value.type_str(), value.span()).into()),
        }
    }
}

impl<'de, T> Deserialize<'de> for Vec<T>
where
    T: Deserialize<'de>,
{
    fn deserialize(value: &mut value::Item<'de>) -> Result<Self, DeserError> {
        let span = value.span();
        let value::ValueMut::Array(arr) = value.as_mut() else {
            return Err(expected("an array", value.type_str(), span).into());
        };
        let arr = std::mem::take(arr);

        let mut errors = Vec::new();
        let mut s = Vec::new();
        for mut v in arr {
            match T::deserialize(&mut v) {
                Ok(v) => s.push(v),
                Err(mut err) => errors.append(&mut err.errors),
            }
        }

        if errors.is_empty() {
            Ok(s)
        } else {
            Err(DeserError {
                errors: Box::new(errors),
            })
        }
    }
}
