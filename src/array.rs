#![allow(unsafe_code)]

use crate::arena::Arena;
use crate::value::Value;
use std::alloc::Layout;
use std::ptr::NonNull;

const MIN_CAP: u32 = 4;

/// A growable array of TOML [`Value`]s backed by a flat allocation with 32-bit
/// length and capacity.
pub struct Array<'de> {
    len: u32,
    cap: u32,
    ptr: NonNull<Value<'de>>,
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
    pub fn with_capacity(cap: u32, arena: &Arena) -> Self {
        let mut arr = Self::new();
        if cap > 0 {
            arr.grow_to(cap, arena);
        }
        arr
    }

    /// Creates an array containing a single value.
    pub fn with_single(value: Value<'de>, arena: &Arena) -> Self {
        let mut arr = Self::with_capacity(MIN_CAP, arena);
        unsafe {
            arr.ptr.as_ptr().write(value);
        }
        arr.len = 1;
        arr
    }

    /// Appends a value to the end of the array.
    #[inline]
    pub fn push(&mut self, value: Value<'de>, arena: &Arena) {
        let len = self.len;
        if len == self.cap {
            self.grow(arena);
        }
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
    pub fn get(&self, index: usize) -> Option<&Value<'de>> {
        if index < self.len as usize {
            Some(unsafe { &*self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the element at the given index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value<'de>> {
        if index < self.len as usize {
            Some(unsafe { &mut *self.ptr.as_ptr().add(index) })
        } else {
            None
        }
    }

    /// Removes and returns the last element, or `None` if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<Value<'de>> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            Some(unsafe { self.ptr.as_ptr().add(self.len as usize).read() })
        }
    }

    /// Returns a mutable reference to the last element.
    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut Value<'de>> {
        if self.len == 0 {
            None
        } else {
            Some(unsafe { &mut *self.ptr.as_ptr().add(self.len as usize - 1) })
        }
    }

    /// Returns an iterator over references to the elements.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, Value<'de>> {
        self.as_slice().iter()
    }

    /// Returns the contents as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[Value<'de>] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    /// Returns the contents as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Value<'de>] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    #[cold]
    fn grow(&mut self, arena: &Arena) {
        let new_cap = if self.cap == 0 {
            MIN_CAP
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        self.grow_to(new_cap, arena);
    }

    fn grow_to(&mut self, new_cap: u32, arena: &Arena) {
        let new_layout = Layout::array::<Value<'_>>(new_cap as usize).expect("layout overflow");
        let new_ptr = arena.alloc(new_layout).cast::<Value<'de>>();
        if self.cap > 0 {
            // Safety: old buffer has self.len initialized Values; new buffer
            // has room for new_cap >= self.len elements.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.ptr.as_ptr(),
                    new_ptr.as_ptr(),
                    self.len as usize,
                );
            }
            // Old buffer is abandoned; the arena reclaims it on drop.
        }
        self.ptr = new_ptr;
        self.cap = new_cap;
    }
}

impl std::fmt::Debug for Array<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a, 'de> IntoIterator for &'a Array<'de> {
    type Item = &'a Value<'de>;
    type IntoIter = std::slice::Iter<'a, Value<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, 'de> IntoIterator for &'a mut Array<'de> {
    type Item = &'a mut Value<'de>;
    type IntoIter = std::slice::IterMut<'a, Value<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

/// Consuming iterator over an [`Array`].
pub struct IntoIter<'de> {
    arr: Array<'de>,
    index: u32,
}

impl<'de> Iterator for IntoIter<'de> {
    type Item = Value<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.arr.len {
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
    type Item = Value<'de>;
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            arr: self,
            index: 0,
        }
    }
}

#[cfg(test)]
#[path = "./array_tests.rs"]
mod tests;
