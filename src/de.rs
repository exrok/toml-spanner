#[cfg(test)]
#[path = "./de_tests.rs"]
mod tests;

use std::hash::BuildHasher;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::{collections::BTreeMap, hash::Hash};

use foldhash::HashMap;

use std::fmt::{self, Debug, Display};

use crate::{
    Arena, Key, Span, Table,
    error::{ErrorKind, Error},
    item::{self, Item},
    parser::{INDEXED_TABLE_THRESHOLD, KeyRef},
};

/// Guides extraction from a [`Table`] by tracking which fields have been
/// consumed.
///
/// Create one via [`Document::helper`](crate::Document::helper) for the root table,
/// or [`Item::table_helper`] / [`TableHelper::new`] for nested tables.
/// Then extract fields with [`required`](Self::required) and
/// [`optional`](Self::optional), and finish with
/// [`expect_empty`](Self::expect_empty) to reject unknown keys.
///
/// Errors are accumulated in the shared [`Context`] rather than failing on
/// the first problem, so a single parse pass can report multiple issues.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, FromToml, Item, Context, Failed, TableHelper};
///
/// struct Config {
///     name: String,
///     port: u16,
///     debug: bool,
/// }
///
/// impl<'de> FromToml<'de> for Config {
///     fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
///         let mut th = value.table_helper(ctx)?;
///         let name = th.required("name")?;
///         let port = th.required("port")?;
///         let debug = th.optional("debug").unwrap_or(false);
///         th.expect_empty()?;
///         Ok(Config { name, port, debug })
///     }
/// }
/// ```
pub struct TableHelper<'ctx, 'table, 'de> {
    pub ctx: &'ctx mut Context<'de>,
    pub table: &'table Table<'de>,
    // -1 means don't use table index.
    table_id: i32,
    // Used for detecting unused fields or iterating over remaining for flatten into collection.
    used_count: u32,
    used: &'de mut FixedBitset,
}

#[repr(transparent)]
struct FixedBitset([u64]);

impl FixedBitset {
    #[allow(clippy::mut_from_ref)]
    pub fn new(capacity: usize, arena: &Arena) -> &mut FixedBitset {
        let bitset_bucket_count = capacity.div_ceil(64);
        let bitset = arena
            .alloc(bitset_bucket_count * std::mem::size_of::<u64>())
            .cast::<u64>();
        for offset in 0..bitset_bucket_count {
            // SAFETY: `bitset_len * size_of::<u64>()` bytes were allocated above,
            // so `bitset.add(offset)` for offset in 0..bitset_len is within bounds.
            unsafe {
                bitset.add(offset).write(0);
            }
        }
        // SAFETY: bitset points to `bitset_len` initialized u64 values in the arena.
        let slice = unsafe { std::slice::from_raw_parts_mut(bitset.as_ptr(), bitset_bucket_count) };
        // SAFETY: FixedBitset is #[repr(transparent)] over [u64].
        unsafe { &mut *(slice as *mut [u64] as *mut FixedBitset) }
    }

    pub fn insert(&mut self, index: usize) -> bool {
        let offset = index >> 6;
        let bit = 1 << (index & 63);
        let old = self.0[offset];
        self.0[offset] |= bit;
        old & bit == 0
    }

    pub fn get(&self, index: usize) -> bool {
        let offset = index >> 6;
        let bit = 1 << (index & 63);
        self.0[offset] & bit != 0
    }
}

/// An iterator over table entries that were **not** consumed by
/// [`TableHelper::required`], [`TableHelper::optional`] or similar methods.
///
/// Obtained via [`TableHelper::into_remaining`].
pub struct RemainingEntriesIter<'t, 'de> {
    entries: &'t [(Key<'de>, Item<'de>)],
    remaining_cells: std::slice::Iter<'de, u64>,
    bits: u64,
}
impl RemainingEntriesIter<'_, '_> {
    fn next_bucket(&mut self) -> bool {
        let Some(bucket) = self.remaining_cells.next() else {
            return false;
        };
        debug_assert!(self.entries.len() > 64);
        let Some(remaining) = self.entries.get(64..) else {
            // Shouldn't occur in practice, but no need to panic here.
            return false;
        };
        self.entries = remaining;
        self.bits = !*bucket;
        true
    }
}

impl<'t, 'de> Iterator for RemainingEntriesIter<'t, 'de> {
    type Item = &'t (Key<'de>, Item<'de>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(bits) = NonZeroU64::new(self.bits) {
                let bit_index = bits.trailing_zeros() as usize;
                self.bits &= self.bits - 1;
                return self.entries.get(bit_index);
            }
            if !self.next_bucket() {
                return None;
            }
        }
    }
}

impl<'ctx, 't, 'de> TableHelper<'ctx, 't, 'de> {
    /// Creates a new helper for the given table.
    ///
    /// Prefer [`Item::table_helper`] when implementing [`FromToml`], or
    /// [`Document::helper`](crate::Document::helper) for the root table.
    pub fn new(ctx: &'ctx mut Context<'de>, table: &'t Table<'de>) -> Self {
        let table_id = if table.len() > INDEXED_TABLE_THRESHOLD {
            // Note due to 512MB limit this will fit in i32.
            table.entries()[0].0.span.start as i32
        } else {
            -1
        };
        Self {
            used: FixedBitset::new(table.len(), ctx.arena),
            ctx,
            table,
            table_id,
            used_count: 0,
        }
    }
    /// Looks up a key-value entry without marking it as consumed.
    ///
    /// This is useful for peeking at a field before deciding how to
    /// convert it. The entry will still be flagged as unexpected by
    /// [`expect_empty`](Self::expect_empty) unless it is later consumed by
    /// [`required`](Self::required) or [`optional`](Self::optional).
    pub fn get_entry(&self, key: &str) -> Option<&'t (Key<'de>, Item<'de>)> {
        if self.table_id < 0 {
            for entry in self.table.entries() {
                if entry.0.name == key {
                    return Some(entry);
                }
            }
            None
        } else {
            match self.ctx.index.get(&KeyRef::new(key, self.table_id as u32)) {
                Some(index) => Some(&self.table.entries()[*index]),
                None => None,
            }
        }
    }

    /// Extracts a required field and transforms it with `func`.
    ///
    /// Looks up `name`, marks it as consumed, and passes the [`Item`] to
    /// `func`. This is useful for parsing string values via
    /// [`Item::parse`] or applying custom validation without implementing
    /// [`FromToml`].
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] if the key is absent or if `func` returns an error.
    /// In both cases the error is pushed onto the shared [`Context`].
    pub fn required_mapped<T>(
        &mut self,
        name: &'static str,
        func: fn(&Item<'de>) -> Result<T, Error>,
    ) -> Result<T, Failed> {
        let Some((_, item)) = self.optional_entry(name) else {
            return Err(self.report_missing_field(name));
        };

        func(item).map_err(|err| {
            self.ctx.push_error(Error::custom(err, item.span_unchecked()))
        })
    }

    /// Extracts an optional field and transforms it with `func`.
    ///
    /// Returns [`None`] if the key is missing (no error recorded) or if
    /// `func` returns an error (the error is pushed onto the [`Context`]).
    /// The field is marked as consumed so
    /// [`expect_empty`](Self::expect_empty) will not flag it as unexpected.
    pub fn optional_mapped<T>(
        &mut self,
        name: &'static str,
        func: fn(&Item<'de>) -> Result<T, Error>,
    ) -> Option<T> {
        let Some((_, item)) = self.optional_entry(name) else {
            return None;
        };

        func(item)
            .map_err(|err| {
                self.ctx.push_error(Error::custom(err, item.span_unchecked()))
            })
            .ok()
    }

    /// Returns the raw [`Item`] for a required field.
    ///
    /// Like [`required`](Self::required) but skips conversion, giving
    /// direct access to the parsed value. The field is marked as consumed.
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] and records a
    /// [`MissingField`](crate::ErrorKind::MissingField) error if the key is
    /// absent.
    pub fn required_item(&mut self, name: &'static str) -> Result<&'t Item<'de>, Failed> {
        self.required_entry(name).map(|(_, value)| value)
    }

    /// Returns the raw [`Item`] for an optional field.
    ///
    /// Like [`optional`](Self::optional) but skips conversion, giving
    /// direct access to the parsed value. Returns [`None`] when the key is
    /// missing (no error recorded). The field is marked as consumed.
    pub fn optional_item(&mut self, name: &'static str) -> Option<&'t Item<'de>> {
        self.optional_entry(name).map(|(_, value)| value)
    }

    /// Returns the `(`[`Key`]`, `[`Item`]`)` pair for a required field.
    ///
    /// Use this when you need the key's [`Span`](crate::Span) in addition to
    /// the value. The field is marked as consumed.
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] and records a
    /// [`MissingField`](crate::ErrorKind::MissingField) error if the key is
    /// absent.
    pub fn required_entry(
        &mut self,
        name: &'static str,
    ) -> Result<&'t (Key<'de>, Item<'de>), Failed> {
        match self.optional_entry(name) {
            Some(entry) => Ok(entry),
            None => Err(self.report_missing_field(name)),
        }
    }

    /// Returns the `(`[`Key`]`, `[`Item`]`)` pair for an optional field.
    ///
    /// Returns [`None`] when the key is missing (no error recorded). Use
    /// this when you need the key's [`Span`](crate::Span) in addition to
    /// the value. The field is marked as consumed.
    pub fn optional_entry(&mut self, key: &str) -> Option<&'t (Key<'de>, Item<'de>)> {
        let entry = self.get_entry(key)?;
        // SAFETY: `entry` was returned by get_entry(), which either performs a
        // linear scan of self.table.entries() or indexes into that same slice
        // via the hash index. In both cases `entry` points to an element within
        // the slice whose base pointer is `base`. offset_from is valid because
        // both pointers derive from the same allocation and the result is a
        // non-negative element index (< table.len()).
        let index = unsafe {
            let ptr = entry as *const (Key<'de>, Item<'de>);
            let base = self.table.entries().as_ptr();
            ptr.offset_from(base) as usize
        };
        if self.used.insert(index) {
            self.used_count += 1;
        }
        Some(entry)
    }

    #[cold]
    fn report_missing_field(&mut self, name: &'static str) -> Failed {
        self.ctx.errors.push(Error::new(
            ErrorKind::MissingField(name),
            self.table.span(),
        ));
        Failed
    }

    /// Extracts and converts a required field via [`FromToml`].
    ///
    /// The field is marked as consumed so [`expect_empty`](Self::expect_empty)
    /// will not flag it as unexpected.
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] if the key is absent or if conversion fails.
    /// In both cases the error is pushed onto the shared [`Context`].
    pub fn required<T: FromToml<'de>>(&mut self, name: &'static str) -> Result<T, Failed> {
        let Some((_, val)) = self.optional_entry(name) else {
            return Err(self.report_missing_field(name));
        };

        T::from_toml(self.ctx, val)
    }

    /// Extracts and converts an optional field via [`FromToml`], returning
    /// [`None`] if the key is missing or conversion fails (recording the
    /// error in the [`Context`]).
    ///
    /// The field is marked as consumed so [`expect_empty`](Self::expect_empty)
    /// will not flag it as unexpected.
    pub fn optional<T: FromToml<'de>>(&mut self, name: &str) -> Option<T> {
        let Some((_, val)) = self.optional_entry(name) else {
            return None;
        };

        #[allow(clippy::manual_ok_err)]
        match T::from_toml(self.ctx, val) {
            Ok(value) => Some(value),
            // Note: The parent will already have recorded the error
            Err(_) => None,
        }
    }

    /// Returns the number of unused entries remaining in the table.
    pub fn remaining_count(&self) -> usize {
        self.table.len() - self.used_count as usize
    }

    /// Iterate over unused `&(Key<'de>, Item<'de>)` entries in the table.
    pub fn into_remaining(self) -> RemainingEntriesIter<'t, 'de> {
        let entries = self.table.entries();
        let mut remaining_cells = self.used.0.iter();
        RemainingEntriesIter {
            bits: if let Some(value) = remaining_cells.next() {
                !*value
            } else {
                0
            },
            entries,
            remaining_cells,
        }
    }

    /// Finishes field extraction, recording an error if any fields were not
    /// consumed by [`required`](Self::required) or
    /// [`optional`](Self::optional).
    ///
    /// Call this as the last step in a [`FromToml`] implementation to
    /// reject unknown keys.
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] and pushes an [`ErrorKind::UnexpectedKeys`](crate::ErrorKind::UnexpectedKeys)
    /// error if unconsumed fields remain.
    #[inline(never)]
    pub fn expect_empty(self) -> Result<(), Failed> {
        if self.used_count as usize == self.table.len() {
            return Ok(());
        }

        let mut had_unexpected = false;
        for (i, (key, _)) in self.table.entries().iter().enumerate() {
            if !self.used.get(i) {
                self.ctx.errors.push(Error::new(
                    ErrorKind::UnexpectedKey,
                    key.span,
                ));
                had_unexpected = true;
            }
        }

        if had_unexpected {
            Err(Failed)
        } else {
            Ok(())
        }
    }
}

/// Shared state that accumulates errors and holds the arena.
///
/// A `Context` is created by [`parse`](crate::parse) and lives inside
/// [`Document`](crate::Document). Pass it into [`TableHelper::new`] or
/// [`Item::table_helper`] when implementing [`FromToml`].
///
/// Multiple errors can be recorded during a single conversion pass;
/// inspect them afterwards via [`Document::errors`](crate::Document::errors).
pub struct Context<'de> {
    pub arena: &'de Arena,
    pub(crate) index: HashMap<KeyRef<'de>, usize>,
    pub errors: Vec<Error>,
    pub(crate) source: &'de str,
}

impl<'de> Context<'de> {
    /// Returns the original TOML source string passed to [`parse`](crate::parse).
    pub fn source(&self) -> &'de str {
        self.source
    }
    /// Records a "expected X, found Y" type-mismatch error and returns [`Failed`].
    #[cold]
    pub fn error_expected_but_found(&mut self, message: &'static &'static str, found: &Item<'_>) -> Failed {
        self.errors.push(Error::new(
            ErrorKind::Wanted {
                expected: message,
                found: found.type_str(),
            },
            found.span_unchecked(),
        ));
        Failed
    }

    /// Records an "unknown variant" error listing the accepted variants and returns [`Failed`].
    #[cold]
    pub fn error_unexpected_variant(&mut self, expected: &'static [&'static str], found: &Item<'_>) -> Failed {
        self.errors.push(Error::new(
            ErrorKind::UnexpectedVariant { expected },
            found.span_unchecked(),
        ));
        Failed
    }

    /// Records a custom error message at the given span and returns [`Failed`].
    #[cold]
    pub fn error_message_at(&mut self, message: &'static str, at: Span) -> Failed {
        self.errors.push(Error::custom_static(message, at));
        Failed
    }
    /// Pushes a pre-built [`Error`] and returns [`Failed`].
    #[cold]
    pub fn push_error(&mut self, error: Error) -> Failed {
        self.errors.push(error);
        Failed
    }

    /// Records an out-of-range error for the type `name` and returns [`Failed`].
    #[cold]
    pub fn error_out_of_range(&mut self, name: &'static str, span: Span) -> Failed {
        self.errors.push(Error::new(ErrorKind::OutOfRange(name), span));
        Failed
    }

    /// Records a missing-field error and returns [`Failed`].
    ///
    /// Used by generated `FromToml` implementations that iterate over table
    /// entries instead of using [`TableHelper`].
    #[cold]
    pub fn report_missing_field(&mut self, name: &'static str, span: Span) -> Failed {
        self.errors.push(Error::new(ErrorKind::MissingField(name), span));
        Failed
    }

    /// Records a duplicate-field error and returns [`Failed`].
    ///
    /// Used by generated `FromToml` implementations when a field with aliases
    /// is set more than once (e.g. both the primary key and an alias appear).
    #[cold]
    pub fn report_duplicate_field(&mut self, name: &'static str, span: Span) -> Failed {
        self.errors.push(Error::new(ErrorKind::DuplicateField(name), span));
        Failed
    }
}

pub use crate::Failed;

/// Trait for types that can be constructed from a TOML [`Item`].
///
/// Implement this on your own types to enable extraction via
/// [`TableHelper::required`] and [`TableHelper::optional`].
/// Built-in implementations are provided for primitive types, `String`,
/// `Vec<T>`, `Box<T>`, `Option<T>` (via `optional`), and more.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Item, Context, FromToml, Failed, TableHelper};
///
/// struct Point {
///     x: f64,
///     y: f64,
/// }
///
/// impl<'de> FromToml<'de> for Point {
///     fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
///         let mut th = value.table_helper(ctx)?;
///         let x = th.required("x")?;
///         let y = th.required("y")?;
///         th.expect_empty()?;
///         Ok(Point { x, y })
///     }
/// }
/// ```
pub trait FromToml<'de>: Sized {
    /// Attempts to construct `Self` from a TOML [`Item`].
    ///
    /// On failure, records one or more errors in `ctx` and returns
    /// `Err(`[`Failed`]`)`.
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed>;
}

/// Trait for types that can be constructed from flattened TOML table entries.
///
/// Used with `#[toml(flatten)]` on struct fields. Built-in implementations
/// exist for `HashMap` and `BTreeMap`.
///
/// If your type already implements [`FromToml`], you do not need to implement
/// this trait. Use `#[toml(flatten, with = flatten_any)]` in your derive
/// instead. See [`helper::flatten_any`](crate::helper::flatten_any).
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `FromFlattened`",
    note = "if `{Self}` implements `FromToml`, you can use `#[toml(flatten, with = flatten_any)]` instead of a manual `FromFlattened` impl"
)]
pub trait FromFlattened<'de>: Sized {
    /// Intermediate accumulator type used during conversion.
    type Partial;
    /// Creates an empty accumulator to collect flattened entries.
    fn init() -> Self::Partial;
    /// Inserts a single key-value pair into the accumulator.
    fn insert(
        ctx: &mut Context<'de>,
        key: &Key<'de>,
        item: &Item<'de>,
        partial: &mut Self::Partial,
    ) -> Result<(), Failed>;
    /// Converts the accumulator into the final value after all entries
    /// have been inserted.
    fn finish(ctx: &mut Context<'de>, partial: Self::Partial) -> Result<Self, Failed>;
}

/// Converts a TOML key into a map key, preserving span information.
fn key_from_toml<'de, K: FromToml<'de>>(
    ctx: &mut Context<'de>,
    key: &Key<'de>,
) -> Result<K, Failed> {
    let item = Item::string_spanned(key.name, key.span);
    K::from_toml(ctx, &item)
}

impl<'de, K, V, H> FromFlattened<'de> for std::collections::HashMap<K, V, H>
where
    K: Hash + Eq + FromToml<'de>,
    V: FromToml<'de>,
    H: Default + BuildHasher,
{
    type Partial = Self;
    fn init() -> Self {
        std::collections::HashMap::default()
    }
    fn insert(
        ctx: &mut Context<'de>,
        key: &Key<'de>,
        item: &Item<'de>,
        partial: &mut Self::Partial,
    ) -> Result<(), Failed> {
        let k = key_from_toml(ctx, key)?;
        let v = match V::from_toml(ctx, item) {
            Ok(v) => v,
            Err(_) => return Err(Failed),
        };
        partial.insert(k, v);
        Ok(())
    }
    fn finish(_ctx: &mut Context<'de>, partial: Self::Partial) -> Result<Self, Failed> {
        Ok(partial)
    }
}

impl<'de, K, V, H> FromToml<'de> for std::collections::HashMap<K, V, H>
where
    K: Hash + Eq + FromToml<'de>,
    V: FromToml<'de>,
    H: Default + BuildHasher,
{
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let table = value.expect_table(ctx)?;
        let mut map = std::collections::HashMap::default();
        let mut had_error = false;
        for (key, item) in table {
            let k = match key_from_toml(ctx, key) {
                Ok(k) => k,
                Err(_) => {
                    had_error = true;
                    continue;
                }
            };
            match V::from_toml(ctx, item) {
                Ok(v) => {
                    map.insert(k, v);
                }
                Err(_) => had_error = true,
            }
        }
        if had_error { Err(Failed) } else { Ok(map) }
    }
}

impl<'de, K, V> FromFlattened<'de> for BTreeMap<K, V>
where
    K: Ord + FromToml<'de>,
    V: FromToml<'de>,
{
    type Partial = Self;
    fn init() -> Self {
        BTreeMap::new()
    }
    fn insert(
        ctx: &mut Context<'de>,
        key: &Key<'de>,
        item: &Item<'de>,
        partial: &mut Self::Partial,
    ) -> Result<(), Failed> {
        let k = key_from_toml(ctx, key)?;
        let v = match V::from_toml(ctx, item) {
            Ok(v) => v,
            Err(_) => return Err(Failed),
        };
        partial.insert(k, v);
        Ok(())
    }
    fn finish(_ctx: &mut Context<'de>, partial: Self::Partial) -> Result<Self, Failed> {
        Ok(partial)
    }
}

impl<'de, T: FromToml<'de>, const N: usize> FromToml<'de> for [T; N] {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let boxed_slice = Box::<[T]>::from_toml(ctx, value)?;
        match <Box<[T; N]>>::try_from(boxed_slice) {
            Ok(array) => Ok(*array),
            Err(res) => Err(ctx.push_error(Error::custom(
                format!(
                    "Expect Array Size: found {} but expected {}",
                    res.len(),
                    N
                ),
                value.span_unchecked(),
            ))),
        }
    }
}

impl<'de> FromToml<'de> for String {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_str() {
            Some(s) => Ok(s.to_string()),
            None => Err(ctx.error_expected_but_found(&"a string", value)),
        }
    }
}

impl<'de> FromToml<'de> for PathBuf {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_str() {
            Some(s) => Ok(PathBuf::from(s)),
            None => Err(ctx.error_expected_but_found(&"a path", value)),
        }
    }
}

impl<'de, T: FromToml<'de>> FromToml<'de> for Option<T> {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        T::from_toml(ctx, value).map(Some)
    }
}

impl<'de, T: FromToml<'de>> FromToml<'de> for Box<T> {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match T::from_toml(ctx, value) {
            Ok(v) => Ok(Box::new(v)),
            Err(e) => Err(e),
        }
    }
}
impl<'de, T: FromToml<'de>> FromToml<'de> for Box<[T]> {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match Vec::<T>::from_toml(ctx, value) {
            Ok(vec) => Ok(vec.into_boxed_slice()),
            Err(e) => Err(e),
        }
    }
}
impl<'de> FromToml<'de> for Box<str> {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            item::Value::String(&s) => Ok(s.into()),
            _ => Err(ctx.error_expected_but_found(&"a string", value)),
        }
    }
}
impl<'de> FromToml<'de> for &'de str {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            item::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found(&"a string", value)),
        }
    }
}

impl<'de> FromToml<'de> for std::borrow::Cow<'de, str> {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            item::Value::String(s) => Ok(std::borrow::Cow::Borrowed(*s)),
            _ => Err(ctx.error_expected_but_found(&"a string", value)),
        }
    }
}

impl<'de> FromToml<'de> for bool {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_bool() {
            Some(b) => Ok(b),
            None => Err(ctx.error_expected_but_found(&"a bool", value)),
        }
    }
}

fn deser_integer_ctx(
    ctx: &mut Context<'_>,
    value: &Item<'_>,
    min: i64,
    max: i64,
    name: &'static str,
) -> Result<i64, Failed> {
    let span = value.span_unchecked();
    match value.as_i64() {
        Some(i) if i >= min && i <= max => Ok(i),
        Some(_) => Err(ctx.error_out_of_range(name, span)),
        None => Err(ctx.error_expected_but_found(&"an integer", value)),
    }
}

macro_rules! integer_new {
    ($($num:ty),+) => {$(
        impl<'de> FromToml<'de> for $num {
            fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
                match deser_integer_ctx(ctx, value, <$num>::MIN as i64, <$num>::MAX as i64, stringify!($num)) {
                    Ok(i) => Ok(i as $num),
                    Err(e) => Err(e),
                }
            }
        }
    )+};
}

integer_new!(i8, i16, i32, isize, u8, u16, u32);

impl<'de> FromToml<'de> for i64 {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        deser_integer_ctx(ctx, value, i64::MIN, i64::MAX, "i64")
    }
}

impl<'de> FromToml<'de> for u64 {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match deser_integer_ctx(ctx, value, 0, i64::MAX, "u64") {
            Ok(i) => Ok(i as u64),
            Err(e) => Err(e),
        }
    }
}

impl<'de> FromToml<'de> for usize {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        const MAX: i64 = if usize::BITS < 64 {
            usize::MAX as i64
        } else {
            i64::MAX
        };
        match deser_integer_ctx(ctx, value, 0, MAX, "usize") {
            Ok(i) => Ok(i as usize),
            Err(e) => Err(e),
        }
    }
}

impl<'de> FromToml<'de> for f32 {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_f64() {
            Some(f) => Ok(f as f32),
            None => Err(ctx.error_expected_but_found(&"a float", value)),
        }
    }
}

impl<'de> FromToml<'de> for f64 {
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_f64() {
            Some(f) => Ok(f),
            None => Err(ctx.error_expected_but_found(&"a float", value)),
        }
    }
}

impl<'de, T> FromToml<'de> for Vec<T>
where
    T: FromToml<'de>,
{
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let arr = value.expect_array(ctx)?;
        let mut result = Vec::with_capacity(arr.len());
        let mut had_error = false;
        for item in arr {
            match T::from_toml(ctx, item) {
                Ok(v) => result.push(v),
                Err(_) => had_error = true,
            }
        }
        if had_error { Err(Failed) } else { Ok(result) }
    }
}

impl<'de, K, V> FromToml<'de> for BTreeMap<K, V>
where
    K: Ord + FromToml<'de>,
    V: FromToml<'de>,
{
    fn from_toml(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let table = value.expect_table(ctx)?;
        let mut map = BTreeMap::new();
        let mut had_error = false;
        for (key, item) in table {
            let k = match key_from_toml(ctx, key) {
                Ok(k) => k,
                Err(_) => {
                    had_error = true;
                    continue;
                }
            };
            match V::from_toml(ctx, item) {
                Ok(v) => {
                    map.insert(k, v);
                }
                Err(_) => had_error = true,
            }
        }
        if had_error { Err(Failed) } else { Ok(map) }
    }
}

impl<'de> Item<'de> {
    /// Returns a string, or records an error with a custom `expected` message.
    ///
    /// Use this instead of [`expect_string`](Self::expect_string) when the
    /// expected value is more specific than just "a string" — for example,
    /// `"an IPv4 address"` or `"a hex color"`.
    pub fn expect_custom_string(
        &self,
        ctx: &mut Context<'de>,
        expected: &'static &'static str,
    ) -> Result<&'de str, Failed> {
        match self.value() {
            item::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found(expected, self)),
        }
    }
    /// Returns a string, or records an error if this is not a string.
    pub fn expect_string(&self, ctx: &mut Context<'de>) -> Result<&'de str, Failed> {
        match self.value() {
            item::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found(&"a string", self)),
        }
    }

    /// Returns an array reference, or records an error if this is not an array.
    pub fn expect_array(&self, ctx: &mut Context<'de>) -> Result<&crate::Array<'de>, Failed> {
        match self.as_array() {
            Some(arr) => Ok(arr),
            None => Err(ctx.error_expected_but_found(&"an array", self)),
        }
    }

    /// Returns a table reference, or records an error if this is not a table.
    pub fn expect_table(&self, ctx: &mut Context<'de>) -> Result<&crate::Table<'de>, Failed> {
        match self.as_table() {
            Some(table) => Ok(table),
            None => Err(ctx.error_expected_but_found(&"a table", self)),
        }
    }

    /// Creates a [`TableHelper`] for this item, returning an error if it is not a table.
    ///
    /// This is the typical entry point for implementing [`FromToml`].
    pub fn table_helper<'ctx, 'item>(
        &'item self,
        ctx: &'ctx mut Context<'de>,
    ) -> Result<TableHelper<'ctx, 'item, 'de>, Failed> {
        let Some(table) = self.as_table() else {
            return Err(ctx.error_expected_but_found(&"a table", self));
        };
        Ok(TableHelper::new(ctx, table))
    }
}

/// Collects all errors encountered during parsing and conversion.
///
/// Returned by [`from_str`](crate::from_str) and [`Document::to`](crate::Document::to).
/// Contains one or more [`Error`] values, each with its own source span.
///
/// # Examples
///
/// ```
/// let result = toml_spanner::from_str::<std::collections::HashMap<String, String>>(
///     "bad toml {"
/// );
/// assert!(result.is_err());
/// let err = result.unwrap_err();
/// assert!(!err.errors.is_empty());
/// ```
pub struct FromTomlError {
    /// The accumulated errors.
    pub errors: Vec<Error>,
}

impl Display for FromTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(first) = self.errors.first() else {
            return f.write_str("deserialization failed");
        };
        Display::fmt(first, f)?;
        let remaining = self.errors.len() - 1;
        if remaining > 0 {
            write!(
                f,
                " (+{remaining} more error{})",
                if remaining == 1 { "" } else { "s" }
            )?;
        }
        Ok(())
    }
}

impl Debug for FromTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FromTomlError")
            .field("errors", &self.errors)
            .finish()
    }
}

impl std::error::Error for FromTomlError {}

impl From<Error> for FromTomlError {
    fn from(error: Error) -> Self {
        Self {
            errors: vec![error],
        }
    }
}

impl From<Vec<Error>> for FromTomlError {
    fn from(errors: Vec<Error>) -> Self {
        Self { errors }
    }
}
