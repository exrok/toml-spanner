#[cfg(test)]
#[path = "./array_tests.rs"]
mod tests;

use crate::MaybeItem;
use crate::Span;
use crate::arena::Arena;
use crate::item::{ArrayStyle, FLAG_AOT, FLAG_ARRAY, Item, ItemMetadata, TAG_ARRAY};
use std::mem::size_of;
use std::ptr::NonNull;

const MIN_CAP: u32 = 4;

#[repr(C, align(8))]
pub(crate) struct InternalArray<'de> {
    pub(super) len: u32,
    pub(super) cap: u32,
    pub(super) ptr: NonNull<Item<'de>>,
}

impl<'de> Default for InternalArray<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> InternalArray<'de> {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            len: 0,
            cap: 0,
            ptr: NonNull::dangling(),
        }
    }

    pub(crate) fn with_capacity(cap: u32, arena: &'de Arena) -> Self {
        let mut arr = Self::new();
        if cap > 0 {
            arr.grow_to(cap, arena);
        }
        arr
    }

    pub(crate) fn with_single(value: Item<'de>, arena: &'de Arena) -> Self {
        let mut arr = Self::with_capacity(MIN_CAP, arena);
        // SAFETY: with_capacity allocated space for at least MIN_CAP items,
        // so writing at index 0 is within bounds.
        unsafe {
            arr.ptr.as_ptr().write(value);
        }
        arr.len = 1;
        arr
    }

    #[inline]
    pub(crate) fn push(&mut self, value: Item<'de>, arena: &'de Arena) {
        let len = self.len;
        if len == self.cap {
            self.grow(arena);
        }
        // SAFETY: grow() ensures len < cap, so ptr.add(len) is within the allocation.
        unsafe {
            self.ptr.as_ptr().add(len as usize).write(value);
        }
        self.len = len + 1;
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> Option<&Item<'de>> {
        if index < self.len as usize {
            // SAFETY: index < len is checked above, so the pointer is within
            // initialized elements.
            Some(unsafe { &*self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut Item<'de>> {
        if index < self.len as usize {
            // SAFETY: index < len is checked above.
            Some(unsafe { &mut *self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<Item<'de>> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            // SAFETY: len was > 0 and was just decremented, so ptr.add(len)
            // points to the last initialized element.
            Some(unsafe { self.ptr.as_ptr().add(self.len as usize).read() })
        }
    }

    #[inline]
    pub(crate) fn last_mut(&mut self) -> Option<&mut Item<'de>> {
        if self.len == 0 {
            None
        } else {
            // SAFETY: len > 0 is checked above, so ptr.add(len - 1) is within bounds.
            Some(unsafe { &mut *self.ptr.as_ptr().add(self.len as usize - 1) })
        }
    }

    #[inline]
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Item<'de>> {
        self.as_slice().iter()
    }

    #[inline]
    pub(crate) fn as_slice(&self) -> &[Item<'de>] {
        if self.len == 0 {
            &[]
        } else {
            // SAFETY: len > 0 is checked above. ptr points to arena-allocated
            // memory with at least len initialized items.
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    #[inline]
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Item<'de>] {
        if self.len == 0 {
            &mut []
        } else {
            // SAFETY: same as as_slice() — ptr is valid for len initialized items.
            unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    #[cold]
    fn grow(&mut self, arena: &'de Arena) {
        let new_cap = if self.cap == 0 {
            MIN_CAP
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        self.grow_to(new_cap, arena);
    }

    fn grow_to(&mut self, new_cap: u32, arena: &'de Arena) {
        // On 64-bit, u32 * size_of::<Item>() cannot overflow usize.
        #[cfg(target_pointer_width = "32")]
        let new_size = (new_cap as usize)
            .checked_mul(size_of::<Item<'_>>())
            .expect("capacity overflow");
        #[cfg(not(target_pointer_width = "32"))]
        let new_size = new_cap as usize * size_of::<Item<'_>>();
        if self.cap > 0 {
            let old_size = self.cap as usize * size_of::<Item<'_>>();
            // Safety: ptr was returned by a prior arena alloc of old_size bytes.
            self.ptr = unsafe { arena.realloc(self.ptr.cast(), old_size, new_size).cast() };
        } else {
            self.ptr = arena.alloc(new_size).cast();
        }
        self.cap = new_cap;
    }

    /// Deep-clones this array into `arena`. Keys and strings are shared
    /// with the source.
    pub(crate) fn clone_in(&self, arena: &'de Arena) -> Self {
        let len = self.len as usize;
        if len == 0 {
            return Self::new();
        }
        let size = len * size_of::<Item<'de>>();
        let dst: NonNull<Item<'de>> = arena.alloc(size).cast();
        let src = self.ptr.as_ptr();
        let dst_ptr = dst.as_ptr();

        let mut run_start = 0;
        for i in 0..len {
            // SAFETY: i < len, so src.add(i) is within initialized elements.
            if unsafe { !(*src.add(i)).is_scalar() } {
                if run_start < i {
                    // SAFETY: src[run_start..i] are scalars — bitwise copy is
                    // correct. Source and destination are disjoint arena regions.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            src.add(run_start),
                            dst_ptr.add(run_start),
                            i - run_start,
                        );
                    }
                }
                // SAFETY: the item is an aggregate; deep-clone it.
                unsafe {
                    dst_ptr.add(i).write((*src.add(i)).clone_in(arena));
                }
                run_start = i + 1;
            }
        }
        if run_start < len {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    src.add(run_start),
                    dst_ptr.add(run_start),
                    len - run_start,
                );
            }
        }

        Self {
            len: self.len,
            cap: self.len,
            ptr: dst,
        }
    }
}

impl std::fmt::Debug for InternalArray<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a, 'de> IntoIterator for &'a InternalArray<'de> {
    type Item = &'a Item<'de>;
    type IntoIter = std::slice::Iter<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut InternalArray<'de> {
    type Item = &'a mut Item<'de>;
    type IntoIter = std::slice::IterMut<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

/// Consuming iterator over an [`InternalArray`], yielding [`Item`]s.
pub(crate) struct InternalArrayIntoIter<'de> {
    arr: InternalArray<'de>,
    index: u32,
}

impl<'de> Iterator for InternalArrayIntoIter<'de> {
    type Item = Item<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.arr.len {
            // SAFETY: index < len is checked above, so the read is within
            // initialized elements.
            let val = unsafe { self.arr.ptr.as_ptr().add(self.index as usize).read() };
            self.index += 1;
            Some(val)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.arr.len - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl<'de> ExactSizeIterator for InternalArrayIntoIter<'de> {}

impl<'de> IntoIterator for InternalArray<'de> {
    type Item = Item<'de>;
    type IntoIter = InternalArrayIntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        InternalArrayIntoIter {
            arr: self,
            index: 0,
        }
    }
}

impl<'de> std::ops::Index<usize> for InternalArray<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(item) = self.get(index) {
            return MaybeItem::from_ref(item);
        }
        &crate::item::NONE
    }
}

// Public Array wrapper (same layout as Item when tag == TAG_ARRAY)

/// A TOML array with span information.
///
/// An `Array` stores [`Item`] elements in insertion order with arena-allocated
/// backing storage. It carries the byte-offset [`Span`] from the source
/// document.
///
/// # Accessing elements
///
/// - **Index operator** — `array[i]` returns a [`MaybeItem`] that never
///   panics on out-of-bounds access.
/// - **`get` / `get_mut`** — return `Option<&Item>` / `Option<&mut Item>`.
/// - **Iteration** — `for item in &array { ... }`.
///
/// # Mutation
///
/// [`push`](Self::push) appends an element. An [`Arena`] is required because
/// array storage is arena-allocated.
#[repr(C)]
pub struct Array<'de> {
    pub(crate) value: InternalArray<'de>,
    pub(crate) meta: ItemMetadata,
}

const _: () = assert!(std::mem::size_of::<Array<'_>>() == std::mem::size_of::<Item<'_>>());
const _: () = assert!(std::mem::align_of::<Array<'_>>() == std::mem::align_of::<Item<'_>>());

impl<'de> Array<'de> {
    /// Creates an empty array in format-hints mode (no source span).
    pub fn new() -> Self {
        Self {
            meta: ItemMetadata::hints(TAG_ARRAY, FLAG_ARRAY),
            value: InternalArray::new(),
        }
    }

    /// Creates an empty array with pre-allocated capacity.
    ///
    /// Returns `None` if `cap` exceeds `u32::MAX`.
    pub fn try_with_capacity(cap: usize, arena: &'de Arena) -> Option<Self> {
        let cap: u32 = cap.try_into().ok()?;
        Some(Self {
            meta: ItemMetadata::hints(TAG_ARRAY, FLAG_ARRAY),
            value: InternalArray::with_capacity(cap, arena),
        })
    }

    /// Creates an empty array in span mode (parser-produced).
    #[cfg(test)]
    pub(crate) fn new_spanned(span: Span) -> Self {
        Self {
            meta: ItemMetadata::spanned(TAG_ARRAY, FLAG_ARRAY, span.start, span.end),
            value: InternalArray::new(),
        }
    }

    /// Returns the byte-offset span of this array in the source document.
    /// Only valid on parser-produced arrays (span mode).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn span_unchecked(&self) -> Span {
        self.meta.span_unchecked()
    }

    /// Returns the source span, or `0..0` if this array was constructed
    /// programmatically (format-hints mode).
    pub fn span(&self) -> Span {
        self.meta.span()
    }

    /// Appends a value to the end of the array.
    #[inline]
    pub fn push(&mut self, value: Item<'de>, arena: &'de Arena) {
        self.value.push(value, arena);
    }

    /// Returns the number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns `true` if the array contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Returns a reference to the element at the given index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Item<'de>> {
        self.value.get(index)
    }

    /// Returns a mutable reference to the element at the given index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Item<'de>> {
        self.value.get_mut(index)
    }

    /// Removes and returns the last element, or `None` if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Item<'de>> {
        self.value.pop()
    }

    /// Returns a mutable reference to the last element.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut Item<'de>> {
        self.value.last_mut()
    }

    /// Returns an iterator over references to the elements.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, Item<'de>> {
        self.value.iter()
    }

    /// Returns the contents as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Item<'de>] {
        self.value.as_slice()
    }

    /// Returns the contents as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Item<'de>] {
        self.value.as_mut_slice()
    }

    /// Converts this `Array` into an [`Item`] with the same span and payload.
    pub fn as_item(&self) -> &Item<'de> {
        // SAFETY: Array is #[repr(C)] { InternalArray, ItemMetadata }.
        // Item  is #[repr(C)] { Payload,       ItemMetadata }.
        // Payload is a union whose `array` field is ManuallyDrop<InternalArray>
        // (#[repr(transparent)]). Both types are 24 bytes, align 8 (verified
        // by const assertions). The field offsets match (data at 0..16,
        // metadata at 16..24). Only a shared reference is returned.
        unsafe { &*(self as *const Array<'de>).cast::<Item<'de>>() }
    }

    /// Converts this `Array` into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        // SAFETY: Same layout argument as as_item(). Size and alignment
        // equality verified by const assertions. The tag in ItemMetadata is
        // preserved unchanged through the transmute.
        unsafe { std::mem::transmute(self) }
    }

    /// Returns the kind of this array (inline or header/array-of-tables).
    #[inline]
    pub fn style(&self) -> ArrayStyle {
        match self.meta.flag() {
            FLAG_AOT => ArrayStyle::Header,
            _ => ArrayStyle::Inline,
        }
    }

    /// Sets the kind of this array.
    #[inline]
    pub fn set_style(&mut self, kind: ArrayStyle) {
        let flag = match kind {
            ArrayStyle::Inline => FLAG_ARRAY,
            ArrayStyle::Header => FLAG_AOT,
        };
        self.meta.set_flag(flag);
    }

    /// Deep-clones this array into `arena`. Keys and strings are shared
    /// with the source.
    pub fn clone_in(&self, arena: &'de Arena) -> Array<'de> {
        Array {
            value: self.value.clone_in(arena),
            meta: self.meta,
        }
    }
}

impl<'de> Default for Array<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Array<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<'de> std::ops::Index<usize> for Array<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(item) = self.value.get(index) {
            return MaybeItem::from_ref(item);
        }
        &crate::item::NONE
    }
}

impl<'a, 'de> IntoIterator for &'a Array<'de> {
    type Item = &'a Item<'de>;
    type IntoIter = std::slice::Iter<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut Array<'de> {
    type Item = &'a mut Item<'de>;
    type IntoIter = std::slice::IterMut<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.as_mut_slice().iter_mut()
    }
}

/// Consuming iterator over an [`Array`], yielding [`Item`]s.
pub struct IntoIter<'de> {
    arr: InternalArray<'de>,
    index: u32,
}

impl<'de> Iterator for IntoIter<'de> {
    type Item = Item<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.arr.len {
            // SAFETY: index < len is checked above, so the read is within
            // initialized elements.
            let val = unsafe { self.arr.ptr.as_ptr().add(self.index as usize).read() };
            self.index += 1;
            Some(val)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.arr.len - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl<'de> ExactSizeIterator for IntoIter<'de> {}

impl<'de> IntoIterator for Array<'de> {
    type Item = Item<'de>;
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            arr: self.value,
            index: 0,
        }
    }
}
