#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

/// Growable array of TOML values
mod array;
pub mod de;
pub mod de_helpers;
mod error;
mod parser;
mod span;
/// A 16-byte string type for borrowed or owned string data
mod str;
/// TOML table: flat list of key-value pairs
mod table;
mod value;

pub use array::Array;
pub use de::parse;
pub use error::{DeserError, Error, ErrorKind};
pub use span::{Span, Spanned};
pub use str::Str;
pub use table::Table;
pub use value::{Key, Value, ValueOwned, ValueRef};

#[cfg(feature = "serde")]
pub mod impl_serde;

/// This crate's equivalent to [`serde::Deserialize`](https://docs.rs/serde/latest/serde/de/trait.Deserialize.html)
pub trait Deserialize<'de>: Sized {
    /// Given a mutable [`Value`], allows you to deserialize the type from it,
    /// or accumulate 1 or more errors
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError>;
}

/// This crate's equivalent to [`serde::DeserializeOwned`](https://docs.rs/serde/latest/serde/de/trait.DeserializeOwned.html)
///
/// This is useful if you want to use trait bounds
pub trait DeserializeOwned: for<'de> Deserialize<'de> {}
impl<T> DeserializeOwned for T where T: for<'de> Deserialize<'de> {}
