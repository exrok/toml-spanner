#![allow(unsafe_code)]

use std::alloc::Layout;
use std::cell::Cell;
use std::ptr::{self, NonNull};

const SLAB_ALIGN: usize = std::mem::align_of::<SlabHeader>();
const HEADER_SIZE: usize = std::mem::size_of::<SlabHeader>();
const INITIAL_SLAB_SIZE: usize = 1024;

const _: () = assert!(HEADER_SIZE == 16);
const _: () = assert!(SLAB_ALIGN == 8);

#[repr(C)]
struct SlabHeader {
    prev: Option<NonNull<SlabHeader>>,
    size: usize,
}

// Safety: EMPTY_SLAB is an immutable sentinel (prev=None, size=0). SlabHeaders
// on the heap are only reachable through Arena, which is !Sync due to Cell.
unsafe impl Sync for SlabHeader {}

static EMPTY_SLAB: SlabHeader = SlabHeader {
    prev: None,
    size: 0,
};

/// A bump allocator that allocates from increasingly large slabs.
///
/// All allocations are bulk-freed when the arena is dropped. Individual
/// deallocation is not supported.
pub struct Arena {
    ptr: Cell<NonNull<u8>>,
    end: Cell<NonNull<u8>>,
    slab: Cell<NonNull<SlabHeader>>,
}

const _: () = assert!(std::mem::size_of::<Arena>() == 24);

impl Arena {
    pub fn new() -> Self {
        // Safety: EMPTY_SLAB is a static with a stable address.
        let sentinel =
            unsafe { NonNull::new_unchecked(&EMPTY_SLAB as *const SlabHeader as *mut SlabHeader) };
        let dangling = NonNull::dangling();
        Arena {
            ptr: Cell::new(dangling),
            end: Cell::new(dangling),
            slab: Cell::new(sentinel),
        }
    }

    /// Allocate `layout.size()` bytes with the given alignment.
    ///
    /// Returns a non-null pointer to the allocated region. Aborts on OOM.
    #[inline]
    pub(crate) fn alloc(&self, layout: Layout) -> NonNull<u8> {
        if layout.size() == 0 {
            // Safety: layout.align() is always a non-zero power of two.
            return unsafe { NonNull::new_unchecked(layout.align() as *mut u8) };
        }

        let ptr = self.ptr.get().as_ptr() as usize;
        let aligned = (ptr + layout.align() - 1) & !(layout.align() - 1);
        let new_ptr = aligned + layout.size();

        if new_ptr <= self.end.get().as_ptr() as usize {
            // Safety: new_ptr is within the current slab's bounds.
            unsafe {
                self.ptr.set(NonNull::new_unchecked(new_ptr as *mut u8));
                NonNull::new_unchecked(aligned as *mut u8)
            }
        } else {
            self.alloc_slow(layout)
        }
    }

    #[cold]
    #[inline(never)]
    fn alloc_slow(&self, layout: Layout) -> NonNull<u8> {
        self.grow(layout);

        let ptr = self.ptr.get().as_ptr() as usize;
        let aligned = (ptr + layout.align() - 1) & !(layout.align() - 1);
        let new_ptr = aligned + layout.size();
        debug_assert!(new_ptr <= self.end.get().as_ptr() as usize);

        // Safety: grow() guarantees the new slab is large enough.
        unsafe {
            self.ptr.set(NonNull::new_unchecked(new_ptr as *mut u8));
            NonNull::new_unchecked(aligned as *mut u8)
        }
    }

    /// Create a scratch buffer that writes into the arena's current slab.
    ///
    /// # Safety
    ///
    /// The caller must not call `alloc` on this arena while the returned
    /// `Scratch` is alive. The scratch exclusively owns the bump region.
    pub(crate) unsafe fn scratch(&self) -> Scratch<'_> {
        let start = self.ptr.get();
        let cap = self.end.get().as_ptr() as usize - start.as_ptr() as usize;
        Scratch {
            arena: self,
            start,
            len: 0,
            cap,
        }
    }

    fn grow(&self, layout: Layout) {
        let current_size = unsafe { self.slab.get().as_ref().size };

        let min_slab = HEADER_SIZE
            .checked_add(layout.align() - 1)
            .and_then(|s| s.checked_add(layout.size()))
            .expect("layout overflow");

        let new_size = current_size
            .saturating_mul(2)
            .max(min_slab)
            .max(INITIAL_SLAB_SIZE);

        let slab_layout =
            Layout::from_size_align(new_size, SLAB_ALIGN).expect("slab layout overflow");

        let raw = unsafe { std::alloc::alloc(slab_layout) };
        let Some(base) = NonNull::new(raw) else {
            std::alloc::handle_alloc_error(slab_layout);
        };

        // Safety: base points to a freshly allocated region of new_size bytes.
        unsafe {
            let header_ptr = base.as_ptr().cast::<SlabHeader>();
            header_ptr.write(SlabHeader {
                prev: Some(self.slab.get()),
                size: new_size,
            });

            self.slab.set(NonNull::new_unchecked(header_ptr));
            self.ptr
                .set(NonNull::new_unchecked(base.as_ptr().add(HEADER_SIZE)));
            self.end
                .set(NonNull::new_unchecked(base.as_ptr().add(new_size)));
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        let mut current = self.slab.get();
        loop {
            // Safety: current is either a heap slab or the static sentinel.
            let header = unsafe { current.as_ref() };
            if header.size == 0 {
                break;
            }
            let prev = header.prev;
            // Safety: header.size and SLAB_ALIGN match the layout used in grow().
            let slab_layout = unsafe { Layout::from_size_align_unchecked(header.size, SLAB_ALIGN) };
            unsafe {
                std::alloc::dealloc(current.as_ptr().cast(), slab_layout);
            }
            match prev {
                Some(p) => current = p,
                None => break,
            }
        }
    }
}

/// A temporary byte buffer that writes directly into an [`Arena`] slab.
///
/// Scratch tracks its own write position without advancing the arena's bump
/// pointer. On [`commit`](Scratch::commit) the arena pointer is advanced past
/// the committed bytes. If the scratch is dropped without committing, the arena
/// pointer is unchanged and the space is reused by subsequent allocations.
pub(crate) struct Scratch<'a> {
    arena: &'a Arena,
    start: NonNull<u8>,
    len: usize,
    cap: usize,
}

impl<'a> Scratch<'a> {
    #[inline]
    pub fn push(&mut self, byte: u8) {
        let len = self.len;
        if len == self.cap {
            self.grow_slow(1);
        }
        // Safety: len < cap, so start + len is within the slab.
        unsafe {
            self.start.as_ptr().add(len).write(byte);
        }
        self.len = len + 1;
    }

    #[inline]
    pub fn extend(&mut self, bytes: &[u8]) {
        if bytes.len() > self.cap - self.len {
            self.grow_slow(bytes.len());
        }
        // Safety: cap - len >= bytes.len(), so the copy is in bounds.
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.start.as_ptr().add(self.len),
                bytes.len(),
            );
        }
        self.len += bytes.len();
    }

    /// Push bytes while stripping underscores. Returns `false` if any
    /// underscore is not placed between two ASCII digits.
    #[inline]
    pub(crate) fn push_strip_underscores(&mut self, bytes: &[u8]) -> bool {
        let mut prev = 0u8;
        for &b in bytes {
            if b == b'_' {
                if !prev.is_ascii_digit() {
                    return false;
                }
            } else {
                if prev == b'_' && !b.is_ascii_digit() {
                    return false;
                }
                self.push(b);
            }
            prev = b;
        }
        prev != b'_'
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // Safety: start..start+len was written by us and is within the slab.
        unsafe { std::slice::from_raw_parts(self.start.as_ptr(), self.len) }
    }

    /// Finalize the scratch data and return it as a byte slice tied to the
    /// arena's lifetime. Advances the arena's bump pointer past the committed
    /// bytes.
    pub fn commit(self) -> &'a [u8] {
        if self.len == 0 {
            return &[];
        }
        // Safety: start..start+len is valid scratch memory within the arena.
        let slice = unsafe { std::slice::from_raw_parts(self.start.as_ptr(), self.len) };
        // Safety: start + len is within the slab (we ensured capacity on every write).
        unsafe {
            self.arena
                .ptr
                .set(NonNull::new_unchecked(self.start.as_ptr().add(self.len)));
        }
        slice
    }

    #[cold]
    #[inline(never)]
    fn grow_slow(&mut self, additional: usize) {
        let required = self.len.checked_add(additional).expect("scratch overflow");
        let new_cap = self.cap.saturating_mul(2).max(required);

        let layout = Layout::from_size_align(new_cap, 1).expect("scratch layout overflow");
        self.arena.grow(layout);

        // Copy existing scratch data to the start of the new slab.
        let new_start = self.arena.ptr.get();
        if self.len > 0 {
            // Safety: old data at self.start..+len is still valid (old slab hasn't been freed).
            // New slab has at least new_cap bytes of data space >= required > self.len.
            unsafe {
                ptr::copy_nonoverlapping(self.start.as_ptr(), new_start.as_ptr(), self.len);
            }
        }
        self.start = new_start;
        self.cap = self.arena.end.get().as_ptr() as usize - new_start.as_ptr() as usize;
    }
}

#[cfg(test)]
#[path = "./arena_tests.rs"]
mod tests;
