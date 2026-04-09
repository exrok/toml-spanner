#[cfg(test)]
#[path = "./owned_item_tests.rs"]
mod tests;

use crate::{Arena, Item};

/// A self-contained TOML value that owns its backing [`Arena`].
///
/// An [`Item`] normally borrows from an externally managed [`Arena`].
/// `OwnedItem` bundles the item together with its own arena so the
/// value can be stored, returned, or moved independently of any
/// parse context.
///
/// Create an `OwnedItem` by converting from an [`Item`] reference or
/// value. Access the underlying [`Item`] through [`item()`](Self::item).
///
/// When the `from-toml` feature is enabled, `OwnedItem` implements
/// [`FromToml`](crate::FromToml) so it can be used directly in
/// deserialization structs. When the `to-toml` feature is enabled, it
/// implements [`ToToml`](crate::ToToml) as well.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, OwnedItem, parse};
///
/// let arena = Arena::new();
/// let doc = parse("greeting = 'hello'", &arena).unwrap();
/// let owned = OwnedItem::from(doc.table()["greeting"].item().unwrap());
///
/// // The owned item is independent of the original arena.
/// drop(arena);
/// assert_eq!(owned.item().as_str(), Some("hello"));
/// ```
pub struct OwnedItem {
    item: Item<'static>,
    _arena: Arena,
}

impl<'a> From<&Item<'a>> for OwnedItem {
    /// Creates an `OwnedItem` by deep-cloning `item` into a fresh arena.
    ///
    /// All strings (keys and values) are copied, so the result is fully
    /// independent of the source arena.
    fn from(item: &Item<'a>) -> Self {
        let arena = Arena::new();
        let item = item.deep_clone_in(&arena);
        // SAFETY: `deep_clone_in` copies all borrowed data into `arena`,
        // so `item` borrows exclusively from `arena`. We erase to `'static`
        // because Rust cannot express "borrows from a sibling field".
        // Sound because Arena is heap-only, Item is drop-free, and `item()`
        // re-narrows the lifetime to `&'a self`.
        Self {
            item: unsafe { std::mem::transmute::<Item<'_>, Item<'static>>(item) },
            _arena: arena,
        }
    }
}

impl<'a> From<Item<'a>> for OwnedItem {
    /// Creates an `OwnedItem` by deep-cloning `item` into a fresh arena.
    ///
    /// This is a convenience wrapper that delegates to `From<&Item>`.
    fn from(item: Item<'a>) -> Self {
        OwnedItem::from(&item)
    }
}

impl OwnedItem {
    /// Returns a reference to the contained [`Item`].
    ///
    /// The returned item borrows from `self` and provides the same
    /// accessor methods as any other [`Item`] (`as_str()`, `as_table()`,
    /// `value()`, etc.).
    pub fn item<'a>(&'a self) -> &'a Item<'a> {
        // SAFETY: Shortens `Item<'static>` to `Item<'a>`. Item is covariant
        // in `'de`, so this is a subtyping cast the compiler cannot verify
        // because `'static` is a fiction for the arena-owned data.
        // This is the standard, self-referential workaround.
        unsafe { std::mem::transmute::<&Item<'static>, &Item<'a>>(&self.item) }
    }
}

#[cfg(feature = "from-toml")]
impl<'a> crate::FromToml<'a> for OwnedItem {
    fn from_toml(_: &mut crate::Context<'a>, item: &Item<'a>) -> Result<Self, crate::Failed> {
        Ok(OwnedItem::from(item))
    }
}

#[cfg(feature = "to-toml")]
impl crate::ToToml for OwnedItem {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, crate::ToTomlError> {
        Ok(self.item().clone_in(arena))
    }
}
