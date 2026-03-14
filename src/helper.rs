//! Contains submodules for use with the `#[toml(with = ...)]` field attribute.

use crate::{Item, Table};

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

/// Flattens any type that already implements [`FromToml`] or [`ToToml`]
/// into a parent struct, without an explicit [`FromFlattened`] or
/// [`ToFlattened`] implementation.
///
/// This is the easiest way to flatten structs, tagged enums, adjacently
/// tagged enums, and other complex types. It works with any type the derive
/// macro can produce, including all enum representations (`tag`,
/// `tag`+`content`, `untagged`).
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
/// `flatten_any` works by collecting unrecognized key-value pairs into a
/// temporary table and then passing that table through the regular
/// [`FromToml`] path (and vice-versa for [`ToToml`]).
/// This is not as efficient as a native [`FromFlattened`]/[`ToFlattened`]
/// implementation could be, but it is plenty fast for virtually all use
/// cases and extremely flexible.
///
/// [`FromToml`]: crate::FromToml
/// [`ToToml`]: crate::ToToml
/// [`FromFlattened`]: crate::FromFlattened
/// [`ToFlattened`]: crate::ToFlattened
pub mod flatten_any {
    #[cfg(feature = "to-toml")]
    use crate::{Arena, ToToml, ToTomlError};
    use crate::{Context, Failed, FromToml, Item, Key, Table, helper::ShallowTable};

    pub fn init<'a, 'b>() -> ShallowTable<'a, 'b> {
        ShallowTable {
            table: Table::new(),
            __marker: std::marker::PhantomData,
        }
    }

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
        partial.table.insert(*key, shallow, ctx.arena);
        Ok(())
    }

    pub fn finish<'de, 'b, T: FromToml<'de>>(
        ctx: &mut Context<'de>,
        partial: ShallowTable<'de, 'b>,
    ) -> Result<T, Failed> {
        T::from_toml(ctx, partial.table.as_item())
    }

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
            table.insert(key, val, arena);
        }
        Ok(())
    }
}
