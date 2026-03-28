//! A high-performance TOML parser that preserves span information for
//! values and keys.
//!
//! # Parsing and Traversal
//!
//! Use [`parse`] with a TOML string and an [`Arena`] to get a [`Document`].
//! ```
//! let arena = toml_spanner::Arena::new();
//! let doc = toml_spanner::parse("key = 'value'", &arena).unwrap();
//! ```
//! Traverse the tree via index operators, which return a [`MaybeItem`]:
//! ```
//! # let arena = toml_spanner::Arena::new();
//! # let doc = toml_spanner::parse("", &arena).unwrap();
//! let name: Option<&str> = doc["name"].as_str();
//! let numbers: Option<i64> = doc["numbers"][50].as_i64();
//! ```
//! Use [`MaybeItem::item()`] to get an [`Item`] containing a [`Value`] and [`Span`].
//! ```rust
//! # use toml_spanner::{Value, Span};
//! # let arena = toml_spanner::Arena::new();
//! # let doc = toml_spanner::parse("item = 0", &arena).unwrap();
//! let Some(item) = doc["item"].item() else {
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
//! [`Document::helper()`] creates a [`TableHelper`] for type-safe field extraction
//! via [`FromToml`]. Errors accumulate in the [`Document`]'s context rather than
//! failing on the first error.
//!
//! ```
//! # let arena = toml_spanner::Arena::new();
//! # let mut doc = toml_spanner::parse("name = 'hello'", &arena).unwrap();
//! let mut helper = doc.helper();
//! let name: String = helper.required("name").ok().unwrap();
//! ```
//!
//! [`Item::parse`] extracts values from string items via [`std::str::FromStr`].
//!
//! ```
//! # fn main() -> Result<(), toml_spanner::Error> {
//! # let arena = toml_spanner::Arena::new();
//! # let doc = toml_spanner::parse("ip-address = '127.0.0.1'", &arena).unwrap();
//! let item = doc["ip-address"].item().unwrap();
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
//!         let mut th = value.table_helper(ctx)?;
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
//! let mut doc = toml_spanner::parse(content, &arena).unwrap();
//!
//! // Null-coalescing index operators: missing keys return a None-like
//! // MaybeItem instead of panicking.
//! assert_eq!(doc["things"][0]["color"].as_str(), None);
//! assert_eq!(doc["things"][1]["color"].as_str(), Some("green"));
//!
//! // Deserialize typed values out of the document table.
//! let mut helper = doc.helper();
//! let things: Vec<Things> = helper.required("things").ok().unwrap();
//! let dev_mode: bool = helper.optional("dev-mode").unwrap_or(false);
//! // Error if unconsumed fields remain.
//! helper.expect_empty().ok();
//!
//! assert_eq!(things.len(), 2);
//! assert_eq!(things[0].name, "hammer");
//! assert!(dev_mode);
//! ```
//!
//! </details>
//!
//! ## Derive Macro
//!
//! The [`Toml`] derive macro generates [`FromToml`] and [`ToToml`]
//! implementations. A bare `#[derive(Toml)]` generates [`FromToml`] only.
//! Annotate with `#[toml(Toml)]` for both directions.
//!
//! ```
//! use toml_spanner::{Arena, Toml};
//!
//! #[derive(Debug, Toml)]
//! #[toml(Toml)]
//! struct Config {
//!     name: String,
//!     port: u16,
//!     #[toml(default)]
//!     debug: bool,
//! }
//!
//! let arena = Arena::new();
//! let mut doc = toml_spanner::parse("name = 'app'\nport = 8080", &arena).unwrap();
//! let config = doc.to::<Config>().unwrap();
//! assert_eq!(config.name, "app");
//!
//! let output = toml_spanner::to_string(&config).unwrap();
//! assert!(output.contains("name = \"app\""));
//! ```
//!
//! See the [`Toml`] macro documentation for all supported attributes
//! (`rename`, `default`, `flatten`, `skip`, tagged enums, etc.).
//!
//! ## Serialization
//!
//! Types implementing [`ToToml`] can be written back to TOML text with
//! [`to_string`] or the [`Formatting`] builder for more control.
//!
//! ```
//! use toml_spanner::{Arena, Formatting};
//! use std::collections::BTreeMap;
//!
//! let mut map = BTreeMap::new();
//! map.insert("key", "value");
//!
//! // Quick one-liner
//! let output = toml_spanner::to_string(&map).unwrap();
//!
//! // Preserve formatting from a parsed document
//! let arena = Arena::new();
//! let doc = toml_spanner::parse("key = \"old\"\n", &arena).unwrap();
//! let output = Formatting::preserved_from(&doc).format(&map).unwrap();
//! ```
//!
//! See [`Formatting`] for indentation, format preservation, and other options.
//!
#![cfg_attr(docsrs, feature(doc_cfg))]
mod arena;
#[cfg(feature = "from-toml")]
mod de;
#[cfg(feature = "to-toml")]
mod emit;
mod error;
#[cfg(feature = "from-toml")]
pub mod helper;
mod item;
mod parser;
#[cfg(feature = "to-toml")]
mod ser;
mod span;
mod time;

/// Error sentinel indicating a failure.
///
/// Error details are recorded in the shared [`Context`].
#[derive(Debug)]
pub struct Failed;

pub use arena::Arena;
#[cfg(feature = "from-toml")]
pub use de::FromTomlError;
#[cfg(feature = "from-toml")]
pub use de::{Context, FromFlattened, FromToml, TableHelper};
#[cfg(feature = "to-toml")]
pub use emit::Indent;
#[cfg(feature = "to-toml")]
use emit::{EmitConfig, emit_with_config};
#[cfg(feature = "to-toml")]
use emit::{reproject, reproject_with_span_identity};
pub use error::{Error, ErrorKind, TomlPath};
pub use item::array::Array;
pub use item::table::Table;
pub use item::{ArrayStyle, Integer, Item, Key, Kind, MaybeItem, TableStyle, Value, ValueMut};
#[cfg(feature = "from-toml")]
pub use parser::parse_recoverable;
pub use parser::{Document, parse};
#[cfg(feature = "to-toml")]
pub use ser::ToTomlError;
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
/// For borrowing or non-fatal errors, use [`parse`] and [`Document`] methods.
///
/// # Errors
///
/// Returns a [`FromTomlError`] containing all parse or conversion errors
/// encountered.
#[cfg(feature = "from-toml")]
pub fn from_str<T: for<'a> FromToml<'a>>(document: &str) -> Result<T, FromTomlError> {
    let arena = Arena::new();
    let mut doc = match parse(document, &arena) {
        Ok(doc) => doc,
        Err(e) => {
            return Err(FromTomlError { errors: vec![e] });
        }
    };
    doc.to()
}

/// Serializes a [`ToToml`] value into a TOML document string with default formatting.
///
/// The value must serialize to a table at the top level. For format
/// preservation or custom indentation, use [`Formatting`].
///
/// # Errors
///
/// Returns [`ToTomlError`] if serialization fails or the top-level value
/// is not a table.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use toml_spanner::to_string;
///
/// let mut map = BTreeMap::new();
/// map.insert("key", "value");
/// let output = to_string(&map).unwrap();
/// assert!(output.contains("key = \"value\""));
/// ```
#[cfg(feature = "to-toml")]
pub fn to_string(value: &dyn ToToml) -> Result<String, ToTomlError> {
    Formatting::default().format(value)
}

/// Controls how TOML output is formatted when serializing.
///
/// [`Formatting::preserved_from`] preserves formatting from a previously
/// parsed document, use `Formatting::default()` for standard formatting.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, Formatting};
/// use std::collections::BTreeMap;
///
/// let arena = Arena::new();
/// let source = "key = \"value\"\n";
/// let doc = toml_spanner::parse(source, &arena).unwrap();
///
/// let mut map = BTreeMap::new();
/// map.insert("key", "updated");
///
/// let output = Formatting::preserved_from(&doc).format(&map).unwrap();
/// assert!(output.contains("key = \"updated\""));
/// ```
#[cfg(feature = "to-toml")]
#[derive(Default)]
pub struct Formatting<'a> {
    formatting_from: Option<&'a Document<'a>>,
    indent: Indent,
    span_projection_identity: bool,
}

#[cfg(feature = "to-toml")]
impl<'a> Formatting<'a> {
    /// Creates a formatting template from a parsed document.
    ///
    /// Indent style is auto-detected from the source text, defaulting to
    /// 4 spaces when no indentation is found.
    pub fn preserved_from(doc: &'a Document<'a>) -> Self {
        let indent = doc.detect_indent();
        Self {
            formatting_from: Some(doc),
            indent,
            span_projection_identity: false,
        }
    }

    /// Enables span projection identity for array reprojection.
    ///
    /// By default, no assumptions are made about the spans of the format
    /// target. With span projection identity enabled, spans of the target
    /// are assumed to correspond to spans of the formatting reference.
    /// This allows precise identity tracking of array elements through
    /// reordering, removal, and deep mutation instead of the default
    /// best-effort content-based matching.
    ///
    /// Intended for the lower-level [`Table`] mutation APIs where the
    /// target was produced by parsing the same text as the formatting
    /// reference. When round-tripping through [`FromToml`] and [`ToToml`],
    /// spans are not preserved and this flag should not be used.
    ///
    /// Breaking the span correspondence assumption leads to unspecified
    /// behavior, including panics or invalid TOML generation.
    ///
    /// [`FromToml`]: crate::FromToml
    /// [`ToToml`]: crate::ToToml
    pub fn with_span_projection_identity(mut self) -> Self {
        self.span_projection_identity = true;
        self
    }

    /// Sets the indentation style for expanded inline arrays.
    /// Overrides auto-detection.
    pub fn with_indentation(mut self, indent: Indent) -> Self {
        self.indent = indent;
        self
    }

    /// Serializes a [`ToToml`] value into a TOML string.
    ///
    /// The value must serialize to a table at the top level.
    ///
    /// # Errors
    ///
    /// Returns [`ToTomlError`] if serialization fails or the top-level value
    /// is not a table.
    pub fn format(&self, value: &dyn ToToml) -> Result<String, ToTomlError> {
        let arena = Arena::new();
        let item = value.to_toml(&arena)?;
        let Some(table) = item.into_table() else {
            return Err(ToTomlError {
                message: "Top-level item must be a table".into(),
            });
        };
        let buffer = self.format_table_to_bytes(table, &arena);
        match String::from_utf8(buffer) {
            Ok(s) => Ok(s),
            Err(_) => Err(ToTomlError {
                message: "Failed to convert emitted bytes into a UTF-8 string".into(),
            }),
        }
    }

    /// Formats a [`Table`] directly into bytes.
    ///
    /// Low-level primitive that normalizes and (when a source document
    /// is set) reprojects the table before emission. The provided arena
    /// is used for temporary allocations during emission.
    pub fn format_table_to_bytes(&self, mut table: Table<'_>, arena: &Arena) -> Vec<u8> {
        let mut items = Vec::new();
        let mut buffer = Vec::new();
        if let Some(formatting_from) = self.formatting_from {
            if self.span_projection_identity {
                reproject_with_span_identity(formatting_from, &mut table, &mut items);
            } else {
                reproject(formatting_from, &mut table, &mut items);
            }
            emit_with_config(
                table.normalize(),
                &EmitConfig {
                    projected_source_items: &items,
                    projected_source_text: formatting_from.ctx.source(),
                    indent: self.indent,
                },
                arena,
                &mut buffer,
            );
        } else {
            emit_with_config(
                table.normalize(),
                &EmitConfig {
                    indent: self.indent,
                    ..EmitConfig::default()
                },
                arena,
                &mut buffer,
            );
        }
        buffer
    }
}
