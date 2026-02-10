#![allow(unsafe_code)]

use crate::value::Value;
use std::alloc::{Layout, alloc, dealloc, realloc};
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
    pub fn with_capacity(cap: u32) -> Self {
        let mut arr = Self::new();
        if cap > 0 {
            arr.grow_to(cap);
        }
        arr
    }

    /// Creates an array containing a single value.
    pub fn with_single(value: Value<'de>) -> Self {
        let mut arr = Self::with_capacity(MIN_CAP);
        unsafe {
            arr.ptr.as_ptr().write(value);
        }
        arr.len = 1;
        arr
    }

    /// Appends a value to the end of the array.
    #[inline]
    pub fn push(&mut self, value: Value<'de>) {
        if self.len == self.cap {
            self.grow();
        }
        unsafe {
            self.ptr.as_ptr().add(self.len as usize).write(value);
        }
        self.len += 1;
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
    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            MIN_CAP
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        self.grow_to(new_cap);
    }

    fn grow_to(&mut self, new_cap: u32) {
        let new_layout = Layout::array::<Value<'_>>(new_cap as usize).expect("layout overflow");
        let new_ptr = if self.cap == 0 {
            unsafe { alloc(new_layout) }
        } else {
            let old_layout =
                Layout::array::<Value<'_>>(self.cap as usize).expect("layout overflow");
            unsafe { realloc(self.ptr.as_ptr().cast(), old_layout, new_layout.size()) }
        };
        self.ptr = match NonNull::new(new_ptr.cast()) {
            Some(p) => p,
            None => std::alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }
}

impl Drop for Array<'_> {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.ptr.as_ptr(),
                self.len as usize,
            ));
            dealloc(
                self.ptr.as_ptr().cast(),
                Layout::array::<Value<'_>>(self.cap as usize).unwrap(),
            );
        }
    }
}

impl std::fmt::Debug for Array<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

// &Array -> yields &Value
impl<'a, 'de> IntoIterator for &'a Array<'de> {
    type Item = &'a Value<'de>;
    type IntoIter = std::slice::Iter<'a, Value<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

// &mut Array -> yields &mut Value
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

impl<'de> Drop for IntoIter<'de> {
    fn drop(&mut self) {
        // Drop remaining elements that weren't consumed
        while self.index < self.arr.len {
            unsafe {
                std::ptr::drop_in_place(self.arr.ptr.as_ptr().add(self.index as usize));
            }
            self.index += 1;
        }
        // Prevent Array::drop from double-dropping the elements
        self.arr.len = 0;
    }
}

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
