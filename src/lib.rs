//! A high-performance TOML parser that preserves byte-offset span information
//! for every parsed value.
//!
//! All values are represented as compact 24-byte [`Item`]s with bit-packed
//! spans. Strings are zero-copy where possible, borrowing directly from the
//! input; escape sequences are committed into a caller-supplied [`Arena`].
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
//! let things: Vec<Things> = table.required("things")?;
//! let dev_mode: bool = table.optional("dev-mode")?.unwrap_or(false);
//! // Error if unused fields exist.
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
mod str;
mod table;
mod value;

pub use arena::Arena;
pub use array::Array;
pub use error::{Error, ErrorKind};
pub use parser::parse;
pub use span::{Span, Spanned};
pub use str::Str;
pub use table::Table;
pub use value::{Item, Key, Value, ValueMut};

#[cfg(feature = "serde")]
pub mod impl_serde;

/// This crate's equivalent to [`serde::Deserialize`](https://docs.rs/serde/latest/serde/de/trait.Deserialize.html).
pub trait Deserialize<'de>: Sized {
    /// Deserializes `Self` from the given [`Item`], returning an error on failure.
    fn deserialize(item: &mut Item<'de>) -> Result<Self, Error>;
}

/// This crate's equivalent to [`serde::DeserializeOwned`](https://docs.rs/serde/latest/serde/de/trait.DeserializeOwned.html).
///
/// Useful for trait bounds that require deserialization from any lifetime.
pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}
