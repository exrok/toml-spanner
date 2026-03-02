use crate::ast::{
    self, DefaultKind, DeriveTargetInner, DeriveTargetKind, EnumKind, EnumVariant, Field,
    FieldAttrs, Generic, GenericKind, ENUM_CONTAINS_STRUCT_VARIANT, ENUM_CONTAINS_TUPLE_VARIANT,
    ENUM_CONTAINS_UNIT_VARIANT, FROM_ITEM, TO_ITEM,
};
use crate::case::RenameRule;
use crate::util::MemoryPool;
use crate::writer::RustWriter;
use crate::Error;
use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[allow(unused)]
enum StaticToken {
    Ident(&'static str),
    Punct(char, bool),
}
#[allow(unused)]
use StaticToken::Ident as StaticIdent;
#[allow(unused)]
use StaticToken::Punct as StaticPunct;
#[allow(unused)]
fn tt_append_blit(output: &mut RustWriter, chr: &str) {
    output
        .buf
        .extend(chr.as_bytes().iter().map(|tok| match *tok {
            1 => TokenTree::Ident(Ident::new("hello", Span::call_site())),
            v => TokenTree::Punct(Punct::new(
                ':',
                if v & 0b1 == 0 {
                    Spacing::Joint
                } else {
                    Spacing::Alone
                },
            )),
        }));
}

struct GenericBoundFormatting {
    lifetimes: bool,
    bounds: bool,
}
fn fmt_generics(buffer: &mut RustWriter, generics: &[Generic], fmt: GenericBoundFormatting) {
    let mut first = true;
    for generic in generics {
        if !fmt.lifetimes
            && match generic.kind {
                GenericKind::Lifetime => true,
                _ => false,
            }
        {
            continue;
        }
        if first {
            first = false;
        } else {
            buffer.blit_punct(13);
        }
        match generic.kind {
            GenericKind::Lifetime => {
                buffer.blit_punct(7);
            }
            GenericKind::Type => (),
            GenericKind::Const => {
                buffer.blit_ident(12);
            }
        }
        buffer.buf.push(generic.ident.clone().into());
        if fmt.bounds && !generic.bounds.is_empty() {
            buffer.blit_punct(9);
            buffer.buf.extend(generic.bounds.iter().cloned());
        }
    }
}
#[allow(dead_code)]
const DEAD_USE: GenericBoundFormatting = GenericBoundFormatting {
    lifetimes: false,
    bounds: false,
};
const USE: GenericBoundFormatting = GenericBoundFormatting {
    lifetimes: true,
    bounds: false,
};
const DEF: GenericBoundFormatting = GenericBoundFormatting {
    lifetimes: true,
    bounds: true,
};
struct Ctx<'a> {
    lifetime: Ident,
    generics: &'a [Generic<'a>],
    crate_path: Vec<TokenTree>,
    target: &'a DeriveTargetInner<'a>,
}
impl<'a> Ctx<'a> {
    fn new(out: &mut RustWriter, target: &'a DeriveTargetInner) -> Ctx<'a> {
        let crate_path = if let Some(value) = &target.path_override {
            let content = value.to_string();
            #[allow(unused)]
            let inner = &content[1..content.len() - 1];
            {
                out.blit(0, 3);
            };
            std::mem::take(&mut out.buf)
        } else {
            {
                out.blit(0, 3);
            };
            std::mem::take(&mut out.buf)
        };
        let (lt, generics) = if let [Generic {
            kind: GenericKind::Lifetime,
            ident,
            bounds,
        }, rest @ ..] = &target.generics[..]
        {
            if !bounds.is_empty() {
                Error::msg("Bounded lifetimes currently unsupported")
            }
            ((*ident).clone(), rest)
        } else {
            (Ident::new("de", Span::call_site()), &target.generics[..])
        };
        Ctx {
            lifetime: lt,
            generics,
            crate_path,
            target,
        }
    }
}
fn field_name_literal_toml(_ctx: &Ctx, field: &Field, rename_rule: RenameRule) -> Literal {
    if let Some(name) = field.attr.rename(FROM_ITEM) {
        return name.clone();
    }
    if rename_rule != RenameRule::None {
        Literal::string(&rename_rule.apply_to_field(&field.name.to_string()))
    } else {
        Literal::string(&field.name.to_string())
    }
}
fn impl_from_item(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    {
        output.blit_punct(11);
        {
            let at = output.buf.len();
            output.blit_ident(11);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit(3, 3);
        output.push_ident(&ctx.lifetime);
        if !ctx.generics.is_empty() {
            output.blit_punct(13);
            fmt_generics(output, ctx.generics, DEF);
        };
        output.blit_punct(2);
        output.buf.extend_from_slice(&ctx.crate_path);
        output.blit(6, 5);
        output.push_ident(&ctx.lifetime);
        output.blit(11, 2);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(2);
        };
        if !target.where_clauses.is_empty()
            || !target.generic_field_types.is_empty()
            || !target.generic_flatten_field_types.is_empty()
        {
            output.blit_ident(27);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(15, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(20, 2);
            {
                let at = output.buf.len();
                output.blit(22, 4);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(26, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(31, 5);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(36, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 14);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(55, 4);
            output
                .buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, inner)));
            output.tt_group(Delimiter::Brace, at);
        };
    };
}
fn is_option_type(field: &Field) -> bool {
    if let [TokenTree::Ident(ident), TokenTree::Punct(punct), ..] = field.ty {
        punct.as_char() == '<' && ident.to_string() == "Option"
    } else {
        false
    }
}
fn struct_from_item(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let mut flatten_field: Option<&Field> = None;
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            if flatten_field.is_some() {
                Error::msg("Only one #[toml(flatten)] field is allowed per struct")
            }
            flatten_field = Some(field);
        }
    }
    let start = out.buf.len();
    {
        out.blit(59, 6);
        {
            let at = out.buf.len();
            out.blit_ident(92);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(65, 2);
    };
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            {
                out.blit(67, 5);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(58);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(72, 7);
            };
            continue;
        }
        if field.flags & Field::WITH_FROMITEM_SKIP != 0 {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit(79, 7);
                    }
                }
            } else {
                out.blit_ident(91);
                out.push_ident(field.name);
                out.blit(79, 7);
            }
        } else if is_option_type(field) {
            out.blit(67, 2);
            out.push_ident(field.name);
            out.blit_punct(9);
            out.buf.extend_from_slice(field.ty);
            out.blit(86, 3);
        } else {
            out.blit(67, 2);
            out.push_ident(field.name);
            out.blit(89, 5);
            out.buf.extend_from_slice(field.ty);
            out.blit(94, 2);
        }
    }
    let match_arms_start = out.buf.len();
    for field in fields {
        if field.flags & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = is_option_type(field);
        let with_path = field.with(FROM_ITEM);
        let is_required = !is_option && !is_default;
        let arm_body_start = out.buf.len();
        if let Some(with) = with_path {
            if is_required {
                {
                    out.blit_ident(81);
                    out.buf.extend_from_slice(with);
                    out.blit(96, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(83);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(101, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(79);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(88);
                        {
                            let at = out.buf.len();
                            out.blit_ident(57);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(103, 4);
                        {
                            let at = out.buf.len();
                            out.blit(107, 3);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(110, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(116, 6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            } else {
                {
                    out.blit_ident(81);
                    out.buf.extend_from_slice(with);
                    out.blit(96, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(83);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(101, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(79);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(88);
                        {
                            let at = out.buf.len();
                            out.blit_ident(57);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.blit(107, 3);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(110, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(116, 6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
        } else if is_required {
            {
                out.blit_ident(81);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit_ident(79);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(101, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(88);
                    {
                        let at = out.buf.len();
                        out.blit_ident(34);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(103, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(34);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit_ident(81);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit_ident(79);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(101, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(88);
                    {
                        let at = out.buf.len();
                        out.blit_ident(73);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(131, 3);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        let arm_body = out.split_off_stream(arm_body_start);
        {
            out.buf.push(name_lit.into());
            out.blit(99, 2);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
        };
    }
    if let Some(ff) = flatten_field {
        {
            out.blit(134, 3);
            {
                let at = out.buf.len();
                out.blit(137, 4);
                out.buf.extend_from_slice(ff.ty);
                out.blit_ident(58);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(141, 5);
                {
                    let at = out.buf.len();
                    out.blit(146, 9);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    } else {
        {
            out.blit(134, 3);
            {
                let at = out.buf.len();
                out.blit(105, 2);
                {
                    let at = out.buf.len();
                    out.blit(155, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("unexpected key")));
                        out.blit(158, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    }
    let match_arms = out.split_off_stream(match_arms_start);
    let for_body_start = out.buf.len();
    {
        out.blit(162, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, match_arms)));
    };
    let for_body = out.split_off_stream(for_body_start);
    let for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(148, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(53);
        out.buf.push(for_pat);
        out.blit(166, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, for_body)));
    };
    if let Some(ff) = flatten_field {
        {
            out.blit_ident(91);
            out.push_ident(ff.name);
            out.blit(70, 2);
            out.buf.extend_from_slice(ff.ty);
            out.blit_ident(58);
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(15, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(168, 5);
            {
                let at = out.buf.len();
                out.blit(173, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    for field in fields {
        if field.flags & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN) != 0
            || is_option_type(field)
        {
            continue;
        }
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(91);
                            out.push_ident(field.name);
                            out.blit_punct(3);
                            out.push_ident(field.name);
                            out.blit(176, 2);
                            {
                                let at = out.buf.len();
                                out.blit(178, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(180, 4);
                    }
                }
            } else {
                out.blit_ident(91);
                out.push_ident(field.name);
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(180, 4);
            }
        } else {
            let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
            let else_body_start = out.buf.len();
            {
                out.blit(105, 2);
                {
                    let at = out.buf.len();
                    out.blit(184, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(187, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
            let else_body = out.split_off_stream(else_body_start);
            {
                out.blit(192, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(194, 4);
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, else_body)));
                out.blit_punct(1);
            };
        }
    }
    {
        out.blit_ident(89);
        {
            let at = out.buf.len();
            out.blit_ident(80);
            {
                let at = out.buf.len();
                {
                    for field in fields {
                        out.push_ident(field.name);
                        out.blit_punct(13);
                    }
                };
                out.tt_group(Delimiter::Brace, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}
fn impl_to_item(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    let lf = Ident::new("__de", Span::mixed_site());
    {
        output.blit_punct(11);
        {
            let at = output.buf.len();
            output.blit_ident(11);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit_ident(15);
        if !target.generics.is_empty() {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, DEF);
            output.blit_punct(2);
        };
        output.buf.extend_from_slice(&ctx.crate_path);
        output.blit(198, 4);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(2);
        };
        if !target.where_clauses.is_empty()
            || !target.generic_field_types.is_empty()
            || !target.generic_flatten_field_types.is_empty()
        {
            output.blit_ident(27);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(202, 4);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(206, 4);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(210, 4);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit_punct(2);
            {
                let at = output.buf.len();
                output.blit(214, 2);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit(216, 6);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(222, 5);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit_punct(2);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 12);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(36, 5);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit(13, 2);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(55, 4);
            output
                .buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, inner)));
            output.tt_group(Delimiter::Brace, at);
        };
    };
}
fn struct_to_item(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let mut flatten_field: Option<&Field> = None;
    let mut non_skip_count = 0usize;
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            flatten_field = Some(field);
        } else if field.flags & Field::WITH_TO_ITEM_SKIP == 0 {
            non_skip_count += 1;
        }
    }
    let start = out.buf.len();
    {
        out.blit(192, 2);
        {
            let at = out.buf.len();
            out.blit(227, 2);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(3);
        out.buf.extend_from_slice(&ctx.crate_path);
        out.blit(229, 6);
        {
            let at = out.buf.len();
            out.buf.push(TokenTree::Literal(Literal::usize_unsuffixed(
                non_skip_count,
            )));
            out.blit(235, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_ident(77);
        {
            let at = out.buf.len();
            out.blit(239, 4);
            {
                let at = out.buf.len();
                out.buf.push(TokenTree::Literal(Literal::string(
                    "Table capacity exceeded maximum",
                )));
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit_punct(1);
    };
    for field in fields {
        if field.flags & (Field::WITH_TO_ITEM_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
        let with_path = field.with(TO_ITEM);
        let skip_if = field.skip(TO_ITEM).filter(|tokens| !tokens.is_empty());
        let first_ty_ident = if let Some(TokenTree::Ident(ident)) = field.ty.first() {
            ident.to_string()
        } else {
            String::new()
        };
        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            {
                out.blit(243, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(246, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(252, 3);
                    {
                        let at = out.buf.len();
                        out.blit(255, 3);
                        out.push_ident(field.name);
                        out.blit(217, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(258, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(264, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(79);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(267, 6);
                {
                    let at = out.buf.len();
                    out.blit(255, 3);
                    out.push_ident(field.name);
                    out.blit(217, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                {
                    let at = out.buf.len();
                    out.blit(243, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(273, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(243, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(246, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(280, 6);
                    {
                        let at = out.buf.len();
                        out.blit(255, 3);
                        out.push_ident(field.name);
                        out.blit(217, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(258, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(286, 2);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(skip_tokens);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit(255, 3);
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, emit_body)));
            };
        }
    }
    if let Some(ff) = flatten_field {
        {
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(288, 6);
            {
                let at = out.buf.len();
                out.blit(255, 3);
                out.push_ident(ff.name);
                out.blit(294, 6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    {
        out.blit_ident(89);
        {
            let at = out.buf.len();
            out.blit(300, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn handle_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);
    if target.from_item {
        if target.transparent_impl {
            let [single_field] = fields else {
                Error::msg("Struct must contain a single field to use transparent")
            };
            let body = {
                let len = output.buf.len();
                output.blit_punct(5);
                output.buf.extend_from_slice(single_field.ty);
                output.blit_ident(58);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(304, 5);
                {
                    let at = output.buf.len();
                    output.blit(309, 3);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_from_item(output, &ctx, body);
        } else {
            struct_from_item(output, &ctx, fields);
        }
    }
    if target.to_item {
        if target.transparent_impl {
            let [single_field] = fields else {
                Error::msg("Struct must contain a single field to use transparent")
            };
            let body = {
                let len = output.buf.len();
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(280, 6);
                {
                    let at = output.buf.len();
                    output.blit(255, 3);
                    output.push_ident(single_field.name);
                    output.blit(217, 2);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_to_item(output, &ctx, body);
        } else {
            struct_to_item(output, &ctx, fields);
        }
    }
}
fn handle_tuple_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);
    if target.from_item {
        if let [single_field] = fields {
            let body = {
                let len = output.buf.len();
                output.blit_ident(89);
                {
                    let at = output.buf.len();
                    output.push_ident(&target.name);
                    {
                        let at = output.buf.len();
                        output.blit_punct(5);
                        output.buf.extend_from_slice(single_field.ty);
                        output.blit_ident(58);
                        output.buf.extend_from_slice(&ctx.crate_path);
                        output.blit(6, 5);
                        output.push_ident(&ctx.lifetime);
                        output.blit(304, 5);
                        {
                            let at = output.buf.len();
                            output.blit(309, 3);
                            output.tt_group(Delimiter::Parenthesis, at);
                        };
                        output.blit_punct(6);
                        output.tt_group(Delimiter::Parenthesis, at);
                    };
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_from_item(output, &ctx, body);
        } else {
            Error::msg(
                "FromItem on tuple structs requires exactly one field (transparent delegation)",
            )
        }
    }
    if target.to_item {
        if let [_single_field] = fields {
            let body = {
                let len = output.buf.len();
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(280, 6);
                {
                    let at = output.buf.len();
                    output.blit(255, 3);
                    output
                        .buf
                        .push(TokenTree::Literal(Literal::usize_unsuffixed(0)));
                    output.blit(217, 2);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_to_item(output, &ctx, body);
        } else {
            Error::msg(
                "ToItem on tuple structs requires exactly one field (transparent delegation)",
            )
        }
    }
}
fn variant_name_literal(ctx: &Ctx, variant: &EnumVariant) -> Literal {
    if let Some(name) = variant.rename(FROM_ITEM) {
        return name.clone();
    }
    let raw = variant.name.to_string();
    if ctx.target.rename_all != RenameRule::None {
        Literal::string(&ctx.target.rename_all.apply_to_variant(&raw))
    } else {
        Literal::string(&raw)
    }
}
fn variant_field_name_literal(ctx: &Ctx, field: &Field, variant: &EnumVariant) -> Literal {
    if let Some(name) = field.attr.rename(FROM_ITEM) {
        return name.clone();
    }
    let rule = if variant.rename_all != RenameRule::None {
        variant.rename_all
    } else {
        ctx.target.rename_all_fields
    };
    if rule != RenameRule::None {
        Literal::string(&rule.apply_to_field(&field.name.to_string()))
    } else {
        Literal::string(&field.name.to_string())
    }
}
/// Emit field deserialization from a table for a struct variant.
/// Assumes `__subtable` is in scope (a `&Table` to iterate).
/// Generates variable declarations, for-loop with match, and unwrapping.
/// Does NOT emit the final `Ok(Self::Variant { ... })`.
fn emit_variant_fields_from_table(
    out: &mut RustWriter,
    ctx: &Ctx,
    variant: &EnumVariant,
    fields: &[Field],
    skip_keys: &[Literal],
) {
    let mut flatten_field: Option<&Field> = None;
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            if flatten_field.is_some() {
                Error::msg("Only one #[toml(flatten)] field is allowed per variant")
            }
            flatten_field = Some(field);
        }
    }
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            {
                out.blit(67, 5);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(58);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(72, 7);
            };
            continue;
        }
        if field.flags & Field::WITH_FROMITEM_SKIP != 0 {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit(79, 7);
                    }
                }
            } else {
                out.blit_ident(91);
                out.push_ident(field.name);
                out.blit(79, 7);
            }
        } else if is_option_type(field) {
            out.blit(67, 2);
            out.push_ident(field.name);
            out.blit_punct(9);
            out.buf.extend_from_slice(field.ty);
            out.blit(86, 3);
        } else {
            out.blit(67, 2);
            out.push_ident(field.name);
            out.blit(89, 5);
            out.buf.extend_from_slice(field.ty);
            out.blit(94, 2);
        }
    }
    let match_arms_start = out.buf.len();
    for skip_key in skip_keys {
        out.buf.push(skip_key.clone().into());
        out.blit(131, 3);
    }
    for field in fields {
        if field.flags & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = variant_field_name_literal(ctx, field, variant);
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = is_option_type(field);
        let with_path = field.with(FROM_ITEM);
        let is_required = !is_option && !is_default;
        let arm_body_start = out.buf.len();
        if let Some(with) = with_path {
            if is_required {
                {
                    out.blit_ident(81);
                    out.buf.extend_from_slice(with);
                    out.blit(96, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(83);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(101, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(79);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(88);
                        {
                            let at = out.buf.len();
                            out.blit_ident(57);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(103, 4);
                        {
                            let at = out.buf.len();
                            out.blit(107, 3);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(110, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(116, 6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            } else {
                {
                    out.blit_ident(81);
                    out.buf.extend_from_slice(with);
                    out.blit(96, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(83);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(101, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(79);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(88);
                        {
                            let at = out.buf.len();
                            out.blit_ident(57);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.blit(107, 3);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(110, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(116, 6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
        } else if is_required {
            {
                out.blit_ident(81);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit_ident(79);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(101, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(88);
                    {
                        let at = out.buf.len();
                        out.blit_ident(34);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(103, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(34);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit_ident(81);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit_ident(79);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(101, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(79);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(88);
                    {
                        let at = out.buf.len();
                        out.blit_ident(73);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(131, 3);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        let arm_body = out.split_off_stream(arm_body_start);
        {
            out.buf.push(name_lit.into());
            out.blit(99, 2);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
        };
    }
    if let Some(ff) = flatten_field {
        {
            out.blit(134, 3);
            {
                let at = out.buf.len();
                out.blit(137, 4);
                out.buf.extend_from_slice(ff.ty);
                out.blit_ident(58);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(141, 5);
                {
                    let at = out.buf.len();
                    out.blit(146, 9);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    } else {
        {
            out.blit(134, 3);
            {
                let at = out.buf.len();
                out.blit(105, 2);
                {
                    let at = out.buf.len();
                    out.blit(155, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("unexpected key")));
                        out.blit(158, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    }
    let match_arms = out.split_off_stream(match_arms_start);
    let for_body_start = out.buf.len();
    {
        out.blit(162, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, match_arms)));
    };
    let for_body = out.split_off_stream(for_body_start);
    let for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(148, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(53);
        out.buf.push(for_pat);
        out.blit(312, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, for_body)));
    };
    if let Some(ff) = flatten_field {
        {
            out.blit_ident(91);
            out.push_ident(ff.name);
            out.blit(70, 2);
            out.buf.extend_from_slice(ff.ty);
            out.blit_ident(58);
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(15, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(168, 5);
            {
                let at = out.buf.len();
                out.blit(173, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    for field in fields {
        if field.flags & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN) != 0
            || is_option_type(field)
        {
            continue;
        }
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(91);
                            out.push_ident(field.name);
                            out.blit_punct(3);
                            out.push_ident(field.name);
                            out.blit(176, 2);
                            {
                                let at = out.buf.len();
                                out.blit(178, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        out.blit_ident(91);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(180, 4);
                    }
                }
            } else {
                out.blit_ident(91);
                out.push_ident(field.name);
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(180, 4);
            }
        } else {
            let name_lit = variant_field_name_literal(ctx, field, variant);
            let else_body_start = out.buf.len();
            {
                out.blit(105, 2);
                {
                    let at = out.buf.len();
                    out.blit(184, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(187, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
            let else_body = out.split_off_stream(else_body_start);
            {
                out.blit(192, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(194, 4);
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, else_body)));
                out.blit_punct(1);
            };
        }
    }
}
/// Emit field serialization into a table for a struct variant.
/// Assumes `table` and `__ctx` are in scope.
/// Fields are accessed via `ref` bindings from match destructuring,
/// so field names are already references - no `&` prefix needed.
fn emit_variant_fields_to_table(
    out: &mut RustWriter,
    ctx: &Ctx,
    variant: &EnumVariant,
    fields: &[Field],
) {
    for field in fields {
        if field.flags & (Field::WITH_TO_ITEM_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = variant_field_name_literal(ctx, field, variant);
        let with_path = field.with(TO_ITEM);
        let skip_if = field.skip(TO_ITEM).filter(|tokens| !tokens.is_empty());
        let first_ty_ident = if let Some(TokenTree::Ident(ident)) = field.ty.first() {
            ident.to_string()
        } else {
            String::new()
        };
        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            {
                out.blit(314, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(246, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(252, 3);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(217, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(258, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(264, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(26);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(267, 6);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(217, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                {
                    let at = out.buf.len();
                    out.blit(314, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(317, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(314, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(246, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(280, 6);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(217, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(258, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(286, 2);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(skip_tokens);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, emit_body)));
            };
        }
    }
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            {
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(288, 6);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(324, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(65, 2);
            };
        }
    }
}
fn enum_from_item_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let mut expected_msg = String::from("one of: ");
    for (i, v) in variants.iter().enumerate() {
        if i > 0 {
            expected_msg.push_str(", ");
        }
        let l = variant_name_literal(ctx, v);
        let s = l.to_string();
        expected_msg.push_str(&s[1..s.len() - 1]);
    }
    {
        out.blit(330, 6);
        {
            let at = out.buf.len();
            out.blit_ident(92);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(336, 4);
        {
            let at = out.buf.len();
            {
                for variant in variants {
                    let name_lit = variant_name_literal(ctx, variant);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(340, 3);
                        {
                            let at = out.buf.len();
                            out.blit(343, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
            };
            out.blit(346, 4);
            {
                let at = out.buf.len();
                out.blit(350, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string(&expected_msg)));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Brace, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}
fn enum_to_item_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    {
        out.blit_ident(89);
        {
            let at = out.buf.len();
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(353, 6);
            {
                let at = out.buf.len();
                out.blit(359, 2);
                {
                    let at = out.buf.len();
                    {
                        for variant in variants {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.blit(343, 3);
                                out.push_ident(variant.name);
                                out.blit(99, 2);
                                out.buf.push(name_lit.into());
                                out.blit_punct(13);
                            };
                        }
                    };
                    out.tt_group(Delimiter::Brace, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn enum_from_item_external(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let has_unit = ctx.target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0;
    let has_complex =
        ctx.target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) != 0;
    let start = out.buf.len();
    if has_unit {
        let if_body_start = out.buf.len();
        {
            out.blit(361, 3);
            {
                let at = out.buf.len();
                {
                    for variant in variants {
                        if match variant.kind {
                            EnumKind::None => true,
                            _ => false,
                        } {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.buf.push(name_lit.into());
                                out.blit(340, 3);
                                {
                                    let at = out.buf.len();
                                    out.blit(343, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(13);
                            };
                        }
                    }
                };
                out.blit(346, 4);
                {
                    let at = out.buf.len();
                    out.blit(350, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("a known variant")));
                        out.blit(32, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Brace, at);
            };
            out.blit_punct(1);
        };
        let if_body = out.split_off_stream(if_body_start);
        {
            out.blit(264, 3);
            {
                let at = out.buf.len();
                out.blit_ident(42);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(364, 5);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, if_body)));
        };
    }
    if has_complex {
        {
            out.blit(369, 6);
            {
                let at = out.buf.len();
                out.blit_ident(92);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(375, 10);
        };
        let err_body_start = out.buf.len();
        {
            out.blit(105, 2);
            {
                let at = out.buf.len();
                out.blit(350, 3);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::Literal(Literal::string(
                        "a table with exactly one key",
                    )));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
        };
        let err_body = out.split_off_stream(err_body_start);
        let one_lit = TokenTree::Literal(Literal::usize_unsuffixed(1));
        let zero_index = TokenTree::Group(Group::new(
            Delimiter::Bracket,
            TokenStream::from(TokenTree::Literal(Literal::usize_unsuffixed(0))),
        ));
        {
            out.blit(286, 2);
            {
                let at = out.buf.len();
                out.blit(385, 6);
                out.buf.push(one_lit);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, err_body)));
        };
        {
            out.blit_ident(91);
            {
                let at = out.buf.len();
                out.blit(391, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(394, 3);
            out.buf.push(zero_index);
            out.blit_punct(1);
        };
        let arms_start = out.buf.len();
        for variant in variants {
            if match variant.kind {
                EnumKind::None => true,
                _ => false,
            } {
                continue;
            }
            let name_lit = variant_name_literal(ctx, variant);
            match variant.kind {
                EnumKind::Tuple => {
                    if variant.fields.len() != 1 {
                        Error::msg(
                            "Only single-field tuple variants are supported in external tagging",
                        )
                    }
                    {
                        out.buf.push(name_lit.into());
                        out.blit(340, 3);
                        {
                            let at = out.buf.len();
                            out.blit(343, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(122, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(397, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(6);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
                EnumKind::Struct => {
                    let arm_body_start = out.buf.len();
                    {
                        out.blit(400, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(92);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(65, 2);
                    };
                    emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                    {
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit(343, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                {
                                    for field in variant.fields {
                                        out.push_ident(field.name);
                                        out.blit_punct(13);
                                    }
                                };
                                out.tt_group(Delimiter::Brace, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                    };
                    let arm_body = out.split_off_stream(arm_body_start);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(99, 2);
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                    };
                }
                EnumKind::None => {}
            }
        }
        {
            out.blit(346, 4);
            {
                let at = out.buf.len();
                out.blit(350, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("a known variant")));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(13);
        };
        let arms = out.split_off_stream(arms_start);
        {
            out.blit(406, 4);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
        };
    } else if !has_unit {
        {
            out.blit_ident(88);
            {
                let at = out.buf.len();
                out.blit(350, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("a known variant")));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
        };
    }
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}
fn enum_to_item_external(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    out.blit(340, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(353, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported in external tagging")
                }
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(40);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(192, 2);
                        {
                            let at = out.buf.len();
                            out.blit(328, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(229, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(235, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(77);
                        {
                            let at = out.buf.len();
                            out.blit(239, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "Table capacity exceeded maximum",
                                )));
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(246, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(280, 6);
                            {
                                let at = out.buf.len();
                                out.blit(414, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(258, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(417, 2);
                        {
                            let at = out.buf.len();
                            out.blit(419, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & (Field::WITH_TO_ITEM_SKIP | Field::WITH_FLATTEN) == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit(328, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(229, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(235, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(77);
                    {
                        let at = out.buf.len();
                        out.blit(239, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::Literal(Literal::string(
                                "Table capacity exceeded maximum",
                            )));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_punct(1);
                };
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit(423, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(229, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                        out.blit(235, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(77);
                    {
                        let at = out.buf.len();
                        out.blit(239, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::Literal(Literal::string(
                                "Table capacity exceeded maximum",
                            )));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(425, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(429, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(417, 2);
                    {
                        let at = out.buf.len();
                        out.blit(439, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(30);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(359, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn enum_from_item_internal(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
) {
    if ctx.target.enum_flags & ENUM_CONTAINS_TUPLE_VARIANT != 0 {
        Error::msg("Tuple variants are not supported with internal tagging")
    }
    let start = out.buf.len();
    {
        out.blit(59, 6);
        {
            let at = out.buf.len();
            out.blit_ident(92);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(65, 2);
    };
    let tag_loop_body_start = out.buf.len();
    {
        out.blit(443, 6);
        out.buf.push(tag_lit.clone().into());
        {
            let at = out.buf.len();
            out.blit(449, 3);
            {
                let at = out.buf.len();
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(452, 3);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    let tag_loop_body = out.split_off_stream(tag_loop_body_start);
    let tag_for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(148, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit(455, 13);
        out.buf.push(tag_for_pat);
        out.blit(166, 2);
        out.buf.push(TokenTree::Group(Group::new(
            Delimiter::Brace,
            tag_loop_body,
        )));
    };
    let missing_tag_else_start = out.buf.len();
    {
        out.blit(105, 2);
        {
            let at = out.buf.len();
            out.blit(184, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(187, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(1);
    };
    let missing_tag_else = out.split_off_stream(missing_tag_else_start);
    {
        out.blit(192, 2);
        {
            let at = out.buf.len();
            out.blit_ident(64);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(468, 3);
        out.buf.push(TokenTree::Group(Group::new(
            Delimiter::Brace,
            missing_tag_else,
        )));
        out.blit_punct(1);
    };
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                let arm_body_start = out.buf.len();
                let check_body_start = out.buf.len();
                {
                    out.blit(471, 6);
                    out.buf.push(tag_lit.clone().into());
                    {
                        let at = out.buf.len();
                        out.blit(105, 2);
                        {
                            let at = out.buf.len();
                            out.blit(155, 3);
                            {
                                let at = out.buf.len();
                                out.buf
                                    .push(TokenTree::Literal(Literal::string("unexpected key")));
                                out.blit(158, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
                let check_body = out.split_off_stream(check_body_start);
                let check_pat = {
                    let pat_stream = {
                        let len = out.buf.len();
                        out.blit(148, 3);
                        out.split_off_stream(len)
                    };
                    TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
                };
                {
                    out.blit_ident(53);
                    out.buf.push(check_pat);
                    out.blit(166, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, check_body)));
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit(343, 3);
                        out.push_ident(variant.name);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.buf.push(name_lit.into());
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                {
                    out.blit(477, 5);
                };
                emit_variant_fields_from_table(
                    out,
                    ctx,
                    variant,
                    variant.fields,
                    &[tag_lit.clone()],
                );
                {
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit(343, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            {
                                for field in variant.fields {
                                    out.push_ident(field.name);
                                    out.blit_punct(13);
                                }
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.buf.push(name_lit.into());
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    {
        out.blit(346, 4);
        {
            let at = out.buf.len();
            out.blit(350, 3);
            {
                let at = out.buf.len();
                out.buf
                    .push(TokenTree::Literal(Literal::string("a known variant")));
                out.blit(32, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(13);
    };
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(482, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}
fn enum_to_item_internal(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
) {
    let start = out.buf.len();
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(192, 2);
                        {
                            let at = out.buf.len();
                            out.blit(328, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(229, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(235, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(77);
                        {
                            let at = out.buf.len();
                            out.blit(239, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "Table capacity exceeded maximum",
                                )));
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(246, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(353, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(259, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(417, 2);
                        {
                            let at = out.buf.len();
                            out.blit(419, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & (Field::WITH_TO_ITEM_SKIP | Field::WITH_FLATTEN) == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit(328, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(229, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip + 1)));
                        out.blit(235, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(77);
                    {
                        let at = out.buf.len();
                        out.blit(239, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::Literal(Literal::string(
                                "Table capacity exceeded maximum",
                            )));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(410, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(353, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(259, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                {
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit(419, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(30);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(359, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn enum_from_item_adjacent(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
    content_lit: &Literal,
) {
    let start = out.buf.len();
    {
        out.blit(59, 6);
        {
            let at = out.buf.len();
            out.blit_ident(92);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(484, 21);
        out.buf.extend_from_slice(&ctx.crate_path);
        out.blit(36, 5);
        out.push_ident(&ctx.lifetime);
        out.blit(505, 5);
    };
    let extract_arms_start = out.buf.len();
    {
        out.buf.push(tag_lit.clone().into());
        out.blit(99, 2);
        {
            let at = out.buf.len();
            out.blit(449, 3);
            {
                let at = out.buf.len();
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(122, 6);
                {
                    let at = out.buf.len();
                    out.blit(128, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
        out.buf.push(content_lit.clone().into());
        out.blit(99, 2);
        {
            let at = out.buf.len();
            out.blit(510, 3);
            {
                let at = out.buf.len();
                out.blit_ident(83);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit(134, 3);
        {
            let at = out.buf.len();
            out.blit(105, 2);
            {
                let at = out.buf.len();
                out.blit(155, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("unexpected key")));
                    out.blit(158, 4);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    let extract_arms = out.split_off_stream(extract_arms_start);
    let extract_body_start = out.buf.len();
    {
        out.blit(162, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, extract_arms)));
    };
    let extract_body = out.split_off_stream(extract_body_start);
    let extract_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(148, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(53);
        out.buf.push(extract_pat);
        out.blit(166, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, extract_body)));
    };
    let missing_tag_else_start = out.buf.len();
    {
        out.blit(105, 2);
        {
            let at = out.buf.len();
            out.blit(184, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(187, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(1);
    };
    let missing_tag_else = out.split_off_stream(missing_tag_else_start);
    {
        out.blit(192, 2);
        {
            let at = out.buf.len();
            out.blit_ident(64);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(468, 3);
        out.buf.push(TokenTree::Group(Group::new(
            Delimiter::Brace,
            missing_tag_else,
        )));
        out.blit_punct(1);
    };
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(89);
                        {
                            let at = out.buf.len();
                            out.blit(343, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported")
                }
                let arm_body_start = out.buf.len();
                let missing_content_else_start = out.buf.len();
                {
                    out.blit(105, 2);
                    {
                        let at = out.buf.len();
                        out.blit(184, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(187, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                let missing_content_else = out.split_off_stream(missing_content_else_start);
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(56);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(513, 3);
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::Brace,
                        missing_content_else,
                    )));
                    out.blit(417, 2);
                    {
                        let at = out.buf.len();
                        out.blit(343, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(122, 6);
                            {
                                let at = out.buf.len();
                                out.blit(516, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.buf.push(name_lit.into());
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                let missing_content_else_start = out.buf.len();
                {
                    out.blit(105, 2);
                    {
                        let at = out.buf.len();
                        out.blit(184, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(187, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                let missing_content_else = out.split_off_stream(missing_content_else_start);
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(56);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(513, 3);
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::Brace,
                        missing_content_else,
                    )));
                    out.blit(519, 7);
                    {
                        let at = out.buf.len();
                        out.blit_ident(92);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(65, 2);
                };
                emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                {
                    out.blit_ident(89);
                    {
                        let at = out.buf.len();
                        out.blit(343, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            {
                                for field in variant.fields {
                                    out.push_ident(field.name);
                                    out.blit_punct(13);
                                }
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.buf.push(name_lit.into());
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    {
        out.blit(346, 4);
        {
            let at = out.buf.len();
            out.blit(350, 3);
            {
                let at = out.buf.len();
                out.buf
                    .push(TokenTree::Literal(Literal::string("a known variant")));
                out.blit(32, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(13);
    };
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(482, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}
fn enum_to_item_adjacent(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
    content_lit: &Literal,
) {
    let start = out.buf.len();
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(192, 2);
                        {
                            let at = out.buf.len();
                            out.blit(328, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(229, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(235, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(77);
                        {
                            let at = out.buf.len();
                            out.blit(239, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "Table capacity exceeded maximum",
                                )));
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(246, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(353, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(259, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(417, 2);
                        {
                            let at = out.buf.len();
                            out.blit(419, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported")
                }
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(40);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(192, 2);
                        {
                            let at = out.buf.len();
                            out.blit(328, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(229, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                            out.blit(235, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(77);
                        {
                            let at = out.buf.len();
                            out.blit(239, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "Table capacity exceeded maximum",
                                )));
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(246, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(353, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(259, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(246, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(content_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(280, 6);
                            {
                                let at = out.buf.len();
                                out.blit(414, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(258, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(417, 2);
                        {
                            let at = out.buf.len();
                            out.blit(419, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & (Field::WITH_TO_ITEM_SKIP | Field::WITH_FLATTEN) == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit(328, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(229, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(235, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(77);
                    {
                        let at = out.buf.len();
                        out.blit(239, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::Literal(Literal::string(
                                "Table capacity exceeded maximum",
                            )));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_punct(1);
                };
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                {
                    out.blit(192, 2);
                    {
                        let at = out.buf.len();
                        out.blit(423, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(229, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                        out.blit(235, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(77);
                    {
                        let at = out.buf.len();
                        out.blit(239, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::Literal(Literal::string(
                                "Table capacity exceeded maximum",
                            )));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(425, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(353, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(259, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(425, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(246, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(429, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(417, 2);
                    {
                        let at = out.buf.len();
                        out.blit(439, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(343, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(30);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(99, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(359, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn handle_enum(output: &mut RustWriter, target: &DeriveTargetInner, variants: &[EnumVariant]) {
    if target.content.is_some() && target.tag.is_none() {
        Error::msg("content attribute requires tag to also be set")
    }
    let ctx = Ctx::new(output, target);
    let is_string_enum = target.tag.is_none()
        && target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0
        && target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) == 0;
    if target.from_item {
        match (&target.tag, &target.content) {
            (None, _) if is_string_enum => enum_from_item_string(output, &ctx, variants),
            (None, _) => enum_from_item_external(output, &ctx, variants),
            (Some(tag_lit), None) => enum_from_item_internal(output, &ctx, variants, tag_lit),
            (Some(tag_lit), Some(content_lit)) => {
                enum_from_item_adjacent(output, &ctx, variants, tag_lit, content_lit)
            }
        }
    }
    if target.to_item {
        match (&target.tag, &target.content) {
            (None, _) if is_string_enum => enum_to_item_string(output, &ctx, variants),
            (None, _) => enum_to_item_external(output, &ctx, variants),
            (Some(tag_lit), None) => enum_to_item_internal(output, &ctx, variants, tag_lit),
            (Some(tag_lit), Some(content_lit)) => {
                enum_to_item_adjacent(output, &ctx, variants, tag_lit, content_lit)
            }
        }
    }
}
pub fn inner_derive(stream: TokenStream) -> TokenStream {
    let outer_tokens: Vec<TokenTree> = stream.into_iter().collect();
    let mut target = DeriveTargetInner {
        transparent_impl: false,
        name: Ident::new("a", Span::call_site()),
        generics: Vec::new(),
        generic_field_types: Vec::new(),
        generic_flatten_field_types: Vec::new(),
        where_clauses: &[],
        path_override: None,
        from_item: false,
        to_item: false,
        rename_all: crate::case::RenameRule::None,
        rename_all_fields: crate::case::RenameRule::None,
        enum_flags: 0,
        tag: None,
        content: None,
    };
    let (kind, body) = ast::extract_derive_target(&mut target, &outer_tokens);
    if !(target.from_item || target.to_item) {
        target.from_item = true;
    }
    let field_toks: Vec<TokenTree> = body.into_iter().collect();
    let mut tt_buf = Vec::<TokenTree>::new();
    let mut field_buf = Vec::<Field>::new();
    let mut pool = MemoryPool::<FieldAttrs>::new();
    let mut attr_buf = pool.allocator();
    let mut rust_writer = RustWriter::new();
    match kind {
        DeriveTargetKind::Struct => {
            ast::parse_struct_fields(&mut field_buf, &field_toks, &mut attr_buf);
            ast::scan_fields(&mut target, &mut field_buf);
            handle_struct(&mut rust_writer, &target, &field_buf);
        }
        DeriveTargetKind::TupleStruct => {
            let t = Ident::new("a", Span::call_site());
            ast::parse_tuple_fields(&t, &mut field_buf, &field_toks, &mut attr_buf);
            ast::scan_fields(&mut target, &mut field_buf);
            handle_tuple_struct(&mut rust_writer, &target, &field_buf);
        }
        DeriveTargetKind::Enum => {
            let variants = ast::parse_enum(
                &mut target,
                &field_toks,
                &mut tt_buf,
                &mut field_buf,
                &mut attr_buf,
            );
            handle_enum(&mut rust_writer, &target, &variants);
        }
    }
    let ts = rust_writer.split_off_stream(0);
    {
        let len = (&mut rust_writer).buf.len();
        (&mut rust_writer).blit_punct(11);
        {
            let at = (&mut rust_writer).buf.len();
            (&mut rust_writer).blit_ident(2);
            {
                let at = (&mut rust_writer).buf.len();
                (&mut rust_writer).blit(526, 4);
                (&mut rust_writer).tt_group(Delimiter::Parenthesis, at);
            };
            (&mut rust_writer).tt_group(Delimiter::Bracket, at);
        };
        (&mut rust_writer).blit(530, 5);
        (&mut rust_writer)
            .buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, ts)));
        (&mut rust_writer).blit_punct(1);
        (&mut rust_writer).split_off_stream(len)
    }
}
pub fn derive(stream: TokenStream) -> TokenStream {
    Error::try_catch_handle(stream, inner_derive)
}
