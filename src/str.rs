#![allow(unsafe_code)]

//! A borrowed string type used throughout the parser.

use std::borrow::{Borrow, Cow};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::{fmt, str};

/// A borrowed string that references either the TOML source or the parser
/// [`Arena`](crate::Arena).
///
/// `Str` is [`Copy`] and never owns its data. It dereferences to [`str`] and
/// can be converted into [`String`], [`Box<str>`], or [`Cow<str>`](std::borrow::Cow).
#[derive(Copy, Clone)]
pub struct Str<'de> {
    ptr: NonNull<u8>,
    len: usize,
    _marker: PhantomData<&'de str>,
}

const _: () = assert!(std::mem::size_of::<Str<'_>>() == 16);

// SAFETY: The inner data is a `&str` (Send+Sync).
unsafe impl Send for Str<'_> {}
unsafe impl Sync for Str<'_> {}

impl Str<'_> {
    /// Returns the raw pointer and byte length without creating an intermediate `&str`.
    #[inline]
    pub(crate) fn as_raw_parts(&self) -> (NonNull<u8>, usize) {
        (self.ptr, self.len)
    }
}

impl Deref for Str<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr.as_ptr(), self.len);
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
    /// Returns the string slice with the full `'de` lifetime.
    pub fn as_str(&self) -> &'de str {
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr.as_ptr(), self.len);
            str::from_utf8_unchecked(slice)
        }
    }

    #[inline]
    pub(crate) fn from_borrowed(s: &'de str) -> Self {
        // SAFETY: str::as_ptr() is non-null for any valid &str (even "" points to a dangling-but-non-null address).
        let ptr = unsafe { NonNull::new_unchecked(s.as_ptr() as *mut u8) };
        Self {
            ptr,
            len: s.len(),
            _marker: PhantomData,
        }
    }

    /// Copies the contents into a heap-allocated [`Box<str>`].
    pub fn into_boxed_str(self) -> Box<str> {
        (&*self).into()
    }
}

impl<'de> From<&'de str> for Str<'de> {
    #[inline]
    fn from(s: &'de str) -> Self {
        Self::from_borrowed(s)
    }
}

impl<'de> From<Str<'de>> for Cow<'de, str> {
    #[inline]
    fn from(s: Str<'de>) -> Self {
        // Safety: Str's pointer is valid for 'de and contains valid UTF-8.
        let borrowed: &'de str =
            unsafe { str::from_utf8_unchecked(std::slice::from_raw_parts(s.ptr.as_ptr(), s.len)) };
        Cow::Borrowed(borrowed)
    }
}

impl From<Str<'_>> for Box<str> {
    #[inline]
    fn from(s: Str<'_>) -> Self {
        (&*s).into()
    }
}

impl From<Str<'_>> for String {
    #[inline]
    fn from(s: Str<'_>) -> Self {
        (&*s).to_owned()
    }
}

#[cfg(test)]
#[path = "./str_tests.rs"]
mod tests;
