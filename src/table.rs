#[cfg(test)]
#[path = "./table_tests.rs"]
mod tests;

use crate::Span;
use crate::value::{
    FLAG_HEADER, FLAG_MASK, FLAG_SHIFT, FLAG_TABLE, Item, Key, MaybeItem, NONE, TAG_MASK,
    TAG_SHIFT, TAG_TABLE,
};
use std::mem::size_of;
use std::ptr::NonNull;

use crate::arena::Arena;

type TableEntry<'de> = (Key<'de>, Item<'de>);

const MIN_CAP: u32 = 2;

/// A TOML table: a flat list of key-value pairs with linear lookup.
#[repr(C, align(8))]
pub(crate) struct InnerTable<'de> {
    len: u32,
    cap: u32,
    ptr: NonNull<TableEntry<'de>>,
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
    pub fn insert(
        &mut self,
        key: Key<'de>,
        item: Item<'de>,
        arena: &'de Arena,
    ) -> &mut TableEntry<'de> {
        let len = self.len;
        if self.len == self.cap {
            self.grow(arena);
        }
        // SAFETY: grow() ensures len < cap, so ptr.add(len) is within the
        // allocation. The write targets uninitialized memory past the current length.
        unsafe {
            let ptr = self.ptr.as_ptr().add(len as usize);
            ptr.write((key, item));
            self.len = len + 1;
            &mut (*ptr)
        }
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
    pub fn get_entry(&self, name: &str) -> Option<(&Key<'de>, &Item<'de>)> {
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

    /// Removes the first entry matching `name`, returning the key-value pair.
    /// Uses swap-remove, so the ordering of remaining entries may change.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        let idx = self.find_index(name)?;
        Some(self.remove_at(idx))
    }

    /// Returns the span start of the first key. Used as a table discriminator
    /// in the parser's hash index.
    ///
    /// # Safety
    ///
    /// The table must be non-empty (`self.len > 0`).
    #[inline]
    pub(crate) unsafe fn first_key_span_start_unchecked(&self) -> u32 {
        debug_assert!(self.len > 0);
        // SAFETY: caller guarantees len > 0, so the first entry is initialized.
        unsafe { (*self.ptr.as_ptr()).0.span.start }
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        // SAFETY: ptr points to arena-allocated memory with at least len
        // initialized entries. When len == 0, ptr is NonNull::dangling() which
        // satisfies from_raw_parts' alignment requirement for zero-length slices.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
    }

    #[inline]
    pub(crate) fn entries_mut(&mut self) -> &mut [TableEntry<'de>] {
        // SAFETY: same as entries() — ptr is valid for len initialized entries.
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

    /// Remove entry at `idx` by swapping it with the last entry.
    fn remove_at(&mut self, idx: usize) -> (Key<'de>, Item<'de>) {
        let last = self.len as usize - 1;
        // SAFETY: idx was returned by find_index, so idx < len and the
        // pointer is within initialized entries. read() moves the value out.
        let ptr = unsafe { self.ptr.as_ptr().add(idx) };
        let entry = unsafe { ptr.read() };
        if idx != last {
            // Safety: `last` is a valid, initialized index distinct from `idx`.
            unsafe {
                ptr.write(self.ptr.as_ptr().add(last).read());
            }
        }
        self.len -= 1;
        entry
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
        // On 64-bit, u32 * size_of::<TableEntry>() cannot overflow usize.
        #[cfg(target_pointer_width = "32")]
        let new_size = (new_cap as usize)
            .checked_mul(size_of::<TableEntry<'_>>())
            .expect("capacity overflow");
        #[cfg(not(target_pointer_width = "32"))]
        let new_size = new_cap as usize * size_of::<TableEntry<'_>>();
        if self.cap > 0 {
            let old_size = self.cap as usize * size_of::<TableEntry<'_>>();
            // Safety: ptr was returned by a prior arena alloc of old_size bytes.
            self.ptr = unsafe { arena.realloc(self.ptr.cast(), old_size, new_size).cast() };
        } else {
            self.ptr = arena.alloc(new_size).cast();
        }
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

/// Consuming iterator over a [`Table`], yielding `(`[`Key`]`, `[`Item`]`)` pairs.
pub struct IntoIter<'de> {
    table: InnerTable<'de>,
    index: u32,
}

impl<'de> Iterator for IntoIter<'de> {
    type Item = (Key<'de>, Item<'de>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.table.len {
            // SAFETY: index < len is checked above, so the read is within
            // bounds of initialized entries.
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

/// A TOML table with span information.
///
/// A `Table` is the top-level value returned by [`parse`](crate::parse) and is
/// also the value inside any `[section]` or inline `{ ... }` table in TOML.
/// It stores `(`[`Key`]`, `[`Item`]`)` pairs in insertion order.
///
/// # Accessing values
///
/// - **Index operators** — `table["key"]` returns a [`MaybeItem`] that never
///   panics on missing keys, and supports chained indexing.
/// - **`get` / `get_mut`** — return `Option<&Item>` / `Option<&mut Item>`.
///
/// For type-safe deserialization, use [`Item::table_helper`](crate::value::Item::table_helper)
/// to create a [`TableHelper`](crate::de::TableHelper).
///
/// # Lookup performance
///
/// Direct lookups ([`get`](Self::get), `table["key"]`) perform a linear scan
/// over entries — O(n) in the number of keys. For small tables or a handful
/// of lookups, as is typical in TOML, this is well fast enough.
///
/// For structured deserialization of larger tables, use
/// [`TableHelper`](crate::de::TableHelper) via
/// [`Root::helper`](crate::Root::helper) or
/// [`Item::table_helper`](crate::value::Item::table_helper). The
/// [`Context`](crate::de::Context) returned by [`parse`](crate::parse)
/// carries the parser's hash index.
///
/// # Constructing tables
///
/// Tables are normally obtained from [`parse`](crate::parse). To build one
/// programmatically, create a table with [`Table::new`] and insert entries
/// with [`Table::insert`]. An [`Arena`](crate::Arena) is required for
/// insertion because entries are arena-allocated.
///
/// # Iteration
///
/// `Table` implements [`IntoIterator`] (both by reference and by value),
/// yielding `(`[`Key`]`, `[`Item`]`)` pairs.
///
/// Removal via [`remove`](Self::remove) uses swap-remove and may reorder
/// remaining entries.
#[repr(C)]
pub struct Table<'de> {
    pub(crate) value: InnerTable<'de>,
    /// Bits 2-0: tag, bits 31-3: span.start
    start_and_tag: u32,
    /// Bit 0: flag bit (parser-internal), bits 31-1: span.end
    end_and_flag: u32,
}

impl<'de> Table<'de> {
    /// Creates an empty table with the given span.
    pub fn new(span: Span) -> Table<'de> {
        Table {
            start_and_tag: span.start << TAG_SHIFT | TAG_TABLE,
            end_and_flag: (span.end << FLAG_SHIFT) | FLAG_TABLE,
            value: InnerTable::new(),
        }
    }
    /// Returns the byte-offset span of this table in the source document.
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
    pub fn insert(&mut self, key: Key<'de>, value: Item<'de>, arena: &'de Arena) {
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
        self.value.get_entry(name)
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

    /// Removes the first entry matching `name`, returning the key-value pair.
    /// Uses swap-remove, so the ordering of remaining entries may change.
    pub fn remove_entry(&mut self, name: &str) -> Option<(Key<'de>, Item<'de>)> {
        self.value.remove_entry(name)
    }

    /// Returns a slice of all entries.
    #[inline]
    pub fn entries(&self) -> &[TableEntry<'de>] {
        self.value.entries()
    }

    /// Converts this `Table` into an [`Item`] with the same span and payload.
    pub fn as_item(&self) -> &Item<'de> {
        unsafe {
            // SAFETY: Table and Item have the same layout and alignment, so this
            // is safe as long as we don't mutate through the Item reference.
            &*(self as *const Table<'de>).cast::<Item<'de>>()
        }
    }

    /// Converts this `Table` into an [`Item`] with the same span and payload.
    pub fn into_item(self) -> Item<'de> {
        let span = self.span();
        Item::table(self.value, span)
    }
}

impl<'a, 'de> IntoIterator for &'a mut Table<'de> {
    type Item = &'a mut (Key<'de>, Item<'de>);
    type IntoIter = std::slice::IterMut<'a, TableEntry<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.entries_mut().iter_mut()
    }
}
impl<'a, 'de> IntoIterator for &'a Table<'de> {
    type Item = &'a (Key<'de>, Item<'de>);
    type IntoIter = std::slice::Iter<'a, TableEntry<'de>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.entries().iter()
    }
}

impl<'de> IntoIterator for Table<'de> {
    type Item = (Key<'de>, Item<'de>);
    type IntoIter = IntoIter<'de>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            table: self.value,
            index: 0,
        }
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
        self.end_and_flag = (v << FLAG_SHIFT) | (self.end_and_flag & FLAG_MASK);
    }

    #[inline]
    pub(crate) fn extend_span_end(&mut self, new_end: u32) {
        let old = self.end_and_flag;
        let current = old >> FLAG_SHIFT;
        self.end_and_flag = (current.max(new_end) << FLAG_SHIFT) | (old & FLAG_MASK);
    }

    #[inline]
    pub(crate) fn set_header_flag(&mut self) {
        self.end_and_flag = (self.end_and_flag & !FLAG_MASK) | FLAG_HEADER;
    }
}

impl<'de> std::ops::Index<&str> for Table<'de> {
    type Output = MaybeItem<'de>;

    #[inline]
    fn index(&self, index: &str) -> &Self::Output {
        if let Some(item) = self.get(index) {
            return MaybeItem::from_ref(item);
        }
        &NONE
    }
}
