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
            buffer.blit_punct(0);
        }
        match generic.kind {
            GenericKind::Lifetime => {
                buffer.blit_punct(8);
            }
            GenericKind::Type => (),
            GenericKind::Const => {
                buffer.blit_ident(15);
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
        output.blit_punct(14);
        {
            let at = output.buf.len();
            output.blit_ident(14);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit(3, 3);
        output.push_ident(&ctx.lifetime);
        if !ctx.generics.is_empty() {
            output.blit_punct(0);
            fmt_generics(output, ctx.generics, DEF);
        };
        output.blit_punct(3);
        output.buf.extend_from_slice(&ctx.crate_path);
        output.blit(6, 5);
        output.push_ident(&ctx.lifetime);
        output.blit(11, 2);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(6);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(3);
        };
        if !target.where_clauses.is_empty() || !target.generic_field_types.is_empty() {
            output.blit_ident(28);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(15, 2);
            {
                let at = output.buf.len();
                output.blit(17, 4);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(21, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(26, 5);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(31, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(36, 14);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(50, 4);
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
    let start = out.buf.len();
    {
        out.blit(54, 7);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(61, 2);
    };
    for field in fields {
        let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
        let is_skip = field.flags & Field::WITH_FROMITEM_SKIP != 0;
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = is_option_type(field);
        let with_path = field.with(FROM_ITEM);
        if is_skip {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit_punct(2);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit(63, 7);
                    }
                }
            } else {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(63, 7);
            }
        } else if let Some(with) = with_path {
            if is_option || is_default {
                if let Some(default_kind) = field.default(FROM_ITEM) {
                    match default_kind {
                        DefaultKind::Custom(tokens) => {
                            {
                                out.blit_ident(77);
                                out.push_ident(field.name);
                                out.blit(70, 4);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.blit_punct(0);
                                    out.buf.extend_from_slice(with);
                                    out.blit(74, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(77, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(79, 2);
                                    out.buf.extend_from_slice(tokens.as_slice());
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(1);
                            };
                        }
                        DefaultKind::Default => {
                            {
                                out.blit_ident(77);
                                out.push_ident(field.name);
                                out.blit(70, 4);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.blit_punct(0);
                                    out.buf.extend_from_slice(with);
                                    out.blit(74, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(81, 4);
                            };
                        }
                    }
                } else {
                    {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit(70, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.blit_punct(0);
                            out.buf.extend_from_slice(with);
                            out.blit(74, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(81, 4);
                    };
                }
            } else {
                {
                    out.blit_ident(77);
                    out.push_ident(field.name);
                    out.blit(85, 4);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit_punct(0);
                        out.buf.extend_from_slice(with);
                        out.blit(74, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(61, 2);
                };
            }
        } else if is_option {
            {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(89, 4);
                {
                    let at = out.buf.len();
                    out.buf.push(name_lit.into());
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(77);
                            out.push_ident(field.name);
                            out.blit(89, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(77, 2);
                            {
                                let at = out.buf.len();
                                out.blit(79, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        {
                            out.blit_ident(77);
                            out.push_ident(field.name);
                            out.blit(89, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(81, 4);
                        };
                    }
                }
            } else {
                {
                    out.blit_ident(77);
                    out.push_ident(field.name);
                    out.blit(89, 4);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(81, 4);
                };
            }
        } else {
            {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(93, 4);
                {
                    let at = out.buf.len();
                    out.buf.push(name_lit.into());
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(61, 2);
            };
        }
    }
    {
        out.blit(97, 7);
        {
            let at = out.buf.len();
            out.blit_ident(72);
            {
                let at = out.buf.len();
                {
                    for field in fields {
                        out.push_ident(field.name);
                        out.blit_punct(0);
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
        output.blit_punct(14);
        {
            let at = output.buf.len();
            output.blit_ident(14);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit_ident(18);
        if !target.generics.is_empty() {
            output.blit_punct(6);
            fmt_generics(output, &target.generics, DEF);
            output.blit_punct(3);
        };
        output.buf.extend_from_slice(&ctx.crate_path);
        output.blit(104, 4);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(6);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(3);
        };
        if !target.where_clauses.is_empty() || !target.generic_field_types.is_empty() {
            output.blit_ident(28);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(108, 4);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(112, 4);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit_punct(3);
            {
                let at = output.buf.len();
                output.blit(116, 2);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit(118, 6);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(124, 5);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit_punct(3);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(36, 12);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(31, 5);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit(13, 2);
            output.buf.extend_from_slice(&ctx.crate_path);
            output.blit(50, 4);
            output
                .buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, inner)));
            output.tt_group(Delimiter::Brace, at);
        };
    };
}
fn struct_to_item(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let mut non_skip_count = 0usize;
    for field in fields {
        if field.flags & Field::WITH_TO_ITEM_SKIP == 0 {
            non_skip_count += 1;
        }
    }
    let start = out.buf.len();
    {
        out.blit(129, 2);
        {
            let at = out.buf.len();
            out.blit(131, 2);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(2);
        out.buf.extend_from_slice(&ctx.crate_path);
        out.blit(133, 6);
        {
            let at = out.buf.len();
            out.buf.push(TokenTree::Literal(Literal::usize_unsuffixed(
                non_skip_count,
            )));
            out.blit(139, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_ident(58);
        {
            let at = out.buf.len();
            out.blit(143, 4);
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
        if field.flags & Field::WITH_TO_ITEM_SKIP != 0 {
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
                out.blit(147, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(150, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.buf.extend_from_slice(with);
                    out.blit(156, 3);
                    {
                        let at = out.buf.len();
                        out.blit(159, 3);
                        out.push_ident(field.name);
                        out.blit(119, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(162, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(168, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(12);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(171, 6);
                {
                    let at = out.buf.len();
                    out.blit(159, 3);
                    out.push_ident(field.name);
                    out.blit(119, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(4);
                {
                    let at = out.buf.len();
                    out.blit(147, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(177, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(147, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(150, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(184, 6);
                    {
                        let at = out.buf.len();
                        out.blit(159, 3);
                        out.push_ident(field.name);
                        out.blit(119, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(162, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(190, 2);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(skip_tokens);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.blit(159, 3);
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, emit_body)));
            };
        }
    }
    {
        out.blit_ident(73);
        {
            let at = out.buf.len();
            out.blit(192, 4);
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
                output.blit_punct(6);
                output.buf.extend_from_slice(single_field.ty);
                output.blit_ident(13);
                output.buf.extend_from_slice(&ctx.crate_path);
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(196, 5);
                {
                    let at = output.buf.len();
                    output.blit(201, 3);
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
                output.blit(184, 6);
                {
                    let at = output.buf.len();
                    output.blit(159, 3);
                    output.push_ident(single_field.name);
                    output.blit(119, 2);
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
                output.blit_ident(73);
                {
                    let at = output.buf.len();
                    output.push_ident(&target.name);
                    {
                        let at = output.buf.len();
                        output.blit_punct(6);
                        output.buf.extend_from_slice(single_field.ty);
                        output.blit_ident(13);
                        output.buf.extend_from_slice(&ctx.crate_path);
                        output.blit(6, 5);
                        output.push_ident(&ctx.lifetime);
                        output.blit(196, 5);
                        {
                            let at = output.buf.len();
                            output.blit(201, 3);
                            output.tt_group(Delimiter::Parenthesis, at);
                        };
                        output.blit_punct(4);
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
                output.blit(184, 6);
                {
                    let at = output.buf.len();
                    output.blit(159, 3);
                    output
                        .buf
                        .push(TokenTree::Literal(Literal::usize_unsuffixed(0)));
                    output.blit(119, 2);
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
/// Emit field deserialization from a table helper for a struct variant.
/// Generates `let field_name = ...;` statements reading from `th`.
fn emit_variant_fields_from_th(
    out: &mut RustWriter,
    ctx: &Ctx,
    variant: &EnumVariant,
    fields: &[Field],
) {
    for field in fields {
        let name_lit = variant_field_name_literal(ctx, field, variant);
        let is_skip = field.flags & Field::WITH_FROMITEM_SKIP != 0;
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = is_option_type(field);
        let with_path = field.with(FROM_ITEM);
        if is_skip {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit_punct(2);
                        out.buf.extend_from_slice(tokens.as_slice());
                        out.blit_punct(1);
                    }
                    DefaultKind::Default => {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit(63, 7);
                    }
                }
            } else {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(63, 7);
            }
        } else if let Some(with) = with_path {
            if is_option || is_default {
                if let Some(default_kind) = field.default(FROM_ITEM) {
                    match default_kind {
                        DefaultKind::Custom(tokens) => {
                            {
                                out.blit_ident(77);
                                out.push_ident(field.name);
                                out.blit(204, 4);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.blit_punct(0);
                                    out.buf.extend_from_slice(with);
                                    out.blit(74, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(77, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(79, 2);
                                    out.buf.extend_from_slice(tokens.as_slice());
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(1);
                            };
                        }
                        DefaultKind::Default => {
                            {
                                out.blit_ident(77);
                                out.push_ident(field.name);
                                out.blit(204, 4);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.blit_punct(0);
                                    out.buf.extend_from_slice(with);
                                    out.blit(74, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(81, 4);
                            };
                        }
                    }
                } else {
                    {
                        out.blit_ident(77);
                        out.push_ident(field.name);
                        out.blit(204, 4);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.blit_punct(0);
                            out.buf.extend_from_slice(with);
                            out.blit(74, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(81, 4);
                    };
                }
            } else {
                {
                    out.blit_ident(77);
                    out.push_ident(field.name);
                    out.blit(208, 4);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit_punct(0);
                        out.buf.extend_from_slice(with);
                        out.blit(74, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(61, 2);
                };
            }
        } else if is_option {
            {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(212, 4);
                {
                    let at = out.buf.len();
                    out.buf.push(name_lit.into());
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        {
                            out.blit_ident(77);
                            out.push_ident(field.name);
                            out.blit(212, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(77, 2);
                            {
                                let at = out.buf.len();
                                out.blit(79, 2);
                                out.buf.extend_from_slice(tokens.as_slice());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(1);
                        };
                    }
                    DefaultKind::Default => {
                        {
                            out.blit_ident(77);
                            out.push_ident(field.name);
                            out.blit(212, 4);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(81, 4);
                        };
                    }
                }
            } else {
                {
                    out.blit_ident(77);
                    out.push_ident(field.name);
                    out.blit(212, 4);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(81, 4);
                };
            }
        } else {
            {
                out.blit_ident(77);
                out.push_ident(field.name);
                out.blit(216, 4);
                {
                    let at = out.buf.len();
                    out.buf.push(name_lit.into());
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(61, 2);
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
        if field.flags & Field::WITH_TO_ITEM_SKIP != 0 {
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
                out.blit(220, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(150, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.buf.extend_from_slice(with);
                    out.blit(156, 3);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(119, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(162, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(168, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(27);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
                out.buf.extend_from_slice(&ctx.crate_path);
                out.blit(171, 6);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(119, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(4);
                {
                    let at = out.buf.len();
                    out.blit(220, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(223, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(220, 3);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(150, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(184, 6);
                    {
                        let at = out.buf.len();
                        out.push_ident(field.name);
                        out.blit(119, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(162, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(1);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(190, 2);
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
        out.blit(230, 6);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(236, 4);
        {
            let at = out.buf.len();
            {
                for variant in variants {
                    let name_lit = variant_name_literal(ctx, variant);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(240, 3);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                    };
                }
            };
            out.blit(246, 4);
            {
                let at = out.buf.len();
                out.blit(250, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string(&expected_msg)));
                    out.blit(27, 2);
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
        out.blit_ident(73);
        {
            let at = out.buf.len();
            out.buf.extend_from_slice(&ctx.crate_path);
            out.blit(253, 6);
            {
                let at = out.buf.len();
                out.blit(259, 2);
                {
                    let at = out.buf.len();
                    {
                        for variant in variants {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.blit(243, 3);
                                out.push_ident(variant.name);
                                out.blit(240, 2);
                                out.buf.push(name_lit.into());
                                out.blit_punct(0);
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
            out.blit(261, 3);
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
                                out.blit(240, 3);
                                {
                                    let at = out.buf.len();
                                    out.blit(243, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(0);
                            };
                        }
                    }
                };
                out.blit(246, 4);
                {
                    let at = out.buf.len();
                    out.blit(250, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("a known variant")));
                        out.blit(27, 2);
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
            out.blit(168, 3);
            {
                let at = out.buf.len();
                out.blit_ident(37);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(264, 5);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, if_body)));
        };
    }
    if has_complex {
        {
            out.blit(269, 6);
            {
                let at = out.buf.len();
                out.blit_ident(78);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(275, 10);
        };
        let err_body_start = out.buf.len();
        {
            out.blit(285, 2);
            {
                let at = out.buf.len();
                out.blit(250, 3);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::Literal(Literal::string(
                        "a table with exactly one key",
                    )));
                    out.blit(27, 2);
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
            out.blit(190, 2);
            {
                let at = out.buf.len();
                out.blit(287, 6);
                out.buf.push(one_lit);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, err_body)));
        };
        {
            out.blit_ident(77);
            {
                let at = out.buf.len();
                out.blit(293, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(296, 3);
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
                        out.blit(240, 3);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(299, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(305, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                    };
                }
                EnumKind::Struct => {
                    let arm_body_start = out.buf.len();
                    {
                        out.blit(308, 7);
                        {
                            let at = out.buf.len();
                            out.blit_ident(78);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(61, 2);
                    };
                    emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                    {
                        out.blit(315, 7);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                {
                                    for field in variant.fields {
                                        out.push_ident(field.name);
                                        out.blit_punct(0);
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
                        out.blit(240, 2);
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                    };
                }
                EnumKind::None => {}
            }
        }
        {
            out.blit(246, 4);
            {
                let at = out.buf.len();
                out.blit(250, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("a known variant")));
                    out.blit(27, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
        };
        let arms = out.split_off_stream(arms_start);
        {
            out.blit(322, 4);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, arms)));
        };
    } else if !has_unit {
        {
            out.blit_ident(46);
            {
                let at = out.buf.len();
                out.blit(250, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("a known variant")));
                    out.blit(27, 2);
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
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    out.blit(240, 3);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(253, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported in external tagging")
                }
                {
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(35);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(129, 2);
                        {
                            let at = out.buf.len();
                            out.blit(326, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(133, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(139, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(58);
                        {
                            let at = out.buf.len();
                            out.blit(143, 4);
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
                        out.blit(328, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(150, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(184, 6);
                            {
                                let at = out.buf.len();
                                out.blit(332, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(162, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(102, 2);
                        {
                            let at = out.buf.len();
                            out.blit(335, 4);
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
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(129, 2);
                    {
                        let at = out.buf.len();
                        out.blit(326, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(133, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(139, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(58);
                    {
                        let at = out.buf.len();
                        out.blit(143, 4);
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
                    out.blit(129, 2);
                    {
                        let at = out.buf.len();
                        out.blit(339, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(133, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                        out.blit(139, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(58);
                    {
                        let at = out.buf.len();
                        out.blit(143, 4);
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
                    out.blit(341, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(345, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(102, 2);
                    {
                        let at = out.buf.len();
                        out.blit(355, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(29);
                                out.push_ident(field.name);
                                out.blit_punct(0);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(240, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(259, 2);
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
        out.blit(359, 7);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(366, 11);
        {
            let at = out.buf.len();
            out.buf.push(tag_lit.clone().into());
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(61, 2);
    };
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(315, 7);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                {
                    out.blit(315, 7);
                    {
                        let at = out.buf.len();
                        out.blit(243, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            {
                                for field in variant.fields {
                                    out.push_ident(field.name);
                                    out.blit_punct(0);
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
                    out.blit(240, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    {
        out.blit(246, 4);
        {
            let at = out.buf.len();
            out.blit(250, 3);
            {
                let at = out.buf.len();
                out.buf
                    .push(TokenTree::Literal(Literal::string("a known variant")));
                out.blit(27, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(0);
    };
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(377, 2);
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
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(129, 2);
                        {
                            let at = out.buf.len();
                            out.blit(326, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(133, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(139, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(58);
                        {
                            let at = out.buf.len();
                            out.blit(143, 4);
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
                        out.blit(328, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(150, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(253, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(163, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(102, 2);
                        {
                            let at = out.buf.len();
                            out.blit(335, 4);
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
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(129, 2);
                    {
                        let at = out.buf.len();
                        out.blit(326, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(133, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip + 1)));
                        out.blit(139, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(58);
                    {
                        let at = out.buf.len();
                        out.blit(143, 4);
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
                    out.blit(328, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(253, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(163, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(1);
                };
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                {
                    out.blit_ident(73);
                    {
                        let at = out.buf.len();
                        out.blit(335, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(29);
                                out.push_ident(field.name);
                                out.blit_punct(0);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(240, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
            EnumKind::Tuple => {}
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(259, 2);
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
        out.blit(359, 7);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(366, 11);
        {
            let at = out.buf.len();
            out.buf.push(tag_lit.clone().into());
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(61, 2);
    };
    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(315, 7);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
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
                {
                    out.buf.push(name_lit.into());
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(379, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(385, 9);
                        {
                            let at = out.buf.len();
                            out.blit(243, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.extend_from_slice(&ctx.crate_path);
                                out.blit(299, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(394, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                {
                    out.blit(379, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(content_lit.clone().into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(397, 9);
                    {
                        let at = out.buf.len();
                        out.blit_ident(78);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(61, 2);
                };
                emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                {
                    out.blit(315, 7);
                    {
                        let at = out.buf.len();
                        out.blit(243, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            {
                                for field in variant.fields {
                                    out.push_ident(field.name);
                                    out.blit_punct(0);
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
                    out.blit(240, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    {
        out.blit(246, 4);
        {
            let at = out.buf.len();
            out.blit(250, 3);
            {
                let at = out.buf.len();
                out.buf
                    .push(TokenTree::Literal(Literal::string("a known variant")));
                out.blit(27, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(0);
    };
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(377, 2);
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
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(129, 2);
                        {
                            let at = out.buf.len();
                            out.blit(326, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(133, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(1)));
                            out.blit(139, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(58);
                        {
                            let at = out.buf.len();
                            out.blit(143, 4);
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
                        out.blit(328, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(150, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(253, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(163, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(102, 2);
                        {
                            let at = out.buf.len();
                            out.blit(335, 4);
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
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        out.blit_ident(35);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(240, 2);
                    {
                        let at = out.buf.len();
                        out.blit(129, 2);
                        {
                            let at = out.buf.len();
                            out.blit(326, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(133, 6);
                        {
                            let at = out.buf.len();
                            out.buf
                                .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                            out.blit(139, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_ident(58);
                        {
                            let at = out.buf.len();
                            out.blit(143, 4);
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
                        out.blit(328, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(150, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(tag_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(253, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(name_lit.into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(163, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(328, 4);
                        {
                            let at = out.buf.len();
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(150, 6);
                            {
                                let at = out.buf.len();
                                out.buf.push(content_lit.clone().into());
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.buf.extend_from_slice(&ctx.crate_path);
                            out.blit(184, 6);
                            {
                                let at = out.buf.len();
                                out.blit(332, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(162, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(102, 2);
                        {
                            let at = out.buf.len();
                            out.blit(335, 4);
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
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();
                let arm_body_start = out.buf.len();
                {
                    out.blit(129, 2);
                    {
                        let at = out.buf.len();
                        out.blit(326, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(133, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(non_skip)));
                        out.blit(139, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(58);
                    {
                        let at = out.buf.len();
                        out.blit(143, 4);
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
                    out.blit(129, 2);
                    {
                        let at = out.buf.len();
                        out.blit(339, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.buf.extend_from_slice(&ctx.crate_path);
                    out.blit(133, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::usize_unsuffixed(2)));
                        out.blit(139, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_ident(58);
                    {
                        let at = out.buf.len();
                        out.blit(143, 4);
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
                    out.blit(341, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(tag_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(253, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(163, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(341, 4);
                    {
                        let at = out.buf.len();
                        out.buf.extend_from_slice(&ctx.crate_path);
                        out.blit(150, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(345, 10);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(102, 2);
                    {
                        let at = out.buf.len();
                        out.blit(355, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                let arm_body = out.split_off_stream(arm_body_start);
                {
                    out.blit(243, 3);
                    out.push_ident(variant.name);
                    {
                        let at = out.buf.len();
                        {
                            for field in variant.fields {
                                out.blit_ident(29);
                                out.push_ident(field.name);
                                out.blit_punct(0);
                            }
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit(240, 2);
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
                };
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    {
        out.blit(259, 2);
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
        (&mut rust_writer).blit_punct(14);
        {
            let at = (&mut rust_writer).buf.len();
            (&mut rust_writer).blit_ident(2);
            {
                let at = (&mut rust_writer).buf.len();
                (&mut rust_writer).blit(406, 4);
                (&mut rust_writer).tt_group(Delimiter::Parenthesis, at);
            };
            (&mut rust_writer).tt_group(Delimiter::Bracket, at);
        };
        (&mut rust_writer).blit(410, 5);
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
