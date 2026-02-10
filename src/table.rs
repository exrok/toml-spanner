#![allow(unsafe_code)]

use crate::value::{Key, Value};
use std::alloc::{Layout, alloc, dealloc, realloc};
use std::ptr::NonNull;

type TableEntry<'de> = (Key<'de>, Value<'de>);

const MIN_CAP: u32 = 8;

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

    /// Inserts a key-value pair. Does **not** check for duplicates; use
    /// [`entry`](Self::entry) when duplicate detection is needed.
    pub fn insert(&mut self, key: Key<'de>, value: Value<'de>) {
        if self.len == self.cap {
            self.grow();
        }
        unsafe {
            self.ptr.as_ptr().add(self.len as usize).write((key, value));
        }
        self.len += 1;
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

    /// Provides in-place access to an entry via its key.
    pub fn entry(&mut self, key: Key<'de>) -> Entry<'_, 'de> {
        let name = &key.name;
        for i in 0..self.len {
            let entry = unsafe { &*self.ptr.as_ptr().add(i as usize) };
            if entry.0.name == *name {
                return Entry::Occupied(OccupiedEntry {
                    table: self,
                    index: i,
                });
            }
        }
        Entry::Vacant(VacantEntry { table: self, key })
    }

    /// Consumes the table and returns an iterator of keys.
    pub fn into_keys(self) -> IntoKeys<'de> {
        IntoKeys {
            inner: self.into_iter(),
        }
    }

    // -- internal helpers ---------------------------------------------------

    #[inline]
    fn entries(&self) -> &[TableEntry<'de>] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    #[inline]
    fn entries_mut(&mut self) -> &mut [TableEntry<'de>] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len as usize) }
        }
    }

    fn find_index(&self, name: &str) -> Option<u32> {
        for (i, entry) in self.entries().iter().enumerate() {
            if entry.0.name == name {
                return Some(i as u32);
            }
        }
        None
    }

    /// Remove entry at `idx`, shifting subsequent entries left.
    fn remove_at(&mut self, idx: u32) -> (Key<'de>, Value<'de>) {
        let ptr = unsafe { self.ptr.as_ptr().add(idx as usize) };
        let entry = unsafe { ptr.read() };
        let remaining = self.len - idx - 1;
        if remaining > 0 {
            unsafe {
                std::ptr::copy(ptr.add(1), ptr, remaining as usize);
            }
        }
        self.len -= 1;
        entry
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
        let new_layout =
            Layout::array::<TableEntry<'_>>(new_cap as usize).expect("layout overflow");
        let new_ptr = if self.cap == 0 {
            unsafe { alloc(new_layout) }
        } else {
            let old_layout =
                Layout::array::<TableEntry<'_>>(self.cap as usize).expect("layout overflow");
            unsafe { realloc(self.ptr.as_ptr().cast(), old_layout, new_layout.size()) }
        };
        self.ptr = match NonNull::new(new_ptr.cast()) {
            Some(p) => p,
            None => std::alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }
}

impl Drop for Table<'_> {
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
                Layout::array::<TableEntry<'_>>(self.cap as usize).unwrap(),
            );
        }
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

// &Table -> yields (&Key, &Value)
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

// Consuming iterator: Table -> yields (Key, Value)
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

impl Drop for IntoIter<'_> {
    fn drop(&mut self) {
        while self.index < self.table.len {
            unsafe {
                std::ptr::drop_in_place(self.table.ptr.as_ptr().add(self.index as usize));
            }
            self.index += 1;
        }
        self.table.len = 0;
    }
}

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

// ---------------------------------------------------------------------------
// Entry API
// ---------------------------------------------------------------------------

/// A view into a single entry in a [`Table`], which may be vacant or occupied.
pub enum Entry<'a, 'de> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, 'de>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, 'de>),
}

/// A view into an occupied entry in a [`Table`].
pub struct OccupiedEntry<'a, 'de> {
    table: &'a Table<'de>,
    index: u32,
}

impl<'a, 'de> OccupiedEntry<'a, 'de> {
    /// Returns a reference to the entry's key.
    pub fn key(&self) -> &Key<'de> {
        unsafe { &(*self.table.ptr.as_ptr().add(self.index as usize)).0 }
    }
}

/// A view into a vacant entry in a [`Table`].
pub struct VacantEntry<'a, 'de> {
    table: &'a mut Table<'de>,
    key: Key<'de>,
}

impl<'a, 'de> VacantEntry<'a, 'de> {
    /// Inserts a value into the vacant entry.
    pub fn insert(self, value: Value<'de>) {
        self.table.insert(self.key, value);
    }
}
