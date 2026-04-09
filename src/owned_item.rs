#[cfg(test)]
#[path = "./owned_item_tests.rs"]
mod tests;

use crate::Item;
use std::ptr::NonNull;

/// Total size needed for an [`OwnedItem`] allocation, split into an
/// 8-byte-aligned region (table entries, array elements) and a packed
/// string region (key names, string values).
pub(crate) struct OwnedItemSize {
    pub(crate) aligned: usize,
    pub(crate) strings: usize,
}

impl OwnedItemSize {
    pub(crate) const ZERO: Self = Self {
        aligned: 0,
        strings: 0,
    };
}

impl std::ops::AddAssign for OwnedItemSize {
    fn add_assign(&mut self, rhs: Self) {
        self.aligned += rhs.aligned;
        self.strings += rhs.strings;
    }
}

/// Write cursor into an [`OwnedItem`] allocation, used by `emplace_in`
/// methods to copy item trees without an arena.
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
    /// Bumps the aligned pointer forward by `count * size_of::<T>()` bytes,
    /// returning a pointer to the start of the allocated region.
    ///
    /// # Safety
    ///
    /// `count * size_of::<T>()` bytes must remain in the aligned region.
    #[inline]
    pub(crate) unsafe fn alloc_aligned<T>(&mut self, count: usize) -> NonNull<T> {
        let size = count * std::mem::size_of::<T>();
        #[cfg(debug_assertions)]
        // SAFETY: Pointer arithmetic for bounds check only.
        unsafe {
            assert!(self.aligned.add(size) <= self.aligned_end)
        };
        let ptr = self.aligned.cast::<T>();
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
    #[inline]
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
        let size = item.owned_size();
        let total = size.aligned + size.strings;

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

        // SAFETY: base.add(size.aligned) and base.add(total) are within or
        // one-past-the-end of the allocation.
        let mut target = unsafe {
            ItemCopyTarget {
                aligned: base.as_ptr(),
                #[cfg(debug_assertions)]
                aligned_end: base.as_ptr().add(size.aligned),
                string: base.as_ptr().add(size.aligned),
                #[cfg(debug_assertions)]
                string_end: base.as_ptr().add(total),
            }
        };

        // SAFETY: owned_size computed the exact space needed; emplace_in
        // consumes exactly that much from target.
        let new_item = unsafe { item.emplace_in(&mut target) };

        #[cfg(debug_assertions)]
        {
            assert_eq!(
                target.aligned as usize,
                base.as_ptr() as usize + size.aligned
            );
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
    pub fn item<'a>(&'a self) -> &'a Item<'a> {
        // SAFETY: Shortens `Item<'static>` to `Item<'a>`. Item is covariant
        // in `'de`, so this is a subtyping cast the compiler cannot verify
        // because `'static` is a fiction for the owned data.
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
    fn to_toml<'a>(&'a self, arena: &'a crate::Arena) -> Result<Item<'a>, crate::ToTomlError> {
        Ok(self.item().clone_in(arena))
    }
}
