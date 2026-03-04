
use crate::{case::RenameRule, util::Allocator, Error};
use proc_macro::{Delimiter, Ident, Literal, Span, TokenStream, TokenTree};
pub enum GenericKind {
    Lifetime,
    Type,
    Const,
}
pub struct Generic<'a> {
    pub kind: GenericKind,
    pub ident: &'a Ident,
    pub bounds: &'a [TokenTree],
}
pub enum DeriveTargetKind {
    TupleStruct,
    Struct,
    Enum,
}
struct FieldAttr {
    enabled: TraitSet,
    #[allow(dead_code)]
    span: Span,
    inner: FieldAttrInner,
}
pub enum DefaultKind {
    Default,
    Custom(Vec<TokenTree>),
}
enum FieldAttrInner {
    Rename(Literal),
    Default(DefaultKind),
    Skip(Vec<TokenTree>),
    With(Vec<TokenTree>),
    Flatten,
    Alias(Literal),
}
pub struct FieldAttrs {
    attrs: Vec<FieldAttr>,
    flags: u64,
}
impl FieldAttrs {
    pub fn rename(&self, for_trait: TraitSet) -> Option<&Literal> {
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
        self.attrs.iter().any(|attr| {
            attr.enabled & for_trait != 0
                && match attr.inner {
                    FieldAttrInner::Alias(_) => true,
                    _ => false,
                }
        })
    }
    pub fn aliases(&self, for_trait: TraitSet) -> impl Iterator<Item = &Literal> + '_ {
        self.attrs.iter().filter_map(move |attr| {
            if attr.enabled & for_trait != 0 {
                if let FieldAttrInner::Alias(lit) = &attr.inner {
                    return Some(lit);
                }
            }
            None
        })
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
pub struct DeriveTargetInner<'a> {
    pub name: Ident,
    pub generics: Vec<Generic<'a>>,
    pub where_clauses: &'a [TokenTree],
    pub path_override: Option<Literal>,
    pub generic_field_types: Vec<&'a [TokenTree]>,
    pub generic_flatten_field_types: Vec<&'a [TokenTree]>,
    pub transparent_impl: bool,
    pub from_item: bool,
    pub to_item: bool,
    pub rename_all: RenameRule,
    pub rename_all_fields: RenameRule,
    pub enum_flags: EnumFlag,
    pub tag: Option<Literal>,
    pub content: Option<Literal>,
    pub untagged: bool,
}
impl<'a> DeriveTargetInner<'a> {
    pub fn has_lifetime(&self) -> bool {
        self.generics.iter().any(|x| match x.kind {
            GenericKind::Lifetime => true,
            _ => false,
        })
    }
}
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
}
impl<'a> Field<'a> {
    pub const GENERIC: u32 = 1u32 << 0;
    pub const IN_TUPLE: u32 = 1u32 << 1;
    pub const WITH_FROMITEM_DEFAULT: u32 = 1u32 << 2;
    pub const WITH_FROMITEM_SKIP: u32 = 1u32 << 3;
    pub const WITH_TO_ITEM_SKIP: u32 = 1u32 << 4;
    pub const WITH_FLATTEN: u32 = 1u32 << 5;
    pub const WITH_FROMITEM_OPTION: u32 = 1u32 << 6;
    #[allow(dead_code)]
    pub fn is(&self, flags: u32) -> bool {
        self.flags & flags != 0
    }
    #[allow(dead_code)]
    pub fn is_all(&self, flags: u32) -> bool {
        self.flags & flags == flags
    }
}
pub struct EnumVariant<'a> {
    pub name: &'a Ident,
    pub fields: &'a [Field<'a>],
    pub kind: EnumKind,
    #[allow(dead_code)]
    pub attr: &'a FieldAttrs,
    pub rename_all: RenameRule,
    pub try_if: Option<Vec<TokenTree>>,
    pub final_if: Option<Vec<TokenTree>>,
}
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
fn kind_of_token(token: &TokenTree) -> &'static str {
    match token {
        TokenTree::Group(_) => "Group",
        TokenTree::Ident(_) => "Ident",
        TokenTree::Punct(_) => "Punct",
        TokenTree::Literal(_) => "Literal",
    }
}
fn ident_eq(ident: &Ident, value: &str) -> bool {
    ident.to_string() == value
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
        "FromItem" => {
            target.from_item = true;
        }
        "ToItem" => {
            target.to_item = true;
        }
        "Toml" => {
            target.from_item = true;
            target.to_item = true;
        }
        "rename_all" => {
            if target.rename_all != RenameRule::None {
                Error::span_msg("Duplicate rename_all attribute", attr.span())
            }
            let [TokenTree::Literal(rename), rest @ ..] = value else {
                Error::span_msg("Expected a literal", attr.span())
            };
            value = rest;
            target.rename_all = RenameRule::from_literal(&rename);
        }
        "tag" => {
            if target.tag.is_some() {
                Error::span_msg("Duplicate tag attribute", attr.span())
            }
            let [TokenTree::Literal(tag_lit), rest @ ..] = value else {
                Error::span_msg("Expected a string literal for tag", attr.span())
            };
            value = rest;
            target.tag = Some(tag_lit.clone());
        }
        "content" => {
            if target.content.is_some() {
                Error::span_msg("Duplicate content attribute", attr.span())
            }
            let [TokenTree::Literal(content_lit), rest @ ..] = value else {
                Error::span_msg("Expected a string literal for content", attr.span())
            };
            value = rest;
            target.content = Some(content_lit.clone());
        }
        "rename_all_fields" => {
            if target.rename_all_fields != RenameRule::None {
                Error::span_msg("Duplicate rename_all_fields attribute", attr.span())
            }
            let [TokenTree::Literal(rename), rest @ ..] = value else {
                Error::span_msg("Expected a literal", attr.span())
            };
            value = rest;
            target.rename_all_fields = RenameRule::from_literal(&rename);
        }
        "untagged" => {
            if target.untagged {
                Error::span_msg("Duplicate untagged attribute", attr.span())
            }
            target.untagged = true;
        }
        _ => Error::span_msg("Unknown attribute", attr.span()),
    }
    if !value.is_empty() {
        Error::span_msg_ctx("Extra value tokens for", &(attr), attr.span())
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
        let ident = match match (toks).next() {
            Some(t) => t,
            None => Error::msg("Unexpected EOF"),
        } {
            TokenTree::Ident(ident) => ident,
            TokenTree::Punct(ch) if ch.as_char() == '#' => {
                extract_container_attr(
                    target,
                    match (toks).next() {
                        Some(TokenTree::Group(t)) => t,
                        Some(tt) => Error::span_msg_ctx(
                            "Expected a Groupbut found a ",
                            &kind_of_token(&tt),
                            tt.span(),
                        ),
                        None => Error::msg("Unexpected EOF"),
                    }
                    .stream(),
                );
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
    target.name = match (toks).next() {
        Some(TokenTree::Ident(t)) => t,
        Some(tt) => Error::span_msg_ctx(
            "Expected a Identbut found a ",
            &kind_of_token(&tt),
            tt.span(),
        ),
        None => Error::msg("Unexpected EOF"),
    }
    .clone();
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
        None => Error::msg("Empty body"),
        f => Error::msg_ctx("Unhandled feature", &(f.unwrap())),
    }
    'parsing_generics: while let Some(tt) = toks.next() {
        let mut keep = true;
        let (kind, ident, at_colon) = match tt {
            TokenTree::Ident(ident) => match match (toks).next() {
                Some(t) => t,
                None => Error::msg("Unexpected EOF"),
            } {
                TokenTree::Group(_) => Error::msg("Unexpected group"),
                TokenTree::Ident(next_ident) => {
                    if ident_eq(ident, "const") {
                        Error::msg_ctx("unexpected ident", &(&next_ident))
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
                    chr => Error::msg_ctx("Unexpected token after first ident in generic", &(chr)),
                },
                tok => Error::msg_ctx("Unexpected token after first ident in generic", &(tok)),
            },
            TokenTree::Punct(p) => {
                let ch = p.as_char();
                if ch == '\'' {
                    match match (toks).next() {
                        Some(t) => t,
                        None => Error::msg("Unexpected EOF"),
                    } {
                        TokenTree::Ident(ident) => (GenericKind::Lifetime, ident, false),
                        _ => Error::msg("expected ident"),
                    }
                } else {
                    if ch == ',' {
                        continue;
                    }
                    if ch == '>' {
                        break 'parsing_generics;
                    }
                    Error::msg("Unexpected Punct")
                }
            }
            TokenTree::Group(_) => {
                Error::msg("Unhanlded");
            }
            _ => Error::msg("Unhanlded"),
        };
        target.generics.push(Generic {
            kind,
            ident,
            bounds: &[],
        });
        let Some(generic) = target.generics.last_mut() else {
            unsafe { std::hint::unreachable_unchecked() }
        };
        if !at_colon {
            match match (toks).next() {
                Some(t) => t,
                None => Error::msg("Unexpected EOF"),
            } {
                TokenTree::Punct(ch) => match ch.as_char() {
                    ',' => {
                        continue;
                    }
                    '>' => {
                        break 'parsing_generics;
                    }
                    ':' => (),
                    _ => Error::msg("unexpected char"),
                },
                _ => Error::msg("Unexpected tok"),
            }
        }
        let from = toks.as_slice();
        let mut depth = 0i32;
        loop {
            let tok = match (toks).next() {
                Some(t) => t,
                None => Error::msg("Unexpected EOF"),
            };
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
    match match (toks).next() {
        Some(t) => t,
        None => Error::msg("Unexpected EOF"),
    } {
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
                Error::span_msg("Expected where clause", tok.span());
            }
            let [where_clauses @ .., TokenTree::Group(group)] = toks.as_slice() else {
                Error::msg("Expected body after where clauses")
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
        tok => Error::msg_ctx("Expected either body or where clause", &(tok)),
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
        trait_set = FROM_ITEM | TO_ITEM;
    }
    let offset = match name.as_str() {
        "rename" => {
            let Some(TokenTree::Literal(rename)) = value.pop() else {
                Error::span_msg("Expected a literal", ident.span())
            };
            if !value.is_empty() {
                Error::span_msg("Unexpected a single literal", ident.span())
            }
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Rename(rename),
            });
            0u64 * TRAIT_COUNT
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
            1u64 * TRAIT_COUNT
        }
        "skip" => {
            if !value.is_empty() {
                Error::span_msg("skip doesn't take any arguments", ident.span())
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
                Error::span_msg("Expected a function specifying skip criteria", ident.span())
            }
            trait_set &= TO_ITEM;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set & TO_ITEM,
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
                Error::span_msg("flatten doesn't take any arguments", ident.span())
            }
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Flatten,
            });
            4u64 * TRAIT_COUNT
        }
        "required" => {
            if !value.is_empty() {
                Error::span_msg("required doesn't take any arguments", ident.span())
            }
            trait_set &= FROM_ITEM;
            1u64 * TRAIT_COUNT
        }
        "alias" => {
            let Some(TokenTree::Literal(alias)) = value.pop() else {
                Error::span_msg("Expected a literal", ident.span())
            };
            if !value.is_empty() {
                Error::span_msg("Expected a single literal", ident.span())
            }
            trait_set &= FROM_ITEM;
            attrs.attrs.push(FieldAttr {
                enabled: trait_set,
                span: ident.span(),
                inner: FieldAttrInner::Alias(alias),
            });
            return;
        }
        _ => Error::span_msg("Unknown attr field", ident.span()),
    };
    let mask = (trait_set as u64) << offset;
    if attrs.flags & mask != 0 {
        Error::span_msg("Duplicate attribute", ident.span())
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
pub const FROM_ITEM: TraitSet = 1 << 0;
pub const TO_ITEM: TraitSet = 1 << 1;
fn parse_attrs(toks: TokenStream, func: &mut dyn FnMut(TraitSet, Ident, &mut Vec<TokenTree>)) {
    let mut toks = toks.into_iter();
    let mut buf: Vec<TokenTree> = Vec::new();
    'outer: while let Some(tok) = toks.next() {
        let TokenTree::Ident(mut ident) = tok else {
            Error::span_msg("Expected ident", tok.span())
        };
        let mut trait_set = 0;
        'processing: loop {
            if let Some(sep) = toks.next() {
                let sep = match sep {
                    TokenTree::Punct(sep) => sep,
                    TokenTree::Ident(true_ident) => {
                        let text = ident.to_string();
                        match text.as_str() {
                            "FromItem" => trait_set |= FROM_ITEM,
                            "ToItem" => trait_set |= TO_ITEM,
                            "Toml" => trait_set |= FROM_ITEM | TO_ITEM,
                            _ => Error::span_msg("Expected trait or alias", ident.span()),
                        }
                        ident = true_ident;
                        continue 'processing;
                    }
                    _ => {
                        Error::span_msg("Expected either `=` or `,`", sep.span());
                    }
                };
                match sep.as_char() {
                    '=' => (),
                    ',' => {
                        func(trait_set, ident, &mut buf);
                        continue 'outer;
                    }
                    _ => Error::span_msg("Expected either `=` or `,`", sep.span()),
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
fn parse_field_attr<'a>(
    current: &mut Option<&'a mut FieldAttrs>,
    attr_buf: &mut Allocator<'a, FieldAttrs>,
    toks: TokenStream,
) {
    let Some(attrs) = extract_toml_attr(toks) else {
        return;
    };
    let attr = current.get_or_insert_with(|| attr_buf.alloc_default());
    parse_attrs(attrs, &mut |set, ident, buf| {
        parse_single_field_attr(attr, set, ident, buf)
    })
}
struct VariantTemp<'a> {
    name: &'a Ident,
    start: usize,
    end: usize,
    attr: &'a FieldAttrs,
    kind: EnumKind,
    rename_all: RenameRule,
    try_if: Option<Vec<TokenTree>>,
    final_if: Option<Vec<TokenTree>>,
}
pub fn scan_fields<'a>(target: &mut DeriveTargetInner<'a>, fields: &mut Vec<Field<'a>>) {
    let type_generic_names: Vec<_> = target
        .generics
        .iter()
        .filter_map(|a| {
            if !match a.kind {
                GenericKind::Type => true,
                _ => false,
            } {
                return None;
            }
            Some(a.ident.to_string())
        })
        .collect();
    let min_len = type_generic_names.iter().map(|x| x.len()).max();
    for field in fields {
        if let Some(min_length) = min_len {
            for tt in field.ty {
                let TokenTree::Ident(ident) = tt else {
                    continue;
                };
                let ident = ident.to_string();
                if ident.len() < min_length {
                    continue;
                }
                if type_generic_names.iter().any(|x| ident == *x) {
                    field.flags |= Field::GENERIC;
                    if field.flags & Field::WITH_FLATTEN != 0 {
                        target.generic_flatten_field_types.push(field.ty);
                    } else {
                        target.generic_field_types.push(field.ty);
                    }
                    break;
                }
            }
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
    let mut temp = parse_inner_enum_variants(fields, tt_buf, attr_buf);
    {
        for variant in &mut temp {
            match variant.kind {
                EnumKind::Tuple => {
                    target.enum_flags |= ENUM_CONTAINS_TUPLE_VARIANT;
                    let start = field_buf.len();
                    parse_tuple_fields(
                        variant.name,
                        field_buf,
                        &tt_buf[variant.start..variant.end],
                        attr_buf,
                    );
                    variant.start = start;
                    variant.end = field_buf.len();
                }
                EnumKind::Struct => {
                    target.enum_flags |= ENUM_CONTAINS_STRUCT_VARIANT;
                    let start = field_buf.len();
                    parse_struct_fields(field_buf, &tt_buf[variant.start..variant.end], attr_buf);
                    variant.start = start;
                    variant.end = field_buf.len();
                }
                EnumKind::None => {
                    target.enum_flags |= ENUM_CONTAINS_UNIT_VARIANT;
                }
            }
        }
    }
    scan_fields(target, field_buf);
    temp.into_iter()
        .map(|var| EnumVariant {
            name: var.name,
            fields: if match var.kind {
                EnumKind::None => true,
                _ => false,
            } {
                &[]
            } else {
                &field_buf[var.start..var.end]
            },
            kind: var.kind,
            rename_all: var.rename_all,
            attr: var.attr,
            try_if: var.try_if,
            final_if: var.final_if,
        })
        .collect()
}
fn parse_inner_enum_variants<'a>(
    fields: &'a [TokenTree],
    tt_buffer: &mut Vec<TokenTree>,
    attr_buffer: &mut Allocator<'a, FieldAttrs>,
) -> Vec<VariantTemp<'a>> {
    let mut f = fields.iter().enumerate();
    let mut enums: Vec<VariantTemp<'a>> = Vec::new();
    let mut next_attr: Option<&'a mut FieldAttrs> = None;
    let mut next_rename_all = RenameRule::None;
    let mut next_try_if: Option<Vec<TokenTree>> = None;
    let mut next_final_if: Option<Vec<TokenTree>> = None;
    loop {
        let i = if let Some((i, tok)) = f.next() {
            let TokenTree::Punct(punct) = tok else {
                continue;
            };
            let ch = punct.as_char() as u8;
            if ch == b'#' {
                let Some((_, TokenTree::Group(group))) = f.next() else {
                    Error::span_msg("Expected attr after", punct.span())
                };
                if let Some(attrs) = extract_toml_attr(group.stream()) {
                    parse_attrs(attrs, &mut |set, ident, buf| {
                        let name = ident.to_string();
                        if name == "rename_all" {
                            let Some(TokenTree::Literal(rename)) = buf.pop() else {
                                Error::span_msg("Expected a literal", ident.span())
                            };
                            next_rename_all = RenameRule::from_literal(&rename);
                            return;
                        }
                        if name == "try_if" {
                            if next_try_if.is_some() {
                                Error::span_msg("Duplicate try_if attribute", ident.span())
                            }
                            if next_final_if.is_some() {
                                Error::span_msg(
                                    "Cannot combine try_if and final_if on the same variant",
                                    ident.span(),
                                )
                            }
                            if buf.is_empty() {
                                Error::span_msg("try_if requires a predicate", ident.span())
                            }
                            next_try_if = Some(std::mem::take(buf));
                            return;
                        }
                        if name == "final_if" {
                            if next_final_if.is_some() {
                                Error::span_msg("Duplicate final_if attribute", ident.span())
                            }
                            if next_try_if.is_some() {
                                Error::span_msg(
                                    "Cannot combine try_if and final_if on the same variant",
                                    ident.span(),
                                )
                            }
                            if buf.is_empty() {
                                Error::span_msg("final_if requires a predicate", ident.span())
                            }
                            next_final_if = Some(std::mem::take(buf));
                            return;
                        }
                        let attr = next_attr.get_or_insert_with(|| attr_buffer.alloc_default());
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
                                        Error::msg(
                                            "Unexpected EOF while parsing type in enum expression",
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
            Error::msg("Baddness")
        };
        let start = tt_buffer.len();
        let (name, kind) = match tok {
            TokenTree::Group(group) => {
                tt_buffer.extend(group.stream());
                let Some(TokenTree::Ident(ident)) = fields.get(i.saturating_sub(2)) else {
                    Error::span_msg("Expected ident", group.span())
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
            tok => Error::span_msg("Expected either an ident or group", tok.span()),
        };
        enums.push(VariantTemp {
            name,
            start,
            end: tt_buffer.len(),
            attr: if let Some(attr) = next_attr.take() {
                attr
            } else {
                &DEFAULT_ATTR.0
            },
            kind,
            rename_all: std::mem::replace(&mut next_rename_all, RenameRule::None),
            try_if: next_try_if.take(),
            final_if: next_final_if.take(),
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
            f |= Field::WITH_FROMITEM_OPTION;
        }
        for attr in &attr.attrs {
            match &attr.inner {
                FieldAttrInner::Default(..) => {
                    if attr.enabled & FROM_ITEM != 0 {
                        f |= Field::WITH_FROMITEM_DEFAULT;
                    }
                }
                FieldAttrInner::Skip(ref tokens) => {
                    if tokens.is_empty() {
                        if attr.enabled & FROM_ITEM != 0 {
                            f |= Field::WITH_FROMITEM_SKIP;
                        }
                        if attr.enabled & TO_ITEM != 0 {
                            f |= Field::WITH_TO_ITEM_SKIP;
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
                    Error::span_msg("Expected attr after", punct.span())
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
                Error::span_msg("Expected attr after", punct.span())
            };
            parse_field_attr(&mut next_attr, attr_buf, group.stream());
            continue;
        }
        if ch != b':' {
            continue;
        }
        let Some(TokenTree::Ident(name)) = fields.get(i.wrapping_sub(1)) else {
            Error::span_msg("Expected field name before :", punct.span())
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
                let attr = next_attr.get_or_insert_with(|| attr_buf.alloc_default());
                let oo_mask_item = (FROM_ITEM as u64) << (1u64 * TRAIT_COUNT);
                if attr.flags & oo_mask_item == 0 {
                    attr.flags |= OPTION_AUTO_DETECTED;
                    attr.attrs.push(FieldAttr {
                        enabled: FROM_ITEM,
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
