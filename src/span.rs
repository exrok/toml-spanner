//! Byte-offset span types for source location tracking.

#[cfg(test)]
#[path = "./span_tests.rs"]
mod tests;

/// A byte-offset range within a TOML document.
///
/// Convertible to and from [`Range<u32>`](std::ops::Range) and
/// [`Range<usize>`](std::ops::Range).
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct Span {
    /// Start byte offset (inclusive).
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
}

impl Span {
    /// Creates a new [`Span`] from start and end byte offsets.
    #[inline]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns `true` if both start and end are zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == 0 && self.end == 0
    }
}

impl From<Span> for (u32, u32) {
    fn from(s: Span) -> (u32, u32) {
        (s.start, s.end)
    }
}

impl From<Span> for (usize, usize) {
    fn from(s: Span) -> (usize, usize) {
        (s.start as usize, s.end as usize)
    }
}

impl From<std::ops::Range<u32>> for Span {
    fn from(s: std::ops::Range<u32>) -> Self {
        Self::new(s.start, s.end)
    }
}

impl From<Span> for std::ops::Range<u32> {
    fn from(s: Span) -> Self {
        s.start..s.end
    }
}

impl From<Span> for std::ops::Range<usize> {
    fn from(s: Span) -> Self {
        s.start as usize..s.end as usize
    }
}

/// Wraps a value `T` with its source [`Span`].
///
/// Use this as a field type in your [`Deserialize`](crate::Deserialize) structs
/// when you need to preserve span information alongside the deserialized value.
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, Spanned};
///
/// let arena = Arena::new();
/// let mut root = toml_spanner::parse("name = \"hello\"", &arena)?;
/// let name: Spanned<String> = {
///     let mut helper = root.helper();
///     helper.required("name").ok().unwrap()
/// };
/// assert_eq!(name.value, "hello");
/// assert!(name.span.start < name.span.end);
/// # Ok::<(), toml_spanner::Error>(())
/// ```
pub struct Spanned<T> {
    /// The deserialized value.
    pub value: T,
    /// The byte-offset span in the source document.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Creates a [`Spanned`] with the given value and a zero span.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self {
            value,
            span: Span { start: 0, end: 0 },
        }
    }

    /// Creates a [`Spanned`] from a value and a [`Span`].
    #[inline]
    pub const fn with_span(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Consumes the wrapper, returning the inner value.
    #[inline]
    pub fn take(self) -> T {
        self.value
    }

    /// Maps the inner value via [`From`], preserving the span.
    #[inline]
    pub fn map<V>(self) -> Spanned<V>
    where
        V: From<T>,
    {
        Spanned {
            value: self.value.into(),
            span: self.span,
        }
    }
}

impl<T> Default for Spanned<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            value: Default::default(),
            span: Span::default(),
        }
    }
}

impl<T> AsRef<T> for Spanned<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> std::fmt::Debug for Spanned<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> Clone for Spanned<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            span: self.span,
        }
    }
}

impl<T> PartialOrd for Spanned<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, o: &Spanned<T>) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&o.value)
    }
}

impl<T> Ord for Spanned<T>
where
    T: Ord,
{
    fn cmp(&self, o: &Spanned<T>) -> std::cmp::Ordering {
        self.value.cmp(&o.value)
    }
}

impl<T> PartialEq for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, o: &Spanned<T>) -> bool {
        self.value == o.value
    }
}

impl<T> Eq for Spanned<T> where T: Eq {}

impl<T> PartialEq<T> for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, o: &T) -> bool {
        &self.value == o
    }
}

impl<'de, T> crate::de::Deserialize<'de> for Spanned<T>
where
    T: crate::de::Deserialize<'de>,
{
    #[inline]
    fn deserialize(
        ctx: &mut crate::de::Context<'de>,
        value: &crate::value::Item<'de>,
    ) -> Result<Self, crate::de::Failed> {
        let span = value.span();
        let inner = T::deserialize(ctx, value)?;
        Ok(Self { span, value: inner })
    }
}
