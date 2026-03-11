//! A high-performance TOML parser that preserves span information for
//! values and keys.
//!
//! # Parsing and Traversal
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
//! let Span{start, end} = item.span_unchecked();
//! ```
//!
//! ## Deserialization
//!
//! Use [`Root::helper()`] to create a [`TableHelper`] for type-safe field extraction
//! via the [`FromToml`] trait. Errors are accumulated in the [`Root`]'s context
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
//! use toml_spanner::{Arena, FromToml, Item, Context, Failed, TableHelper};
//!
//! #[derive(Debug)]
//! struct Things {
//!     name: String,
//!     value: u32,
//!     color: Option<String>,
//! }
//!
//! impl<'de> FromToml<'de> for Things {
//!     fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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
#[cfg(feature = "from-toml")]
mod de;
#[cfg(feature = "to-toml")]
mod emit;
mod error;
mod item;
mod parser;
#[cfg(feature = "to-toml")]
mod ser;
mod span;
mod time;

pub use item::owned::{OwnedItem, OwnedTable};

pub use arena::Arena;
#[cfg(feature = "from-toml")]
pub use de::{Context, Failed, FromFlattened, FromToml, TableHelper};
#[cfg(feature = "to-toml")]
pub use emit::{EmitConfig, NormalizedTable, emit, emit_with_config, reproject};
pub use error::{Error, ErrorKind};
pub use item::array::Array;
pub use item::items_equal;
pub use item::table::Table;
pub use item::{ArrayStyle, Item, Key, Kind, MaybeItem, TableStyle, Value, ValueMut};
pub use parser::{Root, parse};
#[cfg(feature = "to-toml")]
pub use ser::{ToContext, ToFlattened, ToToml};
pub use span::{Span, Spanned};
pub use time::{Date, DateTime, Time, TimeOffset};

#[cfg(feature = "derive")]
pub use toml_spanner_macros::Toml;

#[cfg(feature = "serde")]
pub mod impl_serde;

#[cfg(feature = "from-toml")]
pub fn from_str<T: for<'a> FromToml<'a>>(document: &str) -> Result<T, Vec<Error>> {
    let arena = Arena::new();
    from_str_in(document, &arena)
}

#[cfg(feature = "from-toml")]
pub fn from_str_in<'de, T: FromToml<'de>>(
    document: &'de str,
    arena: &'de Arena,
) -> Result<T, Vec<Error>> {
    match parse(document, arena) {
        Ok(mut root) => {
            let value = T::from_toml(&mut root.ctx, root.table.as_item());
            match value {
                Ok(v) if root.ctx.errors.is_empty() => Ok(v),
                _ => Err(root.ctx.errors),
            }
        }
        Err(e) => Err(vec![e]),
    }
}

#[cfg(feature = "to-toml")]
pub fn to_string(value: &dyn ToToml) -> Result<String, std::borrow::Cow<'static, str>> {
    let arena = Arena::new();
    let mut context = ToContext {
        arena: &arena,
        error: None,
    };
    let mut item = match value.to_toml(&mut context) {
        Ok(item) => item,
        Err(_) => {
            return Err(context
                .error
                .unwrap_or_else(|| "Failed to convert into item".into()));
        }
    };
    let Some(table) = item.as_table_mut() else {
        return Err("Top-level item must be a table".into());
    };
    let mut buffer = Vec::new();
    emit(table.normalize(), &mut buffer);
    match String::from_utf8(buffer) {
        Ok(s) => Ok(s),
        Err(_) => Err("Failed to convert emitted bytes into a UTF-8 string".into()),
    }
}
