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

pub use arena::Arena;
#[cfg(feature = "from-toml")]
pub use de::{Context, Failed, FromFlattened, FromToml, TableHelper};
#[cfg(all(feature = "to-toml", fuzzing))]
pub use emit::reproject;
#[cfg(all(feature = "to-toml", not(fuzzing)))]
use emit::reproject;
#[cfg(all(feature = "to-toml", fuzzing))]
pub use emit::{EmitConfig, NormalizedTable, emit, emit_with_config};
#[cfg(all(feature = "to-toml", not(fuzzing)))]
use emit::{EmitConfig, emit, emit_with_config};
#[cfg(feature = "from-toml")]
pub use error::FromTomlError;
#[cfg(feature = "to-toml")]
pub use error::ToTomlError;
pub use error::{Error, ErrorKind};
pub use item::array::Array;
pub use item::table::Table;
pub use item::{ArrayStyle, Item, Key, Kind, MaybeItem, TableStyle, Value, ValueMut};
pub use parser::{Root, parse};
#[cfg(feature = "to-toml")]
pub use ser::{ToFlattened, ToToml};
pub use span::{Span, Spanned};
pub use time::{Date, DateTime, Time, TimeOffset};

#[cfg(feature = "derive")]
pub use toml_spanner_macros::Toml;

#[cfg(feature = "serde")]
pub mod impl_serde;

/// Parses and deserializes a TOML document in one step.
///
/// This is a convenience wrapper that allocates its own [`Arena`] and
/// calls [`parse`] followed by [`Root::to`]. Because the arena is
/// local, the deserialized type `T` cannot borrow from the input.
///
/// For more control over lifetimes — for example, to borrow `&'de str`
/// fields directly from the input, or to reuse an arena across multiple
/// parses — use the lower-level API instead:
///
/// ```
/// # fn main() -> Result<(), toml_spanner::FromTomlError> {
/// let arena = toml_spanner::Arena::new();
/// let mut root = toml_spanner::parse("key = 'value'", &arena)?;
/// let config = root.to::<std::collections::HashMap<String, String>>()?;
/// # Ok(())
/// # }
/// ```
///
/// Note that [`to_string_with_config`] with [`TomlConfig::with_formatting_from`]
/// requires a [`Root`] to reproject formatting from, but it does not
/// have to be the same `Root` that produced the deserialized value,
/// any parsed TOML tree can serve as a formatting template.
///
/// # Errors
///
/// Returns a [`FromTomlError`] containing all parse and deserialization
/// errors encountered.
#[cfg(feature = "from-toml")]
pub fn from_str<T: for<'a> FromToml<'a>>(document: &str) -> Result<T, FromTomlError> {
    let arena = Arena::new();
    let mut root = parse(document, &arena)?;
    root.to()
}

#[cfg(feature = "to-toml")]
pub fn to_string(value: &dyn ToToml) -> Result<String, ToTomlError> {
    to_string_with_config(value, TomlConfig::default())
}

// Exists for semver safe extension to configuration, in the future
// could indention settings, comment injection, etc.
#[derive(Default)]
pub struct TomlConfig<'a> {
    formatting_from: Option<&'a Root<'a>>,
}

impl<'a> TomlConfig<'a> {
    pub fn with_formatting_from(mut self, root: &'a Root<'a>) -> Self {
        self.formatting_from = Some(root);
        self
    }
}

#[cfg(feature = "to-toml")]
pub fn to_string_with_config(
    value: &dyn ToToml,
    config: TomlConfig<'_>,
) -> Result<String, ToTomlError> {
    let arena = Arena::new();
    let mut item = value.to_toml(&arena)?;
    let Some(table) = item.as_table_mut() else {
        return Err(ToTomlError {
            message: "Top-level item must be a table".into(),
        });
    };
    let mut items = Vec::new();
    let mut buffer = Vec::new();
    if let Some(formatting_from) = config.formatting_from {
        reproject(formatting_from, table, &mut items);
        emit_with_config(
            table.normalize(),
            &EmitConfig {
                projected_source_items: &items,
                projected_source_text: formatting_from.ctx.source(),
                reprojected_order: true,
            },
            &mut buffer,
        );
    } else {
        emit(table.normalize(), &mut buffer);
    }
    match String::from_utf8(buffer) {
        Ok(s) => Ok(s),
        Err(_) => Err(ToTomlError {
            message: "Failed to convert emitted bytes into a UTF-8 string".into(),
        }),
    }
}
