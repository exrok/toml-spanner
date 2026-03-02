use crate::ast::{
    self, DefaultKind, DeriveTargetInner, DeriveTargetKind, EnumKind, EnumVariant, Field,
    FieldAttrs, Generic, GenericKind, ENUM_CONTAINS_STRUCT_VARIANT, ENUM_CONTAINS_TUPLE_VARIANT,
    ENUM_CONTAINS_UNIT_VARIANT, FROM_ITEM, TO_ITEM,
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
            splat!(out; ::toml_spanner);
            std::mem::take(&mut out.buf)
        } else {
            splat!(out; ::toml_spanner);
            std::mem::take(&mut out.buf)
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
    splat! {
        output;
        ~[[automatically_derived]]
        impl <#[#: &ctx.lifetime]
            [?(!ctx.generics.is_empty()), [fmt_generics(output, ctx.generics, DEF)]] >
         [~&ctx.crate_path]::FromItem<#[#: &ctx.lifetime]> for [#: &target.name][?(any_generics) <
            [fmt_generics(output, &target.generics, USE)]
        >]  [?(!target.where_clauses.is_empty() || !target.generic_field_types.is_empty())
             where [for (ty in &target.generic_field_types) {
                [~ty]: [~&ctx.crate_path]::FromItem<#[#: &ctx.lifetime]>,
            }] [~&target.where_clauses] ]
        {
            fn from_item(
                __ctx: &mut [~&ctx.crate_path]::Context<#[#: &ctx.lifetime]>,
                __item: &[~&ctx.crate_path]::Item<#[#: &ctx.lifetime]>,
            ) -> ::std::result::Result<Self, [~&ctx.crate_path]::Failed> [@TokenTree::Group(Group::new(Delimiter::Brace, inner))]
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

fn struct_from_item(out: &mut RustWriter, ctx: &Ctx, fields: &[Field]) {
    let start = out.buf.len();

    // expect_table instead of table_helper — avoids bitset allocation
    splat!(out;
        let __table = __item.expect_table(__ctx)?;
    );

    // Declare field variables: skip fields get their default, others get Option slots
    for field in fields {
        if field.flags & Field::WITH_FROMITEM_SKIP != 0 {
            if let Some(default_kind) = field.default(FROM_ITEM) {
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
        } else if is_option_type(field) {
            splat!(out; let mut [#: field.name] : [~field.ty] = None;);
        } else {
            splat!(out; let mut [#: field.name] = None :: < [~field.ty] >;);
        }
    }

    // Build match arms for key dispatch
    let match_arms_start = out.buf.len();
    for field in fields {
        if field.flags & Field::WITH_FROMITEM_SKIP != 0 {
            continue;
        }

        let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;
        let is_option = is_option_type(field);
        let with_path = field.with(FROM_ITEM);
        let is_required = !is_option && !is_default;

        let arm_body_start = out.buf.len();

        if let Some(with) = with_path {
            // Custom deserializer: fn(&Item) -> Result<T, Error>
            if is_required {
                splat!(out;
                    match [~with] :: from_item(__value) {
                        Ok(__val) => { [#: field.name] = Some(__val); }
                        Err(__err) => return Err(__ctx.push_error(
                            [~&ctx.crate_path] :: Error :: custom(__err, __value.span_unchecked())
                        )),
                    }
                );
            } else {
                splat!(out;
                    match [~with] :: from_item(__value) {
                        Ok(__val) => { [#: field.name] = Some(__val); }
                        Err(__err) => { __ctx.push_error(
                            [~&ctx.crate_path] :: Error :: custom(__err, __value.span_unchecked())
                        ); }
                    }
                );
            }
        } else if is_required {
            splat!(out;
                match [~&ctx.crate_path] :: FromItem :: from_item(__ctx, __value) {
                    Ok(__val) => { [#: field.name] = Some(__val); }
                    Err(__e) => return Err(__e),
                }
            );
        } else {
            // Optional/default: error already recorded by from_item, just skip
            splat!(out;
                match [~&ctx.crate_path] :: FromItem :: from_item(__ctx, __value) {
                    Ok(__val) => { [#: field.name] = Some(__val); }
                    Err(_) => {}
                }
            );
        }

        let arm_body = out.split_off_stream(arm_body_start);
        splat!(out;
            [@name_lit.into()] =>
                [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
        );
    }

    // Wildcard arm: report unknown key immediately
    splat!(out;
        _ => {
            return Err(__ctx.error_message_at(
                [@TokenTree::Literal(Literal::string("unexpected key"))], __key.span
            ));
        }
    );
    let match_arms = out.split_off_stream(match_arms_start);

    // Build for-loop body (the match statement)
    let for_body_start = out.buf.len();
    splat!(out;
        match __key.name [@TokenTree::Group(Group::new(Delimiter::Brace, match_arms))]
    );
    let for_body = out.split_off_stream(for_body_start);

    // Build for-loop pattern: (__key, __value)
    let for_pat = {
        let pat_stream = token_stream!(out; __key, __value);
        TokenTree::Group(Group::new(Delimiter::Parenthesis, pat_stream))
    };

    // Emit the iterate-and-match loop
    splat!(out;
        for [@for_pat] in __table
            [@TokenTree::Group(Group::new(Delimiter::Brace, for_body))]
    );

    // Unwrap required and default fields after the loop
    for field in fields {
        if field.flags & Field::WITH_FROMITEM_SKIP != 0 || is_option_type(field) {
            continue;
        }

        let is_default = field.flags & Field::WITH_FROMITEM_DEFAULT != 0;

        if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        splat!(out;
                            let [#: field.name] = [#: field.name].unwrap_or_else(|| [~tokens.as_slice()]);
                        );
                    }
                    DefaultKind::Default => {
                        splat!(out;
                            let [#: field.name] = [#: field.name].unwrap_or_default();
                        );
                    }
                }
            } else {
                splat!(out;
                    let [#: field.name] = [#: field.name].unwrap_or_default();
                );
            }
        } else {
            // Required: take + missing-field error
            let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
            let else_body_start = out.buf.len();
            splat!(out;
                return Err(__ctx.report_missing_field([@name_lit.into()], __item.span_unchecked()));
            );
            let else_body = out.split_off_stream(else_body_start);
            splat!(out;
                let Some([#: field.name]) = [#: field.name].take() else
                    [@TokenTree::Group(Group::new(Delimiter::Brace, else_body))]
                ;
            );
        }
    }

    splat!(out;
        Ok(Self {
            [for field in fields { splat!(out; [#: field.name],); }]
        })
    );
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}

fn impl_to_item(output: &mut RustWriter, ctx: &Ctx, inner: TokenStream) {
    let target = ctx.target;
    let any_generics = !target.generics.is_empty();
    let lf = Ident::new("__de", Span::mixed_site());
    splat! {
        output;
        ~[[automatically_derived]]
        impl [?(!target.generics.is_empty()) < [fmt_generics(output, &target.generics, DEF)] >]
         [~&ctx.crate_path]::ToItem for [#: &target.name][?(any_generics) <
            [fmt_generics(output, &target.generics, USE)]
        >]  [?(!target.where_clauses.is_empty() || !target.generic_field_types.is_empty())
             where [for (ty in &target.generic_field_types) {
                [~ty]: [~&ctx.crate_path]::ToItem,
            }] [~&target.where_clauses] ]
        {
            fn to_item<# [#lf]>(& # [#lf] self, __ctx: &mut [~&ctx.crate_path]::ToContext<# [#lf]>)
                -> ::std::result::Result<[~&ctx.crate_path]::Item<# [#lf]>, [~&ctx.crate_path]::Failed>
                [@TokenTree::Group(Group::new(Delimiter::Brace, inner))]
        }
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
    splat!(out;
        let Some(mut __table) = [~&ctx.crate_path]::Table::try_with_capacity(
            [@TokenTree::Literal(Literal::usize_unsuffixed(non_skip_count))],
            __ctx.arena
        ) else {
            return __ctx.report_error([@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
        };
    );
    for field in fields {
        if field.flags & Field::WITH_TO_ITEM_SKIP != 0 {
            continue;
        }
        let name_lit = field_name_literal_toml(ctx, field, ctx.target.rename_all);
        let with_path = field.with(TO_ITEM);
        // Check for skip_if condition (non-empty skip tokens on TO_ITEM)
        let skip_if = field.skip(TO_ITEM).filter(|tokens| !tokens.is_empty());

        // Check if this is an Option field (should use to_optional_item)
        let first_ty_ident = if let Some(TokenTree::Ident(ident)) = field.ty.first() {
            ident.to_string()
        } else {
            String::new()
        };

        // Emit the insert, possibly wrapped in a skip_if condition
        let emit_start = out.buf.len();
        if let Some(with) = with_path {
            splat!(out;
                __table.insert(
                    [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                    [~with]::to_item(&self.[#: field.name], __ctx)?,
                    __ctx.arena,
                );
            );
        } else if first_ty_ident == "Option" {
            splat!(out;
                if let Some(__val) = [~&ctx.crate_path]::ToItem::to_optional_item(&self.[#: field.name], __ctx)? {
                    __table.insert(
                        [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                        __val,
                        __ctx.arena,
                    );
                }
            );
        } else {
            splat!(out;
                __table.insert(
                    [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                    [~&ctx.crate_path]::ToItem::to_item(&self.[#: field.name], __ctx)?,
                    __ctx.arena,
                );
            );
        }

        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            splat!(out;
                if !([~skip_tokens])(&self.[#: field.name])
                    [@TokenTree::Group(Group::new(Delimiter::Brace, emit_body))]
            );
        }
    }
    splat!(out;
        Ok(__table.into_item())
    );
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}

fn handle_struct(output: &mut RustWriter, target: &DeriveTargetInner, fields: &[Field]) {
    let ctx = Ctx::new(output, target);

    if target.from_item {
        if target.transparent_impl {
            let [single_field] = fields else {
                throw!("Struct must contain a single field to use transparent")
            };
            let body = token_stream! {
                output;
                < [~single_field.ty] as [~&ctx.crate_path]::FromItem<#[#: &ctx.lifetime]> >::from_item(
                    __ctx, __item
                )
            };
            impl_from_item(output, &ctx, body);
        } else {
            struct_from_item(output, &ctx, fields);
        }
    }

    if target.to_item {
        if target.transparent_impl {
            let [single_field] = fields else {
                throw!("Struct must contain a single field to use transparent")
            };
            let body = token_stream! {output;
                [~&ctx.crate_path]::ToItem::to_item(&self.[#: single_field.name], __ctx)
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
            let body = token_stream! {
                output;
                Ok([#: &target.name](
                    < [~single_field.ty] as [~&ctx.crate_path]::FromItem<#[#: &ctx.lifetime]> >::from_item(
                        __ctx, __item
                    ) ?
                ))
            };
            impl_from_item(output, &ctx, body);
        } else {
            throw!("FromItem on tuple structs requires exactly one field (transparent delegation)")
        }
    }

    if target.to_item {
        if let [_single_field] = fields {
            let body = token_stream! {output;
                [~&ctx.crate_path]::ToItem::to_item(&self.[@TokenTree::Literal(Literal::usize_unsuffixed(0))], __ctx)
            };
            impl_to_item(output, &ctx, body);
        } else {
            throw!("ToItem on tuple structs requires exactly one field (transparent delegation)")
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
                        splat!(out; let [#: field.name] = [~tokens.as_slice()];);
                    }
                    DefaultKind::Default => {
                        splat!(out; let [#: field.name] = Default::default(););
                    }
                }
            } else {
                splat!(out; let [#: field.name] = Default::default(););
            }
        } else if let Some(with) = with_path {
            if is_option || is_default {
                if let Some(default_kind) = field.default(FROM_ITEM) {
                    match default_kind {
                        DefaultKind::Custom(tokens) => {
                            splat!(out;
                                let [#: field.name] = th.optional_mapped([@name_lit.into()], [~with]::from_item)
                                    .unwrap_or_else(|| [~tokens.as_slice()]);
                            );
                        }
                        DefaultKind::Default => {
                            splat!(out;
                                let [#: field.name] = th.optional_mapped([@name_lit.into()], [~with]::from_item)
                                    .unwrap_or_default();
                            );
                        }
                    }
                } else {
                    splat!(out;
                        let [#: field.name] = th.optional_mapped([@name_lit.into()], [~with]::from_item)
                            .unwrap_or_default();
                    );
                }
            } else {
                splat!(out;
                    let [#: field.name] = th.required_mapped([@name_lit.into()], [~with]::from_item)?;
                );
            }
        } else if is_option {
            splat!(out;
                let [#: field.name] = th.optional([@name_lit.into()]);
            );
        } else if is_default {
            if let Some(default_kind) = field.default(FROM_ITEM) {
                match default_kind {
                    DefaultKind::Custom(tokens) => {
                        splat!(out;
                            let [#: field.name] = th.optional([@name_lit.into()])
                                .unwrap_or_else(|| [~tokens.as_slice()]);
                        );
                    }
                    DefaultKind::Default => {
                        splat!(out;
                            let [#: field.name] = th.optional([@name_lit.into()])
                                .unwrap_or_default();
                        );
                    }
                }
            } else {
                splat!(out;
                    let [#: field.name] = th.optional([@name_lit.into()])
                        .unwrap_or_default();
                );
            }
        } else {
            splat!(out;
                let [#: field.name] = th.required([@name_lit.into()])?;
            );
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
            splat!(out;
                table.insert(
                    [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                    [~with]::to_item([#: field.name], __ctx)?,
                    __ctx.arena,
                );
            );
        } else if first_ty_ident == "Option" {
            splat!(out;
                if let Some(val) = [~&ctx.crate_path]::ToItem::to_optional_item([#: field.name], __ctx)? {
                    table.insert(
                        [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                        val,
                        __ctx.arena,
                    );
                }
            );
        } else {
            splat!(out;
                table.insert(
                    [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                    [~&ctx.crate_path]::ToItem::to_item([#: field.name], __ctx)?,
                    __ctx.arena,
                );
            );
        }

        if let Some(skip_tokens) = skip_if {
            let emit_body = out.split_off_stream(emit_start);
            splat!(out;
                if !([~skip_tokens])([#: field.name])
                    [@TokenTree::Group(Group::new(Delimiter::Brace, emit_body))]
            );
        }
    }
}

// ── String enum ──────────────────────────────────────────────────

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
    splat!(out;
        let s = __item.expect_string(__ctx)?;
        match s {
            [for variant in variants {
                let name_lit = variant_name_literal(ctx, variant);
                splat!(out; [@name_lit.into()] => Ok(Self::[#: variant.name]),);
            }]
            _ => Err(__ctx.error_expected_but_found(
                [@TokenTree::Literal(Literal::string(&expected_msg))], __item))
        }
    );
    let body = out.split_off_stream(start);
    impl_from_item(out, ctx, body);
}

fn enum_to_item_string(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let start = out.buf.len();
    splat!(out;
        Ok([~&ctx.crate_path]::Item::string(match self {
            [for variant in variants {
                let name_lit = variant_name_literal(ctx, variant);
                splat!(out; Self::[#: variant.name] => [@name_lit.into()],);
            }]
        }))
    );
    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}

// ── External tagging ─────────────────────────────────────────────

fn enum_from_item_external(out: &mut RustWriter, ctx: &Ctx, variants: &[EnumVariant]) {
    let has_unit = ctx.target.enum_flags & ENUM_CONTAINS_UNIT_VARIANT != 0;
    let has_complex =
        ctx.target.enum_flags & (ENUM_CONTAINS_STRUCT_VARIANT | ENUM_CONTAINS_TUPLE_VARIANT) != 0;

    let start = out.buf.len();

    // Unit variants: check as_str first
    if has_unit {
        let if_body_start = out.buf.len();
        // Build inner match for string check
        splat!(out;
            return match s {
                [for variant in variants {
                    if matches!(variant.kind, EnumKind::None) {
                        let name_lit = variant_name_literal(ctx, variant);
                        splat!(out; [@name_lit.into()] => Ok(Self::[#: variant.name]),);
                    }
                }]
                _ => Err(__ctx.error_expected_but_found(
                    [@TokenTree::Literal(Literal::string("a known variant"))], __item))
            };
        );
        let if_body = out.split_off_stream(if_body_start);
        splat!(out;
            if let Some(s) = __item.as_str()
                [@TokenTree::Group(Group::new(Delimiter::Brace, if_body))]
        );
    }

    // Complex variants: table with exactly 1 key
    if has_complex {
        splat!(out;
            let table = __item.expect_table(__ctx)?;
            let entries = table.entries();
        );
        let err_body_start = out.buf.len();
        splat!(out;
            return Err(__ctx.error_expected_but_found(
                [@TokenTree::Literal(Literal::string("a table with exactly one key"))], __item));
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
        );
        splat!(out;
            let (key, value) = &entries [@zero_index];
        );

        // Build match arms for complex variants
        let arms_start = out.buf.len();
        for variant in variants {
            if matches!(variant.kind, EnumKind::None) {
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
                            [~&ctx.crate_path]::FromItem::from_item(__ctx, value)?
                        )),
                    );
                }
                EnumKind::Struct => {
                    let arm_body_start = out.buf.len();
                    splat!(out; let mut th = value.table_helper(__ctx)?;);
                    emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                    splat!(out;
                        th.expect_empty()?;
                        Ok(Self::[#: variant.name] {
                            [for field in variant.fields { splat!(out; [#: field.name],); }]
                        })
                    );
                    let arm_body = out.split_off_stream(arm_body_start);
                    splat!(out;
                        [@name_lit.into()] =>
                            [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                    );
                }
                EnumKind::None => {}
            }
        }
        splat!(out;
            _ => Err(__ctx.error_expected_but_found(
                [@TokenTree::Literal(Literal::string("a known variant"))], __item)),
        );
        let arms = out.split_off_stream(arms_start);
        splat!(out;
            match key.name [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
        );
    } else if !has_unit {
        splat!(out;
            Err(__ctx.error_expected_but_found(
                [@TokenTree::Literal(Literal::string("a known variant"))], __item))
        );
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
                splat!(out;
                    Self::[#: variant.name] =>
                        Ok([~&ctx.crate_path]::Item::string([@name_lit.into()])),
                );
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    throw!("Only single-field tuple variants are supported in external tagging")
                }
                splat!(out;
                    Self::[#: variant.name](inner) => {
                        let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                            [@TokenTree::Literal(Literal::usize_unsuffixed(1))], __ctx.arena
                        ) else {
                            return __ctx.report_error(
                                [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                        };
                        table.insert(
                            [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                            [~&ctx.crate_path]::ToItem::to_item(inner, __ctx)?,
                            __ctx.arena,
                        );
                        Ok(table.into_item())
                    }
                );
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();

                // Build the arm body
                let arm_body_start = out.buf.len();
                splat!(out;
                    let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                        [@TokenTree::Literal(Literal::usize_unsuffixed(non_skip))], __ctx.arena
                    ) else {
                        return __ctx.report_error(
                            [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                    };
                );
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                splat!(out;
                    let Some(mut outer) = [~&ctx.crate_path]::Table::try_with_capacity(
                        [@TokenTree::Literal(Literal::usize_unsuffixed(1))], __ctx.arena
                    ) else {
                        return __ctx.report_error(
                            [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                    };
                    outer.insert(
                        [~&ctx.crate_path]::Key::anon([@name_lit.into()]),
                        table.into_item(),
                        __ctx.arena,
                    );
                    Ok(outer.into_item())
                );
                let arm_body = out.split_off_stream(arm_body_start);

                // Emit the pattern
                splat!(out;
                    Self::[#: variant.name] {
                        [for field in variant.fields { splat!(out; ref [#: field.name],); }]
                    } =>
                        [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                );
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    splat!(out;
        match self [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
    );

    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}

// ── Internal tagging ─────────────────────────────────────────────

fn enum_from_item_internal(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
) {
    if ctx.target.enum_flags & ENUM_CONTAINS_TUPLE_VARIANT != 0 {
        throw!("Tuple variants are not supported with internal tagging")
    }

    let start = out.buf.len();
    splat!(out;
        let mut th = __item.table_helper(__ctx)?;
        let tag: &str = th.required([@tag_lit.clone().into()])?;
    );

    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                splat!(out;
                    [@name_lit.into()] => {
                        th.expect_empty()?;
                        Ok(Self::[#: variant.name])
                    }
                );
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                splat!(out;
                    th.expect_empty()?;
                    Ok(Self::[#: variant.name] {
                        [for field in variant.fields { splat!(out; [#: field.name],); }]
                    })
                );
                let arm_body = out.split_off_stream(arm_body_start);
                splat!(out;
                    [@name_lit.into()] =>
                        [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                );
            }
            EnumKind::Tuple => {}
        }
    }
    splat!(out;
        _ => Err(__ctx.error_expected_but_found(
            [@TokenTree::Literal(Literal::string("a known variant"))], __item)),
    );
    let arms = out.split_off_stream(arms_start);
    splat!(out;
        match tag [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
    );

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
                splat!(out;
                    Self::[#: variant.name] => {
                        let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                            [@TokenTree::Literal(Literal::usize_unsuffixed(1))], __ctx.arena
                        ) else {
                            return __ctx.report_error(
                                [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                        };
                        table.insert(
                            [~&ctx.crate_path]::Key::anon([@tag_lit.clone().into()]),
                            [~&ctx.crate_path]::Item::string([@name_lit.into()]),
                            __ctx.arena,
                        );
                        Ok(table.into_item())
                    }
                );
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();

                let arm_body_start = out.buf.len();
                splat!(out;
                    let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                        [@TokenTree::Literal(Literal::usize_unsuffixed(non_skip + 1))], __ctx.arena
                    ) else {
                        return __ctx.report_error(
                            [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                    };
                    table.insert(
                        [~&ctx.crate_path]::Key::anon([@tag_lit.clone().into()]),
                        [~&ctx.crate_path]::Item::string([@name_lit.into()]),
                        __ctx.arena,
                    );
                );
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                splat!(out; Ok(table.into_item()));
                let arm_body = out.split_off_stream(arm_body_start);

                splat!(out;
                    Self::[#: variant.name] {
                        [for field in variant.fields { splat!(out; ref [#: field.name],); }]
                    } =>
                        [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                );
            }
            EnumKind::Tuple => {}
        }
    }
    let arms = out.split_off_stream(arms_start);
    splat!(out;
        match self [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
    );

    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}

// ── Adjacent tagging ─────────────────────────────────────────────

fn enum_from_item_adjacent(
    out: &mut RustWriter,
    ctx: &Ctx,
    variants: &[EnumVariant],
    tag_lit: &Literal,
    content_lit: &Literal,
) {
    let start = out.buf.len();
    splat!(out;
        let mut th = __item.table_helper(__ctx)?;
        let tag: &str = th.required([@tag_lit.clone().into()])?;
    );

    let arms_start = out.buf.len();
    for variant in variants {
        let name_lit = variant_name_literal(ctx, variant);
        match variant.kind {
            EnumKind::None => {
                splat!(out;
                    [@name_lit.into()] => {
                        th.expect_empty()?;
                        Ok(Self::[#: variant.name])
                    }
                );
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    throw!("Only single-field tuple variants are supported")
                }
                splat!(out;
                    [@name_lit.into()] => {
                        let content = th.required_item([@content_lit.clone().into()])?;
                        th.expect_empty()?;
                        Ok(Self::[#: variant.name](
                            [~&ctx.crate_path]::FromItem::from_item(__ctx, content)?
                        ))
                    }
                );
            }
            EnumKind::Struct => {
                let arm_body_start = out.buf.len();
                splat!(out;
                    let content = th.required_item([@content_lit.clone().into()])?;
                    let mut th = content.table_helper(__ctx)?;
                );
                emit_variant_fields_from_th(out, ctx, variant, variant.fields);
                splat!(out;
                    th.expect_empty()?;
                    Ok(Self::[#: variant.name] {
                        [for field in variant.fields { splat!(out; [#: field.name],); }]
                    })
                );
                let arm_body = out.split_off_stream(arm_body_start);
                splat!(out;
                    [@name_lit.into()] =>
                        [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                );
            }
        }
    }
    splat!(out;
        _ => Err(__ctx.error_expected_but_found(
            [@TokenTree::Literal(Literal::string("a known variant"))], __item)),
    );
    let arms = out.split_off_stream(arms_start);
    splat!(out;
        match tag [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
    );

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
                splat!(out;
                    Self::[#: variant.name] => {
                        let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                            [@TokenTree::Literal(Literal::usize_unsuffixed(1))], __ctx.arena
                        ) else {
                            return __ctx.report_error(
                                [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                        };
                        table.insert(
                            [~&ctx.crate_path]::Key::anon([@tag_lit.clone().into()]),
                            [~&ctx.crate_path]::Item::string([@name_lit.into()]),
                            __ctx.arena,
                        );
                        Ok(table.into_item())
                    }
                );
            }
            EnumKind::Tuple => {
                if variant.fields.len() != 1 {
                    throw!("Only single-field tuple variants are supported")
                }
                splat!(out;
                    Self::[#: variant.name](inner) => {
                        let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                            [@TokenTree::Literal(Literal::usize_unsuffixed(2))], __ctx.arena
                        ) else {
                            return __ctx.report_error(
                                [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                        };
                        table.insert(
                            [~&ctx.crate_path]::Key::anon([@tag_lit.clone().into()]),
                            [~&ctx.crate_path]::Item::string([@name_lit.into()]),
                            __ctx.arena,
                        );
                        table.insert(
                            [~&ctx.crate_path]::Key::anon([@content_lit.clone().into()]),
                            [~&ctx.crate_path]::ToItem::to_item(inner, __ctx)?,
                            __ctx.arena,
                        );
                        Ok(table.into_item())
                    }
                );
            }
            EnumKind::Struct => {
                let non_skip = variant
                    .fields
                    .iter()
                    .filter(|f| f.flags & Field::WITH_TO_ITEM_SKIP == 0)
                    .count();

                let arm_body_start = out.buf.len();
                splat!(out;
                    let Some(mut table) = [~&ctx.crate_path]::Table::try_with_capacity(
                        [@TokenTree::Literal(Literal::usize_unsuffixed(non_skip))], __ctx.arena
                    ) else {
                        return __ctx.report_error(
                            [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                    };
                );
                emit_variant_fields_to_table(out, ctx, variant, variant.fields);
                splat!(out;
                    let Some(mut outer) = [~&ctx.crate_path]::Table::try_with_capacity(
                        [@TokenTree::Literal(Literal::usize_unsuffixed(2))], __ctx.arena
                    ) else {
                        return __ctx.report_error(
                            [@TokenTree::Literal(Literal::string("Table capacity exceeded maximum"))]);
                    };
                    outer.insert(
                        [~&ctx.crate_path]::Key::anon([@tag_lit.clone().into()]),
                        [~&ctx.crate_path]::Item::string([@name_lit.into()]),
                        __ctx.arena,
                    );
                    outer.insert(
                        [~&ctx.crate_path]::Key::anon([@content_lit.clone().into()]),
                        table.into_item(),
                        __ctx.arena,
                    );
                    Ok(outer.into_item())
                );
                let arm_body = out.split_off_stream(arm_body_start);

                splat!(out;
                    Self::[#: variant.name] {
                        [for field in variant.fields { splat!(out; ref [#: field.name],); }]
                    } =>
                        [@TokenTree::Group(Group::new(Delimiter::Brace, arm_body))]
                );
            }
        }
    }
    let arms = out.split_off_stream(arms_start);
    splat!(out;
        match self [@TokenTree::Group(Group::new(Delimiter::Brace, arms))]
    );

    let body = out.split_off_stream(start);
    impl_to_item(out, ctx, body);
}

// ── Main dispatch ────────────────────────────────────────────────

fn handle_enum(output: &mut RustWriter, target: &DeriveTargetInner, variants: &[EnumVariant]) {
    if target.content.is_some() && target.tag.is_none() {
        throw!("content attribute requires tag to also be set")
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

    // Default to from_item when using #[derive(Toml)] with no trait specified
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
    token_stream!(
        (&mut rust_writer);
        ~[[allow(clippy::question_mark)]]
        const _: () = [@TokenTree::Group(Group::new(Delimiter::Brace, ts))];
    )
}

pub fn derive(stream: TokenStream) -> TokenStream {
    Error::try_catch_handle(stream, inner_derive)
}
