#![allow(unsafe_code)]

//! A 16-byte string type for borrowed or owned string data.

use std::borrow::{Borrow, Cow};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::{fmt, str};

/// High bit of `len_and_tag`: set = owned (must free), clear = borrowed.
const OWNED_BIT: usize = 1 << (usize::BITS - 1);

/// A 16-byte string that is either borrowed from the TOML source or an owned
/// heap allocation.
///
/// This is a manual tagged union: `(ptr, len | tag_bit)`. The high bit of the
/// length word distinguishes borrowed (0) from owned (1). Maximum string
/// length is `isize::MAX`, matching Rust's allocation limit.
pub struct Str<'de> {
    ptr: NonNull<u8>,
    len_and_tag: usize,
    _marker: PhantomData<&'de str>,
}

const _: () = assert!(std::mem::size_of::<Str<'_>>() == 16);

// SAFETY: The inner data is either a &str (Send+Sync) or a Box<str> (Send+Sync).
unsafe impl Send for Str<'_> {}
unsafe impl Sync for Str<'_> {}

impl Str<'_> {
    #[inline]
    fn is_owned(&self) -> bool {
        self.len_and_tag & OWNED_BIT != 0
    }

    #[inline]
    fn str_len(&self) -> usize {
        self.len_and_tag & !OWNED_BIT
    }

    /// Returns the raw pointer and byte length without creating an intermediate `&str`.
    #[inline]
    pub(crate) fn as_raw_parts(&self) -> (NonNull<u8>, usize) {
        (self.ptr, self.str_len())
    }
}

impl Deref for Str<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr.as_ptr(), self.str_len());
            str::from_utf8_unchecked(slice)
        }
    }
}

impl AsRef<str> for Str<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}

impl Borrow<str> for Str<'_> {
    #[inline]
    fn borrow(&self) -> &str {
        self
    }
}

impl Clone for Str<'_> {
    fn clone(&self) -> Self {
        if self.is_owned() {
            let s: &str = self;
            let boxed: Box<str> = s.into();
            Self::from_box(boxed)
        } else {
            Self {
                ptr: self.ptr,
                len_and_tag: self.len_and_tag,
                _marker: PhantomData,
            }
        }
    }
}

impl Drop for Str<'_> {
    fn drop(&mut self) {
        if self.is_owned() {
            let len = self.str_len();
            let slice = std::ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), len);
            unsafe {
                drop(Box::from_raw(slice as *mut str));
            }
        }
    }
}

impl fmt::Display for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl fmt::Debug for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl PartialEq for Str<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for Str<'_> {}

impl PartialOrd for Str<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Str<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl PartialEq<str> for Str<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        **self == *other
    }
}

impl PartialEq<&str> for Str<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        **self == **other
    }
}

impl Hash for Str<'_> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl Default for Str<'_> {
    #[inline]
    fn default() -> Self {
        Self::from_borrowed("")
    }
}

impl<'de> Str<'de> {
    /// Borrow
    pub fn as_str(&self) -> &str {
        self
    }
    #[inline]
    fn from_borrowed(s: &'de str) -> Self {
        // SAFETY: str::as_ptr() is non-null for any valid &str (even "" points to a dangling-but-non-null address).
        let ptr = unsafe { NonNull::new_unchecked(s.as_ptr() as *mut u8) };
        Self {
            ptr,
            len_and_tag: s.len(),
            _marker: PhantomData,
        }
    }
    /// Into boxed str
    pub fn into_boxed_str(self) -> Box<str> {
        let s = std::mem::ManuallyDrop::new(self);
        if s.is_owned() {
            let slice = std::ptr::slice_from_raw_parts_mut(s.ptr.as_ptr(), s.str_len());
            unsafe { Box::from_raw(slice as *mut str) }
        } else {
            let borrowed: &str =
                unsafe { str::from_utf8_unchecked(std::slice::from_raw_parts(s.ptr.as_ptr(), s.str_len())) };
            borrowed.into()
        }
    }

    #[inline]
    fn from_box(s: Box<str>) -> Self {
        let len = s.len();
        // SAFETY: Box::into_raw always returns a non-null pointer.
        let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(s).cast::<u8>()) };
        Self {
            ptr,
            len_and_tag: len | OWNED_BIT,
            _marker: PhantomData,
        }
    }
}

impl<'de> From<&'de str> for Str<'de> {
    #[inline]
    fn from(s: &'de str) -> Self {
        Self::from_borrowed(s)
    }
}

impl From<String> for Str<'_> {
    #[inline]
    fn from(s: String) -> Self {
        Self::from_box(s.into_boxed_str())
    }
}

impl From<Box<str>> for Str<'_> {
    #[inline]
    fn from(s: Box<str>) -> Self {
        Self::from_box(s)
    }
}

impl<'de> From<Cow<'de, str>> for Str<'de> {
    #[inline]
    fn from(cow: Cow<'de, str>) -> Self {
        match cow {
            Cow::Borrowed(s) => Self::from_borrowed(s),
            Cow::Owned(s) => Self::from_box(s.into_boxed_str()),
        }
    }
}

impl<'de> From<Str<'de>> for Cow<'de, str> {
    #[inline]
    fn from(s: Str<'de>) -> Self {
        let s = std::mem::ManuallyDrop::new(s);
        if s.is_owned() {
            let slice = std::ptr::slice_from_raw_parts_mut(s.ptr.as_ptr(), s.str_len());
            let boxed: Box<str> = unsafe { Box::from_raw(slice as *mut str) };
            Cow::Owned(boxed.into())
        } else {
            let borrowed: &'de str =
                unsafe { str::from_utf8_unchecked(std::slice::from_raw_parts(s.ptr.as_ptr(), s.str_len())) };
            Cow::Borrowed(borrowed)
        }
    }
}
impl From<Str<'_>> for Box<str> {
    #[inline]
    fn from(s: Str<'_>) -> Self {
        let s = std::mem::ManuallyDrop::new(s);
        if s.is_owned() {
            let slice = std::ptr::slice_from_raw_parts_mut(s.ptr.as_ptr(), s.str_len());
            let boxed: Box<str> = unsafe { Box::from_raw(slice as *mut str) };
            boxed
        } else {
            let borrowed: &str =
                unsafe { str::from_utf8_unchecked(std::slice::from_raw_parts(s.ptr.as_ptr(), s.str_len())) };
            borrowed.into()
        }
    }
}

impl From<Str<'_>> for String {
    #[inline]
    fn from(s: Str<'_>) -> Self {
        let s = std::mem::ManuallyDrop::new(s);
        if s.is_owned() {
            let slice = std::ptr::slice_from_raw_parts_mut(s.ptr.as_ptr(), s.str_len());
            let boxed: Box<str> = unsafe { Box::from_raw(slice as *mut str) };
            boxed.into()
        } else {
            let borrowed: &str =
                unsafe { str::from_utf8_unchecked(std::slice::from_raw_parts(s.ptr.as_ptr(), s.str_len())) };
            borrowed.to_owned()
        }
    }
}

#[cfg(test)]
#[path = "./str_tests.rs"]
mod tests;
