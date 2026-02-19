//! A high-performance TOML parser that preserves span information for every
//! parsed value.
//!
//! Strings are zero-copy where possible, borrowing directly from the input;
//! escape sequences are allocated into a caller-supplied [`Arena`].
//!
//! # Quick start
//!
//! Call [`parse`] with a TOML string and an [`Arena`] to get a [`Table`]. Then
//! extract values with [`Table::required`] and [`Table::optional`], or index
//! into nested structures with bracket operators that return [`MaybeItem`]
//! (never panic on missing keys).
//!
//! # Examples
//!
//! ```
//! use toml_spanner::{Arena, Deserialize, Error, Item};
//!
//! #[derive(Debug)]
//! struct Things {
//!     name: String,
//!     value: u32,
//!     color: Option<String>,
//! }
//!
//! impl<'de> Deserialize<'de> for Things {
//!     fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
//!         let table = value.expect_table()?;
//!         Ok(Things {
//!             name: table.required("name")?,
//!             value: table.required("value")?,
//!             color: table.optional("color")?,
//!         })
//!     }
//! }
//!
//! let content = r#"
//! dev-mode = true
//!
//! [[things]]
//! name = "hammer"
//! value = 43
//!
//! [[things]]
//! name = "drill"
//! value = 300
//! color = "green"
//! "#;
//!
//! let arena = Arena::new();
//! let mut table = toml_spanner::parse(content, &arena)?;
//!
//! // Null-coalescing index operators — missing keys return a None-like
//! // MaybeItem instead of panicking.
//! assert_eq!(table["things"][0]["color"].as_str(), None);
//! assert_eq!(table["things"][1]["color"].as_str(), Some("green"));
//!
//! // Deserialize typed values out of the table.
//! let things: Vec<Things> = table.required("things")?;
//! let dev_mode: bool = table.optional("dev-mode")?.unwrap_or(false);
//! // Error if unconsumed fields remain.
//! table.expect_empty()?;
//!
//! assert_eq!(things.len(), 2);
//! assert_eq!(things[0].name, "hammer");
//! assert!(dev_mode);
//! # Ok::<(), Error>(())
//! ```

mod arena;
mod array;
mod de;
mod error;
mod parser;
mod span;
mod table;
mod time;
mod value;

pub use arena::Arena;
pub use array::Array;
pub use error::{Error, ErrorKind};
pub use parser::parse;
pub use span::{Span, Spanned};
pub use table::Table;
pub use time::{Date, Datetime, Time, TimeOffset};
pub use value::{Item, Key, MaybeItem, Value, ValueMut};

#[cfg(feature = "serde")]
pub mod impl_serde;

/// Converts a parsed TOML [`Item`] into a typed Rust value.
///
/// Implement this trait on your own types to extract them from a parsed TOML
/// document via [`Table::required`] and [`Table::optional`].
///
/// Built-in implementations are provided for common types: [`bool`], integer
/// types ([`i8`] through [`i64`], [`u8`] through [`u64`], [`usize`]),
/// floating-point types ([`f32`], [`f64`]), [`String`],
/// [`Cow<'de, str>`](std::borrow::Cow), [`Str`], [`Vec<T>`], and
/// [`Spanned<T>`].
///
/// # Examples
///
/// ```
/// use toml_spanner::{Deserialize, Error, Item};
///
/// struct Endpoint {
///     host: String,
///     port: u16,
/// }
///
/// impl<'de> Deserialize<'de> for Endpoint {
///     fn deserialize(item: &mut Item<'de>) -> Result<Self, Error> {
///         let table = item.expect_table()?;
///         Ok(Endpoint {
///             host: table.required("host")?,
///             port: table.required("port")?,
///         })
///     }
/// }
/// ```
pub trait Deserialize<'de>: Sized {
    /// Deserializes `Self` from the given [`Item`], returning an error on failure.
    fn deserialize(item: &mut Item<'de>) -> Result<Self, Error>;
}

/// Object-safe version of [`Deserialize`] for types that do not borrow from
/// the input.
///
/// Automatically implemented for every `T: for<'de> Deserialize<'de>`.
pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}
