#![allow(elided_lifetimes_in_paths)]
#![allow(dead_code)]
#![allow(clippy::manual_find)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::unnecessary_operation)]
mod ast;
mod case;
mod codegen;
mod lit;
mod util;
mod writer;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[derive(Debug)]
struct InnerError {
    span: Span,
    message: String,
}

#[derive(Debug)]
pub(crate) struct Error(InnerError);

unsafe impl Send for Error {}
impl Error {
    pub(crate) fn to_compiler_error(&self, wrap: bool) -> TokenStream {
        let mut toks: Vec<TokenTree> = Vec::new();
        toks.push(TokenTree::Literal(Literal::string(&self.0.message)));
        let mut group = TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            TokenStream::from_iter(toks.drain(..)),
        ));
        let mut punc = TokenTree::Punct(Punct::new('!', Spacing::Alone));
        punc.set_span(self.0.span);
        group.set_span(self.0.span);

        if wrap {
            toks.push(TokenTree::Ident(Ident::new("compile_error", self.0.span)));
            toks.push(punc);
            toks.push(group);
            toks.push(TokenTree::Punct(Punct::new(';', Spacing::Alone)));
            toks.push(TokenTree::Ident(Ident::new("String", self.0.span)));
            toks.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
            toks.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
            toks.push(TokenTree::Ident(Ident::new("new", self.0.span)));
            toks.push(TokenTree::Group(Group::new(
                Delimiter::Parenthesis,
                TokenStream::new(),
            )));
            let inner = TokenStream::from_iter(toks.drain(..));
            toks.push(TokenTree::Group(Group::new(Delimiter::Brace, inner)));
            TokenStream::from_iter(toks.drain(..))
        } else {
            toks.push(TokenTree::Ident(Ident::new("compile_error", self.0.span)));
            toks.push(punc);
            toks.push(group);
            toks.push(TokenTree::Punct(Punct::new(';', Spacing::Alone)));
            TokenStream::from_iter(toks.drain(..))
        }
    }

    pub(crate) fn try_catch_handle(
        ts: TokenStream,
        func: fn(TokenStream) -> TokenStream,
    ) -> TokenStream {
        match std::panic::catch_unwind(move || func(ts)) {
            Ok(e) => e,
            Err(err) => {
                if let Some(value) = err.downcast_ref::<Error>() {
                    value.to_compiler_error(false)
                } else {
                    Error::from_ctx().to_compiler_error(false)
                }
            }
        }
    }
    pub(crate) fn from_ctx() -> Error {
        Error(InnerError {
            span: Span::call_site(),
            message: "Error in context".to_string(),
        })
    }
    pub fn throw(self) -> ! {
        std::panic::panic_any(self);
    }
    pub(crate) fn msg(message: &str) -> ! {
        Error(InnerError {
            span: Span::call_site(),
            message: message.to_string(),
        })
        .throw();
    }

    pub(crate) fn msg_ctx(message: &str, fmt: &dyn std::fmt::Display) -> ! {
        Error(InnerError {
            span: Span::call_site(),
            message: format!("{}: {}", message, fmt),
        })
        .throw();
    }
    pub(crate) fn span_msg(message: &str, span: Span) -> ! {
        Error(InnerError {
            span,
            message: message.to_string(),
        })
        .throw();
    }
    pub(crate) fn span_msg_ctx(message: &str, fmt: &dyn std::fmt::Display, span: Span) -> ! {
        Error(InnerError {
            span,
            message: format!("{}: {}", fmt, message),
        })
        .throw()
    }
}

/// Unified derive macro for implementing TOML conversion traits.
///
/// Configured with attributes in the form `#[toml(...)]` on containers,
/// variants, and fields.
///
/// The traits to derive are specified via container attributes. The currently
/// supported traits are:
/// - [`FromToml`]
/// - [`ToToml`]
///
/// When `#[derive(Toml)]` is used with no trait attribute, it defaults to [`FromToml`].
///
/// The rest of the attributes are described in the following tables. Note that some
/// attributes apply only to certain traits.
///
/// ## Container Attributes
/// These are `toml` attributes that appear above a `struct` or `enum`.
///
/// | Format | Supported Traits | Description |
/// |--------|------------------|-------------|
/// | `content = "..."` | `FromToml`, `ToToml` | Field containing the data content of an adjacently tagged enum. Must be used with `tag`. |
/// | `deny_unknown_fields` | `FromToml` | Unknown keys cause an error and immediately return `Failed`. |
/// | `ignore_unknown_fields` | `FromToml` | Unknown keys are silently ignored (no errors recorded). |
/// | `recoverable` | `FromToml` | Defers field failures to accumulate as many errors as possible. |
/// | `warn_unknown_fields` | `FromToml` | Unknown keys record errors but still construct the value (same as default). |
/// | `from = Type` | `FromToml` | Deserialize by deserializing `Type`, then converting via `From`. |
/// | `rename_all = "..."` | `FromToml`, `ToToml` | Renames variants and fields not explicitly renamed. |
/// | `rename_all_fields = "..."` | `FromToml`, `ToToml` | On enums, overrides `rename_all` for fields in struct variants. |
/// | `tag = "..."` | `FromToml`, `ToToml` | Field containing the enum variant discriminator. |
/// | `transparent` | `FromToml`, `ToToml` | Traits delegate to the single inner type. |
/// | `try_from = Type` | `FromToml` | Deserialize by deserializing `Type`, then converting via `TryFrom`. |
/// | `untagged` | `FromToml`, `ToToml` | Only data content of an enum is stored. |
///
/// ## Enum Variant Attributes
/// These are `toml` attributes that appear above a variant in an enum.
///
/// | Format | Supported Traits | Description |
/// |--------|------------------|-------------|
/// | `final_if = \|ctx, item\| bool` | `FromToml` | Untagged only: skip variant when predicate is false, commit on match. |
/// | `other` | `FromToml` | Catch-all variant for unknown tag values. Unit variant only. |
/// | `rename = "..."` | `FromToml`, `ToToml` | Use provided string as variant name. |
/// | `rename_all = "..."` | `FromToml`, `ToToml` | Renames fields within this variant. Overrides container `rename_all_fields`. |
/// | `try_if = \|ctx, item\| bool` | `FromToml` | Untagged only: skip variant when predicate is false, fall through on failure. |
///
/// ## Field Attributes
/// These are `toml` attributes that appear above a field inside a struct or enum variant.
///
/// | Format | Supported Traits | Description |
/// |--------|------------------|-------------|
/// | `alias = "..."` | `FromToml` | Use provided string as an alternative field name. Can appear multiple times. |
/// | `default [= ...]` | `FromToml` | Use `Default::default()` or provided expression if field is missing. |
/// | `flatten` | `FromToml`, `ToToml` | Flatten the contents of the field into the container it is defined in. |
/// | `rename = "..."` | `FromToml`, `ToToml` | Use provided string as field name. |
/// | `required` | `FromToml` | Field must be present even if the type is `Option<T>`. |
/// | `skip` | `FromToml`, `ToToml` | Omit field while serializing, use default value when deserializing. |
/// | `skip_if = ...` | `ToToml` | Omit field while serializing if provided predicate returns true. |
/// | `style = ...` | `ToToml` | Control serialization style for tables and arrays (`Header`, `Inline`, `Dotted`). |
/// | `with = ...` | `FromToml`, `ToToml` | Use methods from specified module instead of trait. [Read more](#tomlwith---on-fields) |
///
/// ## Trait Aliases
///
/// In the container attributes to specify the traits to derive, and as a prefix on
/// other attributes to specify which traits that attribute should apply to, you can use
/// the following alias to specify multiple traits at once:
///
/// | Alias | Traits Included | Description |
/// |-------|-----------------|-------------|
/// | `From` | `FromToml` | Shorthand for the deserialization trait |
/// | `To` | `ToToml` | Shorthand for the serialization trait |
/// | `Toml` | `FromToml`, `ToToml` | Both deserialization and serialization traits |
///
/// For example, `#[toml(FromToml rename = "old", ToToml rename = "new")]` applies
/// different renames for each direction. An attribute like `#[toml(Toml rename = "x")]`
/// is equivalent to applying `rename = "x"` to both traits.
///
/// If an attribute only supports a subset of the traits specified, the rest are
/// ignored. For example, `#[toml(Toml skip_if = is_empty)]` only affects `ToToml`
/// since `skip_if` is not supported by `FromToml`.
///
/// ### Detailed Field Attribute Descriptions
///
/// #### `#[toml(with = ...)]` on fields
///
/// Uses the functions from the provided module path when a conversion trait method
/// would normally be used.
///
/// The functions correspond to the methods of each trait:
///
/// | Trait | Function |
/// |-------|----------|
/// | [`FromToml`] | `fn from_toml<'de>(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<T, Failed>` |
/// | [`ToToml`] | `fn to_toml(value: &T, arena: &Arena) -> Result<Item, ToTomlError>` |
///
/// For `Option<T>` fields (auto-detected optional), the `with` module operates on
/// the inner type `T`. If the field is annotated with `default` or `required`, the
/// module operates on `Option<T>` directly.
///
/// ##### Example of `with` attribute
/// ```ignore
/// mod bool_as_int {
///     use toml_spanner::{Arena, Context, Failed, Item, ToTomlError};
///
///     pub fn from_toml<'de>(
///         ctx: &mut Context<'de>,
///         item: &Item<'de>,
///     ) -> Result<bool, Failed> {
///         let val = i64::from_toml(ctx, item)?;
///         Ok(val != 0)
///     }
///
///     pub fn to_toml(
///         value: &bool,
///         arena: &Arena,
///     ) -> Result<Item, ToTomlError> {
///         (*value as i64).to_toml(arena)
///     }
/// }
///
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml)]
/// struct Example {
///     #[toml(with = bool_as_int)]
///     value: bool,
/// }
/// ```
///
/// Note: The functions in the with module can be generic.
///
/// When combined with `flatten`, the module instead provides functions matching
/// the [`FromFlattened`]/[`ToFlattened`] trait method signatures:
///
/// ```ignore
/// mod my_helper {
///     use toml_spanner::{Arena, Context, Failed, Item, Key, Table, ToTomlError};
///
///     // FromFlattened equivalent
///     pub fn init() -> Partial { /* ... */ }
///     pub fn insert<'de>(
///         ctx: &mut Context<'de>,
///         key: &Key<'de>,
///         item: &Item<'de>,
///         partial: &mut Partial,
///     ) -> Result<(), Failed> { /* ... */ }
///     pub fn finish<'de>(
///         ctx: &mut Context<'de>,
///         parent: &Table<'de>,
///         partial: Partial,
///     ) -> Result<FieldType, Failed> { /* ... */ }
///
///     // ToFlattened equivalent
///     pub fn to_flattened<'a>(
///         val: &'a FieldType,
///         arena: &'a Arena,
///         table: &mut Table<'a>,
///     ) -> Result<(), ToTomlError> { /* ... */ }
/// }
///
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml)]
/// struct Config {
///     name: String,
///     #[toml(flatten, with = my_helper)]
///     extra_keys: Vec<String>,
/// }
/// ```
///
/// The `Partial` type can be anything. It is inferred from the `init` return type.
///
/// For the common case of flattening a type that already implements
/// [`FromToml`]/[`ToToml`], use the built-in [`flatten_any`] helper instead of writing a
/// custom module (see [Built-in Helpers](#built-in-helpers)).
///
/// #### `#[toml(skip)]`
///
/// Omit the field while serializing and use the default value when deserializing.
/// If no default value is specified with `default` then `Default::default()` is used.
///
/// `skip` is useful when you need a field on the Rust side that is not present in
/// the TOML data.
///
/// #### `#[toml(flatten)]`
///
/// Captures all unrecognized keys from the table into a map-like field. The field
/// type must implement [`FromFlattened`] (for deserialization) and/or [`ToFlattened`]
/// (for serialization). Built-in implementations exist for `BTreeMap<K, V>` and
/// `HashMap<K, V>`.
///
/// At most one flatten field is allowed per struct or variant.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml)]
/// struct Config {
///     name: String,
///     #[toml(flatten)]
///     extras: BTreeMap<String, String>,
/// }
/// ```
///
/// Combine `flatten` with `with = path` to use custom logic or to flatten types
/// that implement [`FromToml`]/[`ToToml`] rather than the flatten traits. The built-in
/// [`flatten_any`] module handles the common case.
///
/// #### `#[toml(default [= ...])]`
///
/// For non-`Option` types, use `Default::default()` or the provided expression
/// when the field is missing from the TOML input.
///
/// For `Option<T>` fields, the auto-detection as optional can be overridden:
///
/// ```ignore
/// // Optional (auto-detected): missing -> None, `with` produces T
/// bar: Option<u32>,
///
/// // Default: missing -> Default::default() (None), `with` produces Option<T>
/// #[toml(default)]
/// bar: Option<u32>,
///
/// // Default with custom value: missing -> Some(42), `with` produces Option<T>
/// #[toml(default = Some(42))]
/// bar: Option<u32>,
///
/// // Required: missing -> error, `with` produces Option<T>
/// #[toml(required)]
/// bar: Option<u32>,
/// ```
///
/// #### `#[toml(style = ...)]`
///
/// Controls how table-valued or array-valued fields are rendered during
/// serialization. Has no effect on deserialization.
///
/// Accepted values: `Header`, `Inline`, `Dotted`, `Implicit`
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(ToToml)]
/// struct Config {
///     #[toml(style = Header)]
///     server: ServerSection,
///     #[toml(style = Inline)]
///     point: Point,
/// }
/// ```
///
/// When applied to a `Vec<T>` of tables, `Header` emits `[[key]]` sections while
/// `Inline` emits an inline array.
///
/// #### `#[toml(skip_if = ...)]`
///
/// Omit the field if the provided predicate function returns true. The predicate
/// can be specified by a path or inline using closure syntax. The predicate is
/// provided the current field value via reference.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(ToToml)]
/// struct Example {
///     #[toml(skip_if = str::is_empty)]
///     value: String,
///     #[toml(skip_if = |s| *s == u32::MAX)]
///     sentinel: u32,
/// }
/// ```
///
/// ### Detailed Container Attribute Descriptions
///
/// #### `#[toml(transparent)]`
///
/// Must be used on a struct containing a single field. The conversion traits
/// delegate directly to the inner field's implementation.
///
/// #### `#[toml(rename_all = "...")]`
///
/// The possible values are `"lowercase"`, `"UPPERCASE"`, `"PascalCase"`,
/// `"camelCase"`, `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"kebab-case"`,
/// `"SCREAMING-KEBAB-CASE"`.
///
/// On a struct, this renames all fields. On an enum, this renames both variant
/// names and field names within struct variants. Use `rename_all_fields` to apply a
/// different rule to fields than to variant names.
///
/// #### `#[toml(rename_all_fields = "...")]`
///
/// Accepts the same values as `rename_all`. Only meaningful on enums: overrides
/// `rename_all` for fields in struct variants while leaving variant names
/// unaffected.
///
/// Per-variant `#[toml(rename_all = "...")]` overrides both the container
/// `rename_all` and `rename_all_fields` for that variant's fields.
///
/// #### Unknown field policies
///
/// By default, unrecognized keys are recorded as errors in the deserialization
/// [`Context`] but do not cause [`Failed`] to be returned (warn behavior). This
/// allows multiple problems to be reported at once while still constructing
/// the value.
///
/// `#[toml(warn_unknown_fields)]` explicitly selects the default warn behavior.
/// This is useful when combined with an error tag (see below).
///
/// `#[toml(deny_unknown_fields)]` makes unknown keys fatal: the first
/// unrecognized key records an error and immediately returns [`Failed`].
///
/// `#[toml(ignore_unknown_fields)]` silently discards unknown keys without
/// recording any errors.
///
/// Both `warn_unknown_fields` and `deny_unknown_fields` support an optional
/// error tag in brackets. The tag is stored in the [`UnexpectedKey { tag }`](https://docs.rs/toml-spanner/latest/toml_spanner/enum.ErrorKind.html#variant.UnexpectedKey)
/// error variant and can be used for programmatic filtering or attaching
/// additional diagnostics.
///
/// ```ignore
/// const MY_TAG: u32 = 42;
///
/// #[derive(Toml)]
/// #[toml(FromToml, warn_unknown_fields[MY_TAG])]
/// struct Config {
///     name: String,
/// }
/// ```
///
/// #### `recoverable`
///
/// By default, the first required field that fails deserialization immediately
/// returns [`Failed`]. Adding `recoverable` continues through all remaining
/// fields, reporting as many errors as possible including missing fields.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, recoverable)]
/// struct ServerConfig {
///     host: String,
///     port: u16,
///     debug: bool,
/// }
/// ```
///
/// Avoid using on structs that serve as variants of an `untagged` enum, as
/// untagged deserialization relies on early failure to distinguish variants.
///
/// #### `#[toml(from = Type)]` / `#[toml(try_from = Type)]`
///
/// Instead of deserializing each field individually, the macro deserializes a proxy
/// type and converts the result. `from` uses `From<Type>` and `try_from` uses
/// `TryFrom<Type>` (error type must implement `Display`).
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, from = RawConfig)]
/// struct AppConfig {
///     label: String,
///     port: u16,
/// }
///
/// impl From<RawConfig> for AppConfig {
///     fn from(raw: RawConfig) -> Self {
///         AppConfig {
///             label: raw.name.to_uppercase(),
///             port: raw.port,
///         }
///     }
/// }
/// ```
///
/// These attributes apply to `FromToml` only. Normal `ToToml` generation is
/// unaffected and can be combined freely.
///
/// ### Enum Representations
///
/// #### Default enum representation for TOML
///
/// For all-unit enums, the variant serializes as a plain string value.
/// For mixed enums, the variant name becomes a table key (external tagging).
///
/// <table width="100%">
/// <tr><td width="47%">Enum</td><td>TOML for Example::Alpha</td><td>TOML for Example::Beta</td></tr><tr><td>
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml)]
/// enum Example {
///     Alpha {
///         field: bool
///     },
///     Beta
/// }
/// ```
///
/// </td><td>
///
/// ```toml
/// [Alpha]
/// field = true
/// ```
///
/// </td><td>
///
/// ```toml
/// "Beta"
/// ```
///
/// </td></tr></table>
///
/// #### Enum with `#[toml(tag = "...")]`
///
/// May not be used with `untagged`. The tag field is stored alongside the
/// variant's own fields (internal tagging). Tuple variants are not supported.
///
/// <table width="100%">
/// <tr><td width="47%">Enum</td><td>TOML for Example::Alpha</td><td>TOML for Example::Beta</td></tr><tr><td>
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml,
///     tag = "kind")]
/// enum Example {
///     Alpha {
///         field: bool
///     },
///     Beta
/// }
/// ```
///
/// </td><td>
///
/// ```toml
/// kind = "Alpha"
/// field = true
/// ```
///
/// </td><td>
///
/// ```toml
/// kind = "Beta"
/// ```
///
/// </td></tr></table>
///
/// #### Enum with `#[toml(tag = "...", content = "...")]`
///
/// May not be used with `untagged`. The `content` attribute must be used with
/// `tag`. Supports unit, struct, and tuple variants (adjacent tagging).
///
/// <table width="100%">
/// <tr><td width="47%">Enum</td><td>TOML for Example::Alpha</td><td>TOML for Example::Beta</td></tr><tr><td>
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml,
///     tag = "kind",
///     content = "data")]
/// enum Example {
///     Alpha {
///         field: bool
///     },
///     Beta
/// }
/// ```
///
/// </td><td>
///
/// ```toml
/// kind = "Alpha"
///
/// [data]
/// field = true
/// ```
///
/// </td><td>
///
/// ```toml
/// kind = "Beta"
/// ```
///
/// </td></tr></table>
///
/// #### Enum with `#[toml(untagged)]`
///
/// Variants are distinguished by structure, not a tag field. Deserialization tries
/// each variant in declaration order until one succeeds. Errors from failed
/// attempts are automatically cleaned up.
///
/// <table width="100%">
/// <tr><td width="47%">Enum</td><td>TOML for Example::Variant</td></tr><tr><td>
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml,
///     untagged)]
/// enum Example {
///     Variant {
///         field: bool
///     }
/// }
/// ```
///
/// </td><td>
///
/// ```toml
/// field = true
/// ```
///
/// </td></tr></table>
///
/// ##### Branch hints (`try_if`, `final_if`)
///
/// For untagged enums, branch hints control which variants are attempted based on a
/// predicate over the input item. The predicate receives a [`Context`] reference and
/// an [`Item`] reference and returns `bool`.
///
/// **`try_if`**: If the predicate returns false, the variant is skipped. On true,
/// deserialization is attempted and on failure the next variant is tried.
///
/// **`final_if`**: If the predicate returns false, the variant is skipped. On true,
/// the variant is committed to and errors propagate immediately.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, untagged)]
/// enum Flexible {
///     #[toml(final_if = |_ctx, item| item.kind() == toml_spanner::Kind::Boolean)]
///     Flag(bool),
///     #[toml(try_if = |_ctx, item| item.kind() == toml_spanner::Kind::Integer)]
///     Num(i64),
///     Text(String),
/// }
/// ```
///
/// ### Built-in Helpers
///
/// The [`toml_spanner::helper`] module provides modules for use with the `with` field
/// attribute. These cover common patterns so you do not need to write custom
/// conversion functions.
///
/// #### [`parse_string`]
///
/// Deserializes a TOML string into any type implementing `FromStr`.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml)]
/// struct Server {
///     #[toml(with = parse_string)]
///     addr: IpAddr,
/// }
/// ```
///
/// #### [`display`]
///
/// Serializes any type implementing `Display` as a TOML string.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(ToToml)]
/// struct Server {
///     #[toml(with = display)]
///     addr: IpAddr,
/// }
/// ```
///
/// For full round-trip support, use trait-scoped `with`:
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(FromToml, ToToml)]
/// struct Server {
///     #[toml(FromToml with = parse_string, ToToml with = display)]
///     addr: IpAddr,
/// }
/// ```
///
/// #### [`flatten_any`]
///
/// Flattens any type implementing [`FromToml`]/[`ToToml`] into a parent struct,
/// without writing a manual [`FromFlattened`]/[`ToFlattened`] implementation.
///
/// ```ignore
/// #[derive(Toml)]
/// #[toml(Toml)]
/// struct Labeled {
///     name: String,
///     #[toml(flatten, with = flatten_any)]
///     point: Point,
/// }
/// ```
///
/// [`flatten_any`] works by collecting unrecognized key-value pairs into a temporary
/// table and passing it through the regular [`FromToml`]/[`ToToml`] path.
///
/// ## Why a single unified derive macro?
///
/// By convention, derive macros are typically named after the trait they implement.
/// The unified `#[derive(Toml)]` approach reduces compilation time because it:
///
/// 1. Avoids the overhead of multiple derive macro invocations.
/// 2. Needs to parse the input only once.
/// 3. Allows traits to share code, as the macro knows the full set being implemented.
///
/// [`FromToml`]: https://docs.rs/toml-spanner/latest/toml_spanner/trait.FromToml.html
/// [`ToToml`]: https://docs.rs/toml-spanner/latest/toml_spanner/trait.ToToml.html
/// [`FromFlattened`]: https://docs.rs/toml-spanner/latest/toml_spanner/trait.FromFlattened.html
/// [`ToFlattened`]: https://docs.rs/toml-spanner/latest/toml_spanner/trait.ToFlattened.html
/// [`Context`]: https://docs.rs/toml-spanner/latest/toml_spanner/struct.Context.html
/// [`Item`]: https://docs.rs/toml-spanner/latest/toml_spanner/struct.Item.html
/// [`Failed`]: https://docs.rs/toml-spanner/latest/toml_spanner/struct.Failed.html
/// [`toml_spanner::helper`]: https://docs.rs/toml-spanner/latest/toml_spanner/helper/index.html
/// [`flatten_any`]: https://docs.rs/toml-spanner/latest/toml_spanner/helper/flatten_any/index.html
/// [`parse_string`]: https://docs.rs/toml-spanner/latest/toml_spanner/helper/parse_string/index.html
/// [`display`]: https://docs.rs/toml-spanner/latest/toml_spanner/helper/display/index.html
#[proc_macro_derive(Toml, attributes(toml))]
pub fn derive_toml(input: TokenStream) -> TokenStream {
    codegen::derive(input)
}
