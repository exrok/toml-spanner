use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::{self, Debug, Display},
    rc::Rc,
    sync::Arc,
};

use crate::{Arena, Array, Item, Key, Table, item::Value};

/// extracted out to avoid code bloat
fn optional_to_required<'a>(
    optional: Result<Option<Item<'a>>, ToTomlError>,
) -> Result<Item<'a>, ToTomlError> {
    match optional {
        Ok(Some(item)) => Ok(item),
        Ok(None) => Err(ToTomlError::from("required value was None")),
        Err(e) => Err(e),
    }
}

fn required_to_optional<'a>(
    required: Result<Item<'a>, ToTomlError>,
) -> Result<Option<Item<'a>>, ToTomlError> {
    match required {
        Ok(item) => Ok(Some(item)),
        Err(e) => Err(e),
    }
}

/// Trait for types that can be converted into a TOML [`Item`] tree.
///
/// Implement either [`to_toml`](Self::to_toml) or
/// [`to_optional_toml`](Self::to_optional_toml); default implementations
/// bridge between them. Built-in implementations cover primitive types,
/// `String`, `Vec<T>`, `HashMap`, `BTreeMap`, `Option<T>`, and more.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, Item, Key, Table, ToToml, ToTomlError};
///
/// struct Color { r: u8, g: u8, b: u8 }
///
/// impl ToToml for Color {
///     fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
///         let mut table = Table::new();
///         table.insert(Key::anon("r"), Item::from(self.r as i64), arena);
///         table.insert(Key::anon("g"), Item::from(self.g as i64), arena);
///         table.insert(Key::anon("b"), Item::from(self.b as i64), arena);
///         Ok(table.into_item())
///     }
/// }
/// ```
pub trait ToToml {
    /// Produces a TOML [`Item`] representing this value.
    ///
    /// Override this method when the value is always present. The default
    /// implementation delegates to [`to_optional_toml`](Self::to_optional_toml)
    /// and returns an error if `None` is produced.
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        optional_to_required(self.to_optional_toml(arena))
    }
    /// Produces an optional TOML [`Item`] representing this value.
    ///
    /// Override this method when the value may be absent (e.g. `Option<T>`
    /// returning `None` to omit the field). The default implementation
    /// delegates to [`to_toml`](Self::to_toml) and wraps the result in
    /// [`Some`].
    fn to_optional_toml<'a>(&'a self, arena: &'a Arena) -> Result<Option<Item<'a>>, ToTomlError> {
        required_to_optional(self.to_toml(arena))
    }
}

impl<K: ToToml> ToToml for BTreeSet<K> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut array) = Array::try_with_capacity(self.len(), arena) else {
            return length_of_array_exceeded_maximum();
        };
        for item in self {
            array.push(item.to_toml(arena)?, arena);
        }
        Ok(array.into_item())
    }
}

impl<K: ToToml, H> ToToml for std::collections::HashSet<K, H> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut array) = Array::try_with_capacity(self.len(), arena) else {
            return length_of_array_exceeded_maximum();
        };
        for item in self {
            array.push(item.to_toml(arena)?, arena);
        }
        Ok(array.into_item())
    }
}

impl<const N: usize, T: ToToml> ToToml for [T; N] {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.as_slice().to_toml(arena)
    }
}

impl<T: ToToml> ToToml for Option<T> {
    fn to_optional_toml<'a>(&'a self, arena: &'a Arena) -> Result<Option<Item<'a>>, ToTomlError> {
        match self {
            Some(value) => value.to_optional_toml(arena),
            None => Ok(None),
        }
    }
}

impl ToToml for str {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::string(self))
    }
}

impl ToToml for String {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::string(self))
    }
}

impl<T: ToToml> ToToml for Box<T> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(&*self, arena)
    }
}

impl<T: ToToml> ToToml for [T] {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut array) = Array::try_with_capacity(self.len(), arena) else {
            return length_of_array_exceeded_maximum();
        };
        for item in self {
            array.push(item.to_toml(arena)?, arena);
        }
        Ok(array.into_item())
    }
}

impl<T: ToToml> ToToml for Vec<T> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.as_slice().to_toml(arena)
    }
}

impl ToToml for f32 {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::from(*self as f64))
    }
}

impl ToToml for f64 {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::from(*self))
    }
}

impl ToToml for bool {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(Item::from(*self))
    }
}

impl<T: ToToml + ?Sized> ToToml for &T {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(self, arena)
    }
}

impl<T: ToToml + ?Sized> ToToml for &mut T {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(self, arena)
    }
}

impl<T: ToToml> ToToml for Rc<T> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(self, arena)
    }
}

impl<T: ToToml> ToToml for Arc<T> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(self, arena)
    }
}

impl<'b, T: ToToml + Clone> ToToml for Cow<'b, T> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        <T as ToToml>::to_toml(self, arena)
    }
}

impl ToToml for char {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let mut buf = [0; 4];
        Ok(Item::string(arena.alloc_str(self.encode_utf8(&mut buf))))
    }
}

impl ToToml for std::path::Path {
    fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        match self.to_str() {
            Some(s) => Ok(Item::string(s)),
            None => return ToTomlError::msg("path contains invalid UTF-8 characters"),
        }
    }
}

impl ToToml for std::path::PathBuf {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        self.as_path().to_toml(arena)
    }
}

impl ToToml for Array<'_> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(self.clone_in(arena).into_item())
    }
}

impl ToToml for Table<'_> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(self.clone_in(arena).into_item())
    }
}

impl ToToml for Item<'_> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        Ok(self.clone_in(arena))
    }
}

macro_rules! direct_upcast_integers {
    ($($tt:tt),*) => {
        $(impl ToToml for $tt {
            fn to_toml<'a>(&'a self, _: &'a Arena) -> Result<Item<'a>, ToTomlError> {
                Ok(Item::from(*self as i64))
            }
        })*
    };
}

direct_upcast_integers!(u8, i8, i16, u16, i32, u32, i64);

/// Trait for types that can be converted into flattened TOML table entries.
///
/// Used with `#[toml(flatten)]` on struct fields. Built-in implementations
/// exist for `HashMap` and `BTreeMap`.
///
/// If your type already implements [`ToToml`], you do not need to implement
/// this trait. Use `#[toml(flatten, with = flatten_any)]` in your derive
/// instead. See [`helper::flatten_any`](crate::helper::flatten_any).
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `ToFlattened`",
    note = "if `{Self}` implements `ToToml`, you can use `#[toml(flatten, with = flatten_any)]` instead of a manual `ToFlattened` impl"
)]
pub trait ToFlattened {
    /// Inserts this value's entries directly into an existing table.
    ///
    /// Each key-value pair is inserted into `table` rather than wrapping
    /// them in a nested sub-table.
    fn to_flattened<'a>(
        &'a self,
        arena: &'a Arena,
        table: &mut Table<'a>,
    ) -> Result<(), ToTomlError>;
}

/// Converts a map key to a TOML key string via `ToToml`.
fn key_to_str<'a>(item: &Item<'a>) -> Option<&'a str> {
    match item.value() {
        Value::String(s) => Some(*s),
        _ => None,
    }
}

impl<K: ToToml, V: ToToml> ToFlattened for BTreeMap<K, V> {
    fn to_flattened<'a>(
        &'a self,
        arena: &'a Arena,
        table: &mut Table<'a>,
    ) -> Result<(), ToTomlError> {
        for (k, v) in self {
            let key_item = k.to_toml(arena)?;
            let Some(key_str) = key_to_str(&key_item) else {
                return map_key_did_not_serialize_to_string();
            };
            table.insert(Key::anon(key_str), v.to_toml(arena)?, arena);
        }
        Ok(())
    }
}

impl<K: ToToml, V: ToToml, H> ToFlattened for HashMap<K, V, H> {
    fn to_flattened<'a>(
        &'a self,
        arena: &'a Arena,
        table: &mut Table<'a>,
    ) -> Result<(), ToTomlError> {
        for (k, v) in self {
            let key_item = k.to_toml(arena)?;
            let Some(key_str) = key_to_str(&key_item) else {
                return map_key_did_not_serialize_to_string();
            };
            table.insert(Key::anon(key_str), v.to_toml(arena)?, arena);
        }
        Ok(())
    }
}

impl<K: ToToml, V: ToToml> ToToml for BTreeMap<K, V> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut table) = Table::try_with_capacity(self.len(), arena) else {
            return length_of_table_exceeded_maximum();
        };
        self.to_flattened(arena, &mut table)?;
        Ok(table.into_item())
    }
}

impl<K: ToToml, V: ToToml, H> ToToml for HashMap<K, V, H> {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut table) = Table::try_with_capacity(self.len(), arena) else {
            return length_of_table_exceeded_maximum();
        };
        self.to_flattened(arena, &mut table)?;
        Ok(table.into_item())
    }
}

#[cold]
fn map_key_did_not_serialize_to_string() -> Result<(), ToTomlError> {
    Err(ToTomlError::from("map key did not serialize to a string"))
}
#[cold]
fn length_of_array_exceeded_maximum<T>() -> Result<T, ToTomlError> {
    Err(ToTomlError::from(
        "length of array exceeded maximum capacity",
    ))
}

#[cold]
fn length_of_table_exceeded_maximum<T>() -> Result<T, ToTomlError> {
    Err(ToTomlError::from(
        "length of table exceeded maximum capacity",
    ))
}

/// An error produced during [`ToToml`] conversion or TOML emission.
///
/// Returned by [`to_string`](crate::to_string),
/// [`to_string_with`](crate::to_string_with), and
/// [`ToToml::to_toml`].
pub struct ToTomlError {
    /// The error message.
    pub message: Cow<'static, str>,
}

impl ToTomlError {
    /// Returns `Err(ToTomlError)` with the given static message.
    #[cold]
    pub fn msg<T>(msg: &'static str) -> Result<T, Self> {
        Err(Self {
            message: Cow::Borrowed(msg),
        })
    }
}

impl Display for ToTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Debug for ToTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToTomlError")
            .field("message", &self.message)
            .finish()
    }
}

impl std::error::Error for ToTomlError {}

impl From<Cow<'static, str>> for ToTomlError {
    fn from(message: Cow<'static, str>) -> Self {
        Self { message }
    }
}

impl From<&'static str> for ToTomlError {
    fn from(message: &'static str) -> Self {
        Self {
            message: Cow::Borrowed(message),
        }
    }
}
