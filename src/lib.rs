//! A high-performance TOML parser that preserves span information for every
//! parsed value.
//!
//! Strings are zero-copy where possible, borrowing directly from the input;
//! escape sequences are allocated into a caller-supplied [`Arena`].
//!
//! # Quick start
//!
//! Use [`parse`] with a TOML string and an [`Arena`] to get a [`Root`].
//! ```
//! # fn main() -> Result<(), toml_spanner::Error> {
//! let arena = toml_spanner::Arena::new();
//! let root = toml_spanner::parse("key = 'value'", &arena)?;
//! # Ok(())
//! # }
//! ```
//! Traverse the tree for inspection via index operators which return a [`MaybeItem`]:
//! ```
//! # let arena = toml_spanner::Arena::new();
//! # let root = toml_spanner::parse("", &arena).unwrap();
//! let name: Option<&str> = root["name"].as_str();
//! let numbers: Option<i64> = root["numbers"][50].as_i64();
//! ```
//! Use the [`MaybeItem::item()`] method get an [`Item`] which contains a [`Value`] and [`Span`].
//! ```rust
//! # use toml_spanner::{Value, Span};
//! # let arena = toml_spanner::Arena::new();
//! # let root = toml_spanner::parse("item = 0", &arena).unwrap();
//! let Some(item) = root["item"].item() else {
//!     panic!("Missing key `custom`");
//! };
//! match item.value() {
//!      Value::String(string) => {},
//!      Value::Integer(integer) => {}
//!      Value::Float(float) => {},
//!      Value::Boolean(boolean) => {},
//!      Value::Array(array) => {},
//!      Value::Table(table) => {},
//!      Value::DateTime(date_time) => {},
//! }
//! // Get byte offset of where item was defined in the source.
//! let Span{start, end} = item.span();
//! ```
//!
//! ## Deserialization
//!
//! Use [`Root::helper()`] to create a [`TableHelper`] for type-safe field extraction
//! via the [`Deserialize`] trait. Errors are accumulated in the [`Root`]'s context
//! rather than failing on the first error.
//!
//! ```
//! # fn main() -> Result<(), toml_spanner::Error> {
//! # let arena = toml_spanner::Arena::new();
//! # let mut root = toml_spanner::parse("name = 'hello'", &arena)?;
//! let mut helper = root.helper();
//! let name: String = helper.required("name").ok().unwrap();
//! # Ok(())
//! # }
//! ```
//!
//! Extract values with [`Item::parse`] which uses [`std::str::FromStr`] expecting a String kinded TOML Value.
//!
//! ```
//! # fn main() -> Result<(), toml_spanner::Error> {
//! # let arena = toml_spanner::Arena::new();
//! # let root = toml_spanner::parse("ip-address = '127.0.0.1'", &arena)?;
//! let item = root["ip-address"].item().unwrap();
//! let ip: std::net::Ipv4Addr = item.parse()?;
//! # Ok(())
//! # }
//! ```
//!
//! <details>
//! <summary>Toggle More Extensive Example</summary>
//!
//! ```
//! use toml_spanner::{Arena, Deserialize, Item, de::{Context, Failed, TableHelper}};
//!
//! #[derive(Debug)]
//! struct Things {
//!     name: String,
//!     value: u32,
//!     color: Option<String>,
//! }
//!
//! impl<'de> Deserialize<'de> for Things {
//!     fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
//!         let Some(table) = value.as_table() else {
//!             return Err(ctx.error_expected_but_found("a table", value));
//!         };
//!         let mut th = TableHelper::new(ctx, table);
//!         let name = th.required("name")?;
//!         let value = th.required("value")?;
//!         let color = th.optional("color");
//!         th.expect_empty()?;
//!         Ok(Things { name, value, color })
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
//! let mut root = toml_spanner::parse(content, &arena)?;
//!
//! // Null-coalescing index operators — missing keys return a None-like
//! // MaybeItem instead of panicking.
//! assert_eq!(root["things"][0]["color"].as_str(), None);
//! assert_eq!(root["things"][1]["color"].as_str(), Some("green"));
//!
//! // Deserialize typed values out of the root table.
//! let mut helper = root.helper();
//! let things: Vec<Things> = helper.required("things").ok().unwrap();
//! let dev_mode: bool = helper.optional("dev-mode").unwrap_or(false);
//! // Error if unconsumed fields remain.
//! helper.expect_empty().ok();
//!
//! assert_eq!(things.len(), 2);
//! assert_eq!(things[0].name, "hammer");
//! assert!(dev_mode);
//! # Ok::<(), toml_spanner::Error>(())
//! ```
//!
//! </details>

mod arena;
mod array;
pub mod de;
mod error;
mod parser;
mod span;
mod table;
mod time;
mod value;

pub use arena::Arena;
pub use array::Array;
pub use de::{Context, Deserialize, Failed, TableHelper};
pub use error::{Error, ErrorKind};
pub use parser::{Root, parse};
pub use span::{Span, Spanned};
pub use table::Table;
pub use time::{Date, DateTime, Time, TimeOffset};
pub use value::{Item, Key, Kind, MaybeItem, Value, ValueMut};

#[cfg(feature = "serde")]
pub mod impl_serde;
