use crate::{case::RenameRule, util::Allocator, Error};

use proc_macro2::{Delimiter, Ident, Literal, Span, TokenStream, TokenTree};

#[cfg_attr(feature = "debug", derive(Debug))]
pub enum GenericKind {
    Lifetime,
    Type,
    Const,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Generic<'a> {
    pub kind: GenericKind,
    pub ident: &'a Ident,
    pub bounds: &'a [TokenTree],
}
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum UnknownFieldPolicy {
    Warn { tag: Option<Vec<TokenTree>> },
    Ignore,
    Deny { tag: Option<Vec<TokenTree>> },
}

impl UnknownFieldPolicy {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Warn { tag: None })
    }

    pub fn tag(&self) -> Option<&[TokenTree]> {
        match self {
            Self::Warn { tag } | Self::Deny { tag } => tag.as_deref(),
            Self::Ignore => None,
        }
    }
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub enum DeriveTargetKind {
    TupleStruct,
    Struct,
    Enum,
}

#[cfg_attr(feature = "debug", derive(Debug))]
struct FieldAttr {
    enabled: TraitSet,
    #[allow(dead_code)]
    span: Span,
    inner: FieldAttrInner,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub enum DefaultKind {
    Default,
    Custom(Vec<TokenTree>),
}

#[cfg_attr(feature = "debug", derive(Debug))]
enum FieldAttrInner {
    Rename(Literal),
    Default(DefaultKind),
    Skip(Vec<TokenTree>),
    With(Vec<TokenTree>),
    Flatten,
    Alias(Literal),
    DeprecatedAlias {
        tag: Option<Vec<TokenTree>>,
        alias: Literal,
    },
    Style(Ident),
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub struct FieldAttrs {
    attrs: Vec<FieldAttr>,
    flags: u64,
}

impl FieldAttrs {
    pub fn rename(&self, for_trait: TraitSet) -> Option<&Literal> {
        // rename is at slot 0
        if self.flags & for_trait as u64 == 0 {
            return None;
        }
        for attr in &self.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Rename(lit) = &attr.inner {
                    return Some(lit);
                }
            }
        }
        None
    }
    pub fn has_aliases(&self, for_trait: TraitSet) -> bool {
        for attr in &self.attrs {
            if attr.enabled & for_trait != 0
                && matches!(
                    attr.inner,
                    FieldAttrInner::Alias(_) | FieldAttrInner::DeprecatedAlias { .. }
                )
            {
                return true;
            }
        }
        false
    }
    pub fn for_each_alias(&self, for_trait: TraitSet, f: &mut dyn FnMut(&Literal)) {
        for attr in &self.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Alias(lit) = &attr.inner {
                    f(lit);
                }
            }
        }
    }
    pub fn has_deprecated_aliases(&self, for_trait: TraitSet) -> bool {
        for attr in &self.attrs {
            if attr.enabled & for_trait != 0
                && matches!(attr.inner, FieldAttrInner::DeprecatedAlias { .. })
            {
                return true;
            }
        }
        false
    }
    pub fn for_each_deprecated_alias(
        &self,
        for_trait: TraitSet,
        f: &mut dyn FnMut(Option<&[TokenTree]>, &Literal),
    ) {
        for attr in &self.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::DeprecatedAlias { ref tag, ref alias } = attr.inner {
                    f(tag.as_deref(), alias);
                }
            }
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for FieldAttrs {
    fn default() -> Self {
        Self {
            attrs: Default::default(),
            flags: 0,
        }
    }
}

type EnumFlag = u8;
pub const ENUM_CONTAINS_UNIT_VARIANT: u8 = 0b0000_0001;
pub const ENUM_CONTAINS_STRUCT_VARIANT: u8 = 0b0000_0010;
pub const ENUM_CONTAINS_TUPLE_VARIANT: u8 = 0b0000_0100;

#[cfg_attr(feature = "debug", derive(Debug))]
pub struct DeriveTargetInner<'a> {
    pub name: Ident,
    pub generics: Vec<Generic<'a>>,
    pub where_clauses: &'a [TokenTree],
    pub path_override: Option<Literal>,
    pub generic_field_types: Vec<&'a [TokenTree]>,
    pub generic_flatten_field_types: Vec<&'a [TokenTree]>,
    pub transparent_impl: bool,
    pub from_toml: bool,
    pub to_toml: bool,
    pub rename_all: RenameRule,
    pub rename_all_fields: RenameRule,
    pub enum_flags: EnumFlag,
    pub tag: Option<Literal>,
    pub content: Option<Literal>,
    pub untagged: bool,
    pub from_type: Option<Vec<TokenTree>>,
    pub try_from_type: Option<Vec<TokenTree>>,
    pub unknown_fields: UnknownFieldPolicy,
    pub recoverable: bool,
}

impl<'a> DeriveTargetInner<'a> {
    pub fn has_lifetime(&self) -> bool {
        for g in &self.generics {
            if matches!(g.kind, GenericKind::Lifetime) {
                return true;
            }
        }
        false
    }
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Field<'a> {
    pub name: &'a Ident,
    pub ty: &'a [TokenTree],
    #[allow(dead_code)]
    pub attr: &'a FieldAttrs,
    pub flags: u32,
}

impl<'a> Field<'a> {
    pub fn default(&self, for_trait: TraitSet) -> Option<&DefaultKind> {
        for attr in &self.attr.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Default(tokens) = &attr.inner {
                    return Some(tokens);
                }
            }
        }
        None
    }
    pub fn skip(&self, for_trait: TraitSet) -> Option<&[TokenTree]> {
        for attr in &self.attr.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Skip(skip) = &attr.inner {
                    return Some(skip);
                }
            }
        }
        None
    }
    pub fn with(&self, for_trait: TraitSet) -> Option<&[TokenTree]> {
        for attr in &self.attr.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::With(with) = &attr.inner {
                    return Some(with);
                }
            }
        }
        None
    }
    pub fn style(&self, for_trait: TraitSet) -> Option<&Ident> {
        for attr in &self.attr.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Style(ident) = &attr.inner {
                    return Some(ident);
                }
            }
        }
        None
    }
}

impl<'a> Field<'a> {
    pub const GENERIC: u32 = 1u32 << 0;
    pub const IN_TUPLE: u32 = 1u32 << 1;
    pub const WITH_FROM_TOML_DEFAULT: u32 = 1u32 << 2;
    pub const WITH_FROM_TOML_SKIP: u32 = 1u32 << 3;
    pub const WITH_TO_TOML_SKIP: u32 = 1u32 << 4;
    pub const WITH_FLATTEN: u32 = 1u32 << 5;
    pub const WITH_FROM_TOML_OPTION: u32 = 1u32 << 6;
    #[allow(dead_code)]
    pub fn is(&self, flags: u32) -> bool {
        self.flags & flags != 0
    }
    #[allow(dead_code)]
    pub fn is_all(&self, flags: u32) -> bool {
        self.flags & flags == flags
    }
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub struct EnumVariant<'a> {
    pub name: &'a Ident,
    pub fields: &'a [Field<'a>],
    pub kind: EnumKind,
    #[allow(dead_code)]
    pub attr: &'a FieldAttrs,
    pub rename_all: RenameRule,
    pub try_if: Option<Vec<TokenTree>>,
    pub final_if: Option<Vec<TokenTree>>,
    pub other: bool,
    buf_start: usize,
    buf_end: usize,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub enum EnumKind {
    Tuple,
    Struct,
    None,
}

impl Copy for EnumKind {}
impl Clone for EnumKind {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> EnumVariant<'a> {
    pub fn rename(&self, for_trait: TraitSet) -> Option<&Literal> {
        for attr in &self.attr.attrs {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Rename(lit) = &attr.inner {
                    return Some(lit);
                }
            }
        }
        None
    }
}
//@DELETE_START
macro_rules! throw {
    ($literal: literal @ $span: expr, $($tt:tt)*) => {
        Error::span_msg_ctx($literal, &($($tt)*), $span)
    };
    ($literal: literal, $($tt:tt)*) => {
        Error::msg_ctx($literal, &($($tt)*))
    };
    ($literal: literal @ $span: expr) => {
        Error::span_msg($literal, $span)
    };
    ($literal: literal) => {
        Error::msg($literal)
    };
}

macro_rules! expect_next {
    ($ident:ident, $expr:expr) => {
        match ($expr).next() {
            Some(TokenTree::$ident(t)) => t,
            Some(tt) => Error::span_msg_ctx(
                concat!("Expected a ", stringify!($ident), "but found a "),
                &kind_of_token(tt),
                tt.span(),
            ),
            None => Error::msg("Unexpected EOF"),
        }
    };
}

macro_rules! next {
    ($expr:expr) => {
        match ($expr).next() {
            Some(t) => t,
            None => Error::msg("Unexpected EOF"),
        }
    };
}

fn kind_of_token(token: &TokenTree) -> &'static str {
    match token {
        TokenTree::Group(_) => "Group",
        TokenTree::Ident(_) => "Ident",
        TokenTree::Punct(_) => "Punct",
        TokenTree::Literal(_) => "Literal",
    }
}

// helps reduce LLVM bloat but keeping the drop in here
// of the string here
fn ident_eq(ident: &Ident, value: &str) -> bool {
    ident.to_string() == value
}
fn parse_unknown_fields_tag(value: &mut [TokenTree]) -> Option<Vec<TokenTree>> {
    if let Some(TokenTree::Group(group)) = value.first() {
        if group.delimiter() == Delimiter::Bracket {
            let tokens: Vec<_> = group.stream().into_iter().collect();
            return Some(tokens);
        }
    }
    None
}

fn parse_container_attr(
    target: &mut DeriveTargetInner<'_>,
    attr: Ident,
    mut value: &mut [TokenTree],
) {
    let key = attr.to_string();
    match key.as_str() {
        "transparent" => {
            target.transparent_impl = true;
        }
        "FromToml" => {
            target.from_toml = true;
        }
        "ToToml" => {
            target.to_toml = true;
        }
        "Toml" => {
            target.from_toml = true;
            target.to_toml = true;
        }
        "rename_all" => {
            if target.rename_all != RenameRule::None {
                throw!("Duplicate rename_all attribute" @ attr.span())
            }
            let [TokenTree::Literal(rename), rest @ ..] = value else {
                throw!("Expected a literal" @ attr.span())
            };
            value = rest;
            target.rename_all = RenameRule::from_literal(rename);
        }
        "tag" => {
            if target.tag.is_some() {
                throw!("Duplicate tag attribute" @ attr.span())
            }
            let [TokenTree::Literal(tag_lit), rest @ ..] = value else {
                throw!("Expected a string literal for tag" @ attr.span())
            };
            value = rest;
            target.tag = Some(tag_lit.clone());
        }
        "content" => {
            if target.content.is_some() {
                throw!("Duplicate content attribute" @ attr.span())
            }
            let [TokenTree::Literal(content_lit), rest @ ..] = value else {
                throw!("Expected a string literal for content" @ attr.span())
            };
            value = rest;
            target.content = Some(content_lit.clone());
        }
        "rename_all_fields" => {
            if target.rename_all_fields != RenameRule::None {
                throw!("Duplicate rename_all_fields attribute" @ attr.span())
            }
            let [TokenTree::Literal(rename), rest @ ..] = value else {
                throw!("Expected a literal" @ attr.span())
            };
            value = rest;
            target.rename_all_fields = RenameRule::from_literal(rename);
        }
        "untagged" => {
            if target.untagged {
                throw!("Duplicate untagged attribute" @ attr.span())
            }
            target.untagged = true;
        }
        "from" => {
            if target.from_type.is_some() {
                throw!("Duplicate from attribute" @ attr.span())
            }
            if value.is_empty() {
                throw!("Expected a type for from" @ attr.span())
            }
            target.from_type = Some(value.to_vec());
            value = &mut [];
        }
        "try_from" => {
            if target.try_from_type.is_some() {
                throw!("Duplicate try_from attribute" @ attr.span())
            }
            if value.is_empty() {
                throw!("Expected a type for try_from" @ attr.span())
            }
            target.try_from_type = Some(value.to_vec());
            value = &mut [];
        }
        "deny_unknown_fields" => {
            if !target.unknown_fields.is_default() {
                throw!("Duplicate unknown fields policy attribute" @ attr.span())
            }
            let tag = parse_unknown_fields_tag(value);
            value = &mut [];
            target.unknown_fields = UnknownFieldPolicy::Deny { tag };
        }
        "warn_unknown_fields" => {
            if !target.unknown_fields.is_default() {
                throw!("Duplicate unknown fields policy attribute" @ attr.span())
            }
            let tag = parse_unknown_fields_tag(value);
            value = &mut [];
            target.unknown_fields = UnknownFieldPolicy::Warn { tag };
        }
        "ignore_unknown_fields" => {
            if !target.unknown_fields.is_default() {
                throw!("Duplicate unknown fields policy attribute" @ attr.span())
            }
            target.unknown_fields = UnknownFieldPolicy::Ignore;
        }
        "recoverable" => {
            if target.recoverable {
                throw!("Duplicate recoverable attribute" @ attr.span())
            }
            target.recoverable = true;
        }
        _ => throw!("Unknown attribute" @ attr.span()),
    }
    if !value.is_empty() {
        throw!("Extra value tokens for" @ attr.span(), attr)
    }
}

fn extract_container_attr(target: &mut DeriveTargetInner<'_>, stream: TokenStream) {
    let mut toks = stream.into_iter();
    let Some(TokenTree::Ident(ident)) = toks.next() else {
        return;
    };
    let Some(TokenTree::Group(group)) = toks.next() else {
        return;
    };
    let name = ident.to_string();
    if name == "toml" {
        parse_attrs(group.stream(), &mut |_, attr, value| {
            parse_container_attr(target, attr, value);
        });
    }
}

pub fn extract_derive_target<'a>(
    target: &mut DeriveTargetInner<'a>,
    toks: &'a [TokenTree],
) -> (DeriveTargetKind, TokenStream) {
    let mut toks = toks.iter();
    let kind = loop {
        let ident = match next!(toks) {
            TokenTree::Ident(ident) => ident,
            TokenTree::Punct(ch) if ch.as_char() == '#' => {
                extract_container_attr(target, expect_next!(Group, toks).stream());
                continue;
            }
            _ => continue,
        };
        let ident = ident.to_string();
        match ident.as_str() {
            "struct" => break DeriveTargetKind::Struct,
            "enum" => break DeriveTargetKind::Enum,
            _ => continue,
        }
    };

    target.name = expect_next!(Ident, toks).clone();

    match toks.next() {
        Some(TokenTree::Group(group)) => {
            return (
                if group.delimiter() == Delimiter::Parenthesis {
                    DeriveTargetKind::TupleStruct
                } else {
                    kind
                },
                group.stream(),
            );
        }
        Some(TokenTree::Punct(ch)) if ch.as_char() == '<' => (),
        Some(TokenTree::Punct(ch)) if ch.as_char() == ';' => {
            return (kind, TokenStream::new());
        }
        None => throw!("Empty body"),
        f => throw!("Unhandled feature", f.unwrap()),
    }
    'parsing_generics: while let Some(tt) = toks.next() {
        let mut keep = true;
        let (kind, ident, at_colon) = match tt {
            TokenTree::Ident(ident) => match next!(toks) {
                TokenTree::Group(_) => throw!("Unexpected group"),
                TokenTree::Ident(next_ident) => {
                    if ident_eq(ident, "const") {
                        throw!("unexpected ident", &next_ident)
                    }
                    (GenericKind::Const, next_ident, false)
                }
                TokenTree::Punct(ch) => match ch.as_char() as u8 {
                    b':' => (GenericKind::Type, ident, true),
                    b'=' => {
                        keep = false;
                        (GenericKind::Type, ident, true)
                    }
                    b',' => {
                        target.generics.push(Generic {
                            kind: GenericKind::Type,
                            ident,
                            bounds: &[],
                        });
                        continue;
                    }
                    b'>' => {
                        target.generics.push(Generic {
                            kind: GenericKind::Type,
                            ident,
                            bounds: &[],
                        });
                        break 'parsing_generics;
                    }
                    chr => {
                        throw!("Unexpected token after first ident in generic", chr)
                    }
                },
                tok => {
                    throw!("Unexpected token after first ident in generic", tok)
                }
            },
            TokenTree::Punct(p) => {
                let ch = p.as_char();
                if ch == '\'' {
                    match next!(toks) {
                        TokenTree::Ident(ident) => (GenericKind::Lifetime, ident, false),
                        _ => {
                            throw!("expected ident")
                        }
                    }
                } else {
                    if ch == ',' {
                        continue;
                    }
                    if ch == '>' {
                        break 'parsing_generics;
                    }
                    throw!("Unexpected Punct")
                }
            }
            TokenTree::Group(_) => {
                throw!("Unhanlded");
            }
            _ => throw!("Unhanlded"),
        };
        target.generics.push(Generic {
            kind,
            ident,
            bounds: &[],
        });
        let Some(generic) = target.generics.last_mut() else {
            // Due to using cargo-expand we can't use the unreachable macro here
            unsafe { std::hint::unreachable_unchecked() }
        };
        if !at_colon {
            // could have we attr
            match next!(toks) {
                TokenTree::Punct(ch) => match ch.as_char() {
                    ',' => {
                        continue;
                    }
                    '>' => {
                        break 'parsing_generics;
                    }

                    ':' => (),
                    _ => throw!("unexpected char"),
                },
                _ => throw!("Unexpected tok"),
            }
        }
        let from = toks.as_slice();
        let mut depth = 0i32;
        loop {
            let tok = next!(toks);
            if let TokenTree::Punct(p) = &tok {
                match p.as_char() as u8 {
                    b',' => break,
                    b'=' => {
                        if depth == 0 {
                            generic.bounds = if keep {
                                &from[..(from.len() - toks.len()) - 1]
                            } else {
                                &[]
                            };
                            keep = false;
                        }
                    }
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth < 0 {
                            if keep {
                                generic.bounds = &from[..(from.len() - toks.len()) - 1];
                            }
                            break 'parsing_generics;
                        }
                    }
                    _ => (),
                }
            }
        }
        if keep {
            generic.bounds = &from[..(from.len() - toks.len()) - 1];
        }
    }
    match next!(toks) {
        TokenTree::Group(group) => (
            if group.delimiter() == Delimiter::Parenthesis {
                DeriveTargetKind::TupleStruct
            } else {
                kind
            },
            group.stream(),
        ),
        TokenTree::Ident(tok) => {
            if ident_eq(tok, "where") {
                throw!("Expected where clause" @ tok.span());
            }
            let [where_clauses @ .., TokenTree::Group(group)] = toks.as_slice() else {
                throw!("Expected body after where clauses")
            };
            target.where_clauses = where_clauses;
            (
                if group.delimiter() == Delimiter::Parenthesis {
                    DeriveTargetKind::TupleStruct
                } else {
                    kind
                },
                group.stream(),
            )
        }
        tok => throw!("Expected either body or where clause", tok),
    }
}

const TRAIT_COUNT: u64 = 2;
const OPTION_AUTO_DETECTED: u64 = 1u64 << 32;
fn parse_single_field_attr(
    attrs: &mut FieldAttrs,
    mut trait_set: TraitSet,
    ident: Ident,
    value: &mut Vec<TokenTree>,
) {
    let name = ident.to_string();
    if trait_set == 0 {
        trait_set = FROM_TOML | TO_TOML;
    }
    let offset = match name.as_str() {
        "rename" => {
            let Some(TokenTree::Literal(rename)) = value.pop() else {
                throw!("Expected a literal" @ ident.span())
            };
            if !value.is_empty() {
                throw!("Unexpected a single literal" @ ident.span())
            }
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Rename(rename),
            });
            0
        }
        "default" => {
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Default(if value.is_empty() {
                    DefaultKind::Default
                } else {
                    DefaultKind::Custom(std::mem::take(value))
                }),
            });
            TRAIT_COUNT
        }
        "skip" => {
            if !value.is_empty() {
                throw!("skip doesn't take any arguments" @ ident.span())
            }
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Skip(Vec::new()),
            });
            2u64 * TRAIT_COUNT
        }
        "skip_if" => {
            if value.is_empty() {
                throw!("Expected a function specifying skip criteria" @ ident.span())
            }
            trait_set &= TO_TOML;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set & TO_TOML,
                span: ident.span(),
                inner: FieldAttrInner::Skip(std::mem::take(value)),
            });
            2u64 * TRAIT_COUNT
        }
        "with" => {
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::With(std::mem::take(value)),
            });
            3u64 * TRAIT_COUNT
        }
        "flatten" => {
            if !value.is_empty() {
                throw!("flatten doesn't take any arguments" @ ident.span())
            }
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Flatten,
            });
            4u64 * TRAIT_COUNT
        }
        "style" => {
            let Some(TokenTree::Ident(style_ident)) = value.pop() else {
                throw!("Expected a style name (Header, Inline, Dotted, Implicit)" @ ident.span())
            };
            if !value.is_empty() {
                throw!("Expected a single style name" @ ident.span())
            }
            let style_name = style_ident.to_string();
            match style_name.as_str() {
                "Header" | "Inline" | "Dotted" | "Implicit" => (),
                _ => {
                    throw!("Unknown style, expected Header, Inline, Dotted, or Implicit" @ style_ident.span())
                }
            }
            trait_set &= TO_TOML;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set & TO_TOML,
                span: ident.span(),
                inner: FieldAttrInner::Style(style_ident),
            });
            5u64 * TRAIT_COUNT
        }
        "required" => {
            if !value.is_empty() {
                throw!("required doesn't take any arguments" @ ident.span())
            }
            trait_set &= FROM_TOML;
            // Same slot as default: mutually exclusive and prevents Option auto-detection
            TRAIT_COUNT
        }
        "alias" => {
            let Some(TokenTree::Literal(alias)) = value.pop() else {
                throw!("Expected a literal" @ ident.span())
            };
            if !value.is_empty() {
                throw!("Expected a single literal" @ ident.span())
            }
            trait_set &= FROM_TOML;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Alias(alias),
            });
            // Return early: alias can appear multiple times on the same field
            return;
        }
        "deprecated_alias" => {
            let Some(TokenTree::Literal(alias)) = value.pop() else {
                throw!("Expected a string literal" @ ident.span())
            };
            let tag = if let Some(TokenTree::Group(group)) = value.pop() {
                if group.delimiter() != Delimiter::Bracket {
                    throw!("Expected bracket group for tag" @ ident.span())
                }
                if !value.is_empty() {
                    throw!("Unexpected tokens" @ ident.span())
                }
                Some(group.stream().into_iter().collect::<Vec<_>>())
            } else {
                if !value.is_empty() {
                    throw!("Unexpected tokens" @ ident.span())
                }
                None
            };
            trait_set &= FROM_TOML;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::DeprecatedAlias { tag, alias },
            });
            return;
        }
        _ => throw!("Unknown attr field" @ ident.span()),
    };
    let mask = (trait_set as u64) << offset;
    if attrs.flags & mask != 0 {
        throw!("Duplicate attribute" @ ident.span())
    }
    attrs.flags |= mask;
}

fn extract_toml_attr(group: TokenStream) -> Option<TokenStream> {
    let mut toks = group.into_iter();
    {
        let Some(TokenTree::Ident(ident)) = toks.next() else {
            return None;
        };
        if !ident_eq(&ident, "toml") {
            return None;
        }
    }
    let Some(TokenTree::Group(group)) = toks.next() else {
        return None;
    };
    Some(group.stream())
}

pub type TraitSet = u8;
pub const FROM_TOML: TraitSet = 1 << 0;
pub const TO_TOML: TraitSet = 1 << 1;
fn parse_attrs(toks: TokenStream, func: &mut dyn FnMut(TraitSet, Ident, &mut Vec<TokenTree>)) {
    let mut toks = toks.into_iter();
    let mut buf: Vec<TokenTree> = Vec::new();
    'outer: while let Some(tok) = toks.next() {
        let TokenTree::Ident(mut ident) = tok else {
            throw!("Expected ident" @ tok.span())
        };
        let mut trait_set = 0;
        'processing: loop {
            if let Some(sep) = toks.next() {
                let sep = match sep {
                    TokenTree::Punct(sep) => sep,
                    TokenTree::Ident(true_ident) => {
                        let text = ident.to_string();
                        match text.as_str() {
                            "FromToml" => trait_set |= FROM_TOML,
                            "ToToml" => trait_set |= TO_TOML,
                            "Toml" => trait_set |= FROM_TOML | TO_TOML,
                            _ => throw!("Expected trait or alias" @ ident.span()),
                        }
                        ident = true_ident;
                        continue 'processing;
                    }
                    TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => {
                        buf.push(TokenTree::Group(group));
                        let Some(next) = toks.next() else { break };
                        let TokenTree::Punct(p) = next else {
                            throw!("Expected `=` or `,`" @ next.span());
                        };
                        p
                    }
                    _ => {
                        throw!("Expected either `=` or `,`" @ sep.span());
                    }
                };
                match sep.as_char() {
                    '=' => (),
                    ',' => {
                        func(trait_set, ident, &mut buf);
                        continue 'outer;
                    }
                    _ => throw!("Expected either `=` or `,`" @ sep.span()),
                }
                let mut in_pipe = false;
                for tok in toks.by_ref() {
                    if let TokenTree::Punct(punct) = &tok {
                        if punct.as_char() == '|' {
                            in_pipe ^= true;
                        }
                        if punct.as_char() == ',' && !in_pipe {
                            break;
                        }
                    }
                    buf.push(tok);
                }
            }
            break;
        }
        func(trait_set, ident, &mut buf);
        buf.clear();
    }
}

fn ensure_attr<'a, 'b>(
    opt: &'b mut Option<&'a mut FieldAttrs>,
    buf: &mut Allocator<'a, FieldAttrs>,
) -> &'b mut &'a mut FieldAttrs {
    if opt.is_none() {
        *opt = Some(buf.alloc_default());
    }
    opt.as_mut().unwrap()
}

fn parse_field_attr<'a>(
    current: &mut Option<&'a mut FieldAttrs>,
    attr_buf: &mut Allocator<'a, FieldAttrs>,
    toks: TokenStream,
) {
    let Some(attrs) = extract_toml_attr(toks) else {
        return;
    };
    let attr = ensure_attr(current, attr_buf);
    parse_attrs(attrs, &mut |set, ident, buf| {
        parse_single_field_attr(attr, set, ident, buf)
    })
}

fn option_inner_ty_ast(ty: &[TokenTree]) -> &[TokenTree] {
    if let [TokenTree::Ident(id), _, inner @ .., TokenTree::Punct(close)] = ty {
        if id.to_string() == "Option" && close.as_char() == '>' {
            return inner;
        }
    }
    ty
}

pub fn scan_fields<'a>(target: &mut DeriveTargetInner<'a>, fields: &mut Vec<Field<'a>>) {
    let has_type_generics = target
        .generics
        .iter()
        .any(|g| matches!(g.kind, GenericKind::Type));
    if !has_type_generics {
        return;
    }

    for field in fields {
        for tt in field.ty {
            let TokenTree::Ident(ident) = tt else {
                continue;
            };
            let ident_str = ident.to_string();
            let is_generic = target
                .generics
                .iter()
                .any(|g| matches!(g.kind, GenericKind::Type) && g.ident.to_string() == ident_str);
            if !is_generic {
                continue;
            }

            field.flags |= Field::GENERIC;
            if field.flags & Field::WITH_FLATTEN != 0 {
                if field.with(FROM_TOML).is_none() && field.with(TO_TOML).is_none() {
                    target.generic_flatten_field_types.push(field.ty);
                }
            } else {
                let ty = if field.flags & Field::WITH_FROM_TOML_OPTION != 0 {
                    option_inner_ty_ast(field.ty)
                } else {
                    field.ty
                };
                target.generic_field_types.push(ty);
            }
            break;
        }
    }
}

pub fn parse_enum<'a>(
    target: &mut DeriveTargetInner<'a>,
    fields: &'a [TokenTree],
    tt_buf: &'a mut Vec<TokenTree>,
    field_buf: &'a mut Vec<Field<'a>>,
    attr_buf: &mut Allocator<'a, FieldAttrs>,
) -> Vec<EnumVariant<'a>> {
    let mut variants = parse_inner_enum_variants(fields, tt_buf, attr_buf);
    for variant in &mut variants {
        match variant.kind {
            EnumKind::Tuple => {
                target.enum_flags |= ENUM_CONTAINS_TUPLE_VARIANT;
                let start = field_buf.len();
                parse_tuple_fields(
                    variant.name,
                    field_buf,
                    &tt_buf[variant.buf_start..variant.buf_end],
                    attr_buf,
                );
                variant.buf_start = start;
                variant.buf_end = field_buf.len();
            }
            EnumKind::Struct => {
                target.enum_flags |= ENUM_CONTAINS_STRUCT_VARIANT;
                let start = field_buf.len();
                parse_struct_fields(
                    field_buf,
                    &tt_buf[variant.buf_start..variant.buf_end],
                    attr_buf,
                );
                variant.buf_start = start;
                variant.buf_end = field_buf.len();
            }
            EnumKind::None => {
                target.enum_flags |= ENUM_CONTAINS_UNIT_VARIANT;
            }
        }
    }
    scan_fields(target, field_buf);
    for variant in &mut variants {
        if !matches!(variant.kind, EnumKind::None) {
            variant.fields = &field_buf[variant.buf_start..variant.buf_end];
        }
    }
    variants
}

fn parse_inner_enum_variants<'a>(
    fields: &'a [TokenTree],
    tt_buffer: &mut Vec<TokenTree>,
    attr_buffer: &mut Allocator<'a, FieldAttrs>,
) -> Vec<EnumVariant<'a>> {
    let mut f = fields.iter().enumerate();
    let mut enums: Vec<EnumVariant<'a>> = Vec::new();
    let mut next_attr: Option<&'a mut FieldAttrs> = None;
    let mut next_rename_all = RenameRule::None;
    let mut next_try_if: Option<Vec<TokenTree>> = None;
    let mut next_final_if: Option<Vec<TokenTree>> = None;
    let mut next_other = false;
    loop {
        let i = if let Some((i, tok)) = f.next() {
            let TokenTree::Punct(punct) = tok else {
                continue;
            };
            let ch = punct.as_char() as u8;
            if ch == b'#' {
                let Some((_, TokenTree::Group(group))) = f.next() else {
                    throw!("Expected attr after" @ punct.span())
                };
                if let Some(attrs) = extract_toml_attr(group.stream()) {
                    parse_attrs(attrs, &mut |set, ident, buf| {
                        let name = ident.to_string();
                        if name == "rename_all" {
                            let Some(TokenTree::Literal(rename)) = buf.pop() else {
                                throw!("Expected a literal" @ ident.span())
                            };
                            next_rename_all = RenameRule::from_literal(&rename);
                            return;
                        }
                        if name == "try_if" {
                            if next_try_if.is_some() {
                                throw!("Duplicate try_if attribute" @ ident.span())
                            }
                            if next_final_if.is_some() {
                                throw!("Cannot combine try_if and final_if on the same variant" @ ident.span())
                            }
                            if buf.is_empty() {
                                throw!("try_if requires a predicate" @ ident.span())
                            }
                            next_try_if = Some(std::mem::take(buf));
                            return;
                        }
                        if name == "final_if" {
                            if next_final_if.is_some() {
                                throw!("Duplicate final_if attribute" @ ident.span())
                            }
                            if next_try_if.is_some() {
                                throw!("Cannot combine try_if and final_if on the same variant" @ ident.span())
                            }
                            if buf.is_empty() {
                                throw!("final_if requires a predicate" @ ident.span())
                            }
                            next_final_if = Some(std::mem::take(buf));
                            return;
                        }
                        if name == "other" {
                            next_other = true;
                            return;
                        }
                        let attr = ensure_attr(&mut next_attr, attr_buffer);
                        parse_single_field_attr(attr, set, ident, buf);
                    });
                }
                continue;
            }
            if ch == b',' {
            } else if ch == b'=' {
                let mut colon_stage = 0;
                while let Some((_, tok)) = f.next() {
                    let TokenTree::Punct(punct) = tok else {
                        continue;
                    };
                    match punct.as_char() as u8 {
                        b':' => {
                            colon_stage += 1;
                            continue;
                        }
                        b'<' => {
                            if colon_stage == 2 {
                                let mut depth = 1i32;
                                loop {
                                    let Some((_e, tok)) = f.next() else {
                                        throw!(
                                            "Unexpected EOF while parsing type in enum expression"
                                        );
                                    };
                                    let TokenTree::Punct(punct) = tok else {
                                        continue;
                                    };
                                    match punct.as_char() as u8 {
                                        b'<' => depth += 1,
                                        b'>' => {
                                            depth -= 1;
                                            if depth <= 0 {
                                                break;
                                            }
                                        }
                                        _ => continue,
                                    }
                                }
                            }
                        }
                        b',' => break,
                        _ => (),
                    }
                    colon_stage = 0;
                }
            } else {
                continue;
            };
            i
        } else {
            fields.len()
        };

        let Some(tok) = fields.get(i.saturating_sub(1)) else {
            throw!("Baddness")
        };

        let start = tt_buffer.len();
        let (name, kind) = match tok {
            TokenTree::Group(group) => {
                tt_buffer.extend(group.stream());
                let Some(TokenTree::Ident(ident)) = fields.get(i.saturating_sub(2)) else {
                    throw!("Expected ident" @ group.span())
                };
                (
                    ident,
                    if group.delimiter() == Delimiter::Brace {
                        EnumKind::Struct
                    } else {
                        EnumKind::Tuple
                    },
                )
            }
            TokenTree::Ident(ident) => (ident, EnumKind::None),
            tok => throw!("Expected either an ident or group" @ tok.span()),
        };
        enums.push(EnumVariant {
            name,
            fields: &[],
            buf_start: start,
            buf_end: tt_buffer.len(),
            attr: if let Some(attr) = next_attr.take() {
                attr
            } else {
                &DEFAULT_ATTR.0
            },
            kind,
            rename_all: std::mem::replace(&mut next_rename_all, RenameRule::None),
            try_if: next_try_if.take(),
            final_if: next_final_if.take(),
            other: std::mem::replace(&mut next_other, false),
        });
        if f.len() == 0 {
            break;
        }
    }
    enums
}

fn flags_from_attr(attr: &Option<&mut FieldAttrs>) -> u32 {
    let mut f = 0;
    if let Some(attr) = attr {
        if attr.flags & OPTION_AUTO_DETECTED != 0 {
            f |= Field::WITH_FROM_TOML_OPTION;
        }
        for attr in &attr.attrs {
            match &attr.inner {
                FieldAttrInner::Default(..) => {
                    if attr.enabled & FROM_TOML != 0 {
                        f |= Field::WITH_FROM_TOML_DEFAULT;
                    }
                }
                FieldAttrInner::Skip(ref tokens) => {
                    // Only set unconditional skip flags when there's no condition
                    if tokens.is_empty() {
                        if attr.enabled & FROM_TOML != 0 {
                            f |= Field::WITH_FROM_TOML_SKIP;
                        }
                        if attr.enabled & TO_TOML != 0 {
                            f |= Field::WITH_TO_TOML_SKIP;
                        }
                    }
                }
                FieldAttrInner::Flatten => {
                    f |= Field::WITH_FLATTEN;
                }
                _ => (),
            }
        }
    }
    f
}

pub fn parse_tuple_fields<'a>(
    fake_name: &'a Ident,
    output: &mut Vec<Field<'a>>,
    fields: &'a [TokenTree],
    attr_buf: &mut Allocator<'a, FieldAttrs>,
) {
    let mut f = fields.iter().enumerate();
    let mut next_attr: Option<&mut FieldAttrs> = None;
    while let Some((mut i, tok)) = f.next() {
        if let TokenTree::Punct(punct) = tok {
            if punct.as_char() == '#' {
                let Some((_, TokenTree::Group(group))) = f.next() else {
                    throw!("Expected attr after" @ punct.span())
                };
                parse_field_attr(&mut next_attr, attr_buf, group.stream());
                continue;
            }
        };
        let mut depth = 0i32;
        let end = loop {
            let Some((e, tok)) = f.next() else {
                break fields.len();
            };
            let TokenTree::Punct(punct) = tok else {
                continue;
            };
            match punct.as_char() as u8 {
                b'<' => depth += 1,
                b'>' => depth -= 1,
                b',' if depth <= 0 => break e,
                _ => continue,
            }
        };

        // Remove visibility to store just the type
        if let TokenTree::Ident(ident) = &fields[i] {
            if ident_eq(ident, "pub") {
                i += 1;
                if i + 1 != end {
                    if let TokenTree::Group(group) = &fields[i] {
                        if group.delimiter() == Delimiter::Parenthesis {
                            i += 1;
                        }
                    }
                }
            }
        }

        output.push(Field {
            name: fake_name,
            ty: &fields[i..end],
            flags: Field::IN_TUPLE | flags_from_attr(&next_attr),
            attr: if let Some(attr) = next_attr.take() {
                attr
            } else {
                &DEFAULT_ATTR.0
            },
        })
    }
}

struct DefaultFieldAttr(FieldAttrs);
unsafe impl Sync for DefaultFieldAttr {}
static DEFAULT_ATTR: DefaultFieldAttr = const {
    DefaultFieldAttr(FieldAttrs {
        attrs: Vec::new(),
        flags: 0,
    })
};

pub fn parse_struct_fields<'a>(
    output: &mut Vec<Field<'a>>,
    fields: &'a [TokenTree],
    attr_buf: &mut Allocator<'a, FieldAttrs>,
) {
    let mut f = fields.iter().enumerate();
    let mut next_attr: Option<&'a mut FieldAttrs> = None;
    while let Some((i, tok)) = f.next() {
        let TokenTree::Punct(punct) = tok else {
            continue;
        };
        let ch = punct.as_char() as u8;
        if ch == b'#' {
            let Some((_, TokenTree::Group(group))) = f.next() else {
                throw!("Expected attr after" @ punct.span())
            };
            parse_field_attr(&mut next_attr, attr_buf, group.stream());
            continue;
        }
        if ch != b':' {
            continue;
        }
        let Some(TokenTree::Ident(name)) = fields.get(i.wrapping_sub(1)) else {
            throw!("Expected field name before :" @ punct.span())
        };
        let mut depth = 0i32;
        let end = loop {
            let Some((e, tok)) = f.next() else {
                break fields.len();
            };
            let TokenTree::Punct(punct) = tok else {
                continue;
            };
            match punct.as_char() as u8 {
                b'<' => depth += 1,
                b'>' => depth -= 1,
                b',' if depth <= 0 => break e,
                _ => continue,
            }
        };
        if let [TokenTree::Ident(ident), TokenTree::Punct(punct), ..] = &fields[i + 1..end] {
            if punct.as_char() == '<' && ident_eq(ident, "Option") {
                let attr = ensure_attr(&mut next_attr, attr_buf);
                let oo_mask_item = (FROM_TOML as u64) << (TRAIT_COUNT);
                if attr.flags & oo_mask_item == 0 {
                    attr.flags |= OPTION_AUTO_DETECTED;
                    attr.attrs.push(FieldAttr {
                        enabled: FROM_TOML,
                        span: ident.span(),
                        inner: FieldAttrInner::Default(DefaultKind::Default),
                    });
                }
            }
        }
        output.push(Field {
            name,
            ty: &fields[i + 1..end],
            flags: Field::IN_TUPLE | flags_from_attr(&next_attr),
            attr: if let Some(attr) = next_attr.take() {
                attr
            } else {
                &DEFAULT_ATTR.0
            },
        })
    }
}
