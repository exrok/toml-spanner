#![allow(unsafe_code)]

#[cfg(test)]
#[path = "./table_tests.rs"]
mod tests;

use crate::span::Spanned;
use crate::value::{
    FLAG_BIT, FLAG_SHIFT, Item, Key, TAG_MASK, TAG_SHIFT, TAG_TABLE, TAG_TABLE_HEADER,
};
use crate::{Deserialize, Error, ErrorKind, Span};
use std::alloc::Layout;
use std::ptr::NonNull;

use crate::arena::Arena;

type TableEntry<'de> = (Key<'de>, Item<'de>);

const MIN_CAP: u32 = 2;

/// A TOML table: a flat list of key-value pairs with linear lookup.
pub struct InnerTable<'de> {
    len: u32,
    cap: u32,
    ptr: NonNull<TableEntry<'de>>,
}

impl<'de> Default for InnerTable<'de> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> InnerTable<'de> {
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
    pub fn insert(&mut self, key: Key<'de>, value: Item<'de>, arena: &Arena) {
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
    pub fn get_key_value(&self, name: &str) -> Option<(&Key<'de>, &Item<'de>)> {
        for entry in self.entries() {
            if entry.0.name == name {
                return Some((&entry.0, &entry.1));
            }
        }
        None
    }

    /// Returns a reference to the value for `name`.
    pub fn get(&self, name: &str) -> Option<&Item<'de>> {
        for entry in self.entries() {
            if entry.0.name == name {
                return Some(&entry.1);
            }
        }
        None
    }

    /// Returns a mutable reference to the value for `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Item<'de>> {
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
    pub fn remove(&mut self, name: &str) -> Option<Item<'de>> {
        self.remove_entry(name).map(|(_, v)| v)
    }

    /// Removes the first entry matching `name`, returning the key-value pair.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        let idx = self.find_index(name)?;
        Some(self.remove_at(idx))
    }

    /// Returns an iterator over mutable references to the values.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Item<'de>> {
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
        unsafe { (*self.ptr.as_ptr()).0.span.start }
    }

    /// Returns key-value references at a given index (unchecked in release).
    #[inline]
    pub(crate) unsafe fn get_mut_unchecked(&mut self, index: usize) -> &mut (Key<'de>, Item<'de>) {
        debug_assert!(index < self.len as usize);
        unsafe { &mut *self.ptr.as_ptr().add(index) }
    }
    /// Returns key-value references at a given index (unchecked in release).
    #[inline]
    pub(crate) fn get_key_value_at(&self, index: usize) -> (&Key<'de>, &Item<'de>) {
        debug_assert!(index < self.len as usize);
        unsafe {
            let entry = &*self.ptr.as_ptr().add(index);
            (&entry.0, &entry.1)
        }
    }

    /// Returns a mutable value reference at a given index (unchecked in release).
    #[inline]
    pub(crate) fn get_mut_at(&mut self, index: usize) -> &mut Item<'de> {
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
    fn remove_at(&mut self, idx: usize) -> (Key<'de>, Item<'de>) {
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

impl std::fmt::Debug for InnerTable<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (k, v) in self.entries() {
            map.entry(k, v);
        }
        map.finish()
    }
}

impl<'a, 'de> IntoIterator for &'a InnerTable<'de> {
    type Item = (&'a Key<'de>, &'a Item<'de>);
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
    type Item = (&'a Key<'de>, &'a Item<'de>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_, '_> {}

impl<'de> IntoIterator for InnerTable<'de> {
    type Item = (Key<'de>, Item<'de>);
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
    table: InnerTable<'de>,
    index: u32,
}

impl<'de> Iterator for IntoIter<'de> {
    type Item = (Key<'de>, Item<'de>);

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

#[repr(C)]
pub struct Table<'de> {
    /// Bits 2-0: tag, bits 31-3: span.start
    start_and_tag: u32,
    /// Bit 0: flag bit (parser-internal), bits 31-1: span.end
    end_and_flag: u32,
    pub(crate) value: InnerTable<'de>,
}

impl<'de> Table<'de> {
    pub fn new(span: Span) -> Table<'de> {
        Table {
            start_and_tag: span.start << TAG_SHIFT | TAG_TABLE,
            end_and_flag: span.end << FLAG_SHIFT,
            value: InnerTable::new(),
        }
    }
    pub fn span(&self) -> Span {
        Span {
            start: self.start_and_tag >> TAG_SHIFT,
            end: self.end_and_flag >> FLAG_SHIFT,
        }
    }
}

impl<'de> Default for Table<'de> {
    fn default() -> Self {
        Self::new(Span::default())
    }
}

impl std::fmt::Debug for Table<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<'de> Table<'de> {
    /// Inserts a key-value pair. Does **not** check for duplicates.
    pub fn insert(&mut self, key: Key<'de>, value: Item<'de>, arena: &Arena) {
        self.value.insert(key, value, arena);
    }

    /// Returns the number of entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns `true` if the table has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Linear scan for a key, returning both key and value references.
    pub fn get_key_value(&self, name: &str) -> Option<(&Key<'de>, &Item<'de>)> {
        self.value.get_key_value(name)
    }

    /// Returns a reference to the value for `name`.
    pub fn get(&self, name: &str) -> Option<&Item<'de>> {
        self.value.get(name)
    }

    /// Returns a mutable reference to the value for `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Item<'de>> {
        self.value.get_mut(name)
    }

    /// Returns `true` if the table contains the key.
    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.value.contains_key(name)
    }

    /// Removes the first entry matching `name`, returning its value.
    pub fn remove(&mut self, name: &str) -> Option<Item<'de>> {
        self.value.remove(name)
    }

    /// Removes the first entry matching `name`, returning the key-value pair.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        self.value.remove_entry(name)
    }

    /// Returns an iterator over mutable references to the values.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Item<'de>> {
        self.value.values_mut()
    }

    /// Consumes the table and returns an iterator of keys.
    pub fn into_keys(self) -> IntoKeys<'de> {
        self.value.into_keys()
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        self.value.entries()
    }

    /// Converts this `Table` into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        let span = self.span();
        Item::table(self.value, span)
    }

    /// Deserializes a required field from the table.
    #[inline]
    pub fn required<T: Deserialize<'de>>(&mut self, name: &'static str) -> Result<T, Error> {
        Ok(self.required_s(name)?.value)
    }

    /// Deserializes a required field, returning it with span information.
    pub fn required_s<T: Deserialize<'de>>(
        &mut self,
        name: &'static str,
    ) -> Result<Spanned<T>, Error> {
        let Some(mut val) = self.value.remove(name) else {
            return Err(Error {
                kind: ErrorKind::MissingField(name),
                span: self.span(),
                line_info: None,
            });
        };

        Spanned::<T>::deserialize(&mut val)
    }

    /// Deserializes an optional field from the table.
    #[inline]
    pub fn optional<T: Deserialize<'de>>(&mut self, name: &str) -> Result<Option<T>, Error> {
        Ok(self.optional_s(name)?.map(|v| v.value))
    }

    /// Deserializes an optional field, returning it with span information.
    pub fn optional_s<T: Deserialize<'de>>(
        &mut self,
        name: &str,
    ) -> Result<Option<Spanned<T>>, Error> {
        let Some(mut val) = self.value.remove(name) else {
            return Ok(None);
        };

        Spanned::<T>::deserialize(&mut val).map(Some)
    }

    /// Checks that all keys have been consumed.
    ///
    /// Returns [`ErrorKind::UnexpectedKeys`] if the table still has entries.
    pub fn finalize(&self) -> Result<(), Error> {
        if !self.value.is_empty() {
            let keys = self
                .value
                .entries()
                .iter()
                .map(|(key, _)| (key.name.into(), key.span))
                .collect();

            return Err(Error::from((ErrorKind::UnexpectedKeys { keys }, self.span())));
        }

        Ok(())
    }
}

impl<'a, 'de> IntoIterator for &'a Table<'de> {
    type Item = (&'a Key<'de>, &'a Item<'de>);
    type IntoIter = Iter<'a, 'de>;

    fn into_iter(self) -> Self::IntoIter {
        (&self.value).into_iter()
    }
}

impl<'de> IntoIterator for Table<'de> {
    type Item = (Key<'de>, Item<'de>);
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

const _: () = assert!(std::mem::size_of::<Table<'_>>() == std::mem::size_of::<Item<'_>>());
const _: () = assert!(std::mem::align_of::<Table<'_>>() == std::mem::align_of::<Item<'_>>());

impl<'de> Table<'de> {
    #[inline]
    pub(crate) fn span_start(&self) -> u32 {
        self.start_and_tag >> TAG_SHIFT
    }

    #[inline]
    pub(crate) fn set_span_start(&mut self, v: u32) {
        self.start_and_tag = (v << TAG_SHIFT) | (self.start_and_tag & TAG_MASK);
    }

    #[inline]
    pub(crate) fn set_span_end(&mut self, v: u32) {
        self.end_and_flag = (v << FLAG_SHIFT) | (self.end_and_flag & FLAG_BIT);
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        let old = self.end_and_flag;
        let current = old >> FLAG_SHIFT;
        self.end_and_flag = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_BIT);
    }

    #[inline]
    pub(crate) fn set_header_tag(&mut self) {
        self.start_and_tag = (self.start_and_tag & !TAG_MASK) | TAG_TABLE_HEADER;
    }
}
