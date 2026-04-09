#[cfg(test)]
#[path = "./owned_item_tests.rs"]
mod tests;

use crate::item::{TAG_ARRAY, TAG_STRING, TAG_TABLE};
use crate::{Item, Key, Kind, Table};
use std::mem::size_of;
use std::ptr::NonNull;

/// Write cursor into an [`OwnedItem`] allocation, used by
/// [`Item::emplace_in`] to copy item trees without an arena.
///
/// The allocation is split into two contiguous regions:
/// - **aligned** (front): table entries and array elements (8-byte aligned)
/// - **string** (back): key names and string values (packed, no alignment)
pub(crate) struct ItemCopyTarget {
    pub(crate) aligned: *mut u8,
    #[cfg(debug_assertions)]
    pub(crate) aligned_end: *mut u8,
    pub(crate) string: *mut u8,
    #[cfg(debug_assertions)]
    pub(crate) string_end: *mut u8,
}

impl ItemCopyTarget {
    /// Bumps the aligned pointer forward by `size` bytes, returning
    /// a pointer to the start of the allocated region.
    ///
    /// # Safety
    ///
    /// `size` bytes must remain in the aligned region.
    pub(crate) unsafe fn alloc_aligned(&mut self, size: usize) -> NonNull<u8> {
        #[cfg(debug_assertions)]
        // SAFETY: Pointer arithmetic for bounds check only.
        unsafe {
            assert!(self.aligned.add(size) <= self.aligned_end)
        };
        let ptr = self.aligned;
        // SAFETY: Caller guarantees sufficient space in the aligned region.
        unsafe {
            self.aligned = self.aligned.add(size);
            NonNull::new_unchecked(ptr)
        }
    }

    /// Copies a string into the string region and returns a reference to it.
    ///
    /// # Safety
    ///
    /// `s.len()` bytes must remain in the string region. The returned
    /// reference is `'static` only because OwnedItem manages the backing
    /// memory; the caller must not let it escape.
    pub(crate) unsafe fn copy_str(&mut self, s: &str) -> &'static str {
        if s.is_empty() {
            return "";
        }
        let len = s.len();
        #[cfg(debug_assertions)]
        // SAFETY: Pointer arithmetic for bounds check only.
        unsafe {
            assert!(self.string.add(len) <= self.string_end)
        };
        // SAFETY: Caller guarantees sufficient space. Source and destination
        // do not overlap (source is the parsed input or arena, destination is
        // the OwnedItem allocation).
        unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), self.string, len);
            let result =
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.string, len));
            self.string = self.string.add(len);
            result
        }
    }
}

/// Computes the total aligned and string bytes needed to deep-copy `item`.
fn compute_size(item: &Item<'_>, aligned: &mut usize, strings: &mut usize) {
    match item.tag() {
        TAG_STRING => {
            if let Some(s) = item.as_str() {
                *strings += s.len();
            }
        }
        TAG_TABLE => {
            // SAFETY: tag == TAG_TABLE guarantees this is a table item.
            let table = unsafe { item.as_table_unchecked() };
            *aligned += table.len() * size_of::<(Key<'_>, Item<'_>)>();
            for (key, child) in table {
                *strings += key.name.len();
                compute_size(child, aligned, strings);
            }
        }
        TAG_ARRAY => {
            // SAFETY: tag == TAG_ARRAY guarantees this is an array item.
            let array = unsafe { item.as_array_unchecked() };
            *aligned += array.len() * size_of::<Item<'_>>();
            for child in array {
                compute_size(child, aligned, strings);
            }
        }
        _ => {}
    }
}

pub struct OwnedTable {
    inner: OwnedItem,
}

impl OwnedTable {
    #[inline(always)]
    pub fn table<'a>(&'a self) -> &'a Table<'a> {
        // SAFETY: OwnedItem guarantees the item is valid for the lifetime of self.
        unsafe { self.inner.item().as_table_unchecked() }
    }
}

impl From<&Table<'_>> for OwnedTable {
    fn from(value: &Table<'_>) -> Self {
        let owned_item = OwnedItem::from(value.as_item());
        debug_assert_eq!(owned_item.item().kind(), Kind::Table);
        Self { inner: owned_item }
    }
}

/// A self-contained TOML value that owns its backing storage.
///
/// An [`Item`] normally borrows from an externally managed [`Arena`](crate::Arena).
/// `OwnedItem` bundles the item together with its own allocation so the
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
    ptr: NonNull<u8>,
    capacity: usize,
}

impl std::fmt::Debug for OwnedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.item.fmt(f)
    }
}

impl Clone for OwnedItem {
    fn clone(&self) -> Self {
        OwnedItem::from(self.item())
    }
}

impl Drop for OwnedItem {
    fn drop(&mut self) {
        if self.capacity > 0 {
            // Pedantically, remove the allocated Item before deallocating.
            // probrably not needed, MIRI seams to be happy without it.
            // self.item = Item::from(false);

            // SAFETY: ptr was allocated with Layout { size: capacity, align: 8 }
            // in `From<&Item>`. capacity > 0 guarantees a real allocation.
            unsafe {
                let layout = std::alloc::Layout::from_size_align_unchecked(self.capacity, 8);
                std::alloc::dealloc(self.ptr.as_ptr(), layout);
            }
        }
    }
}

impl<'a> From<&Item<'a>> for OwnedItem {
    /// Creates an `OwnedItem` by copying `item` into a single managed allocation.
    ///
    /// All strings (keys and values) are copied, and all table/array
    /// backing storage is laid out in one contiguous buffer. The result
    /// is fully independent of the source arena.
    fn from(item: &Item<'a>) -> Self {
        let mut aligned = 0usize;
        let mut strings = 0usize;
        compute_size(item, &mut aligned, &mut strings);
        let total = aligned + strings;

        if total == 0 {
            // SAFETY: When total is 0 the item is either a non-string scalar
            // (no borrowed data), an empty string (payload is static ""), or
            // an empty container (dangling pointer with len 0). Transmuting
            // the lifetime is safe because nothing actually borrows from an
            // arena. Item has no Drop impl.
            return Self {
                item: unsafe { std::mem::transmute_copy(item) },
                ptr: NonNull::dangling(),
                capacity: 0,
            };
        }

        let layout = std::alloc::Layout::from_size_align(total, 8).expect("layout overflow");
        // SAFETY: layout has non-zero size (total > 0).
        let raw = unsafe { std::alloc::alloc(layout) };
        let Some(base) = NonNull::new(raw) else {
            std::alloc::handle_alloc_error(layout);
        };

        // SAFETY: base.add(aligned) and base.add(total) are within or
        // one-past-the-end of the allocation.
        let mut target = unsafe {
            ItemCopyTarget {
                aligned: base.as_ptr(),
                #[cfg(debug_assertions)]
                aligned_end: base.as_ptr().add(aligned),
                string: base.as_ptr().add(aligned),
                #[cfg(debug_assertions)]
                string_end: base.as_ptr().add(total),
            }
        };

        // SAFETY: compute_size computed the exact space needed; emplace_in
        // consumes exactly that much from target.
        let new_item = unsafe { item.emplace_in(&mut target) };

        #[cfg(debug_assertions)]
        {
            assert_eq!(target.aligned as usize, base.as_ptr() as usize + aligned);
            assert_eq!(target.string as usize, base.as_ptr() as usize + total);
        }

        Self {
            item: new_item,
            ptr: base,
            capacity: total,
        }
    }
}

impl<'a> From<Item<'a>> for OwnedItem {
    /// Creates an `OwnedItem` by copying `item` into a single managed allocation.
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
    #[inline(always)]
    pub fn item<'a>(&'a self) -> &'a Item<'a> {
        &self.item
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
    fn to_toml<'a>(&'a self, arena: &'a crate::Arena) -> Result<Item<'a>, crate::ToTomlError> {
        Ok(self.item().clone_in(arena))
    }
}

#[cfg(feature = "to-toml")]
impl crate::ToFlattened for OwnedItem {
    fn to_flattened<'a>(
        &'a self,
        arena: &'a crate::Arena,
        table: &mut crate::Table<'a>,
    ) -> Result<(), crate::ToTomlError> {
        self.item().to_flattened(arena, table)
    }
}

#[cfg(feature = "from-toml")]
impl<'a> crate::FromToml<'a> for OwnedTable {
    fn from_toml(ctx: &mut crate::Context<'a>, item: &Item<'a>) -> Result<Self, crate::Failed> {
        let Ok(table) = item.require_table(ctx) else {
            return Err(crate::Failed);
        };
        Ok(OwnedTable::from(table))
    }
}

#[cfg(feature = "to-toml")]
impl crate::ToToml for OwnedTable {
    fn to_toml<'a>(&'a self, arena: &'a crate::Arena) -> Result<Item<'a>, crate::ToTomlError> {
        Ok(self.table().as_item().clone_in(arena))
    }
}

#[cfg(feature = "to-toml")]
impl crate::ToFlattened for OwnedTable {
    fn to_flattened<'a>(
        &'a self,
        arena: &'a crate::Arena,
        table: &mut crate::Table<'a>,
    ) -> Result<(), crate::ToTomlError> {
        self.table().to_flattened(arena, table)
    }
}
