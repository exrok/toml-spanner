use crate::ast::{
    self, DefaultKind, DeriveTargetInner, DeriveTargetKind, EnumKind, EnumVariant, Field,
    FieldAttrs, Generic, GenericKind, UnknownFieldPolicy, ENUM_CONTAINS_STRUCT_VARIANT,
    ENUM_CONTAINS_TUPLE_VARIANT, ENUM_CONTAINS_UNIT_VARIANT, FROM_TOML, TO_TOML,
};
use crate::case::RenameRule;
use crate::util::MemoryPool;
use crate::writer::RustWriter;
use crate::Error;
use proc_macro2::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[rustfmt::skip]
macro_rules! throw {
    ($literal: literal @ $span: expr, $($tt:tt)*) => { Error::span_msg_ctx($literal, &($($tt)*), $span) };
    ($literal: literal, $($tt:tt)*) => { Error::msg_ctx($literal, &($($tt)*)) };
    ($literal: literal @ $span: expr) => { Error::span_msg($literal, $span) };
    ($literal: literal) => { Error::msg($literal) };
}

#[allow(unused)]
enum StaticToken {
    Ident(&'static str),
    // bool: true if alone
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

#[rustfmt::skip]
macro_rules! append_tok {
    ($ident:ident $d:tt) => {
       $d.tt_ident(stringify!($ident))
    };
    ({} $d: tt) => {
        $d.tt_group_empty(Delimiter::Brace)
    };
    (() $d: tt) => {
        $d.tt_group_empty(Delimiter::Parenthesis)
    };
    ([] $d:tt) => {
        $d.tt_group_empty(Delimiter::Bracket)
    };
    ({$($tt:tt)*} $d: tt) => {{
        let at = $d.buf.len(); $(append_tok!($tt $d);)* $d.tt_group(Delimiter::Brace, at);
    }};
    (($($tt:tt)*) $d: tt) => {{
        let at = $d.buf.len(); $(append_tok!($tt $d);)* $d.tt_group(Delimiter::Parenthesis, at);
    }};
    ([[$($tt:tt)*]] $d:tt) => {{
        let at = $d.buf.len(); $(append_tok!($tt $d);)* $d.tt_group(Delimiter::Bracket, at);
    }};
    (_ $d:tt) => { $d.tt_ident("_") };
    ([$ident:ident] $d:tt) => {
        $d.buf.push($($tt)*)
    };
    ([?($($cond:tt)*) $($body:tt)*] $d:tt) => {
        if $($cond)* { $(append_tok!($body $d);)* }
    };
    ([#: $($tt:tt)*] $d:tt) => {
        $d.push_ident($($tt)*)
    };
    ([@$($tt:tt)*] $d:tt) => {
        $d.buf.push($($tt)*)
    };
    ([for ($($iter:tt)*) {$($body:tt)*}] $d:tt) => {
        for $($iter)* { $(append_tok!($body $d);)* }
    };
    ([#$($tt:tt)*] $d:tt) => {
        $d.buf.push(TokenTree::from($($tt)*.clone()))
    };
    ([~$($tt:tt)*] $d:tt) => {
        $d.buf.extend_from_slice($($tt)*)
    };
    ([$($rust:tt)*] $d:tt) => {{
         $($rust)*
    }};
    (# $d:tt) => { $d.tt_punct_joint('\'') };
    (: $d:tt) => { $d.tt_punct_alone(':') };
    (+ $d:tt) => { $d.tt_punct_alone('+') };
    (~ $d:tt) => { $d.tt_punct_joint('#') };
    (< $d:tt) => { $d.tt_punct_alone('<') };
    (|| $d:tt) => { $d.tt_punct_joint('|');$d.tt_punct_alone('|') };
    (% $d:tt) => { $d.tt_punct_joint(':') };
    (== $d:tt) => {$d.tt_punct_joint('='); $d.tt_punct_alone('=') };
    (!= $d:tt) => {$d.tt_punct_joint('!'); $d.tt_punct_alone('=') };
    (:: $d:tt) => {$d.tt_punct_joint(':'); $d.tt_punct_alone(':') };
    (-> $d:tt) => {$d.tt_punct_joint('-'); $d.tt_punct_alone('>') };
    (=> $d:tt) => {$d.tt_punct_joint('='); $d.tt_punct_alone('>') };
    (>= $d:tt) => {$d.tt_punct_joint('>'); $d.tt_punct_alone('=') };
    (> $d:tt) => { $d.tt_punct_alone('>') };
    (! $d:tt) => { $d.tt_punct_alone('!') };
    (| $d:tt) => { $d.tt_punct_alone('|') };
    (. $d:tt) => { $d.tt_punct_alone('.') };
    (; $d:tt) => { $d.tt_punct_alone(';') };
    (& $d:tt) => { $d.tt_punct_alone('&') };
    (= $d:tt) => { $d.tt_punct_alone('=') };
    (, $d:tt) => { $d.tt_punct_alone(',') };
    (* $d:tt) => { $d.tt_punct_alone('*') };
    (? $d:tt) => { $d.tt_punct_alone('?') };
}

macro_rules! splat { ($d:tt; $($tt:tt)*) => { { $(append_tok!($tt $d);)* } } }

macro_rules! token_stream { ($d:tt; $($tt:tt)*) => {{
    let len = $d.buf.len(); $(append_tok!($tt $d);)* $d.split_off_stream(len)
}}}

struct GenericBoundFormatting {
    lifetimes: bool,
    bounds: bool,
}
fn fmt_generics(buffer: &mut RustWriter, generics: &[Generic], fmt: GenericBoundFormatting) {
    let mut first = true;
    for generic in generics {
        if !fmt.lifetimes && matches!(generic.kind, GenericKind::Lifetime) {
            continue;
        }
        if first {
            first = false;
        } else {
            append_tok!(,buffer);
        }
        match generic.kind {
            GenericKind::Lifetime => {
                append_tok!(#buffer);
            }
            GenericKind::Type => (),
            GenericKind::Const => {
                append_tok!(const buffer);
            }
        }
        buffer.buf.push(generic.ident.clone().into());
        if fmt.bounds && !generic.bounds.is_empty() {
            append_tok!(: buffer);
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
            splat!(out; ::toml_spanner);
            TokenTree::Group(Group::new(Delimiter::None, out.split_off_stream(0)))
        } else {
            splat!(out; ::toml_spanner);
            TokenTree::Group(Group::new(Delimiter::None, out.split_off_stream(0)))
        };
        let (lt, generics) = if let [Generic {
            kind: GenericKind::Lifetime,
            ident,
            bounds,
        }, rest @ ..] = &target.generics[..]
        {
            if !bounds.is_empty() {
                throw!("Bounded lifetimes currently unsupported")
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
    splat! {
        output;
        ~[[automatically_derived]]
        impl <#[#: &ctx.lifetime]
            [?(!ctx.generics.is_empty()), [fmt_generics(output, ctx.generics, DEF)]] >
         [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> for [#: &target.name][?(any_generics) <
            [fmt_generics(output, &target.generics, USE)]
        >]  [?(!target.where_clauses.is_empty() || !target.generic_field_types.is_empty() || !target.generic_flatten_field_types.is_empty())
             where [for (ty in &target.generic_field_types) {
                [~ty]: [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]>,
            }] [for (ty in &target.generic_flatten_field_types) {
                [~ty]: [#ctx.crate_path]::FromFlattened<#[#: &ctx.lifetime]>,
            }] [~&target.where_clauses] ]
        {
            fn from_toml(
                __ctx: &mut [#ctx.crate_path]::Context<#[#: &ctx.lifetime]>,
                __item: &[#ctx.crate_path]::Item<#[#: &ctx.lifetime]>,
            ) -> ::std::result::Result<Self, [#ctx.crate_path]::Failed> [@TokenTree::Group(Group::new(Delimiter::Brace, inner))]
        }
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
    splat! {
        output;
        ~[[automatically_derived]]
        impl [?(!target.generics.is_empty()) < [fmt_generics(output, &target.generics, DEF)] >]
         [#ctx.crate_path]::ToToml for [#: &target.name][?(any_generics) <
            [fmt_generics(output, &target.generics, USE)]
        >]  [?(!target.where_clauses.is_empty() || !target.generic_field_types.is_empty() || !target.generic_flatten_field_types.is_empty())
             where [for (ty in &target.generic_field_types) {
                [~ty]: [#ctx.crate_path]::ToToml,
            }] [for (ty in &target.generic_flatten_field_types) {
                [~ty]: [#ctx.crate_path]::ToFlattened,
            }] [~&target.where_clauses] ]
        {
            fn to_toml<# [#lf]>(& # [#lf] self, __arena: & # [#lf] [#ctx.crate_path]::Arena)
                -> ::std::result::Result<[#ctx.crate_path]::Item<# [#lf]>, [#ctx.crate_path]::ToTomlError>
                [@TokenTree::Group(Group::new(Delimiter::Brace, inner))]
        }
    };
}

fn emit_table_alloc(out: &mut RustWriter, ctx: &Ctx, var: &str, capacity: usize) {
    let var_id = Ident::new(var, Span::mixed_site());
    splat!(out;
        let Some(mut [#var_id]) = [#ctx.crate_path]::Table::try_with_capacity(
            [@TokenTree::Literal(Literal::usize_unsuffixed(capacity))],
            __arena
        ) else {
            return Err([#ctx.crate_path]::ToTomlError::from([@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]));
        };
    );
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
                throw!("Only one #[toml(flatten)] field is allowed")
            }
            flatten_field = Some(field);
        }
    }

    if recoverable {
        splat!(out; let mut __failed = false;);
    }

    // Declare field variables
    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            splat!(out; let mut __flatten_partial =);
            emit_flatten_prefix(out, ctx, field, FROM_TOML);
            splat!(out; ::init(););
            continue;
        }
        if field.flags & Field::WITH_FROM_TOML_SKIP != 0 {
            emit_field_default(out, field, FROM_TOML, false);
        } else if field.flags & Field::WITH_FROM_TOML_OPTION != 0 {
            splat!(out; let mut [#: field.name] : [~field.ty] = None;);
        } else {
            splat!(out; let mut [#: field.name] = None::< [~field.ty] >;);
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
            splat!(out; let mut [#: &span_ident] = toml_spanner::Span::new([@zero.clone()], [@zero]););
        }
    }

    // Build for loop: for (__key, __value) in table { match __key.name { ... } }
    emit_for_table_header(out, table_ident);
    let for_body_at = out.buf.len();
    splat!(out; match __key . name);
    let arms_at = out.buf.len();

    // Skip key arms
    for skip_key in skip_keys {
        splat!(out; [@skip_key.clone().into()] => {});
    }

    // Field arms
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

        // Pattern: name | alias1 | deprecated_alias1 =>
        splat!(out; [@name_lit.clone().into()]);
        field.attr.for_each_alias(FROM_TOML, &mut |alias| {
            splat!(out; | [@alias.clone().into()]);
        });
        field
            .attr
            .for_each_deprecated_alias(FROM_TOML, &mut |_, alias| {
                splat!(out; | [@alias.clone().into()]);
            });
        splat!(out; =>);
        let arm_body_at = out.buf.len();

        if has_aliases {
            let span_ident = Ident::new(
                &{
                    let mut s = field.name.to_string();
                    s.push_str("_first_span");
                    s
                },
                Span::mixed_site(),
            );
            splat!(out;
                if [#: field.name].is_some() {
                    return Err(__ctx.report_duplicate_field([@name_lit.clone().into()], __key.span, [#: &span_ident], __value));
                }
            );
        }

        if has_deprecated_aliases {
            let name_for_new = name_lit.clone();
            field
                .attr
                .for_each_deprecated_alias(FROM_TOML, &mut |tag, alias| {
                    splat!(out;
                        if __key . name == [@alias.clone().into()] {
                            __ctx . report_deprecated_field(
                                [if let Some(tag_tokens) = tag {
                                    out.buf.extend_from_slice(tag_tokens);
                                } else {
                                    out.buf.push(TokenTree::Literal(Literal::u32_suffixed(0)));
                                }]
                                , & [@alias.clone().into()]
                                , & [@name_for_new.clone().into()]
                                , __key . span
                                , __value
                            );
                        }
                    );
                });
        }

        // match from_toml_call { Ok(__val) => { field = Some(__val); } Err(...) }
        splat!(out; match);
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
            splat!(out;
                Ok(__val) => { [#: field.name] = Some(__val); [#: &span_ident] = __key.span; }
                [?(is_required && !recoverable) Err(__e) => return Err(__e),]
                [?(is_required && recoverable) Err(_) => { __failed = true; },]
                [?(!is_required) Err(_) => {},]
            );
        } else {
            splat!(out;
                Ok(__val) => { [#: field.name] = Some(__val); }
                [?(is_required && !recoverable) Err(__e) => return Err(__e),]
                [?(is_required && recoverable) Err(_) => { __failed = true; },]
                [?(!is_required) Err(_) => {},]
            );
        }
        out.tt_group(Delimiter::Brace, match_body_at);

        out.tt_group(Delimiter::Brace, arm_body_at);
    }

    // Wildcard arm
    if let Some(ff) = flatten_field {
        splat!(out; _ =>);
        let wild_at = out.buf.len();
        splat!(out; let _ =);
        emit_flatten_prefix(out, ctx, ff, FROM_TOML);
        splat!(out; ::insert( __ctx, __key, __value, &mut __flatten_partial););
        out.tt_group(Delimiter::Brace, wild_at);
    } else {
        emit_unknown_field_arm(out, ctx);
    }
    out.tt_group(Delimiter::Brace, arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);

    if recoverable {
        splat!(out; if __failed);
        let if_at = out.buf.len();
        splat!(out; return Err([#ctx.crate_path]::Failed););
        out.tt_group(Delimiter::Brace, if_at);
    }

    // Finish flatten partial
    if let Some(ff) = flatten_field {
        let table_id = Ident::new(table_ident, Span::mixed_site());
        splat!(out; let [#: ff.name] =);
        emit_flatten_prefix(out, ctx, ff, FROM_TOML);
        splat!(out; ::finish( __ctx, [#table_id], __flatten_partial)?;);
    }

    // Unwrap required and default fields
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
            splat!(out; let Some([#: field.name]) = [#: field.name].take() else);
            let else_at = out.buf.len();
            splat!(out;
                return Err(__ctx.report_missing_field([@name_lit.into()], __item));
            );
            out.tt_group(Delimiter::Brace, else_at);
            splat!(out; ;);
        }
    }
}

/// Emit a field default: either as initial `let field = default;` or as unwrap `let field = field.unwrap_or...;`
fn emit_field_default(out: &mut RustWriter, field: &Field, direction: u8, is_unwrap: bool) {
    if is_unwrap {
        if let Some(default_kind) = field.default(direction) {
            match default_kind {
                DefaultKind::Custom(tokens) => {
                    splat!(out;
                        let [#: field.name] = [#: field.name].unwrap_or_else(|| [~tokens.as_slice()]);
                    );
                }
                DefaultKind::Default => {
                    splat!(out; let [#: field.name] = [#: field.name].unwrap_or_default(););
                }
            }
        } else {
            splat!(out; let [#: field.name] = [#: field.name].unwrap_or_default(););
        }
    } else {
        if let Some(default_kind) = field.default(direction) {
            match default_kind {
                DefaultKind::Custom(tokens) => {
                    splat!(out; let [#: field.name] = [~tokens.as_slice()];);
                }
                DefaultKind::Default => {
                    splat!(out; let [#: field.name] = Default::default(););
                }
            }
        } else {
            splat!(out; let [#: field.name] = Default::default(););
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

        let field_ref = token_stream!(out;
            [?(self_access) & self . [#: field.name]]
            [?(!self_access) [#: field.name]]
        );

        let is_option = first_ty_ident == "Option";

        let style = field.style(TO_TOML);

        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            let val_expr = if is_option {
                splat!(out; if let Some(__val) = [@TokenTree::Group(Group::new(Delimiter::None, field_ref.clone()))]);
                token_stream!(out; __val)
            } else {
                field_ref.clone()
            };
            let insert_start = out.buf.len();
            splat!(out;
                [#table_id].insert_unique(
                    [#ctx.crate_path]::Key::new([@name_lit.into()]),
                    [~with]::to_toml([@TokenTree::Group(Group::new(Delimiter::None, val_expr))], __arena)?
                    [?(let Some(style) = style) .with_style_of_array_or_table([#ctx.crate_path]::TableStyle::[#style])],
                    __arena,
                );
            );
            if is_option {
                let insert_body = out.split_off_stream(insert_start);
                out.buf
                    .push(TokenTree::Group(Group::new(Delimiter::Brace, insert_body)));
            }
        } else if is_option {
            splat!(out;
                if let Some(__val) = [#ctx.crate_path]::ToToml::to_optional_toml(
                    [@TokenTree::Group(Group::new(Delimiter::None, field_ref.clone()))], __arena)?
                {
                    [#table_id].insert_unique(
                        [#ctx.crate_path]::Key::new([@name_lit.into()]),
                        __val
                        [?(let Some(style) = style) .with_style_of_array_or_table([#ctx.crate_path]::TableStyle::[#style])],
                        __arena,
                    );
                }
            );
        } else {
            splat!(out;
                [#table_id].insert_unique(
                    [#ctx.crate_path]::Key::new([@name_lit.into()]),
                    [#ctx.crate_path]::ToToml::to_toml(
                        [@TokenTree::Group(Group::new(Delimiter::None, field_ref.clone()))], __arena)?
                    [?(let Some(style) = style) .with_style_of_array_or_table([#ctx.crate_path]::TableStyle::[#style])],
                    __arena,
                );
            );
        }

        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            splat!(out;
                if !([~skip_tokens])([@TokenTree::Group(Group::new(Delimiter::None, field_ref))])
                    [@TokenTree::Group(Group::new(Delimiter::Brace, emit_body))]
            );
        }
    }

    for field in fields {
        if field.flags & Field::WITH_FLATTEN != 0 {
            let field_ref = token_stream!(out;
                [?(self_access) & self . [#: field.name]]
                [?(!self_access) [#: field.name]]
            );
            emit_flatten_prefix(out, ctx, field, TO_TOML);
            splat!(out;
                ::to_flattened(
                    [@TokenTree::Group(Group::new(Delimiter::None, field_ref))],
                    __arena, &mut [#table_id])?;
            );
        }
    }
}

fn emit_struct_variant_to_arm(out: &mut RustWriter, variant: &EnumVariant, arm_body_start: usize) {
    let arm_body = out.split_off_stream(arm_body_start);
    splat!(out;
        Self::[#: variant.name] {
            [for field in variant.fields { splat!(out; ref [#: field.name],); }]
        } =>
            [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
    );
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
    splat!(out;
        Ok(Self::[#: variant.name] {
            [for field in variant.fields { splat!(out; [#: field.name],); }]
        })
    );
}

fn struct_from_toml(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let start = out.buf.len();
    splat!(out; let __table = __item.expect_table(__ctx)?;);
    emit_table_field_deser(out, ctx, fields, "__table", None, &[]);
    splat!(out;
        Ok(Self {
            [for field in fields { splat!(out; [#: field.name],); }]
        })
    );
    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}

fn struct_to_toml(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let start = out.buf.len();
    emit_table_alloc(out, ctx, "__table", count_ser_fields(fields));
    emit_table_field_ser(out, ctx, fields, "__table", None, true);
    splat!(out; Ok(__table.into_item()));
    let body = out.split_off_stream(start);
    impl_to_toml(out, ctx, body);
}

fn emit_proxy_from_toml(output: &mut RustWriter, ctx: &Ctx) -> bool {
    if let Some(from_ty) = &ctx.target.from_type {
        let body = token_stream! {
            output;
            let __proxy = < [~from_ty] as [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> >::from_toml(
                __ctx, __item
            ) ?;
            Ok(::std::convert::From::from(__proxy))
        };
        impl_from_toml(output, &ctx, body);
        true
    } else if let Some(try_from_ty) = &ctx.target.try_from_type {
        let body = token_stream! {
            output;
            let __proxy = < [~try_from_ty] as [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> >::from_toml(
                __ctx, __item
            ) ?;
            match ::std::convert::TryFrom::try_from(__proxy) {
                Ok(__val) => Ok(__val),
                Err(__e) => Err(__ctx.push_error(
                    [#ctx.crate_path]::Error::custom(__e, __item.span())
                )),
            }
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
                throw!("Struct must contain a single field to use transparent")
            };
            let body = token_stream! {
                output;
                < [~single_field.ty] as [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> >::from_toml(
                    __ctx, __item
                )
            };
            impl_from_toml(output, &ctx, body);
        } else {
            struct_from_toml(output, &ctx, fields);
        }
    }

    if target.to_toml {
        if target.transparent_impl {
            let [single_field] = fields else {
                throw!("Struct must contain a single field to use transparent")
            };
            let body = token_stream! {output;
                [#ctx.crate_path]::ToToml::to_toml(&self.[#: single_field.name], __arena)
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
            let body = token_stream! {
                output;
                Ok([#: &target.name](
                    < [~single_field.ty] as [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> >::from_toml(
                        __ctx, __item
                    ) ?
                ))
            };
            impl_from_toml(output, &ctx, body);
        } else {
            throw!("FromToml on tuple structs requires exactly one field (transparent delegation)")
        }
    }

    if target.to_toml {
        if let [_single_field] = fields {
            let body = token_stream! {output;
                [#ctx.crate_path]::ToToml::to_toml(&self.[@TokenTree::Literal(Literal::usize_unsuffixed(0))], __arena)
            };
            impl_to_toml(output, &ctx, body);
        } else {
            throw!("ToToml on tuple structs requires exactly one field (transparent delegation)")
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
    splat!(out;
        [#: &table_id].insert_unique(
            [#ctx.crate_path]::Key::new([@tag_lit.clone().into()]),
            [#ctx.crate_path]::Item::string([@name_lit.into()]),
            __arena,
        );
    );
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
            splat!(out; _ => {});
        }
        UnknownFieldPolicy::Warn { tag } => {
            splat!(out;
                _ => {
                    __ctx.error_unexpected_key(
                        [emit_tag_value(out, tag.as_deref())]
                        , __value, __key.span);
                }
            );
        }
        UnknownFieldPolicy::Deny { tag } => {
            splat!(out;
                _ => {
                    return Err(__ctx.error_unexpected_key(
                        [emit_tag_value(out, tag.as_deref())]
                        , __value, __key.span));
                }
            );
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
        splat!(out; _ => Ok(Self::[#: ov.name]),);
    } else {
        splat!(out;
            _ => Err(__ctx.error_expected_but_found(
                &[@TokenTree::Literal(Literal::string(msg))], __item)),
        );
    }
}

fn emit_for_table_header(out: &mut RustWriter, table_var: &str) {
    let table_id = Ident::new(table_var, Span::mixed_site());
    splat!(out; for);
    let pat_at = out.buf.len();
    splat!(out; __key, __value);
    out.tt_group(Delimiter::Parenthesis, pat_at);
    splat!(out; in [#table_id]);
}

fn emit_flatten_prefix(out: &mut RustWriter, ctx: &Ctx, field: &Field, direction: u8) {
    if let Some(with) = field.with(direction) {
        out.buf.extend_from_slice(with);
    } else {
        if direction == FROM_TOML {
            splat!(out; < [~field.ty] as [#ctx.crate_path]::FromFlattened<#[#: &ctx.lifetime]> >);
        } else {
            splat!(out; [#ctx.crate_path]::ToFlattened);
        }
    }
}

fn emit_from_toml_call(out: &mut RustWriter, ctx: &Ctx, field: &Field, ty: &[TokenTree]) {
    if let Some(with) = field.with(FROM_TOML) {
        splat!(out; [~with]::from_toml(__ctx, __value));
    } else {
        splat!(out; < [~ty] as [#ctx.crate_path]::FromToml<#[#: &ctx.lifetime]> >::from_toml(__ctx, __value));
    }
}

fn enum_from_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    let other_variant = find_other_variant(variants);
    splat!(out;
        let s = __item.expect_string(__ctx)?;
        match s {
            [for variant in variants {
                if variant.other { continue; }
                let name_lit = variant_name_literal(ctx, variant);
                splat!(out; [@name_lit.into()] => Ok(Self::[#: variant.name]),);
            }]
            [if let Some(ov) = other_variant {
                splat!(out; _ => Ok(Self::[#: ov.name]),);
            } else {
                let expected_array = {
                    let mut ts = TokenStream::new();
                    for variant in variants {
                        if variant.other { continue; }
                        let name_lit = variant_name_literal(ctx, variant);
                        ts.extend([name_lit.into(), TokenTree::Punct(Punct::new(',', Spacing::Alone))]);
                    }
                    TokenTree::Group(Group::new(Delimiter::Bracket, ts))
                };
                splat!(out;
                    _ => Err(__ctx.error_unexpected_variant(
                        &[@expected_array], __item)),
                );
            }]
        }
    );
    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}

fn enum_to_toml_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    splat!(out;
        Ok([#ctx.crate_path]::Item::string(match self {
            [for variant in variants {
                let name_lit = variant_name_literal(ctx, variant);
                splat!(out; Self::[#: variant.name] => [@name_lit.into()],);
            }]
        }))
    );
    let body = out.split_off_stream(start);
    impl_to_toml(out, ctx, body);
}

fn enum_to_toml(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant], mode: &TagMode) {
    let start = out.buf.len();
    splat!(out; match self);
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
                    splat!(out;
                        Self::[#: variant.name] => {
                            [emit_table_alloc(out, ctx, "table", 1)]
                            [emit_tag_insert(out, ctx, "table", tag, name_lit)]
                            Ok(table.into_item())
                        }
                    );
                } else {
                    splat!(out;
                        Self::[#: variant.name] =>
                            Ok([#ctx.crate_path]::Item::string([@name_lit.into()])),
                    );
                }
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    throw!("Only single-field tuple variants are supported")
                }
                if matches!(mode, TagMode::Untagged) {
                    splat!(out;
                        Self::[#: variant.name](inner) =>
                            [#ctx.crate_path]::ToToml::to_toml(inner, __arena),
                    );
                } else {
                    let cap = if tag_lit.is_some() { 2 } else { 1 };
                    splat!(out;
                        Self::[#: variant.name](inner) => {
                            [emit_table_alloc(out, ctx, "table", cap)]
                            [if let Some(tag) = tag_lit {
                                emit_tag_insert(out, ctx, "table", tag, name_lit.clone());
                            }]
                            table.insert_unique(
                                [#ctx.crate_path]::Key::new([match mode {
                                    TagMode::External => { out.buf.push(name_lit.into()); }
                                    TagMode::Adjacent(_, content) => { out.buf.push(TokenTree::Literal((*content).clone())); }
                                    _ => {}
                                }]),
                                [#ctx.crate_path]::ToToml::to_toml(inner, __arena)?,
                                __arena,
                            );
                            Ok(table.into_item())
                        }
                    );
                }
            }
            EnumKind::Struct => {
                let n = count_ser_fields(variant.fields);
                let arm_body_start = out.buf.len();

                let table_cap = n + if matches!(mode, TagMode::Internal(_)) {
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
                        splat!(out;
                            outer.insert_unique(
                                [#ctx.crate_path]::Key::new([@name_lit.into()]),
                                table.into_item(), __arena,
                            );
                            Ok(outer.into_item())
                        );
                    }
                    TagMode::Adjacent(tag, content) => {
                        emit_table_alloc(out, ctx, "outer", 2);
                        emit_tag_insert(out, ctx, "outer", tag, name_lit);
                        splat!(out;
                            outer.insert_unique(
                                [#ctx.crate_path]::Key::new([@TokenTree::Literal((*content).clone())]),
                                table.into_item(), __arena,
                            );
                            Ok(outer.into_item())
                        );
                    }
                    _ => {
                        splat!(out; Ok(table.into_item()));
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
        splat!(out;
            return match s {
                [for variant in variants {
                    if matches!(variant.kind, EnumKind::None) && !variant.other {
                        let name_lit = variant_name_literal(ctx, variant);
                        splat!(out; [@name_lit.into()] => Ok(Self::[#: variant.name]),);
                    }
                }]
                [emit_wildcard_arm(out, ctx, other_variant, "a known variant")]
            };
        );
        let if_body = out.split_off_stream(if_body_start);
        splat!(out;
            if let Some(s) = __item.as_str()
                [@TokenTree::Group(Group::new(Delimiter::Brace, if_body))]
        );
    }

    if has_complex {
        splat!(out;
            let table = __item.expect_table(__ctx)?;
            let entries = table.entries();
        );
        let err_body_start = out.buf.len();
        splat!(out;
            return Err(__ctx.error_expected_but_found(
                &[@TokenTree::Literal(Literal::string("a table with exactly one key"))], __item));
        );
        let err_body = out.split_off_stream(err_body_start);
        let one_lit = TokenTree::Literal(Literal::usize_unsuffixed(1));
        let zero_index = TokenTree::Group(Group::new(
            Delimiter::Bracket,
            TokenStream::from(TokenTree::Literal(Literal::usize_unsuffixed(0))),
        ));
        splat!(out;
            if !(entries.len() == [@one_lit])
                [@TokenTree::Group(Group::new(Delimiter::Brace, err_body))]
            let (key, value) = &entries [@zero_index];
        );

        splat!(out; match key . name);
        let arms_at = out.buf.len();
        for variant in variants {
            if matches!(variant.kind, EnumKind::None) || variant.other {
                continue;
            }
            let name_lit = variant_name_literal(ctx, variant);
            match variant.kind {
                EnumKind::Tuple => {
                    if variant.fields.len() != 1 {
                        throw!("Only single-field tuple variants are supported in external tagging")
                    }
                    splat!(out;
                        [@name_lit.into()] => Ok(Self::[#: variant.name](
                            [#ctx.crate_path]::FromToml::from_toml(__ctx, value)?
                        )),
                    );
                }
                EnumKind::Struct => {
                    splat!(out; [@name_lit.into()] =>);
                    let arm_at = out.buf.len();
                    splat!(out;
                        let __item = value;
                        let __subtable = value.expect_table(__ctx)?;
                    );
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
        splat!(out;
            Err(__ctx.error_expected_but_found(
                &[@TokenTree::Literal(Literal::string("a known variant"))], __item))
        );
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
        throw!("Tuple variants are not supported with internal tagging")
    }

    let start = out.buf.len();

    splat!(out; let __table = __item.expect_table(__ctx)?;);

    // First pass: find tag
    splat!(out; let mut __tag: Option<&str> = None;);
    emit_for_table_header(out, "__table");
    let tag_body_at = out.buf.len();
    splat!(out;
        if __key.name == [@tag_lit.clone().into()] {
            __tag = Some([#ctx.crate_path]::FromToml::from_toml(__ctx, __value)?);
            break;
        }
    );
    out.tt_group(Delimiter::Brace, tag_body_at);

    // Check tag was found
    splat!(out; let Some(__tag) = __tag else);
    let else_at = out.buf.len();
    splat!(out;
        return Err(__ctx.report_missing_field([@tag_lit.clone().into()], __item));
    );
    out.tt_group(Delimiter::Brace, else_at);
    splat!(out; ;);

    // Second pass: dispatch to variant
    let other_variant = find_other_variant(variants);
    splat!(out; match __tag);
    let arms_at = out.buf.len();
    for variant in variants {
        if variant.other {
            continue;
        }
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                splat!(out; [@name_lit.into()] =>);
                let arm_at = out.buf.len();
                emit_for_table_header(out, "__table");
                let check_at = out.buf.len();
                match &ctx.target.unknown_fields {
                    UnknownFieldPolicy::Ignore => {
                        splat!(out; let _ = __key;);
                    }
                    UnknownFieldPolicy::Warn { tag } => {
                        splat!(out;
                            if __key.name != [@tag_lit.clone().into()] {
                                __ctx.error_unexpected_key(
                                    [emit_tag_value(out, tag.as_deref())]
                                    , __value, __key.span);
                            }
                        );
                    }
                    UnknownFieldPolicy::Deny { tag } => {
                        splat!(out;
                            if __key.name != [@tag_lit.clone().into()] {
                                return Err(__ctx.error_unexpected_key(
                                    [emit_tag_value(out, tag.as_deref())]
                                    , __value, __key.span));
                            }
                        );
                    }
                }
                out.tt_group(Delimiter::Brace, check_at);
                splat!(out; Ok(Self::[#: variant.name]));
                out.tt_group(Delimiter::Brace, arm_at);
            }
            EnumKind::Struct => {
                splat!(out; [@name_lit.into()] =>);
                let arm_at = out.buf.len();
                splat!(out; let __subtable = __table;);
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

    splat!(out;
        let __table = __item.expect_table(__ctx)?;
        let mut __tag: Option<&str> = None;
        let mut __content: Option<& [#ctx.crate_path]::Item<#[#: &ctx.lifetime]> > = None;
    );

    // Extraction loop
    emit_for_table_header(out, "__table");
    let for_body_at = out.buf.len();
    splat!(out; match __key . name);
    let extract_arms_at = out.buf.len();
    splat!(out;
        [@tag_lit.clone().into()] => {
            __tag = Some([#ctx.crate_path]::FromToml::from_toml(__ctx, __value)?);
        }
        [@content_lit.clone().into()] => {
            __content = Some(__value);
        }
    );
    emit_unknown_field_arm(out, ctx);
    out.tt_group(Delimiter::Brace, extract_arms_at);
    out.tt_group(Delimiter::Brace, for_body_at);

    // Check tag was found
    splat!(out; let Some(__tag) = __tag else);
    let else_at = out.buf.len();
    splat!(out;
        return Err(__ctx.report_missing_field([@tag_lit.clone().into()], __item));
    );
    out.tt_group(Delimiter::Brace, else_at);
    splat!(out; ;);

    // Dispatch on tag
    let other_variant = find_other_variant(variants);
    splat!(out; match __tag);
    let arms_at = out.buf.len();
    for variant in variants {
        if variant.other {
            continue;
        }
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                splat!(out; [@name_lit.into()] => { Ok(Self::[#: variant.name]) });
            }
            EnumKind::Tuple | EnumKind::Struct => {
                splat!(out; [@name_lit.into()] =>);
                let arm_at = out.buf.len();

                // Require content
                splat!(out; let Some(__content) = __content else);
                let ce_at = out.buf.len();
                splat!(out;
                    return Err(__ctx.report_missing_field([@content_lit.clone().into()], __item));
                );
                out.tt_group(Delimiter::Brace, ce_at);
                splat!(out; ;);

                match variant.kind {
                    EnumKind::Tuple => {
                        if variant.fields.len() != 1 {
                            throw!("Only single-field tuple variants are supported")
                        }
                        splat!(out;
                            Ok(Self::[#: variant.name](
                                [#ctx.crate_path]::FromToml::from_toml(__ctx, __content)?
                            ))
                        );
                    }
                    EnumKind::Struct => {
                        splat!(out;
                            let __item = __content;
                            let __subtable = __content.expect_table(__ctx)?;
                        );
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
                splat!(out;
                    if let Some(__s) = __item.as_str() {
                        if __s == [@name_lit.into()] {
                            return Ok(Self::[#: variant.name]);
                        }
                    }
                    [?(propagate) return Err(__ctx.error_expected_but_found(
                        &[@TokenTree::Literal(Literal::string("a matching variant"))], __item));]
                );
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    throw!("Only single-field tuple variants are supported in untagged enums")
                }
                let match_start = out.buf.len();
                splat!(out;
                    match [#ctx.crate_path]::FromToml::from_toml(__ctx, __item) {
                        Ok(__val) => return Ok(Self::[#: variant.name](__val)),
                        [?(propagate) Err(__e) => return Err(__e),]
                        [?(!propagate) Err(_) => { __ctx.errors.truncate(__err_len); }]
                    }
                );
                if !propagate {
                    let match_body = out.split_off_stream(match_start);
                    splat!(out; { let __err_len = __ctx.errors.len(); [@TokenTree::Group(Group::new(Delimiter::None, match_body))] });
                }
            }
            EnumKind::Struct => {
                let body_start = out.buf.len();
                splat!(out; let __subtable = __item.expect_table(__ctx)?;);
                emit_table_field_deser(out, ctx, variant.fields, "__subtable", Some(variant), &[]);
                if propagate {
                    splat!(out;
                        return Ok(Self::[#: variant.name] {
                            [for field in variant.fields { splat!(out; [#: field.name],); }]
                        });
                    );
                } else {
                    emit_ok_self_variant(out, variant);
                    let closure_body = out.split_off_stream(body_start);
                    let closure_body_group =
                        TokenTree::Group(Group::new(Delimiter::Brace, closure_body));

                    splat!(out; {
                        let __err_len = __ctx.errors.len();
                        let __result: ::std::result::Result<Self, [#ctx.crate_path]::Failed> =
                            (|| [@closure_body_group]) ();
                        match __result {
                            Ok(__val) => return Ok(__val),
                            Err(_) => { __ctx.errors.truncate(__err_len); }
                        }
                    });
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
            splat!(out; {
                let __pred: fn(& mut [#ctx.crate_path]::Context<#[#: &ctx.lifetime]>, & [#ctx.crate_path]::Item<#[#: &ctx.lifetime]>) -> bool = [@pred_group];
                if __pred(__ctx, __item) [@inner_group]
            });
        }
    }

    if !last_is_unhinted {
        splat!(out;
            Err(__ctx.error_expected_but_found(
                &[@TokenTree::Literal(Literal::string("a matching variant"))], __item))
        );
    }

    let body = out.split_off_stream(start);
    impl_from_toml(out, ctx, body);
}

fn handle_enum(output: &mut RustWriter, target: &DeriveTargetInner, variants: &[EnumVariant]) {
    if target.content.is_some() && target.tag.is_none() {
        throw!("content attribute requires tag to also be set")
    }
    if target.untagged && (target.tag.is_some() || target.content.is_some()) {
        throw!("untagged cannot be combined with tag or content attributes")
    }

    if !target.untagged {
        for variant in variants {
            if variant.try_if.is_some() || variant.final_if.is_some() {
                throw!("try_if/final_if can only be used on untagged enums" @ variant.name.span())
            }
        }
    }

    {
        let mut other_count = 0u32;
        for variant in variants {
            if variant.other {
                other_count += 1;
                if !matches!(variant.kind, EnumKind::None) {
                    throw!("#[toml(other)] can only be used on unit variants" @ variant.name.span())
                }
                if target.untagged {
                    throw!("#[toml(other)] cannot be used on untagged enums" @ variant.name.span())
                }
            }
        }
        if other_count > 1 {
            throw!("only one variant can be marked #[toml(other)]")
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
    token_stream!(
        (&mut rust_writer);
        ~[[allow(clippy::question_mark)]]
        const _: () = [@TokenTree::Group(Group::new(Delimiter::Brace, ts))];
    )
}

pub fn derive(stream: TokenStream) -> TokenStream {
    Error::try_catch_handle(stream, inner_derive)
}
