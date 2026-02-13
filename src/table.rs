#![allow(unsafe_code)]

#[cfg(test)]
#[path = "./table_tests.rs"]
mod tests;

use crate::arena::Arena;
use crate::value::{Key, Value};
use std::alloc::Layout;
use std::ptr::NonNull;

type TableEntry<'de> = (Key<'de>, Value<'de>);

const MIN_CAP: u32 = 2;

/// A TOML table: a flat list of key-value pairs with linear lookup.
///
/// Entries are stored in insertion order. Duplicate keys are not allowed (the
/// parser enforces this through the [`Entry`] API).
pub struct Table<'de> {
    len: u32,
    cap: u32,
    ptr: NonNull<TableEntry<'de>>,
}

impl<'de> Default for Table<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> Table<'de> {
    /// Creates an empty table.
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            cap: 0,
            ptr: NonNull::dangling(),
        }
    }

    /// Inserts a key-value pair. Does **not** check for duplicates.
    pub fn insert(&mut self, key: Key<'de>, value: Value<'de>, arena: &Arena) {
        let len = self.len;
        if self.len == self.cap {
            self.grow(arena);
        }
        unsafe {
            self.ptr.as_ptr().add(len as usize).write((key, value));
        }
        self.len = len + 1;
    }

    /// Returns the number of entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns `true` if the table has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Linear scan for a key, returning both key and value references.
    pub fn get_key_value(&self, name: &str) -> Option<(&Key<'de>, &Value<'de>)> {
        for entry in self.entries() {
            if entry.0.name == name {
                return Some((&entry.0, &entry.1));
            }
        }
        None
    }

    /// Returns a reference to the value for `name`.
    pub fn get(&self, name: &str) -> Option<&Value<'de>> {
        for entry in self.entries() {
            if entry.0.name == name {
                return Some(&entry.1);
            }
        }
        None
    }

    /// Returns a mutable reference to the value for `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value<'de>> {
        for entry in self.entries_mut() {
            if entry.0.name == name {
                return Some(&mut entry.1);
            }
        }
        None
    }

    /// Returns `true` if the table contains the key.
    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Removes the first entry matching `name`, returning its value.
    /// Shifts subsequent entries to fill the gap (preserves order).
    pub fn remove(&mut self, name: &str) -> Option<Value<'de>> {
        self.remove_entry(name).map(|(_, v)| v)
    }

    /// Removes the first entry matching `name`, returning the key-value pair.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Value<'de>)> {
        let idx = self.find_index(name)?;
        Some(self.remove_at(idx))
    }

    /// Returns an iterator over mutable references to the values.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value<'de>> {
        self.entries_mut().iter_mut().map(|(_, v)| v)
    }

    /// Consumes the table and returns an iterator of keys.
    pub fn into_keys(self) -> IntoKeys<'de> {
        IntoKeys {
            inner: self.into_iter(),
        }
    }

    /// Returns the span start of the first key. Used as a table discriminator
    /// in the parser's hash index.
    ///
    /// # Panics
    ///
    /// Debug-asserts that the table is non-empty.
    #[inline]
    pub(crate) fn first_key_span_start(&self) -> u32 {
        debug_assert!(self.len > 0);
        unsafe { (*self.ptr.as_ptr()).0.span.start() }
    }

    /// Returns key-value references at a given index (unchecked in release).
    #[inline]
    pub(crate) unsafe fn get_mut_unchecked(&mut self, index: usize) -> &mut (Key<'de>, Value<'de>) {
        debug_assert!(index < self.len as usize);
        unsafe { &mut *self.ptr.as_ptr().add(index) }
    }
    /// Returns key-value references at a given index (unchecked in release).
    #[inline]
    pub(crate) fn get_key_value_at(&self, index: usize) -> (&Key<'de>, &Value<'de>) {
        debug_assert!(index < self.len as usize);
        unsafe {
            let entry = &*self.ptr.as_ptr().add(index);
            (&entry.0, &entry.1)
        }
    }

    /// Returns a mutable value reference at a given index (unchecked in release).
    #[inline]
    pub(crate) fn get_mut_at(&mut self, index: usize) -> &mut Value<'de> {
        debug_assert!(index < self.len as usize);
        unsafe { &mut (*self.ptr.as_ptr().add(index)).1 }
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
    }

    #[inline]
    pub(crate) fn entries_mut(&mut self) -> &mut [TableEntry<'de>] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len as usize) }
    }

    pub(crate) fn find_index(&self, name: &str) -> Option<usize> {
        for (i, entry) in self.entries().iter().enumerate() {
            if entry.0.name == name {
                return Some(i);
            }
        }
        None
    }

    /// Remove entry at `idx`, shifting subsequent entries left.
    fn remove_at(&mut self, idx: usize) -> (Key<'de>, Value<'de>) {
        let ptr = unsafe { self.ptr.as_ptr().add(idx) };
        let entry = unsafe { ptr.read() };
        let remaining = self.len as usize - idx - 1;
        if remaining > 0 {
            unsafe {
                std::ptr::copy(ptr.add(1), ptr, remaining);
            }
        }
        self.len -= 1;
        entry
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
        let new_layout =
            Layout::array::<TableEntry<'_>>(new_cap as usize).expect("layout overflow");
        let new_ptr = arena.alloc(new_layout).cast::<TableEntry<'de>>();
        if self.cap > 0 {
            // Safety: old buffer has self.len initialized entries; new buffer
            // has room for new_cap >= self.len entries.
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

impl std::fmt::Debug for Table<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (k, v) in self.entries() {
            map.entry(k, v);
        }
        map.finish()
    }
}

impl<'a, 'de> IntoIterator for &'a Table<'de> {
    type Item = (&'a Key<'de>, &'a Value<'de>);
    type IntoIter = Iter<'a, 'de>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            inner: self.entries().iter(),
        }
    }
}

/// Borrowing iterator over a [`Table`], yielding `(&Key, &Value)` pairs.
pub struct Iter<'a, 'de> {
    inner: std::slice::Iter<'a, TableEntry<'de>>,
}

impl<'a, 'de> Iterator for Iter<'a, 'de> {
    type Item = (&'a Key<'de>, &'a Value<'de>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_, '_> {}

impl<'de> IntoIterator for Table<'de> {
    type Item = (Key<'de>, Value<'de>);
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            table: self,
            index: 0,
        }
    }
}

/// Consuming iterator over a [`Table`].
pub struct IntoIter<'de> {
    table: Table<'de>,
    index: u32,
}

impl<'de> Iterator for IntoIter<'de> {
    type Item = (Key<'de>, Value<'de>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.table.len {
            let entry = unsafe { self.table.ptr.as_ptr().add(self.index as usize).read() };
            self.index += 1;
            Some(entry)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.table.len - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for IntoIter<'_> {}

/// Consuming iterator that yields only the keys of a [`Table`].
pub struct IntoKeys<'de> {
    inner: IntoIter<'de>,
}

impl<'de> Iterator for IntoKeys<'de> {
    type Item = Key<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
