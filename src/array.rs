#![allow(unsafe_code)]

#[cfg(test)]
#[path = "./array_tests.rs"]
mod tests;

use crate::MaybeItem;
use crate::arena::Arena;
use crate::value::Item;
use std::mem::size_of;
use std::ptr::NonNull;

const MIN_CAP: u32 = 4;

/// A growable array of TOML [`Item`]s.
///
/// Arrays support indexing with `usize` via the `[]` operator, which returns
/// a [`MaybeItem`] — out-of-bounds access returns a `None` variant instead of
/// panicking. Use [`get`](Self::get) / [`get_mut`](Self::get_mut) for
/// `Option`-based access, or iterate with a `for` loop.
///
/// Mutation methods ([`push`](Self::push), [`with_capacity`](Self::with_capacity))
/// require an [`Arena`] because array storage is arena-allocated.
pub struct Array<'de> {
    len: u32,
    cap: u32,
    ptr: NonNull<Item<'de>>,
}

impl<'de> Default for Array<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> Array<'de> {
    /// Creates an empty array.
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            cap: 0,
            ptr: NonNull::dangling(),
        }
    }

    /// Creates an array with pre-allocated capacity.
    pub fn with_capacity(cap: u32, arena: &'de Arena) -> Self {
        let mut arr = Self::new();
        if cap > 0 {
            arr.grow_to(cap, arena);
        }
        arr
    }

    /// Creates an array containing a single value.
    pub fn with_single(value: Item<'de>, arena: &'de Arena) -> Self {
        let mut arr = Self::with_capacity(MIN_CAP, arena);
        // SAFETY: with_capacity allocated space for at least MIN_CAP items,
        // so writing at index 0 is within bounds.
        unsafe {
            arr.ptr.as_ptr().write(value);
        }
        arr.len = 1;
        arr
    }

    /// Appends a value to the end of the array.
    #[inline]
    pub fn push(&mut self, value: Item<'de>, arena: &'de Arena) {
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

    /// Returns the number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns `true` if the array contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a reference to the element at the given index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Item<'de>> {
        if index < self.len as usize {
            // SAFETY: index < len is checked above, so the pointer is within
            // initialized elements.
            Some(unsafe { &*self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the element at the given index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Item<'de>> {
        if index < self.len as usize {
            // SAFETY: index < len is checked above.
            Some(unsafe { &mut *self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    /// Removes and returns the last element, or `None` if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Item<'de>> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            // SAFETY: len was > 0 and was just decremented, so ptr.add(len)
            // points to the last initialized element.
            Some(unsafe { self.ptr.as_ptr().add(self.len as usize).read() })
        }
    }

    /// Returns a mutable reference to the last element.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut Item<'de>> {
        if self.len == 0 {
            None
        } else {
            // SAFETY: len > 0 is checked above, so ptr.add(len - 1) is within bounds.
            Some(unsafe { &mut *self.ptr.as_ptr().add(self.len as usize - 1) })
        }
    }

    /// Returns an iterator over references to the elements.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, Item<'de>> {
        self.as_slice().iter()
    }

    /// Returns the contents as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Item<'de>] {
        if self.len == 0 {
            &[]
        } else {
            // SAFETY: len > 0 is checked above. ptr points to arena-allocated
            // memory with at least len initialized items.
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    /// Returns the contents as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Item<'de>] {
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
}

impl std::fmt::Debug for Array<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a, 'de> IntoIterator for &'a Array<'de> {
    type Item = &'a Item<'de>;
    type IntoIter = std::slice::Iter<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut Array<'de> {
    type Item = &'a mut Item<'de>;
    type IntoIter = std::slice::IterMut<'a, Item<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

/// Consuming iterator over an [`Array`], yielding [`Item`]s.
pub struct IntoIter<'de> {
    arr: Array<'de>,
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
            arr: self,
            index: 0,
        }
    }
}

impl<'de> std::ops::Index<usize> for Array<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        if let Some(item) = self.get(index) {
            return MaybeItem::from_ref(item);
        }
        &crate::value::NONE
    }
}
