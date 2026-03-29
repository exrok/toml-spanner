use crate::ast::{
    self, DefaultKind, DeriveTargetInner, DeriveTargetKind, EnumKind, EnumVariant, Field,
    FieldAttrs, Generic, GenericKind, UnknownFieldPolicy, ENUM_CONTAINS_STRUCT_VARIANT,
    ENUM_CONTAINS_TUPLE_VARIANT, ENUM_CONTAINS_UNIT_VARIANT, FROM_TOML, TO_TOML,
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
fn option_inner_ty(ty: &[TokenTree]) -> &[TokenTree] {
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
                buffer.blit_punct(8);
            }
            GenericKind::Type => (),
            GenericKind::Const => {
                buffer.blit_ident(43);
            }
        }
        buffer.buf.push(generic.ident.clone().into());
        if fmt.bounds && !generic.bounds.is_empty() {
            buffer.blit_punct(9);
            for tok in generic.bounds {
                buffer.buf.push(tok.clone());
            }
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
    crate_path: TokenTree,
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
            TokenTree::Group(Group::new(Delimiter::None, out.split_off_stream(0)))
        } else {
            {
                out.blit(0, 3);
            };
            TokenTree::Group(Group::new(Delimiter::None, out.split_off_stream(0)))
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
fn field_name_lit(ctx: &Ctx, field: &Field, variant: Option<&EnumVariant>) -> Literal {
    if let Some(name) = field.attr.rename(FROM_TOML) {
        return name.clone();
    }
    let rule = match variant {
        Some(v) if v.rename_all != RenameRule::None => v.rename_all,
        Some(_) => ctx.target.rename_all_fields,
        None => ctx.target.rename_all,
    };
    if rule != RenameRule::None {
        Literal::string(&rule.apply_to_field(&field.name.to_string()))
    } else {
        Literal::string(&field.name.to_string())
    }
}
fn impl_from_toml(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    {
        output.blit_punct(15);
        {
            let at = output.buf.len();
            output.blit_ident(42);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit(3, 3);
        output.push_ident(&ctx.lifetime);
        if !ctx.generics.is_empty() {
            output.blit_punct(13);
            fmt_generics(output, ctx.generics, DEF);
        };
        output.blit_punct(2);
        output.buf.push(ctx.crate_path.clone());
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
            output.blit_ident(52);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(ctx.crate_path.clone());
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(ctx.crate_path.clone());
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
                output.buf.push(ctx.crate_path.clone());
                output.blit(26, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(31, 5);
                output.buf.push(ctx.crate_path.clone());
                output.blit(36, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 14);
            output.buf.push(ctx.crate_path.clone());
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
fn impl_to_toml(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    let lf = Ident::new("__de", Span::mixed_site());
    {
        output.blit_punct(15);
        {
            let at = output.buf.len();
            output.blit_ident(42);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit_ident(47);
        if !target.generics.is_empty() {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, DEF);
            output.blit_punct(2);
        };
        output.buf.push(ctx.crate_path.clone());
        output.blit(59, 4);
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
            output.blit_ident(52);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(ctx.crate_path.clone());
                output.blit(63, 4);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(ctx.crate_path.clone());
                output.blit(67, 4);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(71, 4);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit_punct(2);
            {
                let at = output.buf.len();
                output.blit(75, 2);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit(77, 6);
                output.buf.push(TokenTree::from(lf.clone()));
                output.buf.push(ctx.crate_path.clone());
                output.blit(83, 3);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 12);
            output.buf.push(ctx.crate_path.clone());
            output.blit(36, 5);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit(13, 2);
            output.buf.push(ctx.crate_path.clone());
            output.blit(86, 4);
            output
                .buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, inner)));
            output.tt_group(Delimiter::Brace, at);
        };
    };
}
fn emit_table_alloc(out: &mut RustWriter, ctx: &Ctx, var: &str, capacity: usize) {
    let var_id = Ident::new(var, Span::mixed_site());
    {
        out.blit(90, 2);
        {
            let at = out.buf.len();
            out.blit_ident(106);
            out.buf.push(TokenTree::from(var_id.clone()));
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(3);
        out.buf.push(ctx.crate_path.clone());
        out.blit(92, 6);
        {
            let at = out.buf.len();
            out.buf
                .push(TokenTree::Literal(Literal::usize_unsuffixed(capacity)));
            out.blit(78, 2);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_ident(79);
        {
            let at = out.buf.len();
            out.blit(98, 2);
            {
                let at = out.buf.len();
                out.buf.push(ctx.crate_path.clone());
                out.blit(100, 6);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::Literal(Literal::string(
                        "Table capacity exceeded maximum",
                    )));
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit_punct(0);
    };
}
fn emit_table_field_deser(
    out: &mut RustWriter,
    ctx: &Ctx,
    fields: &[Field],
    table_ident: &str,
    variant: Option<&EnumVariant>,
    skip_keys: &[Literal],
) {
    let recoverable = ctx.target.recoverable;
    let mut flatten_field: Option<&Field> = None;
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            if flatten_field.is_some() {
                Error::msg("Only one #[toml(flatten)] field is allowed")
            }
            flatten_field = Some(field);
        }
    }
    if recoverable {
        {
            out.blit(106, 6);
        };
        {
            out.blit(112, 6);
            out.buf.push(TokenTree::Literal(Literal::u64_suffixed(0)));
            out.blit_punct(0);
        };
    }
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            {
                out.blit(118, 4);
            };
            emit_flatten_prefix(out, ctx, field, FROM_TOML);
            {
                out.blit(122, 5);
            };
            continue;
        }
        if field.flags & Field::WITH_FROM_TOML_SKIP != 0 {
            emit_field_default(out, field, FROM_TOML, false);
        } else if field.flags & Field::WITH_FROM_TOML_OPTION != 0 {
            out.blit(106, 2);
            out.push_ident(field.name);
            out.blit_punct(9);
            out.buf.extend_from_slice(field.ty);
            out.blit(127, 3);
        } else {
            out.blit(106, 2);
            out.push_ident(field.name);
            out.blit(130, 5);
            out.buf.extend_from_slice(field.ty);
            out.blit(135, 2);
        }
        if field.attr.has_aliases(FROM_TOML) {
            let span_ident = Ident::new(
                &{
                    let mut s = field.name.to_string();
                    s.push_str("_first_span");
                    s
                },
                Span::mixed_site(),
            );
            let zero = TokenTree::Literal(Literal::u32_suffixed(0));
            {
                out.blit(106, 2);
                out.push_ident(&span_ident);
                out.blit(137, 8);
                {
                    let at = out.buf.len();
                    out.buf.push(zero.clone());
                    out.blit_punct(13);
                    out.buf.push(zero);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(0);
            };
        }
    }
    emit_for_table_header(out, table_ident);
    let for_body_at = out.buf.len();
    {
        out.blit(145, 4);
    };
    let arms_at = out.buf.len();
    for skip_key in skip_keys {
        out.buf.push(skip_key.clone().into());
        out.blit(149, 3);
    }
    let mut required_idx: u32 = 0;
    for field in fields {
        if field.flags & (Field::WITH_FROM_TOML_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = field_name_lit(ctx, field, variant);
        let is_default = field.flags & Field::WITH_FROM_TOML_DEFAULT != 0;
        let is_option = field.flags & Field::WITH_FROM_TOML_OPTION != 0;
        let is_required = !is_option && !is_default;
        let has_aliases = field.attr.has_aliases(FROM_TOML);
        let has_deprecated_aliases = field.attr.has_deprecated_aliases(FROM_TOML);
        {
            out.buf.push(name_lit.clone().into());
        };
        field.attr.for_each_alias(FROM_TOML, &mut |alias| {
            out.blit_punct(11);
            out.buf.push(alias.clone().into());
        });
        field
            .attr
            .for_each_deprecated_alias(FROM_TOML, &mut |_, alias| {
                out.blit_punct(11);
                out.buf.push(alias.clone().into());
            });
        {
            out.blit(149, 2);
        };
        let arm_body_at = out.buf.len();
        if recoverable && is_required {
            if required_idx < 64 {
                let mask = TokenTree::Literal(Literal::u64_suffixed(1u64 << required_idx));
                {
                    out.blit_ident(58);
                    {
                        out.blit(152, 2);
                    };
                    out.buf.push(mask);
                    out.blit_punct(0);
                };
            }
            required_idx += 1;
        }
        if has_aliases {
            let span_ident = Ident::new(
                &{
                    let mut s = field.name.to_string();
                    s.push_str("_first_span");
                    s
                },
                Span::mixed_site(),
            );
            {
                out.blit_ident(110);
                out.push_ident(field.name);
                out.blit(154, 3);
                {
                    let at = out.buf.len();
                    out.blit(98, 2);
                    {
                        let at = out.buf.len();
                        out.blit(157, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.clone().into());
                            out.blit(160, 5);
                            out.push_ident(&span_ident);
                            out.blit(165, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        if has_deprecated_aliases {
            let name_for_new = name_lit.clone();
            field
                .attr
                .for_each_deprecated_alias(FROM_TOML, &mut |tag, alias| {
                    {
                        out.blit(167, 6);
                        out.buf.push(alias.clone().into());
                        {
                            let at = out.buf.len();
                            out.blit(173, 3);
                            {
                                let at = out.buf.len();
                                {
                                    if let Some(tag_tokens) = tag {
                                        out.buf.extend_from_slice(tag_tokens);
                                    } else {
                                        out.buf.push(TokenTree::Literal(Literal::u32_suffixed(0)));
                                    }
                                };
                                out.blit(176, 2);
                                out.buf.push(alias.clone().into());
                                out.blit(176, 2);
                                out.buf.push(name_for_new.clone().into());
                                out.blit(178, 6);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                });
        }
        {
            out.blit_ident(103);
        };
        let ty = if is_option {
            option_inner_ty(field.ty)
        } else {
            field.ty
        };
        emit_from_toml_call(out, ctx, field, ty);
        let match_body_at = out.buf.len();
        if has_aliases {
            let span_ident = Ident::new(
                &{
                    let mut s = field.name.to_string();
                    s.push_str("_first_span");
                    s
                },
                Span::mixed_site(),
            );
            {
                out.blit_ident(114);
                {
                    let at = out.buf.len();
                    out.blit_ident(105);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(149, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(184, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(105);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.push_ident(&span_ident);
                    out.blit(186, 5);
                    out.tt_group(Delimiter::Brace, at);
                };
                if is_required && !recoverable {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(94);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(191, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(94);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                };
                if is_required && recoverable {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(108);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(149, 2);
                    {
                        let at = out.buf.len();
                        out.blit(195, 4);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_punct(13);
                };
                if !is_required {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(108);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(199, 4);
                };
            };
        } else {
            {
                out.blit_ident(114);
                {
                    let at = out.buf.len();
                    out.blit_ident(105);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(149, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.blit(184, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(105);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.tt_group(Delimiter::Brace, at);
                };
                if is_required && !recoverable {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(94);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(191, 4);
                    {
                        let at = out.buf.len();
                        out.blit_ident(94);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                };
                if is_required && recoverable {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(108);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(149, 2);
                    {
                        let at = out.buf.len();
                        out.blit(195, 4);
                        out.tt_group(Delimiter::Brace, at);
                    };
                    out.blit_punct(13);
                };
                if !is_required {
                    out.blit_ident(113);
                    {
                        let at = out.buf.len();
                        out.blit_ident(108);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(199, 4);
                };
            };
        }
        out.tt_group(Delimiter::Brace, match_body_at);
        out.tt_group(Delimiter::Brace, arm_body_at);
    }
    if let Some(ff) = flatten_field {
        {
            out.blit(203, 3);
        };
        let wild_at = out.buf.len();
        {
            out.blit(206, 3);
        };
        emit_flatten_prefix(out, ctx, ff, FROM_TOML);
        {
            out.blit(209, 3);
            {
                let at = out.buf.len();
                out.blit(212, 9);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
        };
        out.tt_group(Delimiter::Brace, wild_at);
    } else {
        emit_unknown_field_arm(out, ctx);
    }
    out.tt_group(Delimiter::Brace, arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);
    if let Some(ff) = flatten_field {
        let table_id = Ident::new(table_ident, Span::mixed_site());
        {
            out.blit_ident(117);
            out.push_ident(ff.name);
            out.blit_punct(3);
        };
        emit_flatten_prefix(out, ctx, ff, FROM_TOML);
        {
            out.blit(221, 3);
            {
                let at = out.buf.len();
                out.blit(212, 2);
                out.buf.push(TokenTree::from(table_id.clone()));
                out.blit(224, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(226, 2);
        };
    }
    if recoverable {
        required_idx = 0;
        for field in fields {
            if field.flags
                & (Field::WITH_FROM_TOML_SKIP | Field::WITH_FLATTEN | Field::WITH_FROM_TOML_OPTION)
                != 0
            {
                continue;
            }
            if field.flags & Field::WITH_FROM_TOML_DEFAULT != 0 {
                continue;
            }
            let name_lit = field_name_lit(ctx, field, variant);
            {
                out.blit_ident(110);
                out.push_ident(field.name);
                out.blit(228, 3);
            };
            let outer_at = out.buf.len();
            if required_idx < 64 {
                let mask = TokenTree::Literal(Literal::u64_suffixed(1u64 << required_idx));
                let zero = TokenTree::Literal(Literal::u64_suffixed(0));
                {
                    out.blit_ident(110);
                };
                let paren_at = out.buf.len();
                {
                    out.blit(231, 2);
                    out.buf.push(mask);
                };
                out.tt_group(Delimiter::Parenthesis, paren_at);
                {
                    out.blit(171, 2);
                    out.buf.push(zero);
                };
                let inner_at = out.buf.len();
                {
                    out.blit(233, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(32, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                };
                out.tt_group(Delimiter::Brace, inner_at);
            } else {
                {
                    out.blit(236, 3);
                };
                let inner_at = out.buf.len();
                {
                    out.blit(233, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(32, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                };
                out.tt_group(Delimiter::Brace, inner_at);
            }
            {
                out.blit(195, 4);
            };
            out.tt_group(Delimiter::Brace, outer_at);
            required_idx += 1;
        }
        {
            out.blit(239, 2);
        };
        let if_at = out.buf.len();
        {
            out.blit(98, 2);
            {
                let at = out.buf.len();
                out.buf.push(ctx.crate_path.clone());
                out.blit(55, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
        };
        out.tt_group(Delimiter::Brace, if_at);
    }
    for field in fields {
        if field.flags
            & (Field::WITH_FROM_TOML_SKIP | Field::WITH_FLATTEN | Field::WITH_FROM_TOML_OPTION)
            != 0
        {
            continue;
        }
        if field.flags & Field::WITH_FROM_TOML_DEFAULT != 0 {
            emit_field_default(out, field, FROM_TOML, true);
        } else if recoverable {
            out.blit_ident(117);
            out.push_ident(field.name);
            out.blit_punct(3);
            out.push_ident(field.name);
            out.blit(241, 4);
        } else {
            let name_lit = field_name_lit(ctx, field, variant);
            {
                out.blit(90, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(245, 4);
            };
            let else_at = out.buf.len();
            {
                out.blit(98, 2);
                {
                    let at = out.buf.len();
                    out.blit(233, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(32, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(0);
            };
            out.tt_group(Delimiter::Brace, else_at);
            {
                out.blit_punct(0);
            };
        }
    }
}
/// Emit a field default: either as initial `let field = default;` or as unwrap `let field = field.unwrap_or...;`
fn emit_field_default(out: &mut RustWriter, field: &Field, direction: u8, is_unwrap: bool) {
    if is_unwrap {
        if let Some(default_kind) = field.default(direction) {
            match default_kind {
                DefaultKind::Custom(tokens) => {
                    {
                        out.blit_ident(117);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(249, 2);
                        {
                            let at = out.buf.len();
                            out.blit(251, 2);
                            out.buf.extend_from_slice(tokens.as_slice());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                    };
                }
                DefaultKind::Default => {
                    out.blit_ident(117);
                    out.push_ident(field.name);
                    out.blit_punct(3);
                    out.push_ident(field.name);
                    out.blit(253, 4);
                }
            }
        } else {
            out.blit_ident(117);
            out.push_ident(field.name);
            out.blit_punct(3);
            out.push_ident(field.name);
            out.blit(253, 4);
        }
    } else {
        if let Some(default_kind) = field.default(direction) {
            match default_kind {
                DefaultKind::Custom(tokens) => {
                    out.blit_ident(117);
                    out.push_ident(field.name);
                    out.blit_punct(3);
                    out.buf.extend_from_slice(tokens.as_slice());
                    out.blit_punct(0);
                }
                DefaultKind::Default => {
                    out.blit_ident(117);
                    out.push_ident(field.name);
                    out.blit(257, 7);
                }
            }
        } else {
            out.blit_ident(117);
            out.push_ident(field.name);
            out.blit(257, 7);
        }
    }
}
fn emit_table_field_ser(
    out: &mut RustWriter,
    ctx: &Ctx,
    fields: &[Field],
    table_ident: &str,
    variant: Option<&EnumVariant>,
    self_access: bool,
) {
    let table_id = Ident::new(table_ident, Span::mixed_site());
    for field in fields {
        if field.flags & (Field::WITH_TO_TOML_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = field_name_lit(ctx, field, variant);
        let with_path = field.with(TO_TOML);
        let skip_if = field.skip(TO_TOML).filter(|tokens| !tokens.is_empty());
        let first_ty_ident = if let Some(TokenTree::Ident(ident)) = field.ty.first() {
            ident.to_string()
        } else {
            String::new()
        };
        let field_ref = {
            let len = out.buf.len();
            if self_access {
                out.blit(264, 3);
                out.push_ident(field.name);
            };
            if !self_access {
                out.push_ident(field.name);
            };
            out.split_off_stream(len)
        };
        let is_option = first_ty_ident == "Option";
        let style = field.style(TO_TOML);
        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            let val_expr = if is_option {
                {
                    out.blit(267, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(105);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(3);
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::None,
                        field_ref.clone(),
                    )));
                };
                let len = out.buf.len();
                out.blit_ident(105);
                out.split_off_stream(len)
            } else {
                field_ref.clone()
            };
            let insert_start = out.buf.len();
            {
                out.buf.push(TokenTree::from(table_id.clone()));
                out.blit(270, 2);
                {
                    let at = out.buf.len();
                    out.buf.push(ctx.crate_path.clone());
                    out.blit(272, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(278, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::None, val_expr)));
                        out.blit(78, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(7);
                    if let Some(style) = style {
                        out.blit(281, 2);
                        {
                            let at = out.buf.len();
                            out.buf.push(ctx.crate_path.clone());
                            out.blit(283, 5);
                            out.buf.push(TokenTree::from(style.clone()));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                    };
                    out.blit(288, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(0);
            };
            if is_option {
                let insert_body = out.split_off_stream(insert_start);
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, insert_body)));
            }
        } else if is_option {
            {
                out.blit(267, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(105);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.push(ctx.crate_path.clone());
                out.blit(291, 6);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::None,
                        field_ref.clone(),
                    )));
                    out.blit(78, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(7);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::from(table_id.clone()));
                    out.blit(270, 2);
                    {
                        let at = out.buf.len();
                        out.buf.push(ctx.crate_path.clone());
                        out.blit(272, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(297, 2);
                        if let Some(style) = style {
                            out.blit(281, 2);
                            {
                                let at = out.buf.len();
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(283, 5);
                                out.buf.push(TokenTree::from(style.clone()));
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                        out.blit(288, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.buf.push(TokenTree::from(table_id.clone()));
                out.blit(270, 2);
                {
                    let at = out.buf.len();
                    out.buf.push(ctx.crate_path.clone());
                    out.blit(272, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.push(ctx.crate_path.clone());
                    out.blit(299, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(TokenTree::Group(Group::new(
                            Delimiter::None,
                            field_ref.clone(),
                        )));
                        out.blit(78, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(7);
                    if let Some(style) = style {
                        out.blit(281, 2);
                        {
                            let at = out.buf.len();
                            out.buf.push(ctx.crate_path.clone());
                            out.blit(283, 5);
                            out.buf.push(TokenTree::from(style.clone()));
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                    };
                    out.blit(288, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(0);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(236, 2);
                {
                    let at = out.buf.len();
                    out.buf.extend_from_slice(skip_tokens);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::None, field_ref)));
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, emit_body)));
            };
        }
    }
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            let field_ref = {
                let len = out.buf.len();
                if self_access {
                    out.blit(264, 3);
                    out.push_ident(field.name);
                };
                if !self_access {
                    out.push_ident(field.name);
                };
                out.split_off_stream(len)
            };
            emit_flatten_prefix(out, ctx, field, TO_TOML);
            {
                out.blit(305, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Group(Group::new(Delimiter::None, field_ref)));
                    out.blit(308, 5);
                    out.buf.push(TokenTree::from(table_id.clone()));
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(226, 2);
            };
        }
    }
}
fn emit_struct_variant_to_arm(out: &mut RustWriter, variant: &EnumVariant, arm_body_start: usize) {
    let arm_body = out.split_off_stream(arm_body_start);
    {
        out.blit(313, 3);
        out.push_ident(variant.name);
        {
            let at = out.buf.len();
            {
                for field in variant.fields {
                    out.blit_ident(22);
                    out.push_ident(field.name);
                    out.blit_punct(13);
                }
            };
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit(149, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
    };
}
fn count_ser_fields(fields: &[Field]) -> usize {
    let mut count = 0;
    for f in fields {
        if f.flags & (Field::WITH_TO_TOML_SKIP | Field::WITH_FLATTEN) == 0 {
            count += 1;
        }
    }
    count
}
/// Emit Ok(Self::Variant { field1, field2, ... }) for struct variant deserialization
fn emit_ok_self_variant(out: &mut RustWriter, variant: &EnumVariant) {
    {
        out.blit_ident(114);
        {
            let at = out.buf.len();
            out.blit(313, 3);
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
}
fn struct_from_toml(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let start = out.buf.len();
    {
        out.blit(316, 6);
        {
            let at = out.buf.len();
            out.blit_ident(116);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(226, 2);
    };
    emit_table_field_deser(out, ctx, fields, "__table", None, &[]);
    {
        out.blit_ident(114);
        {
            let at = out.buf.len();
            out.blit_ident(112);
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
    impl_from_toml(out, ctx, body);
}
fn struct_to_toml(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let start = out.buf.len();
    emit_table_alloc(out, ctx, "__table", count_ser_fields(fields));
    emit_table_field_ser(out, ctx, fields, "__table", None, true);
    {
        out.blit_ident(114);
        {
            let at = out.buf.len();
            out.blit(322, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_to_toml(out, ctx, body);
}
fn emit_proxy_from_toml(output: &mut RustWriter, ctx: &Ctx) -> bool {
    if let Some(from_ty) = &ctx.target.from_type {
        let body = {
            let len = output.buf.len();
            output.blit(326, 4);
            output.buf.extend_from_slice(from_ty);
            output.blit_ident(83);
            output.buf.push(ctx.crate_path.clone());
            output.blit(6, 5);
            output.push_ident(&ctx.lifetime);
            output.blit(330, 5);
            {
                let at = output.buf.len();
                output.blit(335, 3);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(338, 3);
            {
                let at = output.buf.len();
                output.blit(341, 12);
                {
                    let at = output.buf.len();
                    output.blit_ident(68);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.split_off_stream(len)
        };
        impl_from_toml(output, &ctx, body);
        true
    } else if let Some(try_from_ty) = &ctx.target.try_from_type {
        let body = {
            let len = output.buf.len();
            output.blit(326, 4);
            output.buf.extend_from_slice(try_from_ty);
            output.blit_ident(83);
            output.buf.push(ctx.crate_path.clone());
            output.blit(6, 5);
            output.push_ident(&ctx.lifetime);
            output.blit(330, 5);
            {
                let at = output.buf.len();
                output.blit(335, 3);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(353, 15);
            {
                let at = output.buf.len();
                output.blit_ident(68);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            {
                let at = output.buf.len();
                output.blit_ident(114);
                {
                    let at = output.buf.len();
                    output.blit_ident(105);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.blit(368, 3);
                {
                    let at = output.buf.len();
                    output.blit_ident(105);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.blit(371, 2);
                {
                    let at = output.buf.len();
                    output.blit_ident(94);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.blit(373, 3);
                {
                    let at = output.buf.len();
                    output.blit(376, 3);
                    {
                        let at = output.buf.len();
                        output.buf.push(ctx.crate_path.clone());
                        output.blit(379, 6);
                        {
                            let at = output.buf.len();
                            output.blit(385, 6);
                            output.tt_group(Delimiter::Parenthesis, at);
                        };
                        output.tt_group(Delimiter::Parenthesis, at);
                    };
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.blit_punct(13);
                output.tt_group(Delimiter::Brace, at);
            };
            output.split_off_stream(len)
        };
        impl_from_toml(output, &ctx, body);
        true
    } else {
        false
    }
}
fn handle_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);
    if target.from_toml && !emit_proxy_from_toml(output, &ctx) {
        if target.transparent_impl {
            let [single_field] = fields else {
                Error::msg("Struct must contain a single field to use transparent")
            };
            let body = {
                let len = output.buf.len();
                output.blit_punct(5);
                output.buf.extend_from_slice(single_field.ty);
                output.blit_ident(83);
                output.buf.push(ctx.crate_path.clone());
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(330, 5);
                {
                    let at = output.buf.len();
                    output.blit(335, 3);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_from_toml(output, &ctx, body);
        } else {
            struct_from_toml(output, &ctx, fields);
        }
    }
    if target.to_toml {
        if target.transparent_impl {
            let [single_field] = fields else {
                Error::msg("Struct must contain a single field to use transparent")
            };
            let body = {
                let len = output.buf.len();
                output.buf.push(ctx.crate_path.clone());
                output.blit(299, 6);
                {
                    let at = output.buf.len();
                    output.blit(264, 3);
                    output.push_ident(single_field.name);
                    output.blit(78, 2);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_to_toml(output, &ctx, body);
        } else {
            struct_to_toml(output, &ctx, fields);
        }
    }
}
fn handle_tuple_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);
    if target.from_toml && !emit_proxy_from_toml(output, &ctx) {
        if let [single_field] = fields {
            let body = {
                let len = output.buf.len();
                output.blit_ident(114);
                {
                    let at = output.buf.len();
                    output.push_ident(&target.name);
                    {
                        let at = output.buf.len();
                        output.blit_punct(5);
                        output.buf.extend_from_slice(single_field.ty);
                        output.blit_ident(83);
                        output.buf.push(ctx.crate_path.clone());
                        output.blit(6, 5);
                        output.push_ident(&ctx.lifetime);
                        output.blit(330, 5);
                        {
                            let at = output.buf.len();
                            output.blit(335, 3);
                            output.tt_group(Delimiter::Parenthesis, at);
                        };
                        output.blit_punct(7);
                        output.tt_group(Delimiter::Parenthesis, at);
                    };
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_from_toml(output, &ctx, body);
        } else {
            Error::msg(
                "FromToml on tuple structs requires exactly one field (transparent delegation)",
            )
        }
    }
    if target.to_toml {
        if let [_single_field] = fields {
            let body = {
                let len = output.buf.len();
                output.buf.push(ctx.crate_path.clone());
                output.blit(299, 6);
                {
                    let at = output.buf.len();
                    output.blit(264, 3);
                    output
                        .buf
                        .push(TokenTree::Literal(Literal::usize_unsuffixed(0)));
                    output.blit(78, 2);
                    output.tt_group(Delimiter::Parenthesis, at);
                };
                output.split_off_stream(len)
            };
            impl_to_toml(output, &ctx, body);
        } else {
            Error::msg(
                "ToToml on tuple structs requires exactly one field (transparent delegation)",
            )
        }
    }
}
fn variant_name_literal(ctx: &Ctx, variant: &EnumVariant) -> Literal {
    if let Some(name) = variant.rename(FROM_TOML) {
        return name.clone();
    }
    let raw = variant.name.to_string();
    if ctx.target.rename_all != RenameRule::None {
        Literal::string(&ctx.target.rename_all.apply_to_variant(&raw))
    } else {
        Literal::string(&raw)
    }
}
enum TagMode<'a> {
    External,
    Internal(&'a Literal),
    Adjacent(&'a Literal, &'a Literal),
    Untagged,
}
/// Emit a table.insert call for a tag key
fn emit_tag_insert(
    out: &mut RustWriter,
    ctx: &Ctx,
    table_var: &str,
    tag_lit: &Literal,
    name_lit: Literal,
) {
    let table_id = Ident::new(table_var, Span::mixed_site());
    {
        out.push_ident(&table_id);
        out.blit(270, 2);
        {
            let at = out.buf.len();
            out.buf.push(ctx.crate_path.clone());
            out.blit(272, 6);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(13);
            out.buf.push(ctx.crate_path.clone());
            out.blit(391, 6);
            {
                let at = out.buf.len();
                out.buf.push(name_lit.into());
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(288, 3);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(0);
    };
}
fn find_other_variant<'a>(variants: &'a [EnumVariant]) -> Option<&'a EnumVariant<'a>> {
    for v in variants {
        if v.other {
            return Some(v);
        }
    }
    None
}
fn emit_unknown_field_arm(out: &mut RustWriter, ctx: &Ctx) {
    match &ctx.target.unknown_fields {
        UnknownFieldPolicy::Ignore => {
            out.blit(397, 4);
        }
        UnknownFieldPolicy::Warn { tag } => {
            {
                out.blit(203, 3);
                {
                    let at = out.buf.len();
                    out.blit(401, 3);
                    {
                        let at = out.buf.len();
                        {
                            emit_tag_value(out, tag.as_deref())
                        };
                        out.blit(404, 6);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        UnknownFieldPolicy::Deny { tag } => {
            {
                out.blit(203, 3);
                {
                    let at = out.buf.len();
                    out.blit(98, 2);
                    {
                        let at = out.buf.len();
                        out.blit(401, 3);
                        {
                            let at = out.buf.len();
                            {
                                emit_tag_value(out, tag.as_deref())
                            };
                            out.blit(404, 6);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
    }
}
fn emit_tag_value(out: &mut RustWriter, tag: Option<&[TokenTree]>) {
    if let Some(tag_tokens) = tag {
        out.buf.extend_from_slice(tag_tokens);
    } else {
        out.buf.push(TokenTree::Literal(Literal::u32_suffixed(0)));
    }
}
fn emit_wildcard_arm(
    out: &mut RustWriter,
    _ctx: &Ctx,
    other_variant: Option<&EnumVariant>,
    msg: &str,
) {
    if let Some(ov) = other_variant {
        {
            out.blit(410, 4);
            {
                let at = out.buf.len();
                out.blit(313, 3);
                out.push_ident(ov.name);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(13);
        };
    } else {
        {
            out.blit(414, 4);
            {
                let at = out.buf.len();
                out.blit(418, 3);
                {
                    let at = out.buf.len();
                    out.blit_punct(6);
                    out.buf.push(TokenTree::Literal(Literal::string(msg)));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(13);
        };
    }
}
fn emit_for_table_header(out: &mut RustWriter, table_var: &str) {
    let table_id = Ident::new(table_var, Span::mixed_site());
    {
        out.blit_ident(60);
    };
    let pat_at = out.buf.len();
    {
        out.blit(214, 3);
    };
    out.tt_group(Delimiter::Parenthesis, pat_at);
    {
        out.blit_ident(15);
        out.buf.push(TokenTree::from(table_id.clone()));
    };
}
fn emit_flatten_prefix(out: &mut RustWriter, ctx: &Ctx, field: &Field, direction: u8) {
    if let Some(with) = field.with(direction) {
        out.buf.extend_from_slice(with);
    } else {
        if direction == FROM_TOML {
            out.blit_punct(5);
            out.buf.extend_from_slice(field.ty);
            out.blit_ident(83);
            out.buf.push(ctx.crate_path.clone());
            out.blit(15, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(330, 2);
        } else {
            out.buf.push(ctx.crate_path.clone());
            out.blit(67, 3);
        }
    }
}
fn emit_from_toml_call(out: &mut RustWriter, ctx: &Ctx, field: &Field, ty: &[TokenTree]) {
    if let Some(with) = field.with(FROM_TOML) {
        {
            out.buf.extend_from_slice(with);
            out.blit(332, 3);
            {
                let at = out.buf.len();
                out.blit(421, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
        };
    } else {
        {
            out.blit_punct(5);
            out.buf.extend_from_slice(ty);
            out.blit_ident(83);
            out.buf.push(ctx.crate_path.clone());
            out.blit(6, 5);
            out.push_ident(&ctx.lifetime);
            out.blit(330, 5);
            {
                let at = out.buf.len();
                out.blit(421, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
        };
    }
}
fn enum_from_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let other_variant = find_other_variant(variants);
    {
        out.blit(424, 6);
        {
            let at = out.buf.len();
            out.blit_ident(116);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(430, 4);
        {
            let at = out.buf.len();
            {
                for variant in variants {
                    if variant.other {
                        continue;
                    }
                    let name_lit = variant_name_literal(ctx, variant);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(368, 3);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
            };
            {
                if let Some(ov) = other_variant {
                    {
                        out.blit(410, 4);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
                            out.push_ident(ov.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                } else {
                    let expected_array = {
                        let mut ts = TokenStream::new();
                        for variant in variants {
                            if variant.other {
                                continue;
                            }
                            let name_lit = variant_name_literal(ctx, variant);
                            ts.extend([
                                name_lit.into(),
                                TokenTree::Punct(Punct::new(',', Spacing::Alone)),
                            ]);
                        }
                        TokenTree::Group(Group::new(Delimiter::Bracket, ts))
                    };
                    {
                        out.blit(414, 4);
                        {
                            let at = out.buf.len();
                            out.blit(434, 3);
                            {
                                let at = out.buf.len();
                                out.blit_punct(6);
                                out.buf.push(expected_array);
                                out.blit(32, 2);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
            };
            out.tt_group(Delimiter::Brace, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}
fn enum_to_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    {
        out.blit_ident(114);
        {
            let at = out.buf.len();
            out.buf.push(ctx.crate_path.clone());
            out.blit(391, 6);
            {
                let at = out.buf.len();
                out.blit(437, 2);
                {
                    let at = out.buf.len();
                    {
                        for variant in variants {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.blit(313, 3);
                                out.push_ident(variant.name);
                                out.blit(149, 2);
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
    impl_to_toml(out, ctx, body);
}
fn enum_to_toml(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant], mode: &TagMode) {
    let start = out.buf.len();
    {
        out.blit(437, 2);
    };
    let arms_at = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        let tag_lit = match mode {
            TagMode::Internal(t) | TagMode::Adjacent(t, _) => Some(*t),
            _ => None,
        };
        match variant.kind {
            EnumKind::None => {
                if let Some(tag) = tag_lit {
                    {
                        out.blit(313, 3);
                        out.push_ident(variant.name);
                        out.blit(149, 2);
                        {
                            let at = out.buf.len();
                            emit_table_alloc(out, ctx, "table", 1);
                            {
                                emit_tag_insert(out, ctx, "table", tag, name_lit)
                            };
                            out.blit_ident(114);
                            {
                                let at = out.buf.len();
                                out.blit(439, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                } else {
                    {
                        out.blit(313, 3);
                        out.push_ident(variant.name);
                        out.blit(368, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(ctx.crate_path.clone());
                            out.blit(391, 6);
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
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported")
                }
                if match mode {
                    TagMode::Untagged => true,
                    _ => false,
                } {
                    {
                        out.blit(313, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.blit_ident(73);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(149, 2);
                        out.buf.push(ctx.crate_path.clone());
                        out.blit(299, 6);
                        {
                            let at = out.buf.len();
                            out.blit(443, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                } else {
                    let cap = if tag_lit.is_some() { 2 } else { 1 };
                    {
                        out.blit(313, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.blit_ident(73);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(149, 2);
                        {
                            let at = out.buf.len();
                            emit_table_alloc(out, ctx, "table", cap);
                            {
                                if let Some(tag) = tag_lit {
                                    emit_tag_insert(out, ctx, "table", tag, name_lit.clone());
                                }
                            };
                            out.blit(446, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(272, 6);
                                {
                                    let at = out.buf.len();
                                    {
                                        match mode {
                                            TagMode::External => {
                                                out.buf.push(name_lit.into());
                                            }
                                            TagMode::Adjacent(_, content) => {
                                                out.buf
                                                    .push(TokenTree::Literal((*content).clone()));
                                            }
                                            _ => {}
                                        }
                                    };
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(13);
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(299, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(443, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(449, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(339, 2);
                            {
                                let at = out.buf.len();
                                out.blit(439, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                }
            }
            EnumKind::Struct => {
                let n = count_ser_fields(variant.fields);
                let arm_body_start = out.buf.len();
                let table_cap = n + if match mode {
                    TagMode::Internal(_) => true,
                    _ => false,
                } {
                    1
                } else {
                    0
                };
                emit_table_alloc(out, ctx, "table", table_cap);
                if let TagMode::Internal(tag) = mode {
                    emit_tag_insert(out, ctx, "table", tag, name_lit.clone());
                }
                emit_table_field_ser(out, ctx, variant.fields, "table", Some(variant), false);
                match mode {
                    TagMode::External => {
                        emit_table_alloc(out, ctx, "outer", 1);
                        {
                            out.blit(453, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(272, 6);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(456, 8);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(339, 2);
                            {
                                let at = out.buf.len();
                                out.blit(464, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    TagMode::Adjacent(tag, content) => {
                        emit_table_alloc(out, ctx, "outer", 2);
                        emit_tag_insert(out, ctx, "outer", tag, name_lit);
                        {
                            out.blit(453, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(272, 6);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(TokenTree::Literal((*content).clone()));
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(456, 8);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(339, 2);
                            {
                                let at = out.buf.len();
                                out.blit(464, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    _ => {
                        {
                            out.blit_ident(114);
                            {
                                let at = out.buf.len();
                                out.blit(439, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                }
                emit_struct_variant_to_arm(out, variant, arm_body_start);
            }
        }
    }
    out.tt_group(Delimiter::Brace, arms_at);
    let body = out.split_off_stream(start);
    impl_to_toml(out, ctx, body);
}
fn enum_from_toml_external(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let has_unit = ctx.target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0;
    let has_complex =
        ctx.target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) != 0;
    let other_variant = find_other_variant(variants);
    let start = out.buf.len();
    if has_unit {
        let if_body_start = out.buf.len();
        {
            out.blit(468, 3);
            {
                let at = out.buf.len();
                {
                    for variant in variants {
                        if match variant.kind {
                            EnumKind::None => true,
                            _ => false,
                        } && !variant.other
                        {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.buf.push(name_lit.into());
                                out.blit(368, 3);
                                {
                                    let at = out.buf.len();
                                    out.blit(313, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(13);
                            };
                        }
                    }
                };
                emit_wildcard_arm(out, ctx, other_variant, "a known variant");
                out.tt_group(Delimiter::Brace, at);
            };
            out.blit_punct(0);
        };
        let if_body = out.split_off_stream(if_body_start);
        {
            out.blit(267, 3);
            {
                let at = out.buf.len();
                out.blit_ident(75);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(471, 5);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, if_body)));
        };
    }
    if has_complex {
        {
            out.blit(476, 6);
            {
                let at = out.buf.len();
                out.blit_ident(116);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(482, 10);
        };
        let err_body_start = out.buf.len();
        {
            out.blit(98, 2);
            {
                let at = out.buf.len();
                out.blit(418, 3);
                {
                    let at = out.buf.len();
                    out.blit_punct(6);
                    out.buf.push(TokenTree::Literal(Literal::string(
                        "a table with exactly one key",
                    )));
                    out.blit(32, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
        };
        let err_body = out.split_off_stream(err_body_start);
        let one_lit = TokenTree::Literal(Literal::usize_unsuffixed(1));
        let zero_index = TokenTree::Group(Group::new(
            Delimiter::Bracket,
            TokenStream::from(TokenTree::Literal(Literal::usize_unsuffixed(0))),
        ));
        {
            out.blit(236, 2);
            {
                let at = out.buf.len();
                out.blit(492, 6);
                out.buf.push(one_lit);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, err_body)));
            out.blit_ident(117);
            {
                let at = out.buf.len();
                out.blit(498, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(501, 3);
            out.buf.push(zero_index);
            out.blit_punct(0);
        };
        {
            out.blit(504, 4);
        };
        let arms_at = out.buf.len();
        for variant in variants {
            if match variant.kind {
                EnumKind::None => true,
                _ => false,
            } || variant.other
            {
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
                        out.blit(368, 3);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.push(ctx.crate_path.clone());
                                out.blit(508, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(514, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(7);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
                EnumKind::Struct => {
                    {
                        out.buf.push(name_lit.into());
                        out.blit(149, 2);
                    };
                    let arm_at = out.buf.len();
                    {
                        out.blit(517, 11);
                        {
                            let at = out.buf.len();
                            out.blit_ident(116);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(226, 2);
                    };
                    emit_table_field_deser(
                        out,
                        ctx,
                        variant.fields,
                        "__subtable",
                        Some(variant),
                        &[],
                    );
                    emit_ok_self_variant(out, variant);
                    out.tt_group(Delimiter::Brace, arm_at);
                }
                EnumKind::None => {}
            }
        }
        emit_wildcard_arm(out, ctx, other_variant, "a known variant");
        out.tt_group(Delimiter::Brace, arms_at);
    } else if !has_unit {
        {
            out.blit_ident(113);
            {
                let at = out.buf.len();
                out.blit(418, 3);
                {
                    let at = out.buf.len();
                    out.blit_punct(6);
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
    impl_from_toml(out, ctx, body);
}
fn enum_from_toml_internal(
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
        out.blit(316, 6);
        {
            let at = out.buf.len();
            out.blit_ident(116);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(226, 2);
    };
    {
        out.blit(528, 12);
    };
    emit_for_table_header(out, "__table");
    let tag_body_at = out.buf.len();
    {
        out.blit(167, 6);
        out.buf.push(tag_lit.clone().into());
        {
            let at = out.buf.len();
            out.blit(540, 3);
            {
                let at = out.buf.len();
                out.buf.push(ctx.crate_path.clone());
                out.blit(508, 6);
                {
                    let at = out.buf.len();
                    out.blit(421, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(7);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(543, 3);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    out.tt_group(Delimiter::Brace, tag_body_at);
    {
        out.blit(90, 2);
        {
            let at = out.buf.len();
            out.blit_ident(99);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(546, 3);
    };
    let else_at = out.buf.len();
    {
        out.blit(98, 2);
        {
            let at = out.buf.len();
            out.blit(233, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(32, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(0);
    };
    out.tt_group(Delimiter::Brace, else_at);
    {
        out.blit_punct(0);
    };
    let other_variant = find_other_variant(variants);
    {
        out.blit(549, 2);
    };
    let arms_at = out.buf.len();
    for variant in variants {
        if variant.other {
            continue;
        }
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(149, 2);
                };
                let arm_at = out.buf.len();
                emit_for_table_header(out, "__table");
                let check_at = out.buf.len();
                match &ctx.target.unknown_fields {
                    UnknownFieldPolicy::Ignore => {
                        out.blit(551, 5);
                    }
                    UnknownFieldPolicy::Warn { tag } => {
                        {
                            out.blit(556, 6);
                            out.buf.push(tag_lit.clone().into());
                            {
                                let at = out.buf.len();
                                out.blit(401, 3);
                                {
                                    let at = out.buf.len();
                                    {
                                        emit_tag_value(out, tag.as_deref())
                                    };
                                    out.blit(404, 6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(0);
                                out.tt_group(Delimiter::Brace, at);
                            };
                        };
                    }
                    UnknownFieldPolicy::Deny { tag } => {
                        {
                            out.blit(556, 6);
                            out.buf.push(tag_lit.clone().into());
                            {
                                let at = out.buf.len();
                                out.blit(98, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(401, 3);
                                    {
                                        let at = out.buf.len();
                                        {
                                            emit_tag_value(out, tag.as_deref())
                                        };
                                        out.blit(404, 6);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(0);
                                out.tt_group(Delimiter::Brace, at);
                            };
                        };
                    }
                }
                out.tt_group(Delimiter::Brace, check_at);
                {
                    out.blit_ident(114);
                    {
                        let at = out.buf.len();
                        out.blit(313, 3);
                        out.push_ident(variant.name);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                out.tt_group(Delimiter::Brace, arm_at);
            }
            EnumKind::Struct => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(149, 2);
                };
                let arm_at = out.buf.len();
                {
                    out.blit(562, 5);
                };
                emit_table_field_deser(
                    out,
                    ctx,
                    variant.fields,
                    "__subtable",
                    Some(variant),
                    std::slice::from_ref(tag_lit),
                );
                emit_ok_self_variant(out, variant);
                out.tt_group(Delimiter::Brace, arm_at);
            }
            EnumKind::Tuple => {}
        }
    }
    emit_wildcard_arm(out, ctx, other_variant, "a known variant");
    out.tt_group(Delimiter::Brace, arms_at);
    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}
fn enum_from_toml_adjacent(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
    content_lit: &Literal,
) {
    let start = out.buf.len();
    {
        out.blit(316, 6);
        {
            let at = out.buf.len();
            out.blit_ident(116);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(567, 21);
        out.buf.push(ctx.crate_path.clone());
        out.blit(36, 5);
        out.push_ident(&ctx.lifetime);
        out.blit(588, 5);
    };
    emit_for_table_header(out, "__table");
    let for_body_at = out.buf.len();
    {
        out.blit(145, 4);
    };
    let extract_arms_at = out.buf.len();
    {
        out.buf.push(tag_lit.clone().into());
        out.blit(149, 2);
        {
            let at = out.buf.len();
            out.blit(540, 3);
            {
                let at = out.buf.len();
                out.buf.push(ctx.crate_path.clone());
                out.blit(508, 6);
                {
                    let at = out.buf.len();
                    out.blit(421, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(7);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
            out.tt_group(Delimiter::Brace, at);
        };
        out.buf.push(content_lit.clone().into());
        out.blit(149, 2);
        {
            let at = out.buf.len();
            out.blit(593, 3);
            {
                let at = out.buf.len();
                out.blit_ident(102);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(0);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    emit_unknown_field_arm(out, ctx);
    out.tt_group(Delimiter::Brace, extract_arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);
    {
        out.blit(90, 2);
        {
            let at = out.buf.len();
            out.blit_ident(99);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(546, 3);
    };
    let else_at = out.buf.len();
    {
        out.blit(98, 2);
        {
            let at = out.buf.len();
            out.blit(233, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(32, 2);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(0);
    };
    out.tt_group(Delimiter::Brace, else_at);
    {
        out.blit_punct(0);
    };
    let other_variant = find_other_variant(variants);
    {
        out.blit(549, 2);
    };
    let arms_at = out.buf.len();
    for variant in variants {
        if variant.other {
            continue;
        }
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(149, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(114);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
            }
            EnumKind::Tuple | EnumKind::Struct => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(149, 2);
                };
                let arm_at = out.buf.len();
                {
                    out.blit(90, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(87);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(596, 3);
                };
                let ce_at = out.buf.len();
                {
                    out.blit(98, 2);
                    {
                        let at = out.buf.len();
                        out.blit(233, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(32, 2);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(0);
                };
                out.tt_group(Delimiter::Brace, ce_at);
                {
                    out.blit_punct(0);
                };
                match variant.kind {
                    EnumKind::Tuple => {
                        if variant.fields.len() != 1 {
                            Error::msg("Only single-field tuple variants are supported")
                        }
                        {
                            out.blit_ident(114);
                            {
                                let at = out.buf.len();
                                out.blit(313, 3);
                                out.push_ident(variant.name);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(ctx.crate_path.clone());
                                    out.blit(508, 6);
                                    {
                                        let at = out.buf.len();
                                        out.blit(599, 3);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(7);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    EnumKind::Struct => {
                        {
                            out.blit(602, 11);
                            {
                                let at = out.buf.len();
                                out.blit_ident(116);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(226, 2);
                        };
                        emit_table_field_deser(
                            out,
                            ctx,
                            variant.fields,
                            "__subtable",
                            Some(variant),
                            &[],
                        );
                        emit_ok_self_variant(out, variant);
                    }
                    EnumKind::None => {}
                }
                out.tt_group(Delimiter::Brace, arm_at);
            }
        }
    }
    emit_wildcard_arm(out, ctx, other_variant, "a known variant");
    out.tt_group(Delimiter::Brace, arms_at);
    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}
fn enum_from_toml_untagged(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
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
                {
                    out.blit(267, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(40);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(471, 5);
                    {
                        let at = out.buf.len();
                        out.blit(613, 4);
                        out.buf.push(name_lit.into());
                        {
                            let at = out.buf.len();
                            out.blit(617, 2);
                            {
                                let at = out.buf.len();
                                out.blit(313, 3);
                                out.push_ident(variant.name);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(0);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    if propagate {
                        out.blit(98, 2);
                        {
                            let at = out.buf.len();
                            out.blit(418, 3);
                            {
                                let at = out.buf.len();
                                out.blit_punct(6);
                                out.buf.push(TokenTree::Literal(Literal::string(
                                    "a matching variant",
                                )));
                                out.blit(32, 2);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(0);
                    };
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported in untagged enums")
                }
                let match_start = out.buf.len();
                {
                    out.blit_ident(103);
                    out.buf.push(ctx.crate_path.clone());
                    out.blit(508, 6);
                    {
                        let at = out.buf.len();
                        out.blit(335, 3);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    {
                        let at = out.buf.len();
                        out.blit_ident(114);
                        {
                            let at = out.buf.len();
                            out.blit_ident(105);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(619, 4);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.blit_ident(105);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                        if propagate {
                            out.blit_ident(113);
                            {
                                let at = out.buf.len();
                                out.blit_ident(94);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(191, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(94);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(13);
                        };
                        if !propagate {
                            out.blit_ident(113);
                            {
                                let at = out.buf.len();
                                out.blit_ident(108);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(149, 2);
                            {
                                let at = out.buf.len();
                                out.blit(623, 5);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(67);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(0);
                                out.tt_group(Delimiter::Brace, at);
                            };
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
                if !propagate {
                    let match_body = out.split_off_stream(match_start);
                    {
                        let at = out.buf.len();
                        out.blit(628, 10);
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::None, match_body)));
                        out.tt_group(Delimiter::Brace, at);
                    };
                }
            }
            EnumKind::Struct => {
                let body_start = out.buf.len();
                {
                    out.blit(638, 6);
                    {
                        let at = out.buf.len();
                        out.blit_ident(116);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(226, 2);
                };
                emit_table_field_deser(out, ctx, variant.fields, "__subtable", Some(variant), &[]);
                if propagate {
                    {
                        out.blit(617, 2);
                        {
                            let at = out.buf.len();
                            out.blit(313, 3);
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
                        out.blit_punct(0);
                    };
                } else {
                    emit_ok_self_variant(out, variant);
                    let closure_body = out.split_off_stream(body_start);
                    let closure_body_group =
                        TokenTree::Group(Group::new(Delimiter::Brace, closure_body));
                    {
                        {
                            let at = out.buf.len();
                            out.blit(644, 25);
                            out.buf.push(ctx.crate_path.clone());
                            out.blit(669, 5);
                            {
                                let at = out.buf.len();
                                out.blit(251, 2);
                                out.buf.push(closure_body_group);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(674, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(114);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(105);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(619, 4);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(105);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(371, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(108);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(149, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(623, 5);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(67);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(0);
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
            let pred_at = out.buf.len();
            for tok in predicate {
                out.buf.push(tok.clone());
            }
            let pred_stream = out.split_off_stream(pred_at);
            let pred_group = TokenTree::Group(Group::new(Delimiter::Parenthesis, pred_stream));
            {
                {
                    let at = out.buf.len();
                    out.blit(678, 4);
                    {
                        let at = out.buf.len();
                        out.blit(24, 2);
                        out.buf.push(ctx.crate_path.clone());
                        out.blit(26, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit(682, 3);
                        out.buf.push(ctx.crate_path.clone());
                        out.blit(36, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit_punct(2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(685, 4);
                    out.buf.push(pred_group);
                    out.blit(689, 3);
                    {
                        let at = out.buf.len();
                        out.blit(335, 3);
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
            out.blit_ident(113);
            {
                let at = out.buf.len();
                out.blit(418, 3);
                {
                    let at = out.buf.len();
                    out.blit_punct(6);
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
    impl_from_toml(out, ctx, body);
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
    {
        let mut other_count = 0u32;
        for variant in variants {
            if variant.other {
                other_count += 1;
                if !match variant.kind {
                    EnumKind::None => true,
                    _ => false,
                } {
                    Error::span_msg(
                        "#[toml(other)] can only be used on unit variants",
                        variant.name.span(),
                    )
                }
                if target.untagged {
                    Error::span_msg(
                        "#[toml(other)] cannot be used on untagged enums",
                        variant.name.span(),
                    )
                }
            }
        }
        if other_count > 1 {
            Error::msg("only one variant can be marked #[toml(other)]")
        }
    }
    let ctx = Ctx::new(output, target);
    let is_string_enum = !target.untagged
        && target.tag.is_none()
        && target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0
        && target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) == 0;
    if target.from_toml && !emit_proxy_from_toml(output, &ctx) {
        if target.untagged {
            enum_from_toml_untagged(output, &ctx, variants);
        } else {
            match (&target.tag, &target.content) {
                (None, _) if is_string_enum => enum_from_toml_string(output, &ctx, variants),
                (None, _) => enum_from_toml_external(output, &ctx, variants),
                (Some(tag_lit), None) => enum_from_toml_internal(output, &ctx, variants, tag_lit),
                (Some(tag_lit), Some(content_lit)) => {
                    enum_from_toml_adjacent(output, &ctx, variants, tag_lit, content_lit)
                }
            }
        }
    }
    if target.to_toml {
        if is_string_enum {
            enum_to_toml_string(output, &ctx, variants);
        } else {
            let mode = if target.untagged {
                TagMode::Untagged
            } else {
                match (&target.tag, &target.content) {
                    (None, _) => TagMode::External,
                    (Some(tag_lit), None) => TagMode::Internal(tag_lit),
                    (Some(tag_lit), Some(content_lit)) => TagMode::Adjacent(tag_lit, content_lit),
                }
            };
            enum_to_toml(output, &ctx, variants, &mode);
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
        from_toml: false,
        to_toml: false,
        rename_all: crate::case::RenameRule::None,
        rename_all_fields: crate::case::RenameRule::None,
        enum_flags: 0,
        tag: None,
        content: None,
        untagged: false,
        from_type: None,
        try_from_type: None,
        unknown_fields: UnknownFieldPolicy::Warn { tag: None },
        recoverable: false,
    };
    let (kind, body) = ast::extract_derive_target(&mut target, &outer_tokens);
    if !(target.from_toml || target.to_toml) {
        target.from_toml = true;
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
        let len = rust_writer.buf.len();
        rust_writer.blit_punct(15);
        {
            let at = rust_writer.buf.len();
            rust_writer.blit_ident(6);
            {
                let at = rust_writer.buf.len();
                rust_writer.blit(692, 4);
                rust_writer.tt_group(Delimiter::Parenthesis, at);
            };
            rust_writer.tt_group(Delimiter::Bracket, at);
        };
        rust_writer.blit(696, 5);
        rust_writer
            .buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, ts)));
        rust_writer.blit_punct(0);
        rust_writer.split_off_stream(len)
    }
}
pub fn derive(stream: TokenStream) -> TokenStream {
    Error::try_catch_handle(stream, inner_derive)
}
