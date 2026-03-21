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
//! Traverse the tree for inspection via index operators which return a [`MaybeItem`]:
//! ```
//! # let arena = toml_spanner::Arena::new();
//! # let doc = toml_spanner::parse("", &arena).unwrap();
//! let name: Option<&str> = doc["name"].as_str();
//! let numbers: Option<i64> = doc["numbers"][50].as_i64();
//! ```
//! Use the [`MaybeItem::item()`] method get an [`Item`] which contains a [`Value`] and [`Span`].
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
//! Use [`Document::helper()`] to create a [`TableHelper`] for type-safe field extraction
//! via the [`FromToml`] trait. Errors are accumulated in the [`Document`]'s context
//! rather than failing on the first error.
//!
//! ```
//! # let arena = toml_spanner::Arena::new();
//! # let mut doc = toml_spanner::parse("name = 'hello'", &arena).unwrap();
//! let mut helper = doc.helper();
//! let name: String = helper.required("name").ok().unwrap();
//! ```
//!
//! Extract values with [`Item::parse`] which uses [`std::str::FromStr`] expecting a String kinded TOML Value.
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
//! // Null-coalescing index operators — missing keys return a None-like
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

/// Zero-size sentinel indicating that a conversion step failed.
///
/// Real error details are recorded in the shared [`Context`] rather than
/// carried inside `Failed` itself. This allows multiple errors to accumulate
/// during a single [`FromToml`] pass.
#[derive(Debug)]
pub struct Failed;

pub use arena::Arena;
#[cfg(feature = "from-toml")]
pub use de::FromTomlError;
#[cfg(feature = "from-toml")]
pub use de::{Context, FromFlattened, FromToml, TableHelper};
#[cfg(feature = "to-toml")]
use emit::reproject;
#[cfg(feature = "to-toml")]
use emit::{EmitConfig, emit_with_config};
#[cfg(feature = "to-toml")]
pub use emit::Indent;
pub use error::{Error, ErrorKind, TomlPath};
pub use item::array::Array;
pub use item::table::Table;
pub use item::{ArrayStyle, Item, Key, Kind, MaybeItem, TableStyle, Value, ValueMut};
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
/// This is a convenience wrapper that allocates its own [`Arena`] and
/// calls [`parse`] followed by [`Document::to`]. Because the arena is
/// local, the resulting type `T` cannot borrow from the input.
///
/// For more control over lifetimes — for example, to borrow `&'de str`
/// fields directly from the input, or to reuse an arena across multiple
/// parses — use the lower-level API instead:
///
/// ```
/// let arena = toml_spanner::Arena::new();
/// let mut doc = toml_spanner::parse("key = 'value'", &arena).unwrap();
/// let config = doc.to::<std::collections::HashMap<String, String>>().unwrap();
/// ```
///
/// Note that [`Formatting::of`] requires a [`Document`] to reproject
/// formatting from, but it does not have to be the same `Document`
/// that produced the converted value. Any parsed TOML tree can serve
/// as a formatting template.
///
/// # Errors
///
/// Returns a [`FromTomlError`] containing all parse and conversion errors
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

/// Serializes a [`ToToml`] value into a TOML string with default formatting.
///
/// The value must serialize to a table at the top level. For control over
/// formatting (e.g. preserving the layout of a previously parsed document),
/// use [`Formatting::of`].
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
/// Use [`Formatting::of`] to preserve formatting from a previously parsed
/// document. The default produces clean formatted output with no source
/// template.
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
/// let output = Formatting::of(&doc).format(&map).unwrap();
/// assert!(output.contains("key = \"updated\""));
/// ```
#[cfg(feature = "to-toml")]
pub struct Formatting<'a> {
    formatting_from: Option<&'a Document<'a>>,
    indent: Indent,
}

#[cfg(feature = "to-toml")]
impl Default for Formatting<'_> {
    fn default() -> Self {
        Self {
            formatting_from: None,
            indent: Indent::default(),
        }
    }
}

#[cfg(feature = "to-toml")]
impl<'a> Formatting<'a> {
    /// Creates a formatting template from a parsed document.
    ///
    /// Indent style is auto-detected from the source text. If no
    /// indentation is found, defaults to 4 spaces.
    pub fn of(doc: &'a Document<'a>) -> Self {
        let indent = detect_indent(doc.ctx.source().as_bytes());
        Self {
            formatting_from: Some(doc),
            indent,
        }
    }

    /// Sets the indentation style for expanded inline arrays.
    /// Overrides auto-detection.
    pub fn indent(mut self, indent: Indent) -> Self {
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
    /// This is the low-level primitive. The table is normalized and
    /// (when a source document is set) reprojected before emission.
    /// The provided arena is used for temporary allocations during
    /// emission.
    pub fn format_table_to_bytes(
        &self,
        mut table: Table<'_>,
        arena: &Arena,
    ) -> Vec<u8> {
        let mut items = Vec::new();
        let mut buffer = Vec::new();
        if let Some(formatting_from) = self.formatting_from {
            reproject(formatting_from, &mut table, &mut items);
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

/// Detects the indent style used in TOML source text by finding the first
/// indented line inside a multiline bracket (`[` or `{`). Returns
/// `Indent::Spaces(4)` if no indentation pattern is found.
#[cfg(feature = "to-toml")]
fn detect_indent(src: &[u8]) -> Indent {
    let mut depth: u32 = 0;
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth = depth.saturating_sub(1),
            b'"' | b'\'' => {
                i = skip_string(src, i);
                continue;
            }
            b'#' => {
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'\n' if depth > 0 => {
                i += 1;
                if i < src.len() && src[i] == b'\t' {
                    return Indent::Tab;
                }
                let mut spaces = 0u8;
                while i < src.len() && src[i] == b' ' {
                    spaces = spaces.saturating_add(1);
                    i += 1;
                }
                if spaces > 0 && i < src.len() && src[i] != b'\n' && src[i] != b']' && src[i] != b'}' {
                    return Indent::Spaces(spaces);
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    Indent::default()
}

#[cfg(feature = "to-toml")]
fn skip_string(src: &[u8], start: usize) -> usize {
    let delim = src[start];
    let mut i = start + 1;
    let multiline = i + 1 < src.len() && src[i] == delim && src[i + 1] == delim;
    if multiline {
        i += 2;
        let end_seq = [delim, delim, delim];
        while i + 2 < src.len() {
            if src[i] == b'\\' && delim == b'"' {
                i += 2;
                continue;
            }
            if src[i..i + 3] == end_seq {
                return i + 3;
            }
            i += 1;
        }
        src.len()
    } else {
        while i < src.len() {
            if src[i] == b'\\' && delim == b'"' {
                i += 2;
                continue;
            }
            if src[i] == delim || src[i] == b'\n' {
                return i + 1;
            }
            i += 1;
        }
        src.len()
    }
}
