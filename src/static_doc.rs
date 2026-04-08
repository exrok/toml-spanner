#[cfg(test)]
#[path = "./static_doc_tests.rs"]
mod tests;

use crate::arena::Arena;
use crate::item::table::Table;

/// An owned TOML document with no lifetime parameter.
///
/// Owns both the input string (copied into the arena) and the arena itself,
/// producing a self-contained handle that can be stored in long-lived structs
/// or sent across threads.
///
/// Created via [`StaticDocument::parse`]. Access values through
/// [`table()`](Self::table). Deserialize owned types with
/// [`to()`](Self::to) (requires `from-toml` feature).
///
/// ```
/// let doc = toml_spanner::StaticDocument::parse("key = 'value'").unwrap();
/// assert_eq!(doc.table()["key"].as_str(), Some("value"));
/// ```
pub struct StaticDocument {
    table: Table<'static>,
    #[cfg(feature = "from-toml")]
    index: foldhash::HashMap<crate::parser::KeyRef<'static>, usize>,
    #[cfg(feature = "from-toml")]
    source: &'static str,
    _arena: Box<Arena>,
}

// SAFETY: All data is heap-allocated at stable addresses inside the owned
// Box<Arena>. After parsing the Arena is inert (no Cell reads or writes).
unsafe impl Send for StaticDocument {}

impl StaticDocument {
    /// Parses a TOML string into an owned document.
    ///
    /// The input is copied into an internal arena so all parsed data lives
    /// in a single owned allocation. The returned document has no lifetime
    /// parameter and can be freely moved or stored.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::Error) on parse failure.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = toml_spanner::StaticDocument::parse("key = 'value'").unwrap();
    /// assert_eq!(doc.table()["key"].as_str(), Some("value"));
    /// ```
    pub fn parse(input: &str) -> Result<Self, crate::Error> {
        let arena = Box::new(Arena::new());
        let source: &str = arena.alloc_str(&input);

        let doc = crate::parse(source, &arena)?;

        // SAFETY: The lifetime 'de of the parsed data is erased to 'static.
        // This is sound because:
        //
        // 1. All references (table entries, string slices, KeyRef pointers)
        //    point into the Box<Arena>'s heap slabs. The input was copied
        //    into the arena via alloc_str before parsing, so even unescaped
        //    string slices point into arena memory rather than the input.
        //
        // 2. The Box<Arena> is owned by StaticDocument at a stable heap
        //    address. Its slabs remain allocated and at fixed addresses
        //    until the Arena is dropped.
        //
        // 3. Table, Item, Key, Array, and KeyRef have no Drop impls, so
        //    they never dereference arena memory during destruction. The
        //    arena is the last declared field so it drops after the others
        //    (defense-in-depth. The absence of Drop on the referencing
        //    types is the actual invariant).
        //
        // 4. table() returns &'a Table<'a> (not Table<'static>) via
        //    covariance, preventing 'static references from escaping.
        unsafe {
            Ok(StaticDocument {
                table: std::mem::transmute::<Table<'_>, Table<'static>>(doc.table),
                #[cfg(feature = "from-toml")]
                index: std::mem::transmute(doc.ctx.index),
                #[cfg(feature = "from-toml")]
                source: std::mem::transmute::<&str, &'static str>(source),
                _arena: arena,
            })
        }
    }

    /// Returns a shared reference to the root table.
    pub fn table<'a>(&'a self) -> &'a Table<'a> {
        &self.table
    }

    /// Returns the original TOML source text.
    #[cfg(feature = "from-toml")]
    pub fn source(&self) -> &str {
        self.source
    }
}

#[cfg(feature = "from-toml")]
impl StaticDocument {
    /// Deserializes the root table into a typed value `T`.
    ///
    /// The higher-ranked bound `for<'a> FromToml<'a>` ensures `T` cannot
    /// borrow from the document internals, matching the guarantee of
    /// [`from_str`](crate::from_str).
    ///
    /// # Errors
    ///
    /// Returns [`FromTomlError`](crate::FromTomlError) containing all
    /// accumulated errors.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// let mut doc = toml_spanner::StaticDocument::parse("key = 'value'").unwrap();
    /// let map = doc.to::<HashMap<String, String>>().unwrap();
    /// assert_eq!(map["key"], "value");
    /// ```
    pub fn to<T>(&mut self) -> Result<T, crate::de::FromTomlError>
    where
        T: for<'a> crate::FromToml<'a>,
    {
        let index = std::mem::take(&mut self.index);

        // SAFETY: The index and source are transmuted from erased 'static
        // back to a concrete lifetime matching the arena borrow. All
        // KeyRef entries in the index were created by the parser and point
        // into arena memory which is alive for this entire call.
        //
        // from_toml receives &mut Context, but the index is pub(crate) so
        // external FromToml impls cannot access it. The only crate-internal
        // consumer (TableHelper) reads from the index but never inserts,
        // so the index cannot be corrupted with non-arena pointers.
        //
        // ctx.arena is pub, so FromToml impls can allocate from our arena
        // (harmless, memory stays valid) or swap the reference (harmless,
        // we do not read ctx.arena back). The HRTB bound on T prevents
        // the result from borrowing arena-lifetime data.
        let mut ctx = crate::de::Context {
            arena: &self._arena,
            index: unsafe { std::mem::transmute(index) },
            errors: Vec::new(),
            source: self.source,
        };

        let result = T::from_toml(&mut ctx, self.table.as_item());
        crate::de::compute_paths(&self.table, &mut ctx.errors);

        self.index = unsafe { std::mem::transmute(ctx.index) };

        match result {
            Ok(v) if ctx.errors.is_empty() => Ok(v),
            _ => Err(crate::de::FromTomlError { errors: ctx.errors }),
        }
    }

    /// Deserializes into a value `T` that borrows from the document.
    ///
    /// Unlike [`to()`](Self::to) which requires owned types, this consumes
    /// the document and returns a [`StaticDocumentWith<T>`] where `T` can
    /// contain `&str` references into the parsed TOML data.
    ///
    /// # Errors
    ///
    /// Returns [`FromTomlError`](crate::FromTomlError) on deserialization
    /// failure. The document is consumed regardless.
    pub fn to_borrowed<T>(self) -> Result<StaticDocumentWith<T>, crate::de::FromTomlError>
    where
        T: BorrowedValue,
        for<'a> <T as BorrowedValue>::Borrowed<'a>: crate::FromToml<'a>,
    {
        let StaticDocument {
            table,
            index,
            source,
            _arena,
        } = self;

        // SAFETY: The index contains KeyRef<'static> entries pointing into
        // arena memory. We transmute to the concrete borrow lifetime of
        // &_arena. The arena reference is transmuted to 'static so that
        // from_toml produces Borrowed<'static>, which we then store directly
        // as T (the trait contract requires Borrowed<'static> == Self == T).
        //
        // The same pub(crate) index / pub arena reasoning from to() applies.
        // The arena is moved into StaticDocumentWith, keeping all references
        // valid.
        let (value, errors) = {
            let mut ctx = crate::de::Context {
                arena: unsafe { std::mem::transmute::<&Arena, &'static Arena>(&*_arena) },
                index,
                errors: Vec::new(),
                source,
            };

            let result: Result<<T as BorrowedValue>::Borrowed<'static>, _> =
                crate::FromToml::from_toml(&mut ctx, table.as_item());
            crate::de::compute_paths(&table, &mut ctx.errors);

            let value = match result {
                // SAFETY: borrowed contains references into the arena with
                // erased 'static lifetime. The arena is moved into
                // StaticDocumentWith, keeping them valid. erase() transmutes
                // Borrowed<'static> to Self which are the same type by the
                // BorrowedValue safety contract.
                Ok(borrowed) => Some(unsafe { T::erase(borrowed) }),
                Err(_) => None,
            };
            (value, ctx.errors)
        };

        match value {
            Some(v) if errors.is_empty() => Ok(StaticDocumentWith {
                value: v,
                table,
                _arena,
            }),
            _ => Err(crate::de::FromTomlError { errors }),
        }
    }
}

impl std::fmt::Debug for StaticDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.table.fmt(f)
    }
}

/// Bridges a lifetime-carrying type with its `'static`-erased counterpart
/// for use in [`StaticDocumentWith`].
///
/// Implementors are the `'static` version of a type (e.g. `Foo<'static>`).
/// The associated type [`Borrowed`](Self::Borrowed) is the lifetime-generic
/// version (e.g. `Foo<'a>`).
///
/// Use the [`impl_borrowed_value!`] macro for a sound, checked implementation.
///
/// # Safety
///
/// Implementors must satisfy:
///
/// 1. `Self` is `Type<'static>` for some `Type<'a>` that is **covariant**
///    in `'a`. Invariant types (containing `Cell<&'a T>`, `fn(&'a T)`, etc.)
///    are unsound.
///
/// 2. `Borrowed<'a>` is `Type<'a>` (the same type with lifetime `'a`).
///
/// 3. `Self` and `Borrowed<'a>` have identical memory layout for all `'a`
///    (guaranteed by Rust since lifetimes are erased at runtime).
pub unsafe trait BorrowedValue: Sized {
    /// The borrowed form of this type with lifetime `'a`.
    type Borrowed<'a>
    where
        Self: 'a;

    /// Views `&'a self` (which stores `'static` references) as
    /// `&'a Borrowed<'a>` with the lifetime shortened to match the borrow.
    ///
    /// Sound because covariance guarantees `Type<'static>` is a subtype
    /// of `Type<'a>` when `'static: 'a`.
    fn as_borrowed(&self) -> &Self::Borrowed<'_>;

    /// Views `&'a mut self` as `&'a mut Borrowed<'a>`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no references with a lifetime shorter
    /// than the backing storage are written through the returned
    /// reference. [`StaticDocumentWith::with_value_mut`] enforces this
    /// via a higher-ranked bound.
    unsafe fn as_borrowed_mut(&mut self) -> &mut Self::Borrowed<'_>;

    /// Erases the lifetime of a borrowed value to `'static`.
    ///
    /// # Safety
    ///
    /// The caller must ensure the data referenced by `borrowed` lives
    /// as long as the returned `Self` (typically owned by an arena that
    /// outlives the returned value).
    unsafe fn erase(borrowed: Self::Borrowed<'_>) -> Self;
}

/// An owned TOML document bundled with a deserialized value that borrows
/// from it.
///
/// Created by [`StaticDocument::to_borrowed`]. The value `T` may contain
/// `&str` references into the document's arena. Access the deserialized
/// value via [`value()`](Self::value) and the parsed table via
/// [`table()`](Self::table).
pub struct StaticDocumentWith<T: 'static> {
    value: T,
    table: Table<'static>,
    _arena: Box<Arena>,
}

// SAFETY: Same reasoning as StaticDocument. T: Send is required because
// the value is stored and moved with the struct.
unsafe impl<T: Send + 'static> Send for StaticDocumentWith<T> {}

impl<T: BorrowedValue + 'static> StaticDocumentWith<T> {
    /// Returns a reference to the deserialized value with the correct
    /// (non-`'static`) lifetime.
    ///
    /// The returned `&T::Borrowed<'_>` has its lifetime tied to `&self`,
    /// preventing references inside from escaping the document's lifetime.
    pub fn value(&self) -> &T::Borrowed<'_> {
        self.value.as_borrowed()
    }

    /// Mutates the deserialized value through a closure.
    ///
    /// The higher-ranked bound `for<'a> FnOnce(&'a mut T::Borrowed<'a>)`
    /// prevents writing references shorter than `'static` into the value.
    /// The closure must work for ALL lifetimes `'a`, so only `'static`
    /// references (string literals, leaked allocations) or references
    /// already inside the value (which point into the arena) can be
    /// assigned to reference fields. Non-reference fields can be mutated
    /// freely.
    ///
    /// # Examples
    ///
    /// ```
    /// # use toml_spanner::{impl_borrowed_value, FromToml, Item, Context, Failed};
    /// # struct Cfg<'a> { name: &'a str, port: i64 }
    /// # impl<'de> FromToml<'de> for Cfg<'de> {
    /// #     fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
    /// #         let mut th = item.table_helper(ctx)?;
    /// #         let name = th.required("name")?;
    /// #         let port = th.required("port")?;
    /// #         th.expect_empty()?;
    /// #         Ok(Cfg { name, port })
    /// #     }
    /// # }
    /// # impl_borrowed_value!(Cfg);
    /// let doc = toml_spanner::StaticDocument::parse("name = 'old'\nport = 80").unwrap();
    /// let mut with = doc.to_borrowed::<Cfg<'static>>().unwrap();
    ///
    /// with.with_value_mut(|cfg| {
    ///     cfg.port = 443;
    ///     cfg.name = "updated";  // string literals are 'static, always OK
    /// });
    ///
    /// assert_eq!(with.value().port, 443);
    /// assert_eq!(with.value().name, "updated");
    /// ```
    pub fn with_value_mut<R>(
        &mut self,
        f: impl for<'a> FnOnce(&'a mut <T as BorrowedValue>::Borrowed<'a>) -> R,
    ) -> R {
        // SAFETY: The HRTB bound on f ensures it cannot write references
        // with a lifetime shorter than 'static. Since f must accept
        // &mut Borrowed<'a> for ALL 'a (including 'static), any &'b str
        // it tries to write requires 'b: 'a for all 'a, meaning 'b must
        // be 'static. This prevents storing dangling references.
        //
        // as_borrowed_mut transmutes &mut T (&mut Type<'static>) to
        // &mut Type<'a>. The types have identical layout (lifetimes are
        // erased at runtime) and the covariance safety contract of
        // BorrowedValue guarantees the type structure is compatible.
        let borrowed = unsafe { self.value.as_borrowed_mut() };
        f(borrowed)
    }

    /// Returns a shared reference to the root table.
    pub fn table<'a>(&'a self) -> &'a Table<'a> {
        &self.table
    }
}

impl<T: std::fmt::Debug + 'static> std::fmt::Debug for StaticDocumentWith<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticDocumentWith")
            .field("value", &self.value)
            .field("table", &self.table)
            .finish()
    }
}

/// Implements [`BorrowedValue`] for a type with a single lifetime parameter.
///
/// Generates a compile-time covariance assertion that rejects invariant types
/// (e.g. types containing `Cell<&'a T>` or `fn(&'a T)`).
///
/// # Examples
///
/// ```
/// use toml_spanner::{impl_borrowed_value, FromToml, Item, Context, Failed};
///
/// struct Config<'a> {
///     name: &'a str,
///     values: Vec<&'a str>,
/// }
///
/// impl<'de> FromToml<'de> for Config<'de> {
///     fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
///         let mut th = item.table_helper(ctx)?;
///         let name = th.required("name")?;
///         let values = th.required("values")?;
///         th.expect_empty()?;
///         Ok(Config { name, values })
///     }
/// }
///
/// impl_borrowed_value!(Config);
/// ```
#[macro_export]
macro_rules! impl_borrowed_value {
    ($Type:ident) => {
        const _: () = {
            // Compile-time covariance assertion. If $Type is invariant in its
            // lifetime (e.g. contains Cell<&'a T>), this function fails to
            // compile, catching unsound implementations at build time.
            fn _assert_covariant<'long: 'short, 'short>(x: $Type<'long>) -> $Type<'short> {
                x
            }
        };

        // SAFETY: The const assertion above proves $Type is covariant in its
        // lifetime parameter, making the as_borrowed pointer cast sound.
        // erase() transmutes $Type<'a> to $Type<'static>, which has identical
        // layout since lifetimes are erased at runtime.
        unsafe impl $crate::BorrowedValue for $Type<'static> {
            type Borrowed<'a>
                = $Type<'a>
            where
                Self: 'a;

            fn as_borrowed(&self) -> &$Type<'_> {
                self
            }

            unsafe fn as_borrowed_mut(&mut self) -> &mut $Type<'_> {
                // SAFETY: $Type<'static> and $Type<'a> have identical layout.
                // The covariance assertion above guarantees the type is safe
                // to view at a shorter lifetime.
                unsafe { &mut *(self as *mut Self as *mut $Type<'_>) }
            }

            unsafe fn erase(borrowed: $Type<'_>) -> Self {
                unsafe { ::core::mem::transmute(borrowed) }
            }
        }
    };
}
