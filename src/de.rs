#[cfg(test)]
#[path = "./de_tests.rs"]
mod tests;

use std::num::NonZeroU64;

use foldhash::HashMap;

use crate::{
    Arena, Error, ErrorKind, Key, Span, Table,
    parser::{INDEXED_TABLE_THRESHOLD, KeyRef},
    value::{self, Item},
};

/// Guides deserialization of a [`Table`] by tracking which fields have been
/// consumed.
///
/// Create one via [`Root::helper`](crate::Root::helper) for the root table,
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
/// use toml_spanner::{Arena, Deserialize, Item, Context, Failed, TableHelper};
///
/// struct Config {
///     name: String,
///     port: u16,
///     debug: bool,
/// }
///
/// impl<'de> Deserialize<'de> for Config {
///     fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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
        let bitset_len = capacity.div_ceil(64);
        let bitset = arena.alloc(bitset_len).cast::<u64>();
        for offset in 0..bitset_len {
            unsafe {
                bitset.add(offset).write(0);
            }
        }
        let slice = unsafe { std::slice::from_raw_parts_mut(bitset.as_ptr(), bitset_len) };
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
/// [`TableHelper::required`] or [`TableHelper::optional`].
///
/// Obtained via [`TableHelper::into_remaining`].
pub struct RemainingEntriesIter<'t, 'de> {
    entries: &'t [(Key<'de>, Item<'de>)],
    remaining_cells: std::slice::Iter<'de, u64>,
    bits: u64,
}
impl RemainingEntriesIter<'_, '_> {
    fn next_bucket(&mut self) -> bool {
        if let Some(bucket) = self.remaining_cells.next() {
            debug_assert!(self.entries.len() > 64);
            if let Some(remaining) = self.entries.get(64..) {
                self.entries = remaining;
            } else {
                // Shouldn't occur in practice, but no need to panic here.
                return false;
            }
            self.bits = !*bucket;
            true
        } else {
            false
        }
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
    /// Prefer [`Item::table_helper`] when implementing [`Deserialize`], or
    /// [`Root::helper`](crate::Root::helper) for the root table.
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
    /// deserialize it. The entry will still be flagged as unexpected by
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
    /// [`Deserialize`].
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
            self.ctx.push_error(Error {
                kind: ErrorKind::Custom(std::borrow::Cow::Owned(err.to_string())),
                span: item.span(),
            })
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
                self.ctx.push_error(Error {
                    kind: ErrorKind::Custom(std::borrow::Cow::Owned(err.to_string())),
                    span: item.span(),
                })
            })
            .ok()
    }

    /// Returns the raw [`Item`] for a required field.
    ///
    /// Like [`required`](Self::required) but skips deserialization, giving
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
    /// Like [`optional`](Self::optional) but skips deserialization, giving
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
        self.ctx.errors.push(Error {
            kind: ErrorKind::MissingField(name),
            span: self.table.span(),
        });
        Failed
    }

    /// Deserializes a required field, recording an error if the key is missing.
    ///
    /// The field is marked as consumed so [`expect_empty`](Self::expect_empty)
    /// will not flag it as unexpected.
    ///
    /// # Errors
    ///
    /// Returns [`Failed`] if the key is absent or if `T::deserialize` fails.
    /// In both cases the error is pushed onto the shared [`Context`].
    pub fn required<T: Deserialize<'de>>(&mut self, name: &'static str) -> Result<T, Failed> {
        let Some((_, val)) = self.optional_entry(name) else {
            return Err(self.report_missing_field(name));
        };

        T::deserialize(self.ctx, val)
    }

    /// Deserializes an optional field, returning [`None`] if the key is missing
    /// or deserialization fails (recording the error in the [`Context`]).
    ///
    /// The field is marked as consumed so [`expect_empty`](Self::expect_empty)
    /// will not flag it as unexpected.
    pub fn optional<T: Deserialize<'de>>(&mut self, name: &str) -> Option<T> {
        let Some((_, val)) = self.optional_entry(name) else {
            return None;
        };

        #[allow(clippy::manual_ok_err)]
        match T::deserialize(self.ctx, val) {
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

    /// Finishes deserialization, recording an error if any fields were not
    /// consumed by [`required`](Self::required) or
    /// [`optional`](Self::optional).
    ///
    /// Call this as the last step in a [`Deserialize`] implementation to
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

        let mut keys = Vec::new();
        for (i, (key, _)) in self.table.entries().iter().enumerate() {
            if !self.used.get(i) {
                keys.push((key.name.into(), key.span));
            }
        }

        if keys.is_empty() {
            return Ok(());
        }

        self.ctx.errors.push(Error::from((
            ErrorKind::UnexpectedKeys { keys },
            self.table.span(),
        )));
        Err(Failed)
    }
}

/// Shared deserialization state that accumulates errors and holds the arena.
///
/// A `Context` is created by [`parse`](crate::parse) and lives inside
/// [`Root`](crate::Root). Pass it into [`TableHelper::new`] or
/// [`Item::table_helper`] when implementing [`Deserialize`].
///
/// Multiple errors can be recorded during a single deserialization pass;
/// inspect them afterwards via [`Root::errors`](crate::Root::errors).
pub struct Context<'de> {
    pub arena: &'de Arena,
    pub(crate) index: HashMap<KeyRef<'de>, usize>,
    pub errors: Vec<Error>,
}

impl<'de> Context<'de> {
    /// Records a "expected X, found Y" type-mismatch error and returns [`Failed`].
    #[cold]
    pub fn error_expected_but_found(&mut self, message: &'static str, found: &Item<'_>) -> Failed {
        self.errors.push(Error {
            kind: ErrorKind::Wanted {
                expected: message,
                found: found.type_str(),
            },
            span: found.span(),
        });
        Failed
    }

    /// Records a custom error message at the given span and returns [`Failed`].
    #[cold]
    pub fn error_message_at(&mut self, message: &'static str, at: Span) -> Failed {
        self.errors.push(Error {
            kind: ErrorKind::Custom(std::borrow::Cow::Borrowed(message)),
            span: at,
        });
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
        self.errors.push(Error {
            kind: ErrorKind::OutOfRange(name),
            span,
        });
        Failed
    }
}

/// Sentinel indicating that a deserialization error has been recorded in the
/// [`Context`].
///
/// `Failed` carries no data — the actual error details live in
/// [`Context::errors`](Context::errors). Return `Err(Failed)` from
/// [`Deserialize::deserialize`] after calling one of the `Context::error_*`
/// methods.
#[derive(Debug)]
pub struct Failed;

/// Trait for types that can be deserialized from a TOML [`Item`].
///
/// Implement this on your own types to enable extraction via
/// [`TableHelper::required`] and [`TableHelper::optional`].
/// Built-in implementations are provided for primitive types, `String`,
/// `Vec<T>`, `Box<T>`, `Option<T>` (via `optional`), and more.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Item, Context, Deserialize, Failed, TableHelper};
///
/// struct Point {
///     x: f64,
///     y: f64,
/// }
///
/// impl<'de> Deserialize<'de> for Point {
///     fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
///         let mut th = value.table_helper(ctx)?;
///         let x = th.required("x")?;
///         let y = th.required("y")?;
///         th.expect_empty()?;
///         Ok(Point { x, y })
///     }
/// }
/// ```
pub trait Deserialize<'de>: Sized {
    /// Attempts to produce `Self` from a TOML [`Item`].
    ///
    /// On failure, records one or more errors in `ctx` and returns
    /// `Err(`[`Failed`]`)`.
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed>;
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for [T; N] {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let boxed_slice = Box::<[T]>::deserialize(ctx, value)?;
        match <Box<[T; N]>>::try_from(boxed_slice) {
            Ok(array) => Ok(*array),
            Err(res) => Err(ctx.push_error(Error {
                kind: ErrorKind::Custom(std::borrow::Cow::Owned(format!(
                    "Expect Array Size: found {} but expected {}",
                    res.len(),
                    N
                ))),
                span: value.span(),
            })),
        }
    }
}

impl<'de> Deserialize<'de> for String {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_str() {
            Some(s) => Ok(s.to_string()),
            None => Err(ctx.error_expected_but_found("a string", value)),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Box<T> {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match T::deserialize(ctx, value) {
            Ok(v) => Ok(Box::new(v)),
            Err(e) => Err(e),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Box<[T]> {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match Vec::<T>::deserialize(ctx, value) {
            Ok(vec) => Ok(vec.into_boxed_slice()),
            Err(e) => Err(e),
        }
    }
}
impl<'de> Deserialize<'de> for Box<str> {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            value::Value::String(&s) => Ok(s.into()),
            _ => Err(ctx.error_expected_but_found("a string", value)),
        }
    }
}
impl<'de> Deserialize<'de> for &'de str {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            value::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found("a string", value)),
        }
    }
}

impl<'de> Deserialize<'de> for std::borrow::Cow<'de, str> {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.value() {
            value::Value::String(s) => Ok(std::borrow::Cow::Borrowed(*s)),
            _ => Err(ctx.error_expected_but_found("a string", value)),
        }
    }
}

impl<'de> Deserialize<'de> for bool {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_bool() {
            Some(b) => Ok(b),
            None => Err(ctx.error_expected_but_found("a bool", value)),
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
    let span = value.span();
    match value.as_i64() {
        Some(i) if i >= min && i <= max => Ok(i),
        Some(_) => Err(ctx.error_out_of_range(name, span)),
        None => Err(ctx.error_expected_but_found("an integer", value)),
    }
}

macro_rules! integer_new {
    ($($num:ty),+) => {$(
        impl<'de> Deserialize<'de> for $num {
            fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
                match deser_integer_ctx(ctx, value, <$num>::MIN as i64, <$num>::MAX as i64, stringify!($num)) {
                    Ok(i) => Ok(i as $num),
                    Err(e) => Err(e),
                }
            }
        }
    )+};
}

integer_new!(i8, i16, i32, isize, u8, u16, u32);

impl<'de> Deserialize<'de> for i64 {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        deser_integer_ctx(ctx, value, i64::MIN, i64::MAX, "i64")
    }
}

impl<'de> Deserialize<'de> for u64 {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match deser_integer_ctx(ctx, value, 0, i64::MAX, "u64") {
            Ok(i) => Ok(i as u64),
            Err(e) => Err(e),
        }
    }
}

impl<'de> Deserialize<'de> for usize {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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

impl<'de> Deserialize<'de> for f32 {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_f64() {
            Some(f) => Ok(f as f32),
            None => Err(ctx.error_expected_but_found("a float", value)),
        }
    }
}

impl<'de> Deserialize<'de> for f64 {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        match value.as_f64() {
            Some(f) => Ok(f),
            None => Err(ctx.error_expected_but_found("a float", value)),
        }
    }
}

impl<'de, T> Deserialize<'de> for Vec<T>
where
    T: Deserialize<'de>,
{
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let arr = value.expect_array(ctx)?;
        let mut result = Vec::with_capacity(arr.len());
        let mut had_error = false;
        for item in arr {
            match T::deserialize(ctx, item) {
                Ok(v) => result.push(v),
                Err(_) => had_error = true,
            }
        }
        if had_error { Err(Failed) } else { Ok(result) }
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
        expected: &'static str,
    ) -> Result<&'de str, Failed> {
        match self.value() {
            value::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found(expected, self)),
        }
    }
    /// Returns a string, or records an error if this is not a string.
    pub fn expect_string(&self, ctx: &mut Context<'de>) -> Result<&'de str, Failed> {
        match self.value() {
            value::Value::String(s) => Ok(*s),
            _ => Err(ctx.error_expected_but_found("a string", self)),
        }
    }

    /// Returns an array reference, or records an error if this is not an array.
    pub fn expect_array(&self, ctx: &mut Context<'de>) -> Result<&crate::Array<'de>, Failed> {
        match self.as_array() {
            Some(arr) => Ok(arr),
            None => Err(ctx.error_expected_but_found("an array", self)),
        }
    }

    /// Returns a table reference, or records an error if this is not a table.
    pub fn expect_table(&self, ctx: &mut Context<'de>) -> Result<&crate::Table<'de>, Failed> {
        match self.as_table() {
            Some(table) => Ok(table),
            None => Err(ctx.error_expected_but_found("a table", self)),
        }
    }

    /// Creates a [`TableHelper`] for this item, returning an error if it is not a table.
    ///
    /// This is the typical entry point for implementing [`Deserialize`].
    pub fn table_helper<'ctx, 'item>(
        &'item self,
        ctx: &'ctx mut Context<'de>,
    ) -> Result<TableHelper<'ctx, 'item, 'de>, Failed> {
        let Some(table) = self.as_table() else {
            return Err(ctx.error_expected_but_found("a table", self));
        };
        Ok(TableHelper::new(ctx, table))
    }
}
