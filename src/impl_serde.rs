#![cfg_attr(docsrs, doc(cfg(feature = "serde")))]

//! Provides [`serde::Serialize`] support for [`Spanned`]
//!
//! The [`serde::Serialize`] impl for [`Value`](crate::Value) lives in `value.rs`.

use crate::Spanned;

impl<T> serde::Serialize for Spanned<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}
