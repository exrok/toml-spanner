use std::alloc::Layout;
use std::mem::MaybeUninit;

const FREAKY_BUCKET_SIZE: usize = 32;

struct Bucket<T> {
    data: [MaybeUninit<T>; FREAKY_BUCKET_SIZE],
    next: *mut Bucket<T>,
}

pub struct MemoryPool<T> {
    current_bucket: *mut Bucket<T>,
    current_size: usize,
}

impl<T> MemoryPool<T> {
    pub fn new() -> Self {
        Self {
            current_size: 0,
            current_bucket: std::ptr::null_mut(),
        }
    }
    pub fn allocator<'a>(&'a mut self) -> Allocator<'a, T> {
        Allocator { inner: self }
    }
}

impl<T> Drop for MemoryPool<T> {
    fn drop(&mut self) {
        let mut current_bucket = self.current_bucket;
        let mut current_size = self.current_size;
        while current_bucket != std::ptr::null_mut() {
            unsafe {
                let bucket = &mut *current_bucket;
                std::ptr::drop_in_place(std::slice::from_raw_parts_mut(
                    &mut bucket.data as *mut _ as *mut T,
                    current_size,
                ) as *mut [T]);
                current_bucket = bucket.next;
                current_size = FREAKY_BUCKET_SIZE;
                std::alloc::dealloc(current_bucket as *mut u8, Layout::new::<Bucket<T>>());
            }
        }
    }
}

pub struct Allocator<'a, T> {
    inner: &'a mut MemoryPool<T>,
}
impl<'a, T: Default> Allocator<'a, T> {
    pub fn alloc_default(&mut self) -> &'a mut T {
        if self.inner.current_bucket == std::ptr::null_mut()
            || self.inner.current_size >= FREAKY_BUCKET_SIZE
        {
            unsafe {
                let ptr = std::alloc::alloc(Layout::new::<Bucket<T>>()) as *mut Bucket<T>;
                if ptr.is_null() {
                    panic!();
                }
                ptr.byte_add(std::mem::offset_of!(Bucket<T>, next))
                    .cast::<*mut Bucket<T>>()
                    .write(self.inner.current_bucket);

                self.inner.current_bucket = ptr;
                self.inner.current_size = 0;
            }
        }
        unsafe {
            let entries = self.inner.current_bucket as *mut T;
            let tail = entries.add(self.inner.current_size);
            tail.write(<T as Default>::default());
            self.inner.current_size += 1;
            return &mut *tail;
        }
    }
}
