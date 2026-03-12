use std::ops::DerefMut;
use std::ptr::NonNull;

use crate::item::{TAG_ARRAY, TAG_STRING, TAG_TABLE};
use crate::{Item, Key, Table};

/// Sentinel address for `buffer` when no heap allocation is needed (scalar non-string items).
const DANGLING: *mut u8 = 1 as *mut u8;

/// A self-contained, `'static` owner of an [`Table`] and all its transitively
/// referenced data (strings, array elements, table entries).
#[repr(transparent)]
pub struct OwnedTable(OwnedItem);

impl OwnedTable {
    pub fn as_ref<'a>(&'a self) -> &'a Table<'a> {
        // SAFETY: The buffer is guaranteed to be valid for the lifetime of the OwnedItem.
        // Shortening 'static to 'a is always safe (covariant).
        unsafe { std::mem::transmute::<&Item<'static>, &Table<'a>>(&self.0.item) }
    }
    pub fn as_mut<'a>(&'a mut self) -> &'a mut Table<'a> {
        // SAFETY: The buffer is guaranteed to be valid for the lifetime of the OwnedItem.
        // The caller can only mutate scalar payloads or rearrange existing
        // pointers; extending an array/table requires an Arena whose lifetime
        // the compiler constrains to be <= 'a.
        unsafe { std::mem::transmute::<&mut Item<'static>, &mut Table<'a>>(&mut self.0.item) }
    }
}

impl Clone for OwnedTable {
    fn clone(&self) -> Self {
        OwnedTable(OwnedItem::from(self.0.as_ref()))
    }
}

impl From<&Table<'_>> for OwnedTable {
    fn from(table: &Table<'_>) -> Self {
        OwnedTable(OwnedItem::from(table.as_item()))
    }
}

/// A self-contained, `'static` owner of an [`Item`] and all its transitively
/// referenced data (strings, array elements, table entries).
#[repr(C)]
pub struct OwnedItem {
    buffer: NonNull<u8>,
    item: Item<'static>,
}

unsafe impl Send for OwnedItem {}

impl OwnedItem {
    pub fn as_ref<'a>(&'a self) -> &'a Item<'a> {
        // SAFETY: The buffer is guaranteed to be valid for the lifetime of the OwnedItem.
        // Shortening 'static to 'a is always safe (covariant).
        unsafe { std::mem::transmute::<&Item<'static>, &Item<'a>>(&self.item) }
    }
    pub fn as_mut<'a>(&'a mut self) -> &'a mut Item<'a> {
        // SAFETY: The buffer is guaranteed to be valid for the lifetime of the OwnedItem.
        // The caller can only mutate scalar payloads or rearrange existing
        // pointers; extending an array/table requires an Arena whose lifetime
        // the compiler constrains to be <= 'a.
        unsafe { std::mem::transmute::<&mut Item<'static>, &mut Item<'a>>(&mut self.item) }
    }
}

impl Clone for OwnedItem {
    fn clone(&self) -> Self {
        OwnedItem::from(self.as_ref())
    }
}

impl Drop for OwnedItem {
    fn drop(&mut self) {
        let ptr = self.buffer.as_ptr();
        if ptr == DANGLING {
            return;
        }
        // SAFETY: ptr was returned by `alloc` with the layout we stored in
        // the first 8 bytes of the header (written as a u64 for portability).
        unsafe {
            let alloc_size = (ptr as *const u64).read() as usize;
            let layout = std::alloc::Layout::from_size_align(alloc_size, 8)
                .expect("OwnedItem: corrupted alloc_size in header");
            std::alloc::dealloc(ptr, layout);
        }
    }
}

impl From<&Item<'_>> for OwnedItem {
    fn from(item: &Item<'_>) -> Self {
        let mut size = Size {
            string_byte_count: 0,
            entry_byte_count: 0,
        };
        total_item_byte_size(item, &mut size);

        let total = size.string_byte_count + size.entry_byte_count;
        if total == 0 {
            // SAFETY: No heap data to own — scalar non-string item (integer, float,
            // boolean, datetime, empty string, empty array, empty table). Bitwise
            // copy is correct because Item is non-Drop and contains no pointers
            // into data we need to own.
            let owned: Item<'static> =
                unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
            return OwnedItem {
                // SAFETY: DANGLING (1) is non-null.
                buffer: unsafe { NonNull::new_unchecked(DANGLING) },
                item: owned,
            };
        }

        // Header is always 8 bytes so the entry region stays 8-byte aligned
        // (Item and Key require align(8)) regardless of pointer width.
        const HEADER: usize = 8;
        let alloc_size = HEADER + size.entry_byte_count + size.string_byte_count;
        // alloc_size > 0 (total > 0), align is 8 (power of two).
        let layout =
            std::alloc::Layout::from_size_align(alloc_size, 8).expect("OwnedItem: layout overflow");
        // SAFETY: layout has non-zero size.
        let buf = unsafe { std::alloc::alloc(layout) };
        if buf.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        // SAFETY: buf is valid for alloc_size bytes, aligned to 8.
        unsafe {
            // Store the allocation size as u64 in the 8-byte header so Drop
            // can recover the layout on any platform.
            (buf as *mut u64).write(alloc_size as u64);

            let mut cursor = Cursor {
                entry_ptr: buf.add(HEADER),
                string_ptr: buf.add(HEADER + size.entry_byte_count),
                #[cfg(debug_assertions)]
                entry_ptr_end: buf.add(HEADER + size.entry_byte_count),
                #[cfg(debug_assertions)]
                string_ptr_end: buf.add(alloc_size),
            };

            let owned = cursor.to_owned(item);

            debug_assert_eq!(cursor.entry_ptr, buf.add(HEADER + size.entry_byte_count));
            debug_assert_eq!(cursor.string_ptr, buf.add(alloc_size));

            OwnedItem {
                buffer: NonNull::new_unchecked(buf),
                item: owned,
            }
        }
    }
}

struct Size {
    string_byte_count: usize,
    entry_byte_count: usize,
}

type TableEntry<'a> = (Key<'a>, Item<'a>);

fn total_item_byte_size(item: &Item<'_>, size: &mut Size) {
    let tag = item.meta.tag();

    if tag == TAG_STRING {
        // SAFETY: tag == TAG_STRING guarantees the payload is a string.
        let s = unsafe { item.payload.string };
        size.string_byte_count += s.len();
        return;
    }

    if tag < TAG_TABLE {
        return; // integer, float, boolean, datetime
    }

    if tag == TAG_ARRAY {
        // SAFETY: tag == TAG_ARRAY guarantees the payload is an array.
        let arr = unsafe { item.as_array_unchecked() };
        let slice = arr.as_slice();
        size.entry_byte_count += slice.len() * size_of::<Item<'static>>();
        for child in slice {
            total_item_byte_size(child, size);
        }
        return;
    }

    // TAG_TABLE
    // SAFETY: tag >= TAG_TABLE and not TAG_ARRAY, so this is a table.
    let tbl = unsafe { item.as_table_unchecked() };
    let entries = tbl.entries();
    size.entry_byte_count += entries.len() * size_of::<TableEntry<'static>>();
    for (key, val) in entries {
        size.string_byte_count += key.name.len();
        total_item_byte_size(val, size);
    }
}

struct Cursor {
    string_ptr: *mut u8,
    #[cfg(debug_assertions)]
    string_ptr_end: *mut u8,
    entry_ptr: *mut u8,
    #[cfg(debug_assertions)]
    entry_ptr_end: *mut u8,
}

impl Cursor {
    /// Copies `len` bytes from `src` into the string region.
    /// Returns a pointer to the copy.
    ///
    /// # Safety
    ///
    /// `src` must be valid for `len` bytes. Enough space must be reserved.
    #[inline(always)]
    unsafe fn push_str_raw(&mut self, src: *const u8, len: usize) -> *const u8 {
        let dst = self.string_ptr;
        #[cfg(debug_assertions)]
        debug_assert!(unsafe { dst.add(len) } <= self.string_ptr_end);
        // SAFETY: Caller pre-computed string_byte_count to cover all strings.
        unsafe { std::ptr::copy_nonoverlapping(src, dst, len) };
        self.string_ptr = unsafe { dst.add(len) };
        dst
    }

    /// Bump-allocates space for `count` items of type `T` from the entry region.
    ///
    /// # Safety
    ///
    /// Enough space must be reserved in the entry region.
    #[inline(always)]
    unsafe fn alloc_entries<T>(&mut self, count: usize) -> *mut T {
        let byte_count = count * size_of::<T>();
        let ptr = self.entry_ptr as *mut T;
        #[cfg(debug_assertions)]
        debug_assert!(unsafe { self.entry_ptr.add(byte_count) } <= self.entry_ptr_end);
        self.entry_ptr = unsafe { self.entry_ptr.add(byte_count) };
        ptr
    }

    /// Deep-clones `item` into the owned buffer, returning a `'static` copy.
    ///
    /// # Safety
    ///
    /// The cursor must have enough space for all entries and strings reachable
    /// from `item` (as computed by `total_item_byte_size`).
    unsafe fn to_owned(&mut self, item: &Item<'_>) -> Item<'static> {
        let tag = item.meta.tag();

        if tag < TAG_TABLE && tag != TAG_STRING {
            // SAFETY: Non-string scalars have no pointers into borrowed data.
            return unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        }

        match tag {
            TAG_STRING => unsafe { self.clone_string(item) },
            TAG_ARRAY => unsafe { self.clone_array(item) },
            TAG_TABLE => unsafe { self.clone_table(item) },
            _ => {
                debug_assert!(false, "unreachable tag");
                unsafe { std::hint::unreachable_unchecked() }
            }
        }
    }

    #[inline(always)]
    unsafe fn clone_string(&mut self, item: &Item<'_>) -> Item<'static> {
        // SAFETY: caller guarantees tag == TAG_STRING.
        let s = unsafe { item.payload.string };

        if s.is_empty() {
            return unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        }

        let new_ptr = unsafe { self.push_str_raw(s.as_ptr(), s.len()) };

        // Copy the entire 24-byte item, then patch the string data pointer.
        let mut owned: Item<'static> =
            unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        // SAFETY: item is a string; new_ptr points to our owned buffer with s.len() bytes.
        unsafe {
            owned.payload.string =
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(new_ptr, s.len()));
        }
        owned
    }

    unsafe fn clone_array(&mut self, item: &Item<'_>) -> Item<'static> {
        // SAFETY: caller guarantees tag == TAG_ARRAY.
        let arr = unsafe { item.as_array_unchecked() };
        let src_slice = arr.as_slice();
        let len = src_slice.len();

        if len == 0 {
            return unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        }

        let dst: *mut Item<'static> = unsafe { self.alloc_entries(len) };

        // Bulk-copy all items, then fixup non-trivial ones.
        unsafe {
            std::ptr::copy_nonoverlapping(src_slice.as_ptr() as *const Item<'static>, dst, len);
        }

        // Fixup pass: read tags/strings from source (safe), write patches to dst.
        for i in 0..len {
            let src_item = &src_slice[i];
            let tag = src_item.meta.tag();

            if tag == TAG_STRING {
                // SAFETY: tag == TAG_STRING guarantees the payload is a string.
                let s = unsafe { src_item.payload.string };
                if !s.is_empty() {
                    let new_ptr = unsafe { self.push_str_raw(s.as_ptr(), s.len()) };
                    let dst_item = unsafe { &mut *dst.add(i) };
                    // SAFETY: dst item is a string; new_ptr is our owned copy.
                    unsafe {
                        dst_item.payload.string = std::str::from_utf8_unchecked(
                            std::slice::from_raw_parts(new_ptr, s.len()),
                        );
                    }
                }
            } else if tag >= TAG_TABLE {
                // Recursive clone from original source (not the bulk-copied version).
                let dst_item = unsafe { dst.add(i) };
                unsafe { dst_item.write(self.to_owned(src_item)) };
            }
        }

        let mut owned: Item<'static> =
            unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        // SAFETY: item is an array; patch cap and ptr to our owned buffer.
        unsafe {
            let arr = (&mut *std::ptr::addr_of_mut!(owned.payload.array)).deref_mut();
            arr.cap = len as u32;
            arr.ptr = NonNull::new_unchecked(dst);
        }
        owned
    }

    unsafe fn clone_table(&mut self, item: &Item<'_>) -> Item<'static> {
        // SAFETY: caller guarantees tag == TAG_TABLE.
        let tbl = unsafe { item.as_table_unchecked() };
        let src_entries = tbl.entries();
        let len = src_entries.len();

        if len == 0 {
            return unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        }

        let dst: *mut TableEntry<'static> = unsafe { self.alloc_entries(len) };

        // Bulk-copy all entries (keys + items). After this, pointers in dst
        // still reference original arena data — which is valid for reads.
        unsafe {
            std::ptr::copy_nonoverlapping(
                src_entries.as_ptr() as *const TableEntry<'static>,
                dst,
                len,
            );
        }

        // Fixup pass: read key names/tags from source (safe), write patches to dst.
        for i in 0..len {
            let (ref src_key, ref src_val) = src_entries[i];
            let (dst_key, dst_item) = unsafe { &mut *dst.add(i) };

            // Key name fixup: patch dst key to point to our owned copy.
            let name = src_key.name;
            if !name.is_empty() {
                let new_name_ptr = unsafe { self.push_str_raw(name.as_ptr(), name.len()) };
                // SAFETY: new_name_ptr points to our owned buffer with name.len() bytes.
                dst_key.name = unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        new_name_ptr,
                        name.len(),
                    ))
                };
            }

            // Item fixup
            let tag = src_val.meta.tag();

            if tag == TAG_STRING {
                // SAFETY: tag == TAG_STRING guarantees the payload is a string.
                let s = unsafe { src_val.payload.string };
                if !s.is_empty() {
                    let new_ptr = unsafe { self.push_str_raw(s.as_ptr(), s.len()) };
                    // SAFETY: dst item is a string; new_ptr is our owned copy.
                    unsafe {
                        dst_item.payload.string = std::str::from_utf8_unchecked(
                            std::slice::from_raw_parts(new_ptr, s.len()),
                        );
                    }
                }
            } else if tag >= TAG_TABLE {
                // Recursive clone from original source.
                *dst_item = unsafe { self.to_owned(src_val) };
            }
        }

        let mut owned: Item<'static> =
            unsafe { std::ptr::read(item as *const Item<'_> as *const Item<'static>) };
        // SAFETY: item is a table; patch cap and ptr to our owned buffer.
        unsafe {
            let tbl = (&mut *std::ptr::addr_of_mut!(owned.payload.table)).deref_mut();
            tbl.cap = len as u32;
            tbl.ptr = NonNull::new_unchecked(dst);
        }
        owned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arena, parse};

    #[test]
    fn scalar_integer() {
        let arena = Arena::new();
        let root = parse("x = 42", &arena).unwrap();
        let item = root["x"].item().unwrap();
        let owned = OwnedItem::from(item);
        assert_eq!(owned.as_ref().as_i64(), Some(42));
    }

    #[test]
    fn scalar_bool() {
        let arena = Arena::new();
        let root = parse("b = true", &arena).unwrap();
        let item = root["b"].item().unwrap();
        let owned = OwnedItem::from(item);
        assert_eq!(owned.as_ref().as_bool(), Some(true));
    }

    #[test]
    fn string_value() {
        let arena = Arena::new();
        let root = parse("s = 'hello world'", &arena).unwrap();
        let item = root["s"].item().unwrap();
        let owned = OwnedItem::from(item);
        assert_eq!(owned.as_ref().as_str(), Some("hello world"));
    }

    #[test]
    fn empty_string() {
        let arena = Arena::new();
        let root = parse("s = ''", &arena).unwrap();
        let item = root["s"].item().unwrap();
        let owned = OwnedItem::from(item);
        assert_eq!(owned.as_ref().as_str(), Some(""));
    }

    #[test]
    fn inline_array() {
        let arena = Arena::new();
        let root = parse("a = [1, 2, 3]", &arena).unwrap();
        let item = root["a"].item().unwrap();
        let owned = OwnedItem::from(item);
        let arr = owned.as_ref().as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
        assert_eq!(arr[2].as_i64(), Some(3));
    }

    #[test]
    fn array_of_strings() {
        let arena = Arena::new();
        let root = parse("a = ['foo', 'bar']", &arena).unwrap();
        let item = root["a"].item().unwrap();
        let owned = OwnedItem::from(item);
        let arr = owned.as_ref().as_array().unwrap();
        assert_eq!(arr[0].as_str(), Some("foo"));
        assert_eq!(arr[1].as_str(), Some("bar"));
    }

    #[test]
    fn simple_table() {
        let arena = Arena::new();
        let root = parse("[pkg]\nname = 'test'\nversion = 1", &arena).unwrap();
        let item = root["pkg"].item().unwrap();
        let owned = OwnedItem::from(item);
        let tbl = owned.as_ref().as_table().unwrap();
        assert_eq!(tbl["name"].as_str(), Some("test"));
        assert_eq!(tbl["version"].as_i64(), Some(1));
    }

    #[test]
    fn nested_table() {
        let toml = r#"
[a]
x = 1
[a.b]
y = "hello"
arr = [10, 20]
"#;
        let arena = Arena::new();
        let root = parse(toml, &arena).unwrap();
        let item = root["a"].item().unwrap();
        let owned = OwnedItem::from(item);
        let a = owned.as_ref().as_table().unwrap();
        assert_eq!(a["x"].as_i64(), Some(1));
        let b = a["b"].as_table().unwrap();
        assert_eq!(b["y"].as_str(), Some("hello"));
        let arr = b["arr"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_i64(), Some(10));
    }

    #[test]
    fn whole_root_table() {
        let toml = r#"
title = "example"
[server]
host = "localhost"
port = 8080
"#;
        let arena = Arena::new();
        let root = parse(toml, &arena).unwrap();
        let owned = OwnedItem::from(root.table.as_item());
        let tbl = owned.as_ref().as_table().unwrap();
        assert_eq!(tbl["title"].as_str(), Some("example"));
        assert_eq!(tbl["server"]["host"].as_str(), Some("localhost"));
        assert_eq!(tbl["server"]["port"].as_i64(), Some(8080));
    }

    #[test]
    fn drop_no_leak() {
        let arena = Arena::new();
        let root = parse("a = [1, 2]\nb = 'text'", &arena).unwrap();
        let owned = OwnedItem::from(root.table.as_item());
        drop(owned);
    }

    #[test]
    fn owned_item_clone_and_as_mut() {
        let arena = Arena::new();
        let root = parse("[pkg]\nname = 'original'\ncount = 5", &arena).unwrap();
        let owned = OwnedItem::from(root.table.as_item());

        // Clone preserves content
        let cloned = owned.clone();
        assert_eq!(
            cloned.as_ref().as_table().unwrap()["pkg"]["name"].as_str(),
            Some("original")
        );

        // as_mut allows mutation
        let mut owned2 = OwnedItem::from(root["pkg"]["count"].item().unwrap());
        let item = owned2.as_mut();
        assert_eq!(item.as_i64(), Some(5));
    }

    #[test]
    fn owned_table_roundtrip() {
        let toml = "[db]\nhost = 'localhost'\nport = 5432";
        let arena = Arena::new();
        let root = parse(toml, &arena).unwrap();
        let table_ref = root["db"].as_table().unwrap();

        // OwnedTable::from
        let owned = OwnedTable::from(table_ref);

        // as_ref
        let t = owned.as_ref();
        assert_eq!(t["host"].as_str(), Some("localhost"));
        assert_eq!(t["port"].as_i64(), Some(5432));

        // clone
        let cloned = owned.clone();
        assert_eq!(cloned.as_ref()["host"].as_str(), Some("localhost"));

        // as_mut
        let mut owned2 = OwnedTable::from(table_ref);
        let t_mut = owned2.as_mut();
        assert_eq!(t_mut.len(), 2);
    }
}
