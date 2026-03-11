use crate::ast::{
    self, DefaultKind, DeriveTargetInner, DeriveTargetKind, EnumKind, EnumVariant, Field,
    FieldAttrs, Generic, GenericKind, ENUM_CONTAINS_STRUCT_VARIANT, ENUM_CONTAINS_TUPLE_VARIANT,
    ENUM_CONTAINS_UNIT_VARIANT, FROM_TOML, TO_TOML,
};
use crate::case::RenameRule;
use crate::util::MemoryPool;
use crate::writer::RustWriter;
use crate::Error;
use proc_macro::{Delimiter, Group, Ident, Literal, Span, TokenStream, TokenTree};

#[allow(unused)]
enum StaticToken {
    Ident(&'static str),
    Punct(char, bool),
}
#[allow(unused)]
use StaticToken::Ident as StaticIdent;
#[allow(unused)]
use StaticToken::Punct as StaticPunct;
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
                buffer.blit_ident(24);
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
fn field_name_literal_toml(_ctx: &Ctx, field: &Field, rename_rule: RenameRule) -> Literal {
    if let Some(name) = field.attr.rename(FROM_TOML) {
        return name.clone();
    }
    if rename_rule != RenameRule::None {
        Literal::string(&rename_rule.apply_to_field(&field.name.to_string()))
    } else {
        Literal::string(&field.name.to_string())
    }
}
fn variant_field_name_literal(ctx: &Ctx, field: &Field, variant: &EnumVariant) -> Literal {
    if let Some(name) = field.attr.rename(FROM_TOML) {
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
fn field_name_lit(ctx: &Ctx, field: &Field, variant: Option<&EnumVariant>) -> Literal {
    match variant {
        Some(v) => variant_field_name_literal(ctx, field, v),
        None => field_name_literal_toml(ctx, field, ctx.target.rename_all),
    }
}
fn impl_from_toml(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    {
        output.blit_punct(12);
        {
            let at = output.buf.len();
            output.blit_ident(23);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit(3, 3);
        output.push_ident(&ctx.lifetime);
        if !ctx.generics.is_empty() {
            output.blit_punct(13);
            fmt_generics(output, ctx.generics, DEF);
        };
        output.blit_punct(1);
        output.buf.push(TokenTree::from(ctx.crate_path.clone()));
        output.blit(6, 5);
        output.push_ident(&ctx.lifetime);
        output.blit(11, 2);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(1);
        };
        if !target.where_clauses.is_empty()
            || !target.generic_field_types.is_empty()
            || !target.generic_flatten_field_types.is_empty()
        {
            output.blit_ident(35);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
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
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(26, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(31, 5);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(36, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(13, 2);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 14);
            output.buf.push(TokenTree::from(ctx.crate_path.clone()));
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
        output.blit_punct(12);
        {
            let at = output.buf.len();
            output.blit_ident(23);
            output.tt_group(Delimiter::Bracket, at);
        };
        output.blit_ident(27);
        if !target.generics.is_empty() {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, DEF);
            output.blit_punct(1);
        };
        output.buf.push(TokenTree::from(ctx.crate_path.clone()));
        output.blit(59, 4);
        output.push_ident(&target.name);
        if any_generics {
            output.blit_punct(5);
            fmt_generics(output, &target.generics, USE);
            output.blit_punct(1);
        };
        if !target.where_clauses.is_empty()
            || !target.generic_field_types.is_empty()
            || !target.generic_flatten_field_types.is_empty()
        {
            output.blit_ident(35);
            for ty in &target.generic_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(63, 4);
            }
            for ty in &target.generic_flatten_field_types {
                output.buf.extend_from_slice(ty);
                output.blit_punct(9);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(67, 4);
            }
            output.buf.extend_from_slice(&target.where_clauses);
        };
        {
            let at = output.buf.len();
            output.blit(71, 4);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit_punct(1);
            {
                let at = output.buf.len();
                output.blit(75, 2);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit(77, 6);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(83, 5);
                output.buf.push(TokenTree::from(lf.clone()));
                output.blit_punct(1);
                output.tt_group(Delimiter::Parenthesis, at);
            };
            output.blit(41, 12);
            output.buf.push(TokenTree::from(ctx.crate_path.clone()));
            output.blit(36, 5);
            output.buf.push(TokenTree::from(lf.clone()));
            output.blit(13, 2);
            output.buf.push(TokenTree::from(ctx.crate_path.clone()));
            output.blit(55, 4);
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
        out.blit(88, 2);
        {
            let at = out.buf.len();
            out.blit_ident(89);
            out.buf.push(TokenTree::from(var_id.clone()));
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(3);
        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
        out.blit(90, 6);
        {
            let at = out.buf.len();
            out.buf
                .push(TokenTree::Literal(Literal::usize_unsuffixed(capacity)));
            out.blit(96, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_ident(60);
        {
            let at = out.buf.len();
            out.blit(100, 4);
            {
                let at = out.buf.len();
                out.buf.push(TokenTree::Literal(Literal::string(
                    "Table capacity exceeded maximum",
                )));
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(2);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit_punct(2);
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
    let table_id = Ident::new(table_ident, Span::mixed_site());
    let mut flatten_field: Option<&Field> = None;
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            if flatten_field.is_some() {
                Error::msg("Only one #[toml(flatten)] field is allowed")
            }
            flatten_field = Some(field);
        }
    }
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            if let Some(with) = field.with(FROM_TOML) {
                out.blit(104, 4);
                out.buf.extend_from_slice(with);
                out.blit(108, 5);
            } else {
                out.blit(113, 5);
                out.buf.extend_from_slice(field.ty);
                out.blit_ident(64);
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(118, 7);
            }
            continue;
        }
        if field.flags & Field::WITH_FROM_TOML_SKIP != 0 {
            emit_field_default(out, field, FROM_TOML, false);
        } else if field.flags & Field::WITH_FROM_TOML_OPTION != 0 {
            out.blit(104, 2);
            out.push_ident(field.name);
            out.blit_punct(9);
            out.buf.extend_from_slice(field.ty);
            out.blit(125, 3);
        } else {
            out.blit(104, 2);
            out.push_ident(field.name);
            out.blit(128, 5);
            out.buf.extend_from_slice(field.ty);
            out.blit(133, 2);
        }
    }
    {
        out.blit_ident(65);
    };
    let pat_at = out.buf.len();
    {
        out.blit(135, 3);
    };
    out.tt_group(Delimiter::Parenthesis, pat_at);
    {
        out.blit_ident(51);
        out.buf.push(TokenTree::from(table_id.clone()));
    };
    let for_body_at = out.buf.len();
    {
        out.blit(138, 4);
    };
    let arms_at = out.buf.len();
    for skip_key in skip_keys {
        out.buf.push(skip_key.clone().into());
        out.blit(142, 3);
    }
    for field in fields {
        if field.flags & (Field::WITH_FROM_TOML_SKIP | Field::WITH_FLATTEN) != 0 {
            continue;
        }
        let name_lit = field_name_lit(ctx, field, variant);
        let is_default = field.flags & Field::WITH_FROM_TOML_DEFAULT != 0;
        let is_option = field.flags & Field::WITH_FROM_TOML_OPTION != 0;
        let with_path = field.with(FROM_TOML);
        let is_required = !is_option && !is_default;
        let has_aliases = field.attr.has_aliases(FROM_TOML);
        {
            out.buf.push(name_lit.clone().into());
        };
        for alias in field.attr.aliases(FROM_TOML) {
            out.blit_punct(11);
            out.buf.push(alias.clone().into());
        }
        {
            out.blit(142, 2);
        };
        let arm_body_at = out.buf.len();
        if has_aliases {
            {
                out.blit_ident(80);
                out.push_ident(field.name);
                out.blit(145, 3);
                {
                    let at = out.buf.len();
                    out.blit(148, 2);
                    {
                        let at = out.buf.len();
                        out.blit(150, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.blit(153, 4);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
        {
            out.blit_ident(86);
        };
        if let Some(with) = with_path {
            {
                out.buf.extend_from_slice(with);
                out.blit(157, 3);
                {
                    let at = out.buf.len();
                    out.blit(160, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
            };
        } else {
            let ty = if is_option {
                option_inner_ty(field.ty)
            } else {
                field.ty
            };
            {
                out.blit_punct(5);
                out.buf.extend_from_slice(ty);
                out.blit_ident(64);
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(6, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(163, 5);
                {
                    let at = out.buf.len();
                    out.blit(160, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
            };
        }
        let match_body_at = out.buf.len();
        {
            out.blit_ident(93);
            {
                let at = out.buf.len();
                out.blit_ident(79);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(142, 2);
            {
                let at = out.buf.len();
                out.push_ident(field.name);
                out.blit(168, 2);
                {
                    let at = out.buf.len();
                    out.blit_ident(79);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
                out.tt_group(Delimiter::Brace, at);
            };
            if is_required {
                out.blit_ident(92);
                {
                    let at = out.buf.len();
                    out.blit_ident(47);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(170, 4);
                {
                    let at = out.buf.len();
                    out.blit_ident(47);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(13);
            };
            if !is_required {
                out.blit_ident(92);
                {
                    let at = out.buf.len();
                    out.blit_ident(88);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(174, 4);
            };
        };
        out.tt_group(Delimiter::Brace, match_body_at);
        out.tt_group(Delimiter::Brace, arm_body_at);
    }
    if let Some(ff) = flatten_field {
        if let Some(with) = ff.with(FROM_TOML) {
            {
                out.blit(178, 3);
                {
                    let at = out.buf.len();
                    out.blit(181, 3);
                    out.buf.extend_from_slice(with);
                    out.blit(184, 3);
                    {
                        let at = out.buf.len();
                        out.blit(187, 9);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.blit(178, 3);
                {
                    let at = out.buf.len();
                    out.blit(196, 4);
                    out.buf.extend_from_slice(ff.ty);
                    out.blit_ident(64);
                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                    out.blit(15, 5);
                    out.push_ident(&ctx.lifetime);
                    out.blit(200, 5);
                    {
                        let at = out.buf.len();
                        out.blit(187, 9);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        }
    } else {
        {
            out.blit(178, 3);
            {
                let at = out.buf.len();
                out.blit(148, 2);
                {
                    let at = out.buf.len();
                    out.blit(205, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Literal(Literal::string("unexpected key")));
                        out.blit(153, 4);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
                out.tt_group(Delimiter::Brace, at);
            };
        };
    }
    out.tt_group(Delimiter::Brace, arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);
    if let Some(ff) = flatten_field {
        if let Some(with) = ff.with(FROM_TOML) {
            {
                out.blit_ident(95);
                out.push_ident(ff.name);
                out.blit_punct(3);
                out.buf.extend_from_slice(with);
                out.blit(208, 3);
                {
                    let at = out.buf.len();
                    out.blit(211, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(214, 2);
            };
        } else {
            {
                out.blit_ident(95);
                out.push_ident(ff.name);
                out.blit(116, 2);
                out.buf.extend_from_slice(ff.ty);
                out.blit_ident(64);
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(15, 5);
                out.push_ident(&ctx.lifetime);
                out.blit(216, 5);
                {
                    let at = out.buf.len();
                    out.blit(211, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit(214, 2);
            };
        }
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
        } else {
            let name_lit = field_name_lit(ctx, field, variant);
            {
                out.blit(88, 2);
                {
                    let at = out.buf.len();
                    out.push_ident(field.name);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.push_ident(field.name);
                out.blit(221, 4);
            };
            let else_at = out.buf.len();
            {
                out.blit(148, 2);
                {
                    let at = out.buf.len();
                    out.blit(225, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.blit(228, 5);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
            };
            out.tt_group(Delimiter::Brace, else_at);
            {
                out.blit_punct(2);
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
                        out.blit_ident(95);
                        out.push_ident(field.name);
                        out.blit_punct(3);
                        out.push_ident(field.name);
                        out.blit(233, 2);
                        {
                            let at = out.buf.len();
                            out.blit(235, 2);
                            out.buf.extend_from_slice(tokens.as_slice());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                    };
                }
                DefaultKind::Default => {
                    out.blit_ident(95);
                    out.push_ident(field.name);
                    out.blit_punct(3);
                    out.push_ident(field.name);
                    out.blit(237, 4);
                }
            }
        } else {
            out.blit_ident(95);
            out.push_ident(field.name);
            out.blit_punct(3);
            out.push_ident(field.name);
            out.blit(237, 4);
        }
    } else {
        if let Some(default_kind) = field.default(direction) {
            match default_kind {
                DefaultKind::Custom(tokens) => {
                    out.blit_ident(95);
                    out.push_ident(field.name);
                    out.blit_punct(3);
                    out.buf.extend_from_slice(tokens.as_slice());
                    out.blit_punct(2);
                }
                DefaultKind::Default => {
                    out.blit_ident(95);
                    out.push_ident(field.name);
                    out.blit(241, 7);
                }
            }
        } else {
            out.blit_ident(95);
            out.push_ident(field.name);
            out.blit(241, 7);
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
                out.blit(248, 3);
                out.push_ident(field.name);
            };
            if !self_access {
                out.push_ident(field.name);
            };
            out.split_off_stream(len)
        };
        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            {
                out.buf.push(TokenTree::from(table_id.clone()));
                out.blit(251, 2);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                    out.blit(253, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.extend_from_slice(with);
                    out.blit(259, 3);
                    {
                        let at = out.buf.len();
                        out.buf.push(TokenTree::Group(Group::new(
                            Delimiter::None,
                            field_ref.clone(),
                        )));
                        out.blit(78, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(262, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
            };
        } else if first_ty_ident == "Option" {
            {
                out.blit(268, 3);
                {
                    let at = out.buf.len();
                    out.blit_ident(79);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(3);
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(271, 6);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::Group(Group::new(
                        Delimiter::None,
                        field_ref.clone(),
                    )));
                    out.blit(78, 2);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::from(table_id.clone()));
                    out.blit(251, 2);
                    {
                        let at = out.buf.len();
                        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        out.blit(253, 6);
                        {
                            let at = out.buf.len();
                            out.buf.push(name_lit.into());
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(277, 7);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                    out.tt_group(Delimiter::Brace, at);
                };
            };
        } else {
            {
                out.buf.push(TokenTree::from(table_id.clone()));
                out.blit(251, 2);
                {
                    let at = out.buf.len();
                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                    out.blit(253, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(name_lit.into());
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(13);
                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                    out.blit(284, 6);
                    {
                        let at = out.buf.len();
                        out.buf.push(TokenTree::Group(Group::new(
                            Delimiter::None,
                            field_ref.clone(),
                        )));
                        out.blit(78, 2);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(262, 6);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(2);
            };
        }
        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            {
                out.blit(290, 2);
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
                    out.blit(248, 3);
                    out.push_ident(field.name);
                };
                if !self_access {
                    out.push_ident(field.name);
                };
                out.split_off_stream(len)
            };
            if let Some(with) = field.with(TO_TOML) {
                {
                    out.buf.extend_from_slice(with);
                    out.blit(292, 3);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::None, field_ref)));
                        out.blit(295, 5);
                        out.buf.push(TokenTree::from(table_id.clone()));
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(214, 2);
                };
            } else {
                {
                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                    out.blit(300, 6);
                    {
                        let at = out.buf.len();
                        out.buf
                            .push(TokenTree::Group(Group::new(Delimiter::None, field_ref)));
                        out.blit(295, 5);
                        out.buf.push(TokenTree::from(table_id.clone()));
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(214, 2);
                };
            }
        }
    }
}
fn emit_struct_variant_to_arm(out: &mut RustWriter, variant: &EnumVariant, arm_body_start: usize) {
    let arm_body = out.split_off_stream(arm_body_start);
    {
        out.blit(306, 3);
        out.push_ident(variant.name);
        {
            let at = out.buf.len();
            {
                for field in variant.fields {
                    out.blit_ident(9);
                    out.push_ident(field.name);
                    out.blit_punct(13);
                }
            };
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit(142, 2);
        out.buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, arm_body)));
    };
}
fn count_ser_fields(fields: &[Field]) -> usize {
    fields
        .iter()
        .filter(|f| f.flags & (Field::WITH_TO_TOML_SKIP | Field::WITH_FLATTEN) == 0)
        .count()
}
/// Emit Ok(Self::Variant { field1, field2, ... }) for struct variant deserialization
fn emit_ok_self_variant(out: &mut RustWriter, variant: &EnumVariant) {
    {
        out.blit_ident(93);
        {
            let at = out.buf.len();
            out.blit(306, 3);
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
        out.blit(309, 6);
        {
            let at = out.buf.len();
            out.blit_ident(96);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(214, 2);
    };
    emit_table_field_deser(out, ctx, fields, "__table", None, &[]);
    {
        out.blit_ident(93);
        {
            let at = out.buf.len();
            out.blit_ident(91);
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
        out.blit_ident(93);
        {
            let at = out.buf.len();
            out.blit(315, 4);
            out.tt_group(Delimiter::Parenthesis, at);
        };
    };
    let body = out.split_off_stream(start);
    impl_to_toml(out, ctx, body);
}
fn handle_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);
    if target.from_toml {
        if target.transparent_impl {
            let [single_field] = fields else {
                Error::msg("Struct must contain a single field to use transparent")
            };
            let body = {
                let len = output.buf.len();
                output.blit_punct(5);
                output.buf.extend_from_slice(single_field.ty);
                output.blit_ident(64);
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(6, 5);
                output.push_ident(&ctx.lifetime);
                output.blit(163, 5);
                {
                    let at = output.buf.len();
                    output.blit(319, 3);
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
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(284, 6);
                {
                    let at = output.buf.len();
                    output.blit(248, 3);
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
    if target.from_toml {
        if let [single_field] = fields {
            let body = {
                let len = output.buf.len();
                output.blit_ident(93);
                {
                    let at = output.buf.len();
                    output.push_ident(&target.name);
                    {
                        let at = output.buf.len();
                        output.blit_punct(5);
                        output.buf.extend_from_slice(single_field.ty);
                        output.blit_ident(64);
                        output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        output.blit(6, 5);
                        output.push_ident(&ctx.lifetime);
                        output.blit(163, 5);
                        {
                            let at = output.buf.len();
                            output.blit(319, 3);
                            output.tt_group(Delimiter::Parenthesis, at);
                        };
                        output.blit_punct(6);
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
                output.buf.push(TokenTree::from(ctx.crate_path.clone()));
                output.blit(284, 6);
                {
                    let at = output.buf.len();
                    output.blit(248, 3);
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
fn emit_tag_insert(out: &mut RustWriter, ctx: &Ctx, tag_lit: &Literal, name_lit: Literal) {
    {
        out.blit(322, 3);
        {
            let at = out.buf.len();
            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
            out.blit(253, 6);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(13);
            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
            out.blit(325, 6);
            {
                let at = out.buf.len();
                out.buf.push(name_lit.into());
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(263, 5);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(2);
    };
}
fn enum_from_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
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
        out.blit(331, 6);
        {
            let at = out.buf.len();
            out.blit_ident(96);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(337, 4);
        {
            let at = out.buf.len();
            {
                for variant in variants {
                    let name_lit = variant_name_literal(ctx, variant);
                    {
                        out.buf.push(name_lit.into());
                        out.blit(341, 3);
                        {
                            let at = out.buf.len();
                            out.blit(306, 3);
                            out.push_ident(variant.name);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                }
            };
            out.blit(344, 4);
            {
                let at = out.buf.len();
                out.blit(348, 3);
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
    impl_from_toml(out, ctx, body);
}
fn enum_to_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    {
        out.blit_ident(93);
        {
            let at = out.buf.len();
            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
            out.blit(325, 6);
            {
                let at = out.buf.len();
                out.blit(351, 2);
                {
                    let at = out.buf.len();
                    {
                        for variant in variants {
                            let name_lit = variant_name_literal(ctx, variant);
                            {
                                out.blit(306, 3);
                                out.push_ident(variant.name);
                                out.blit(142, 2);
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
        out.blit(351, 2);
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
                        out.blit(306, 3);
                        out.push_ident(variant.name);
                        out.blit(142, 2);
                        {
                            let at = out.buf.len();
                            emit_table_alloc(out, ctx, "table", 1);
                            {
                                emit_tag_insert(out, ctx, tag, name_lit)
                            };
                            out.blit_ident(93);
                            {
                                let at = out.buf.len();
                                out.blit(353, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Brace, at);
                        };
                    };
                } else {
                    {
                        out.blit(306, 3);
                        out.push_ident(variant.name);
                        out.blit(341, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                            out.blit(325, 6);
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
                        out.blit(306, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.blit_ident(52);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(142, 2);
                        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        out.blit(284, 6);
                        {
                            let at = out.buf.len();
                            out.blit(357, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(13);
                    };
                } else {
                    let cap = if tag_lit.is_some() { 2 } else { 1 };
                    {
                        out.blit(306, 3);
                        out.push_ident(variant.name);
                        {
                            let at = out.buf.len();
                            out.blit_ident(52);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(142, 2);
                        {
                            let at = out.buf.len();
                            emit_table_alloc(out, ctx, "table", cap);
                            {
                                if let Some(tag) = tag_lit {
                                    emit_tag_insert(out, ctx, tag, name_lit.clone());
                                }
                            };
                            out.blit(322, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                out.blit(253, 6);
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
                                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                out.blit(284, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(357, 3);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(262, 6);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(360, 2);
                            {
                                let at = out.buf.len();
                                out.blit(353, 4);
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
                match mode {
                    TagMode::External => {
                        emit_table_alloc(out, ctx, "table", n);
                        emit_table_field_ser(
                            out,
                            ctx,
                            variant.fields,
                            "table",
                            Some(variant),
                            false,
                        );
                        emit_table_alloc(out, ctx, "outer", 1);
                        {
                            out.blit(362, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                out.blit(253, 6);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(name_lit.into());
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(365, 10);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(360, 2);
                            {
                                let at = out.buf.len();
                                out.blit(375, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    TagMode::Internal(tag) => {
                        emit_table_alloc(out, ctx, "table", n + 1);
                        emit_tag_insert(out, ctx, tag, name_lit);
                        emit_table_field_ser(
                            out,
                            ctx,
                            variant.fields,
                            "table",
                            Some(variant),
                            false,
                        );
                        {
                            out.blit_ident(93);
                            {
                                let at = out.buf.len();
                                out.blit(353, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    TagMode::Adjacent(tag, content) => {
                        emit_table_alloc(out, ctx, "table", n);
                        emit_table_field_ser(
                            out,
                            ctx,
                            variant.fields,
                            "table",
                            Some(variant),
                            false,
                        );
                        emit_table_alloc(out, ctx, "outer", 2);
                        emit_tag_insert(out, ctx, tag, name_lit);
                        {
                            out.blit(362, 3);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                out.blit(253, 6);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(TokenTree::Literal((*content).clone()));
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(365, 10);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(360, 2);
                            {
                                let at = out.buf.len();
                                out.blit(375, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    TagMode::Untagged => {
                        emit_table_alloc(out, ctx, "table", n);
                        emit_table_field_ser(
                            out,
                            ctx,
                            variant.fields,
                            "table",
                            Some(variant),
                            false,
                        );
                        {
                            out.blit_ident(93);
                            {
                                let at = out.buf.len();
                                out.blit(353, 4);
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
    let start = out.buf.len();
    if has_unit {
        let if_body_start = out.buf.len();
        {
            out.blit(379, 3);
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
                                out.blit(341, 3);
                                {
                                    let at = out.buf.len();
                                    out.blit(306, 3);
                                    out.push_ident(variant.name);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit_punct(13);
                            };
                        }
                    }
                };
                out.blit(344, 4);
                {
                    let at = out.buf.len();
                    out.blit(348, 3);
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
            out.blit_punct(2);
        };
        let if_body = out.split_off_stream(if_body_start);
        {
            out.blit(268, 3);
            {
                let at = out.buf.len();
                out.blit_ident(55);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(382, 5);
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, if_body)));
        };
    }
    if has_complex {
        {
            out.blit(387, 6);
            {
                let at = out.buf.len();
                out.blit_ident(96);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(393, 10);
        };
        let err_body_start = out.buf.len();
        {
            out.blit(148, 2);
            {
                let at = out.buf.len();
                out.blit(348, 3);
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
            out.blit_punct(2);
        };
        let err_body = out.split_off_stream(err_body_start);
        let one_lit = TokenTree::Literal(Literal::usize_unsuffixed(1));
        let zero_index = TokenTree::Group(Group::new(
            Delimiter::Bracket,
            TokenStream::from(TokenTree::Literal(Literal::usize_unsuffixed(0))),
        ));
        {
            out.blit(290, 2);
            {
                let at = out.buf.len();
                out.blit(403, 6);
                out.buf.push(one_lit);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.buf
                .push(TokenTree::Group(Group::new(Delimiter::Brace, err_body)));
            out.blit_ident(95);
            {
                let at = out.buf.len();
                out.blit(409, 3);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(412, 3);
            out.buf.push(zero_index);
            out.blit_punct(2);
        };
        {
            out.blit(415, 4);
        };
        let arms_at = out.buf.len();
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
                        out.blit(341, 3);
                        {
                            let at = out.buf.len();
                            out.blit(306, 3);
                            out.push_ident(variant.name);
                            {
                                let at = out.buf.len();
                                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                out.blit(419, 6);
                                {
                                    let at = out.buf.len();
                                    out.blit(425, 3);
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
                    {
                        out.buf.push(name_lit.into());
                        out.blit(142, 2);
                    };
                    let arm_at = out.buf.len();
                    {
                        out.blit(428, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(96);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(214, 2);
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
        {
            out.blit(344, 4);
            {
                let at = out.buf.len();
                out.blit(348, 3);
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
        out.tt_group(Delimiter::Brace, arms_at);
    } else if !has_unit {
        {
            out.blit_ident(92);
            {
                let at = out.buf.len();
                out.blit(348, 3);
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
        out.blit(309, 6);
        {
            let at = out.buf.len();
            out.blit_ident(96);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(214, 2);
    };
    {
        out.blit(434, 13);
    };
    let pat_at = out.buf.len();
    {
        out.blit(135, 3);
    };
    out.tt_group(Delimiter::Parenthesis, pat_at);
    {
        out.blit(447, 2);
    };
    let tag_body_at = out.buf.len();
    {
        out.blit(449, 6);
        out.buf.push(tag_lit.clone().into());
        {
            let at = out.buf.len();
            out.blit(455, 3);
            {
                let at = out.buf.len();
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(419, 6);
                {
                    let at = out.buf.len();
                    out.blit(160, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit(458, 3);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    out.tt_group(Delimiter::Brace, tag_body_at);
    {
        out.blit(88, 2);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(461, 3);
    };
    let else_at = out.buf.len();
    {
        out.blit(148, 2);
        {
            let at = out.buf.len();
            out.blit(225, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(228, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(2);
    };
    out.tt_group(Delimiter::Brace, else_at);
    {
        out.blit_punct(2);
    };
    {
        out.blit(464, 2);
    };
    let arms_at = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(142, 2);
                };
                let arm_at = out.buf.len();
                {
                    out.blit_ident(65);
                };
                let cpat_at = out.buf.len();
                {
                    out.blit(135, 3);
                };
                out.tt_group(Delimiter::Parenthesis, cpat_at);
                {
                    out.blit(447, 2);
                };
                let check_at = out.buf.len();
                {
                    out.blit(466, 6);
                    out.buf.push(tag_lit.clone().into());
                    {
                        let at = out.buf.len();
                        out.blit(148, 2);
                        {
                            let at = out.buf.len();
                            out.blit(205, 3);
                            {
                                let at = out.buf.len();
                                out.buf
                                    .push(TokenTree::Literal(Literal::string("unexpected key")));
                                out.blit(153, 4);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit_punct(2);
                        out.tt_group(Delimiter::Brace, at);
                    };
                };
                out.tt_group(Delimiter::Brace, check_at);
                {
                    out.blit_ident(93);
                    {
                        let at = out.buf.len();
                        out.blit(306, 3);
                        out.push_ident(variant.name);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                };
                out.tt_group(Delimiter::Brace, arm_at);
            }
            EnumKind::Struct => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(142, 2);
                };
                let arm_at = out.buf.len();
                {
                    out.blit(472, 5);
                };
                emit_table_field_deser(
                    out,
                    ctx,
                    variant.fields,
                    "__subtable",
                    Some(variant),
                    &[tag_lit.clone()],
                );
                emit_ok_self_variant(out, variant);
                out.tt_group(Delimiter::Brace, arm_at);
            }
            EnumKind::Tuple => {}
        }
    }
    {
        out.blit(344, 4);
        {
            let at = out.buf.len();
            out.blit(348, 3);
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
        out.blit(309, 6);
        {
            let at = out.buf.len();
            out.blit_ident(96);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(477, 21);
        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
        out.blit(36, 5);
        out.push_ident(&ctx.lifetime);
        out.blit(498, 5);
    };
    {
        out.blit_ident(65);
    };
    let pat_at = out.buf.len();
    {
        out.blit(135, 3);
    };
    out.tt_group(Delimiter::Parenthesis, pat_at);
    {
        out.blit(447, 2);
    };
    let for_body_at = out.buf.len();
    {
        out.blit(138, 4);
    };
    let extract_arms_at = out.buf.len();
    {
        out.buf.push(tag_lit.clone().into());
        out.blit(142, 2);
        {
            let at = out.buf.len();
            out.blit(455, 3);
            {
                let at = out.buf.len();
                out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                out.blit(419, 6);
                {
                    let at = out.buf.len();
                    out.blit(160, 3);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.blit_punct(6);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(2);
            out.tt_group(Delimiter::Brace, at);
        };
        out.buf.push(content_lit.clone().into());
        out.blit(142, 2);
        {
            let at = out.buf.len();
            out.blit(503, 3);
            {
                let at = out.buf.len();
                out.blit_ident(83);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(2);
            out.tt_group(Delimiter::Brace, at);
        };
        out.blit(178, 3);
        {
            let at = out.buf.len();
            out.blit(148, 2);
            {
                let at = out.buf.len();
                out.blit(205, 3);
                {
                    let at = out.buf.len();
                    out.buf
                        .push(TokenTree::Literal(Literal::string("unexpected key")));
                    out.blit(153, 4);
                    out.tt_group(Delimiter::Parenthesis, at);
                };
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.blit_punct(2);
            out.tt_group(Delimiter::Brace, at);
        };
    };
    out.tt_group(Delimiter::Brace, extract_arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);
    {
        out.blit(88, 2);
        {
            let at = out.buf.len();
            out.blit_ident(78);
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit(461, 3);
    };
    let else_at = out.buf.len();
    {
        out.blit(148, 2);
        {
            let at = out.buf.len();
            out.blit(225, 3);
            {
                let at = out.buf.len();
                out.buf.push(tag_lit.clone().into());
                out.blit(228, 5);
                out.tt_group(Delimiter::Parenthesis, at);
            };
            out.tt_group(Delimiter::Parenthesis, at);
        };
        out.blit_punct(2);
    };
    out.tt_group(Delimiter::Brace, else_at);
    {
        out.blit_punct(2);
    };
    {
        out.blit(464, 2);
    };
    let arms_at = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                {
                    out.buf.push(name_lit.into());
                    out.blit(142, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(93);
                        {
                            let at = out.buf.len();
                            out.blit(306, 3);
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
                    out.blit(142, 2);
                };
                let arm_at = out.buf.len();
                {
                    out.blit(88, 2);
                    {
                        let at = out.buf.len();
                        out.blit_ident(62);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(506, 3);
                };
                let ce_at = out.buf.len();
                {
                    out.blit(148, 2);
                    {
                        let at = out.buf.len();
                        out.blit(225, 3);
                        {
                            let at = out.buf.len();
                            out.buf.push(content_lit.clone().into());
                            out.blit(228, 5);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit_punct(2);
                };
                out.tt_group(Delimiter::Brace, ce_at);
                {
                    out.blit_punct(2);
                };
                match variant.kind {
                    EnumKind::Tuple => {
                        if variant.fields.len() != 1 {
                            Error::msg("Only single-field tuple variants are supported")
                        }
                        {
                            out.blit_ident(93);
                            {
                                let at = out.buf.len();
                                out.blit(306, 3);
                                out.push_ident(variant.name);
                                {
                                    let at = out.buf.len();
                                    out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                                    out.blit(419, 6);
                                    {
                                        let at = out.buf.len();
                                        out.blit(509, 3);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(6);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                        };
                    }
                    EnumKind::Struct => {
                        {
                            out.blit(512, 6);
                            {
                                let at = out.buf.len();
                                out.blit_ident(96);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(214, 2);
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
    {
        out.blit(344, 4);
        {
            let at = out.buf.len();
            out.blit(348, 3);
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
                    out.blit(268, 3);
                    {
                        let at = out.buf.len();
                        out.blit_ident(21);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(382, 5);
                    {
                        let at = out.buf.len();
                        out.blit(518, 4);
                        out.buf.push(name_lit.into());
                        {
                            let at = out.buf.len();
                            out.blit(522, 2);
                            {
                                let at = out.buf.len();
                                out.blit(306, 3);
                                out.push_ident(variant.name);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit_punct(2);
                            out.tt_group(Delimiter::Brace, at);
                        };
                        out.tt_group(Delimiter::Brace, at);
                    };
                    if propagate {
                        out.blit(148, 2);
                        {
                            let at = out.buf.len();
                            out.blit(348, 3);
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
                        out.blit_punct(2);
                    };
                };
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    Error::msg("Only single-field tuple variants are supported in untagged enums")
                }
                if propagate {
                    {
                        out.blit_ident(86);
                        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        out.blit(419, 6);
                        {
                            let at = out.buf.len();
                            out.blit(319, 3);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        {
                            let at = out.buf.len();
                            out.blit_ident(93);
                            {
                                let at = out.buf.len();
                                out.blit_ident(79);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(524, 4);
                            {
                                let at = out.buf.len();
                                out.blit(306, 3);
                                out.push_ident(variant.name);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(79);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(528, 2);
                            {
                                let at = out.buf.len();
                                out.blit_ident(47);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(170, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(47);
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
                            out.blit(530, 11);
                            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                            out.blit(419, 6);
                            {
                                let at = out.buf.len();
                                out.blit(319, 3);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            {
                                let at = out.buf.len();
                                out.blit_ident(93);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(79);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(524, 4);
                                {
                                    let at = out.buf.len();
                                    out.blit(306, 3);
                                    out.push_ident(variant.name);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(79);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(528, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(88);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(142, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(541, 5);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(48);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(2);
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
                        out.blit(546, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(96);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(214, 2);
                    };
                    emit_table_field_deser(
                        out,
                        ctx,
                        variant.fields,
                        "__subtable",
                        Some(variant),
                        &[],
                    );
                    {
                        out.blit(522, 2);
                        {
                            let at = out.buf.len();
                            out.blit(306, 3);
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
                        out.blit_punct(2);
                    };
                } else {
                    let closure_body_start = out.buf.len();
                    {
                        out.blit(546, 6);
                        {
                            let at = out.buf.len();
                            out.blit_ident(96);
                            out.tt_group(Delimiter::Parenthesis, at);
                        };
                        out.blit(214, 2);
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
                    let closure_body = out.split_off_stream(closure_body_start);
                    let closure_body_group =
                        TokenTree::Group(Group::new(Delimiter::Brace, closure_body));
                    {
                        {
                            let at = out.buf.len();
                            out.blit(552, 25);
                            out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                            out.blit(577, 5);
                            {
                                let at = out.buf.len();
                                out.blit(235, 2);
                                out.buf.push(closure_body_group);
                                out.tt_group(Delimiter::Parenthesis, at);
                            };
                            out.blit(582, 4);
                            {
                                let at = out.buf.len();
                                out.blit_ident(93);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(79);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(524, 4);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(79);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(528, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit_ident(88);
                                    out.tt_group(Delimiter::Parenthesis, at);
                                };
                                out.blit(142, 2);
                                {
                                    let at = out.buf.len();
                                    out.blit(541, 5);
                                    {
                                        let at = out.buf.len();
                                        out.blit_ident(48);
                                        out.tt_group(Delimiter::Parenthesis, at);
                                    };
                                    out.blit_punct(2);
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
                    out.blit(586, 4);
                    {
                        let at = out.buf.len();
                        out.blit(24, 2);
                        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        out.blit(26, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit(590, 3);
                        out.buf.push(TokenTree::from(ctx.crate_path.clone()));
                        out.blit(36, 5);
                        out.push_ident(&ctx.lifetime);
                        out.blit_punct(1);
                        out.tt_group(Delimiter::Parenthesis, at);
                    };
                    out.blit(593, 4);
                    out.buf.push(pred_group);
                    out.blit(597, 3);
                    {
                        let at = out.buf.len();
                        out.blit(319, 3);
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
            out.blit_ident(92);
            {
                let at = out.buf.len();
                out.blit(348, 3);
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
    let ctx = Ctx::new(output, target);
    let is_string_enum = !target.untagged
        && target.tag.is_none()
        && target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0
        && target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) == 0;
    if target.from_toml {
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
        let len = (&mut rust_writer).buf.len();
        (&mut rust_writer).blit_punct(12);
        {
            let at = (&mut rust_writer).buf.len();
            (&mut rust_writer).blit_ident(2);
            {
                let at = (&mut rust_writer).buf.len();
                (&mut rust_writer).blit(600, 4);
                (&mut rust_writer).tt_group(Delimiter::Parenthesis, at);
            };
            (&mut rust_writer).tt_group(Delimiter::Bracket, at);
        };
        (&mut rust_writer).blit(604, 5);
        (&mut rust_writer)
            .buf
            .push(TokenTree::Group(Group::new(Delimiter::Brace, ts)));
        (&mut rust_writer).blit_punct(2);
        (&mut rust_writer).split_off_stream(len)
    }
}
pub fn derive(stream: TokenStream) -> TokenStream {
    Error::try_catch_handle(stream, inner_derive)
}
