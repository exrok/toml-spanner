//! Serde serialization support for [`Spanned<T>`](crate::Spanned) and
//! [`Item`](crate::Item).
//!
//! Enabled by the `serde` feature flag. This provides [`serde::Serialize`]
//! implementations only — deserialization uses the [`FromItem`](crate::FromItem)
//! trait instead.

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
