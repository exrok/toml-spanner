//! Contains submodules for use with the `#[toml(with = ...)]` field attribute.

use crate::{Item, Table};

/// Converts a TOML string value into any type implementing [`FromStr`].
///
/// Pair with the `with` field attribute to store types like [`IpAddr`] or
/// [`SocketAddr`] as plain TOML strings. At parse time the string is passed
/// through [`str::parse`], and any [`std::str::FromStr::Err`] is reported
/// as a custom error with the span of the original value.
///
/// For the reverse direction, see [`display`].
///
/// # Examples
///
/// ```rust,ignore
/// use std::net::IpAddr;
/// use toml_spanner::Toml;
/// use toml_spanner::helper::parse_string;
///
/// #[derive(Toml)]
/// #[toml(FromToml)]
/// struct Server {
///     #[toml(with = parse_string)]
///     addr: IpAddr,
/// }
/// ```
///
/// [`FromStr`]: std::str::FromStr
/// [`IpAddr`]: std::net::IpAddr
/// [`SocketAddr`]: std::net::SocketAddr
pub mod parse_string {
    use crate::error::Error;
    use crate::{Context, Failed, Item};

    /// Parses a TOML string value into `T` via [`FromStr`].
    ///
    /// Returns [`Failed`] when the item is not a string or when
    /// [`str::parse`] returns an error.
    ///
    /// [`FromStr`]: std::str::FromStr
    pub fn from_toml<'de, T>(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<T, Failed>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let Some(s) = item.as_str() else {
            return Err(ctx.report_expected_but_found(&"a string", item));
        };
        match s.parse::<T>() {
            Ok(val) => Ok(val),
            Err(e) => Err(ctx.push_error(Error::custom(e, item.span()))),
        }
    }
}

/// Converts any type implementing [`Display`] into a TOML string [`Item`].
///
/// Pair with the `with` field attribute to emit types like [`IpAddr`] or
/// [`SocketAddr`] as plain TOML strings. The value is formatted with
/// [`Display`] and the resulting string is allocated in the arena.
///
/// For the reverse direction, see [`parse_string`].
///
/// # Examples
///
/// ```rust,ignore
/// use std::net::IpAddr;
/// use toml_spanner::Toml;
/// use toml_spanner::helper::display;
///
/// #[derive(Toml)]
/// #[toml(ToToml)]
/// struct Server {
///     #[toml(with = display)]
///     addr: IpAddr,
/// }
/// ```
///
/// [`Display`]: std::fmt::Display
/// [`IpAddr`]: std::net::IpAddr
/// [`SocketAddr`]: std::net::SocketAddr
#[cfg(feature = "to-toml")]
pub mod display {
    use crate::{Arena, Item, ToTomlError};

    /// Converts a value to a TOML string [`Item`] via [`Display`].
    ///
    /// The formatted string is allocated in the arena so it lives as long
    /// as other parsed data.
    ///
    /// [`Item`]: crate::Item
    /// [`Display`]: std::fmt::Display
    pub fn to_toml<'a>(
        value: &impl std::fmt::Display,
        arena: &'a Arena,
    ) -> Result<Item<'a>, ToTomlError> {
        let s = value.to_string();
        Ok(Item::string(arena.alloc_str(&s)))
    }
}

#[doc(hidden)]
pub struct ShallowTable<'de, 'b> {
    table: Table<'de>,
    // Table accumulates Items from the parent table and makes bitwise copies
    // to comply with the regular interface of FromToml. Done naively this would
    // be unsound, because collections like `Array` or similarly to `&mut [Array]`
    // they might not be Drop, but they are owned.
    // However, the marker below ensures that Item remains shared throughout the
    // lifetime of ShallowTable, and ShallowTable itself only uses the Items as
    // shared references. Finally, the use of raw pointers in implementing Item
    // imply no alias restrictions while dormant.
    __marker: std::marker::PhantomData<&'b Item<'de>>,
}

/// Flattens any type implementing [`FromToml`] or [`ToToml`] into a parent
/// struct without a [`FromFlattened`] or [`ToFlattened`] implementation.
///
/// Works with any type the derive macro can produce, including all enum
/// representations (`tag`, `tag`+`content`, `untagged`).
///
/// # Examples
///
/// Apply the `with` attribute together with `flatten` on a field:
///
/// ```rust,ignore
/// use toml_spanner::Toml;
/// use toml_spanner::helper::flatten_any;
///
/// #[derive(Toml)]
/// #[toml(Toml)]
/// struct Point {
///     x: i64,
///     y: i64,
/// }
///
/// #[derive(Toml)]
/// #[toml(Toml)]
/// struct Labeled {
///     name: String,
///     #[toml(flatten, with = flatten_any)]
///     point: Point,
/// }
/// ```
///
/// The TOML representation merges the fields of `Point` directly into
/// `Labeled`:
///
/// ```text
/// name = "origin"
/// x = 10
/// y = 20
/// ```
///
/// # Performance
///
/// `flatten_any` collects unrecognized key-value pairs into a temporary
/// table and passes it through the regular [`FromToml`] path (and
/// vice-versa for [`ToToml`]). Not as efficient as a native
/// [`FromFlattened`]/[`ToFlattened`] implementation, but fast enough for
/// virtually all use cases.
///
/// [`FromToml`]: crate::FromToml
/// [`ToToml`]: crate::ToToml
/// [`FromFlattened`]: crate::FromFlattened
/// [`ToFlattened`]: crate::ToFlattened
pub mod flatten_any {
    use foldhash::HashMapExt;

    #[cfg(feature = "to-toml")]
    use crate::{Arena, ToToml, ToTomlError};
    use crate::{
        Context, Failed, FromToml, Item, Key, Table, error::MaybeTomlPath, helper::ShallowTable,
        parser::KeyRef,
    };

    fn patch_errors<'de, 'b>(
        ctx: &mut Context<'de>,
        original: usize,
        parent: &Table<'de>,
        partial: &ShallowTable<'de, 'b>,
    ) {
        let entries = partial.table.entries();
        let entry_size = std::mem::size_of::<(Key<'_>, Item<'_>)>();
        let base = entries.as_ptr() as *const u8;
        // SAFETY: entries is a valid slice; one-past-the-end is a valid pointer.
        let end = unsafe { base.add(entries.len() * entry_size) };

        // Note: We could make faster index, but we reuse this same index format
        // to reduce code bloat, for this edge case.
        // See: flatten_any_error_patching_large_scale in tests/derive.rs for example
        // where this temporary index kicks in. (10s -> 0.03s)
        let mut temporary_index: foldhash::HashMap<KeyRef<'de>, usize>;

        let errors = &mut ctx.errors[original..];
        // Only use index for tables created from parse.
        // Note: this is only heuristic, but contrived situation underwhich where this
        // would actually introduce a issue is insane.
        let index = if parent.span().is_empty() {
            if parent.len() > 8 && errors.len() > 2 {
                temporary_index = foldhash::HashMap::with_capacity(parent.len());
                // Safety: length is greater the 8, thus more then 0
                let table_id = unsafe { parent.value.first_key_span_start_unchecked() };
                let mut i = 0;
                for (key, _) in parent.entries() {
                    temporary_index.insert(KeyRef::new(key.name, table_id), i);
                    i += 1;
                }
                Some(&temporary_index)
            } else {
                None
            }
        } else {
            Some(&ctx.index)
        };

        for error in errors {
            if !error.path.is_uncomputed() {
                continue;
            }
            if std::ptr::addr_eq(error.path.uncomputed_ptr(), partial) {
                error.path = MaybeTomlPath::uncomputed(parent.as_item());
                error.span = parent.span();
                continue;
            }
            let ptr = error.path.uncomputed_ptr() as *const u8;
            if ptr < base || ptr >= end {
                continue;
            }
            // SAFETY: ptr is within the entries allocation; byte_offset_from is valid.
            let byte_offset = unsafe { ptr.byte_offset_from(base) } as usize;
            let entry_index = byte_offset / entry_size;
            if let Some((key, _)) = &entries.get(entry_index) {
                if let Some((_, item)) = parent.value.get_entry_with_maybe_index(key.name, index) {
                    error.path = MaybeTomlPath::uncomputed(item);
                }
            }
        }
    }

    /// Creates an empty accumulator for collecting flattened entries.
    pub fn init<'a, 'b>() -> ShallowTable<'a, 'b> {
        ShallowTable {
            table: Table::new(),
            __marker: std::marker::PhantomData,
        }
    }

    /// Inserts a single key-value pair into the accumulator.
    pub fn insert<'de, 'b>(
        ctx: &mut Context<'de>,
        key: &Key<'de>,
        item: &'b Item<'de>,
        partial: &mut ShallowTable<'de, 'b>,
    ) -> Result<(), Failed> {
        // SAFETY: Item has no Drop impl. The bitwise copy shares
        // internal table/array pointers with the original. This is safe because:
        // - The arena never frees, so all pointed-to data remains valid for 'de
        // - The 'b lifetime keeps the original immutably borrowed, preventing
        //   mutation of shared backing storage while the ShallowTable exists
        // - finish() passes the shallow table to FromToml::from_toml(&Item),
        //   which only reads through shared references
        let shallow = unsafe { std::ptr::read(item) };
        partial.table.insert_unique(*key, shallow, ctx.arena);
        Ok(())
    }

    /// Converts the accumulated entries into `T` via [`FromToml`].
    pub fn finish<'de, 'b, T: FromToml<'de>>(
        ctx: &mut Context<'de>,
        parent: &Table<'de>,
        partial: ShallowTable<'de, 'b>,
    ) -> Result<T, Failed> {
        let original = ctx.errors.len();
        let result = T::from_toml(ctx, partial.table.as_item());
        if ctx.errors.len() > original {
            patch_errors(ctx, original, parent, &partial);
        }
        result
    }

    /// Converts `value` into [`Item`] entries and inserts them
    /// into `table`.
    #[cfg(feature = "to-toml")]
    pub fn to_flattened<'a, T: ToToml>(
        value: &'a T,
        arena: &'a Arena,
        table: &mut Table<'a>,
    ) -> Result<(), ToTomlError> {
        let item = value.to_toml(arena)?;
        let Some(src) = item.into_table() else {
            return Err(ToTomlError::from("flatten_any: expected a table"));
        };
        for (key, val) in src {
            table.insert_unique(key, val, arena);
        }
        Ok(())
    }
}
