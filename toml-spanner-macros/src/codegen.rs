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
/// Best-effort extraction of the inner type from `Option<T>`.
/// Returns `&ty[2..ty.len()-1]` when the tokens match `Option < ... >`,
/// otherwise returns the full type unchanged.
fn option_inner_ty<'a>(ty: &'a [TokenTree]) -> &'a [TokenTree] {
    if let [TokenTree::Ident(id), _open, inner @ .., TokenTree::Punct(close)] = ty {
        if id.to_string() == "Option" && close.as_char() == '>' {
            return inner;
        }
    }
    ty
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
                buffer.blit_ident(11);
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
        output.blit_punct(15);
        {
            let at = output.buf.len();
            output.blit_ident(10);
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
            output.blit_ident(26);
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
            out.blit_ident(97);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(65, 2);
    };
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            {
                out.blit(67, 5);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(72);
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
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit(79, 7);
                    }
                }
            } else {
                out.blit_ident(96);
                out.push_ident(field.name);
                out.blit(79, 7);
            }
        } else if field.flags & Field::WITH_FROMITEM_OPTION != 0 {
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
        let is_option = field.flags & Field::WITH_FROMITEM_OPTION != 0;
        let with_path = field.with(FROM_ITEM);
        let is_required = !is_option && !is_default;
        let has_aliases = field.attr.has_aliases(FROM_ITEM);
        let arm_body_start = out.buf.len();
        if has_aliases {
            {
                out.blit_ident(78);
                out.push_ident(field.name);
                out.blit(96, 3);
                {
                    let at = out.buf.len();
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(101, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.clone().into());
                            out.blit(104, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        if let Some(with) = with_path {
            if is_required {
                {
                    out.blit_ident(86);
                    out.buf.extend_from_slice(with);
                    out.blit(108, 3);
                    {
                        let at = out.buf.len();
                        out.blit(111, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(114, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(116, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(85);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(93);
                        {
                            let at = out.buf.len();
                            out.blit_ident(63);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(118, 4);
                        {
                            let at = out.buf.len();
                            out.blit_ident(63);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            } else {
                {
                    out.blit_ident(86);
                    out.buf.extend_from_slice(with);
                    out.blit(108, 3);
                    {
                        let at = out.buf.len();
                        out.blit(111, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(114, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(116, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(85);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(93);
                        {
                            let at = out.buf.len();
                            out.blit_ident(84);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(122, 3);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
        } else if is_required {
            {
                out.blit(125, 2);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(6, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(127, 5);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit_ident(85);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(116, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(93);
                    {
                        let at = out.buf.len();
                        out.blit_ident(63);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(118, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(63);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            let inner_ty = if is_option {
                option_inner_ty(field.ty)
            } else {
                field.ty
            };
            {
                out.blit(125, 2);
                out.buf.extend_from_slice(inner_ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(6, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(127, 5);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit_ident(85);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(116, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(93);
                    {
                        let at = out.buf.len();
                        out.blit_ident(84);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(122, 3);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        let arm_body = out.split_off_stream(arm_body_start);
        {
            out.buf.push(name_lit.into());
        };
        for alias in field.attr.aliases(FROM_ITEM) {
            out.blit_punct(11);
            out.buf.push(alias.clone().into());
        }
        {
            out.blit(114, 2);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
        };
    }
    if let Some(ff) = flatten_field {
        {
            out.blit(132, 3);
            {
                let at = out.buf.len();
                out.blit(135, 4);
                out.buf.extend_from_slice(ff.ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(139, 5);
                {
                    let at = out.buf.len();
                    out.blit(144, 9);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    } else {
        {
            out.blit(132, 3);
            {
                let at = out.buf.len();
                out.blit(99, 2);
                {
                    let at = out.buf.len();
                    out.blit(153, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("unexpected key")));
                        out.blit(104, 4);
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
        out.blit(156, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, match_arms)));
    };
    let for_body = out.split_off_stream(for_body_start);
    let for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(146, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(57);
        out.buf.push(for_pat);
        out.blit(160, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, for_body)));
    };
    if let Some(ff) = flatten_field {
        {
            out.blit_ident(96);
            out.push_ident(ff.name);
            out.blit(70, 2);
            out.buf.extend_from_slice(ff.ty);
            out.blit_ident(72);
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(15, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(162, 5);
            {
                let at = out.buf.len();
                out.blit(167, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    for field in fields {
        if field.flags
            & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN | Field::WITH_FROMITEM_OPTION)
            != 0
        {
            continue;
        }
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(96);
                            out.push_ident(field.name);
                            out.blit_punct(3);
                            out.push_ident(field.name);
                            out.blit(170, 2);
                            {
                                let at = out.buf.len();
                                out.blit(172, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(174, 4);
                    }
                }
            } else {
                out.blit_ident(96);
                out.push_ident(field.name);
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(174, 4);
            }
        } else {
            let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
            let else_body_start = out.buf.len();
            {
                out.blit(99, 2);
                {
                    let at = out.buf.len();
                    out.blit(178, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(181, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
            let else_body = out.split_off_stream(else_body_start);
            {
                out.blit(186, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(188, 4);
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, else_body)));
                out.blit_punct(1);
            };
        }
    }
    {
        out.blit_ident(95);
        {
            let at = out.buf.len();
            out.blit_ident(90);
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
        output.blit_punct(15);
        {
            let at = output.buf.len();
            output.blit_ident(10);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit_ident(13);
        if !target.generics.is_empty() {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, DEF);
            output.blit_punct(2);
        };
        output.buf.extend_from_slice(&ctx.crate_path);
        output.blit(192, 4);
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
            output.blit_ident(26);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(196, 4);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(200, 4);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(204, 4);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit_punct(2);
            {
                let at = output.buf.len();
                output.blit(208, 2);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit(210, 6);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(216, 5);
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
        out.blit(186, 2);
        {
            let at = out.buf.len();
            out.blit(221, 2);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(3);
        out.buf.extend_from_slice(&ctx.crate_path);
        out.blit(223, 6);
        {
            let at = out.buf.len();
            out.buf.push(TokenTree::Literal(Literal::usize_unsuffixed(
                non_skip_count,
            )));
            out.blit(229, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_ident(79);
        {
            let at = out.buf.len();
            out.blit(233, 4);
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
                out.blit(237, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(240, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(246, 3);
                    {
                        let at = out.buf.len();
                        out.blit(249, 3);
                        out.push_ident(field.name);
                        out.blit(211, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(252, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(258, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(85);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(261, 6);
                {
                    let at = out.buf.len();
                    out.blit(249, 3);
                    out.push_ident(field.name);
                    out.blit(211, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                {
                    let at = out.buf.len();
                    out.blit(237, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(267, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(237, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(240, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(274, 6);
                    {
                        let at = out.buf.len();
                        out.blit(249, 3);
                        out.push_ident(field.name);
                        out.blit(211, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(252, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(280, 2);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(skip_tokens);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit(249, 3);
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
            out.blit(282, 6);
            {
                let at = out.buf.len();
                out.blit(249, 3);
                out.push_ident(ff.name);
                out.blit(288, 6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    {
        out.blit_ident(95);
        {
            let at = out.buf.len();
            out.blit(294, 4);
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
                output.blit_ident(72);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(127, 5);
                {
                    let at = output.buf.len();
                    output.blit(298, 3);
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
                output.blit(274, 6);
                {
                    let at = output.buf.len();
                    output.blit(249, 3);
                    output.push_ident(single_field.name);
                    output.blit(211, 2);
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
                output.blit_ident(95);
                {
                    let at = output.buf.len();
                    output.push_ident(&target.name);
                    {
                        let at = output.buf.len();
                        output.blit_punct(5);
                        output.buf.extend_from_slice(single_field.ty);
                        output.blit_ident(72);
                        output.buf.extend_from_slice(&ctx.crate_path);
                        output.blit(6, 5);
                        output.push_ident(&ctx.lifetime);
                        output.blit(127, 5);
                        {
                            let at = output.buf.len();
                            output.blit(298, 3);
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
                output.blit(274, 6);
                {
                    let at = output.buf.len();
                    output.blit(249, 3);
                    output
                        .buf
                        .push(TokenTree::Literal(Literal::usize_unsuffixed(0)));
                    output.blit(211, 2);
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
                out.blit_ident(72);
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
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit(79, 7);
                    }
                }
            } else {
                out.blit_ident(96);
                out.push_ident(field.name);
                out.blit(79, 7);
            }
        } else if field.flags & Field::WITH_FROMITEM_OPTION != 0 {
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
        out.blit(122, 3);
    }
    for field in fields {
        if field.flags & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = variant_field_name_literal(ctx, field, variant);
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = field.flags & Field::WITH_FROMITEM_OPTION != 0;
        let with_path = field.with(FROM_ITEM);
        let is_required = !is_option && !is_default;
        let has_aliases = field.attr.has_aliases(FROM_ITEM);
        let arm_body_start = out.buf.len();
        if has_aliases {
            {
                out.blit_ident(78);
                out.push_ident(field.name);
                out.blit(96, 3);
                {
                    let at = out.buf.len();
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(101, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.clone().into());
                            out.blit(104, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        if let Some(with) = with_path {
            if is_required {
                {
                    out.blit_ident(86);
                    out.buf.extend_from_slice(with);
                    out.blit(108, 3);
                    {
                        let at = out.buf.len();
                        out.blit(111, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(114, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(116, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(85);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(93);
                        {
                            let at = out.buf.len();
                            out.blit_ident(63);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(118, 4);
                        {
                            let at = out.buf.len();
                            out.blit_ident(63);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            } else {
                {
                    out.blit_ident(86);
                    out.buf.extend_from_slice(with);
                    out.blit(108, 3);
                    {
                        let at = out.buf.len();
                        out.blit(111, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(114, 2);
                        {
                            let at = out.buf.len();
                            out.push_ident(field.name);
                            out.blit(116, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(85);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit_ident(93);
                        {
                            let at = out.buf.len();
                            out.blit_ident(84);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(122, 3);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
        } else if is_required {
            {
                out.blit(125, 2);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(6, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(127, 5);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit_ident(85);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(116, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(93);
                    {
                        let at = out.buf.len();
                        out.blit_ident(63);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(118, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(63);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            let inner_ty = if is_option {
                option_inner_ty(field.ty)
            } else {
                field.ty
            };
            {
                out.blit(125, 2);
                out.buf.extend_from_slice(inner_ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(6, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(127, 5);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit_ident(85);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(116, 2);
                        {
                            let at = out.buf.len();
                            out.blit_ident(85);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_ident(93);
                    {
                        let at = out.buf.len();
                        out.blit_ident(84);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(122, 3);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        let arm_body = out.split_off_stream(arm_body_start);
        {
            out.buf.push(name_lit.into());
        };
        for alias in field.attr.aliases(FROM_ITEM) {
            out.blit_punct(11);
            out.buf.push(alias.clone().into());
        }
        {
            out.blit(114, 2);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
        };
    }
    if let Some(ff) = flatten_field {
        {
            out.blit(132, 3);
            {
                let at = out.buf.len();
                out.blit(135, 4);
                out.buf.extend_from_slice(ff.ty);
                out.blit_ident(72);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(139, 5);
                {
                    let at = out.buf.len();
                    out.blit(144, 9);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    } else {
        {
            out.blit(132, 3);
            {
                let at = out.buf.len();
                out.blit(99, 2);
                {
                    let at = out.buf.len();
                    out.blit(153, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("unexpected key")));
                        out.blit(104, 4);
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
        out.blit(156, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, match_arms)));
    };
    let for_body = out.split_off_stream(for_body_start);
    let for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(146, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(57);
        out.buf.push(for_pat);
        out.blit(301, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, for_body)));
    };
    if let Some(ff) = flatten_field {
        {
            out.blit_ident(96);
            out.push_ident(ff.name);
            out.blit(70, 2);
            out.buf.extend_from_slice(ff.ty);
            out.blit_ident(72);
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(15, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(162, 5);
            {
                let at = out.buf.len();
                out.blit(167, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(65, 2);
        };
    }
    for field in fields {
        if field.flags
            & (Field::WITH_FROMITEM_SKIP | Field::WITH_FLATTEN | Field::WITH_FROMITEM_OPTION)
            != 0
        {
            continue;
        }
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(96);
                            out.push_ident(field.name);
                            out.blit_punct(3);
                            out.push_ident(field.name);
                            out.blit(170, 2);
                            {
                                let at = out.buf.len();
                                out.blit(172, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        out.blit_ident(96);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(174, 4);
                    }
                }
            } else {
                out.blit_ident(96);
                out.push_ident(field.name);
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(174, 4);
            }
        } else {
            let name_lit = variant_field_name_literal(ctx, field, variant);
            let else_body_start = out.buf.len();
            {
                out.blit(99, 2);
                {
                    let at = out.buf.len();
                    out.blit(178, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(181, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
            let else_body = out.split_off_stream(else_body_start);
            {
                out.blit(186, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(188, 4);
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
                out.blit(303, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(240, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(246, 3);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(211, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(252, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(258, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(25);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(261, 6);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(211, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                {
                    let at = out.buf.len();
                    out.blit(303, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(306, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(303, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(240, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(274, 6);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(211, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(252, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(280, 2);
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
                out.blit(282, 6);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(313, 6);
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
        out.blit(319, 6);
        {
            let at = out.buf.len();
            out.blit_ident(97);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(325, 4);
        {
            let at = out.buf.len();
            {
                for variant in variants {
                    let name_lit = variant_name_literal(ctx, variant);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(329, 3);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
            };
            out.blit(335, 4);
            {
                let at = out.buf.len();
                out.blit(339, 3);
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
        out.blit_ident(95);
        {
            let at = out.buf.len();
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(342, 6);
            {
                let at = out.buf.len();
                out.blit(348, 2);
                {
                    let at = out.buf.len();
                    {
                        for variant in variants {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.blit(332, 3);
                                out.push_ident(variant.name);
                                out.blit(114, 2);
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
            out.blit(350, 3);
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
                                out.blit(329, 3);
                                {
                                    let at = out.buf.len();
                                    out.blit(332, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(13);
                            };
                        }
                    }
                };
                out.blit(335, 4);
                {
                    let at = out.buf.len();
                    out.blit(339, 3);
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
            out.blit(258, 3);
            {
                let at = out.buf.len();
                out.blit_ident(45);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(353, 5);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, if_body)));
        };
    }
    if has_complex {
        {
            out.blit(358, 6);
            {
                let at = out.buf.len();
                out.blit_ident(97);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(364, 10);
        };
        let err_body_start = out.buf.len();
        {
            out.blit(99, 2);
            {
                let at = out.buf.len();
                out.blit(339, 3);
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
            out.blit(280, 2);
            {
                let at = out.buf.len();
                out.blit(374, 6);
                out.buf.push(one_lit);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, err_body)));
        };
        {
            out.blit_ident(96);
            {
                let at = out.buf.len();
                out.blit(380, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(383, 3);
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
                        out.blit(329, 3);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(386, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(392, 3);
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
                        out.blit(395, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(97);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(65, 2);
                    };
                    emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                    {
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
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
                        out.blit(114, 2);
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                    };
                }
                EnumKind::None => {}
            }
        }
        {
            out.blit(335, 4);
            {
                let at = out.buf.len();
                out.blit(339, 3);
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
            out.blit(401, 4);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
        };
    } else if !has_unit {
        {
            out.blit_ident(93);
            {
                let at = out.buf.len();
                out.blit(339, 3);
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
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    out.blit(329, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(342, 6);
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
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(50);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.blit(186, 2);
                        {
                            let at = out.buf.len();
                            out.blit(317, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(223, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(229, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(79);
                        {
                            let at = out.buf.len();
                            out.blit(233, 4);
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
                        out.blit(405, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(240, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(274, 6);
                            {
                                let at = out.buf.len();
                                out.blit(409, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(252, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(412, 2);
                        {
                            let at = out.buf.len();
                            out.blit(414, 4);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(317, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(418, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit(420, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(424, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(412, 2);
                    {
                        let at = out.buf.len();
                        out.blit(434, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(44);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(348, 2);
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
            out.blit_ident(97);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(65, 2);
    };
    let tag_loop_body_start = out.buf.len();
    {
        out.blit(438, 6);
        out.buf.push(tag_lit.clone().into());
        {
            let at = out.buf.len();
            out.blit(444, 3);
            {
                let at = out.buf.len();
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(386, 6);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(447, 3);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    let tag_loop_body = out.split_off_stream(tag_loop_body_start);
    let tag_for_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(146, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit(450, 13);
        out.buf.push(tag_for_pat);
        out.blit(160, 2);
        out.buf.push(TokenTree::Group(Group::new(
            Delimiter::Brace,
            tag_loop_body,
        )));
    };
    let missing_tag_else_start = out.buf.len();
    {
        out.blit(99, 2);
        {
            let at = out.buf.len();
            out.blit(178, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(181, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(1);
    };
    let missing_tag_else = out.split_off_stream(missing_tag_else_start);
    {
        out.blit(186, 2);
        {
            let at = out.buf.len();
            out.blit_ident(64);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(463, 3);
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
                    out.blit(466, 6);
                    out.buf.push(tag_lit.clone().into());
                    {
                        let at = out.buf.len();
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.blit(153, 3);
                            {
                                let at = out.buf.len();
                                out.buf
                                    .push(TokenTree::Literal(Literal::string("unexpected key")));
                                out.blit(104, 4);
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
                        out.blit(146, 3);
                        out.split_off_stream(len)
                    };
                    TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
                };
                {
                    out.blit_ident(57);
                    out.buf.push(check_pat);
                    out.blit(160, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, check_body)));
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit(332, 3);
                        out.push_ident(variant.name);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.buf.push(name_lit.into());
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                {
                    out.blit(472, 5);
                };
                emit_variant_fields_from_table(
                    out,
                    ctx,
                    variant,
                    variant.fields,
                    &[tag_lit.clone()],
                );
                {
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit(332, 3);
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
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    {
        out.blit(335, 4);
        {
            let at = out.buf.len();
            out.blit(339, 3);
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
        out.blit(477, 2);
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
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.blit(186, 2);
                        {
                            let at = out.buf.len();
                            out.blit(317, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(223, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(229, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(79);
                        {
                            let at = out.buf.len();
                            out.blit(233, 4);
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
                        out.blit(405, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(240, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(342, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(253, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(412, 2);
                        {
                            let at = out.buf.len();
                            out.blit(414, 4);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(317, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip + 1)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit(405, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(342, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(253, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                {
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit(414, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(44);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(348, 2);
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
            out.blit_ident(97);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(479, 21);
        out.buf.extend_from_slice(&ctx.crate_path);
        out.blit(36, 5);
        out.push_ident(&ctx.lifetime);
        out.blit(500, 5);
    };
    let extract_arms_start = out.buf.len();
    {
        out.buf.push(tag_lit.clone().into());
        out.blit(114, 2);
        {
            let at = out.buf.len();
            out.blit(444, 3);
            {
                let at = out.buf.len();
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(386, 6);
                {
                    let at = out.buf.len();
                    out.blit(111, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
        out.buf.push(content_lit.clone().into());
        out.blit(114, 2);
        {
            let at = out.buf.len();
            out.blit(505, 3);
            {
                let at = out.buf.len();
                out.blit_ident(83);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(1);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit(132, 3);
        {
            let at = out.buf.len();
            out.blit(99, 2);
            {
                let at = out.buf.len();
                out.blit(153, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("unexpected key")));
                    out.blit(104, 4);
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
        out.blit(156, 4);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, extract_arms)));
    };
    let extract_body = out.split_off_stream(extract_body_start);
    let extract_pat = {
        let pat_stream = {
            let len = out.buf.len();
            out.blit(146, 3);
            out.split_off_stream(len)
        };
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };
    {
        out.blit_ident(57);
        out.buf.push(extract_pat);
        out.blit(160, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, extract_body)));
    };
    let missing_tag_else_start = out.buf.len();
    {
        out.blit(99, 2);
        {
            let at = out.buf.len();
            out.blit(178, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(181, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(1);
    };
    let missing_tag_else = out.split_off_stream(missing_tag_else_start);
    {
        out.blit(186, 2);
        {
            let at = out.buf.len();
            out.blit_ident(64);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(463, 3);
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
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
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
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(178, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(181, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                let missing_content_else = out.split_off_stream(missing_content_else_start);
                {
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(59);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(508, 3);
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::Brace,
                        missing_content_else,
                    )));
                    out.blit(412, 2);
                    {
                        let at = out.buf.len();
                        out.blit(332, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(386, 6);
                            {
                                let at = out.buf.len();
                                out.blit(511, 3);
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
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                let missing_content_else_start = out.buf.len();
                {
                    out.blit(99, 2);
                    {
                        let at = out.buf.len();
                        out.blit(178, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(181, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                let missing_content_else = out.split_off_stream(missing_content_else_start);
                {
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(59);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(508, 3);
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::Brace,
                        missing_content_else,
                    )));
                    out.blit(514, 7);
                    {
                        let at = out.buf.len();
                        out.blit_ident(97);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(65, 2);
                };
                emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                {
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit(332, 3);
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
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    {
        out.blit(335, 4);
        {
            let at = out.buf.len();
            out.blit(339, 3);
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
        out.blit(477, 2);
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
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.blit(186, 2);
                        {
                            let at = out.buf.len();
                            out.blit(317, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(223, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(229, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(79);
                        {
                            let at = out.buf.len();
                            out.blit(233, 4);
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
                        out.blit(405, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(240, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(342, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(253, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(412, 2);
                        {
                            let at = out.buf.len();
                            out.blit(414, 4);
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
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(50);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    {
                        let at = out.buf.len();
                        out.blit(186, 2);
                        {
                            let at = out.buf.len();
                            out.blit(317, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(223, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                            out.blit(229, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(79);
                        {
                            let at = out.buf.len();
                            out.blit(233, 4);
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
                        out.blit(405, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(240, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(342, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(253, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(405, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(240, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(content_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(274, 6);
                            {
                                let at = out.buf.len();
                                out.blit(409, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(252, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(412, 2);
                        {
                            let at = out.buf.len();
                            out.blit(414, 4);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(317, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(418, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit(420, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(342, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(253, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(420, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(240, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(424, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(412, 2);
                    {
                        let at = out.buf.len();
                        out.blit(434, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(44);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(348, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
    };
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}
fn enum_from_item_untagged(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let last_index = variants.len() - 1;
    let last_is_unhinted =
        variants[last_index].try_if.is_none() && variants[last_index].final_if.is_none();
    for (i, variant) in variants.iter().enumerate() {
        let has_hint = variant.try_if.is_some() || variant.final_if.is_some();
        let is_last = i == last_index && !has_hint;
        let propagate = is_last || variant.final_if.is_some();
        let name_lit = variant_name_literal(ctx, variant);
        let inner_start = out.buf.len();
        match variant.kind {
            EnumKind::None => {
                if propagate {
                    {
                        out.blit(258, 3);
                        {
                            let at = out.buf.len();
                            out.blit_ident(39);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(353, 5);
                        {
                            let at = out.buf.len();
                            out.blit(521, 4);
                            out.buf.push(name_lit.into());
                            {
                                let at = out.buf.len();
                                out.blit(525, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(332, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(1);
                                out.tt_group(Delimiter::Brace, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.blit(99, 2);
                        {
                            let at = out.buf.len();
                            out.blit(339, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "a matching variant",
                                )));
                                out.blit(32, 2);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(1);
                    };
                } else {
                    {
                        out.blit(258, 3);
                        {
                            let at = out.buf.len();
                            out.blit_ident(39);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(353, 5);
                        {
                            let at = out.buf.len();
                            out.blit(521, 4);
                            out.buf.push(name_lit.into());
                            {
                                let at = out.buf.len();
                                out.blit(525, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(332, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(1);
                                out.tt_group(Delimiter::Brace, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                }
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported in untagged enums")
                }
                if propagate {
                    {
                        out.blit_ident(86);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(386, 6);
                        {
                            let at = out.buf.len();
                            out.blit(298, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        {
                            let at = out.buf.len();
                            out.blit_ident(95);
                            {
                                let at = out.buf.len();
                                out.blit_ident(85);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(527, 4);
                            {
                                let at = out.buf.len();
                                out.blit(332, 3);
                                out.push_ident(variant.name);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(85);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(531, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(63);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(118, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(63);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                } else {
                    {
                        {
                            let at = out.buf.len();
                            out.blit(533, 11);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(386, 6);
                            {
                                let at = out.buf.len();
                                out.blit(298, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            {
                                let at = out.buf.len();
                                out.blit_ident(95);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(85);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(527, 4);
                                {
                                    let at = out.buf.len();
                                    out.blit(332, 3);
                                    out.push_ident(variant.name);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(85);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(531, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(84);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(114, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(544, 5);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(38);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(1);
                                    out.tt_group(Delimiter::Brace, at);
                                };
                                out.tt_group(Delimiter::Brace, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                }
            }
            EnumKind::Struct => {
                if propagate {
                    {
                        out.blit(549, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(97);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(65, 2);
                    };
                    emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                    {
                        out.blit(525, 2);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
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
                        out.blit_punct(1);
                    };
                } else {
                    let closure_body_start = out.buf.len();
                    {
                        out.blit(549, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(97);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(65, 2);
                    };
                    emit_variant_fields_from_table(out, ctx, variant, variant.fields, &[]);
                    {
                        out.blit_ident(95);
                        {
                            let at = out.buf.len();
                            out.blit(332, 3);
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
                    let closure_body = out.split_off_stream(closure_body_start);
                    let closure_body_group =
                        TokenTree::Group(Group::new(Delimiter::Brace, closure_body));
                    {
                        {
                            let at = out.buf.len();
                            out.blit(555, 25);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(580, 5);
                            {
                                let at = out.buf.len();
                                out.blit(172, 2);
                                out.buf.push(closure_body_group);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(585, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(95);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(85);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(527, 4);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(85);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(531, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(84);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(114, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(544, 5);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(38);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(1);
                                    out.tt_group(Delimiter::Brace, at);
                                };
                                out.tt_group(Delimiter::Brace, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                }
            }
        }
        if let Some(predicate) = variant.try_if.as_deref().or(variant.final_if.as_deref()) {
            let inner_code = out.split_off_stream(inner_start);
            let inner_group = TokenTree::Group(Group::new(Delimiter::Brace, inner_code));
            let pred_stream: TokenStream = predicate.iter().cloned().collect();
            let pred_group = TokenTree::Group(Group::new(Delimiter::Parenthesis, pred_stream));
            {
                {
                    let at = out.buf.len();
                    out.blit(589, 4);
                    {
                        let at = out.buf.len();
                        out.blit(24, 2);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(26, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit(593, 3);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(36, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit_punct(2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(596, 4);
                    out.buf.push(pred_group);
                    out.blit(600, 3);
                    {
                        let at = out.buf.len();
                        out.blit(298, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.buf.push(inner_group);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
    }
    if !last_is_unhinted {
        {
            out.blit_ident(93);
            {
                let at = out.buf.len();
                out.blit(339, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("a matching variant")));
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
fn enum_to_item_untagged(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    out.blit(329, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(342, 6);
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
                    Error::msg("Only single-field tuple variants are supported in untagged enums")
                }
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(50);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(114, 2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(274, 6);
                    {
                        let at = out.buf.len();
                        out.blit(409, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
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
                    out.blit(186, 2);
                    {
                        let at = out.buf.len();
                        out.blit(317, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(223, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(229, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(79);
                    {
                        let at = out.buf.len();
                        out.blit(233, 4);
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
                    out.blit_ident(95);
                    {
                        let at = out.buf.len();
                        out.blit(414, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(332, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(44);
                                out.push_ident(field.name);
                                out.blit_punct(13);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(114, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(348, 2);
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
    if target.untagged && (target.tag.is_some() || target.content.is_some()) {
        Error::msg("untagged cannot be combined with tag or content attributes")
    }
    if !target.untagged {
        for variant in variants {
            if variant.try_if.is_some() || variant.final_if.is_some() {
                Error::span_msg(
                    "try_if/final_if can only be used on untagged enums",
                    variant.name.span(),
                )
            }
        }
    }
    let ctx = Ctx::new(output, target);
    let is_string_enum = !target.untagged
        && target.tag.is_none()
        && target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0
        && target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) == 0;
    if target.from_item {
        if target.untagged {
            enum_from_item_untagged(output, &ctx, variants);
        } else {
            match (&target.tag, &target.content) {
                (None, _) if is_string_enum => enum_from_item_string(output, &ctx, variants),
                (None, _) => enum_from_item_external(output, &ctx, variants),
                (Some(tag_lit), None) => enum_from_item_internal(output, &ctx, variants, tag_lit),
                (Some(tag_lit), Some(content_lit)) => {
                    enum_from_item_adjacent(output, &ctx, variants, tag_lit, content_lit)
                }
            }
        }
    }
    if target.to_item {
        if target.untagged {
            enum_to_item_untagged(output, &ctx, variants);
        } else {
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
        untagged: false,
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
        (&mut rust_writer).blit_punct(15);
        {
            let at = (&mut rust_writer).buf.len();
            (&mut rust_writer).blit_ident(1);
            {
                let at = (&mut rust_writer).buf.len();
                (&mut rust_writer).blit(603, 4);
                (&mut rust_writer).tt_group(Delimiter::Parenthesis, at);
            };
            (&mut rust_writer).tt_group(Delimiter::Bracket, at);
        };
        (&mut rust_writer).blit(607, 5);
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
