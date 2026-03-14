use std::collections::BTreeMap;
use std::fmt;

use toml_spanner::{Context, Failed};

use super::*;

/// Convert a `toml_spanner::Item` into a `toml::Value`.
pub(crate) fn item_to_toml_value(item: &toml_spanner::Item<'_>) -> toml::Value {
    match item.value() {
        toml_spanner::Value::String(&s) => toml::Value::String(s.to_owned()),
        toml_spanner::Value::Integer(&i) => toml::Value::Integer(i),
        toml_spanner::Value::Float(&f) => toml::Value::Float(f),
        toml_spanner::Value::Boolean(&b) => toml::Value::Boolean(b),
        toml_spanner::Value::Array(arr) => {
            toml::Value::Array(arr.iter().map(item_to_toml_value).collect())
        }
        toml_spanner::Value::Table(table) => {
            let mut map = toml::map::Map::new();
            for (key, val) in table {
                map.insert(key.name.to_owned(), item_to_toml_value(val));
            }
            toml::Value::Table(map)
        }
        toml_spanner::Value::DateTime(dt) => {
            let mut buf = std::mem::MaybeUninit::uninit();
            let s = dt.format(&mut buf);
            toml::Value::String(s.to_owned())
        }
    }
}

/// Helper to push a custom error from a Display-able error value.
fn push_custom_error(
    ctx: &mut Context<'_>,
    item: &toml_spanner::Item<'_>,
    err: impl fmt::Display,
) -> Failed {
    ctx.push_error(toml_spanner::Error {
        kind: toml_spanner::ErrorKind::Custom(err.to_string().into()),
        span: item.span(),
    })
}

macro_rules! impl_spanner_deserialize_str_newtype {
    ($name:ident) => {
        impl<'de> toml_spanner::FromToml<'de> for $name {
            fn from_toml(
                ctx: &mut Context<'de>,
                item: &toml_spanner::Item<'de>,
            ) -> Result<Self, Failed> {
                let s = item.expect_string(ctx)?;
                $name::new(s.to_owned()).map_err(|err| push_custom_error(ctx, item, err))
            }
        }
    };
}

impl_spanner_deserialize_str_newtype!(PackageName);
impl_spanner_deserialize_str_newtype!(RegistryName);
impl_spanner_deserialize_str_newtype!(ProfileName);
impl_spanner_deserialize_str_newtype!(FeatureName);
impl_spanner_deserialize_str_newtype!(PathBaseName);

impl<'de> toml_spanner::FromToml<'de> for ProfilePackageSpec {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        s.parse().map_err(|err| push_custom_error(ctx, item, err))
    }
}

impl<'de> toml_spanner::FromToml<'de> for StringOrVec {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(StringOrVec(vec![s.to_owned()])),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::FromToml::from_toml(ctx, item)?;
                Ok(StringOrVec(v))
            }
            _ => Err(ctx.error_expected_but_found("a string or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlOptLevel {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Integer(&i) => Ok(TomlOptLevel(i.to_string())),
            toml_spanner::Value::String(&s) => {
                if s == "s" || s == "z" {
                    Ok(TomlOptLevel(s.to_owned()))
                } else {
                    Err(push_custom_error(
                        ctx,
                        item,
                        format_args!(
                            "must be `0`, `1`, `2`, `3`, `s` or `z`, but found the string: \"{s}\""
                        ),
                    ))
                }
            }
            _ => {
                Err(ctx.error_expected_but_found("an optimization level (integer or string)", item))
            }
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlDebugInfo {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(if b {
                TomlDebugInfo::Full
            } else {
                TomlDebugInfo::None
            }),
            toml_spanner::Value::Integer(&i) => match i {
                0 => Ok(TomlDebugInfo::None),
                1 => Ok(TomlDebugInfo::Limited),
                2 => Ok(TomlDebugInfo::Full),
                _ => Err(push_custom_error(
                    ctx,
                    item,
                    "expected a boolean, 0, 1, 2, \"none\", \"limited\", \"full\", \"line-tables-only\", or \"line-directives-only\"",
                )),
            },
            toml_spanner::Value::String(&s) => match s {
                "none" => Ok(TomlDebugInfo::None),
                "limited" => Ok(TomlDebugInfo::Limited),
                "full" => Ok(TomlDebugInfo::Full),
                "line-directives-only" => Ok(TomlDebugInfo::LineDirectivesOnly),
                "line-tables-only" => Ok(TomlDebugInfo::LineTablesOnly),
                _ => Err(push_custom_error(
                    ctx,
                    item,
                    format_args!(
                        "expected a boolean, 0, 1, 2, \"none\", \"limited\", \"full\", \"line-tables-only\", or \"line-directives-only\", found \"{s}\""
                    ),
                )),
            },
            _ => {
                Err(ctx
                    .error_expected_but_found("a boolean, integer, or string for debug info", item))
            }
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlTrimPaths {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(if b {
                TomlTrimPaths::All
            } else {
                TomlTrimPaths::none()
            }),
            toml_spanner::Value::String(&s) => match s {
                "none" => Ok(TomlTrimPaths::none()),
                "all" => Ok(TomlTrimPaths::All),
                _ => {
                    let val = <TomlTrimPathsValue as toml_spanner::FromToml>::from_toml(ctx, item)?;
                    Ok(val.into())
                }
            },
            toml_spanner::Value::Array(_) => {
                let v: Vec<TomlTrimPathsValue> = toml_spanner::FromToml::from_toml(ctx, item)?;
                Ok(v.into())
            }
            _ => {
                Err(ctx
                    .error_expected_but_found("a boolean, string, or array for trim-paths", item))
            }
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableLints {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let table = item.expect_table(ctx)?;
        let mut workspace = false;
        let mut lints = TomlLints::new();
        for (key, val) in table {
            if key.name == "workspace" {
                match val.as_bool() {
                    Some(true) => workspace = true,
                    Some(false) => workspace = false,
                    None => {
                        ctx.error_expected_but_found("a boolean for workspace", val);
                    }
                }
            } else {
                match <BTreeMap<String, TomlLint> as toml_spanner::FromToml>::from_toml(ctx, val) {
                    Ok(tool_lints) => {
                        lints.insert(key.name.to_owned(), tool_lints);
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(InheritableLints { workspace, lints })
    }
}

impl<'de> toml_spanner::FromToml<'de> for WorkspaceValue {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.as_bool() {
            Some(true) => Ok(WorkspaceValue),
            Some(false) => Err(push_custom_error(ctx, item, "`workspace` cannot be false")),
            None => Err(ctx.error_expected_but_found("a boolean", item)),
        }
    }
}

/// Check if a table item has a `workspace = true` key.
fn is_workspace_inherit(item: &toml_spanner::Item<'_>) -> bool {
    if let Some(table) = item.as_table() {
        if let Some(ws) = table.get("workspace") {
            return ws.as_bool() == Some(true);
        }
    }
    false
}

impl<'de> toml_spanner::FromToml<'de> for InheritableSemverVersion {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.trim().parse::<semver::Version>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableString {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(InheritableField::Value(s.to_owned())),
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableRustVersion {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.parse::<RustVersion>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableVecString {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::FromToml::from_toml(ctx, item)?;
                Ok(InheritableField::Value(v))
            }
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("an array of strings or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableStringOrBool {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(InheritableField::Value(StringOrBool::Bool(b))),
            toml_spanner::Value::String(&s) => {
                Ok(InheritableField::Value(StringOrBool::String(s.to_owned())))
            }
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string, bool, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableVecStringOrBool {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => {
                Ok(InheritableField::Value(VecStringOrBool::Bool(b)))
            }
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::FromToml::from_toml(ctx, item)?;
                Ok(InheritableField::Value(VecStringOrBool::VecString(v)))
            }
            toml_spanner::Value::Table(_) => {
                let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx
                .error_expected_but_found("a boolean, array of strings, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InheritableBtreeMap {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        if is_workspace_inherit(item) {
            let field = <TomlInheritedField as toml_spanner::FromToml>::from_toml(ctx, item)?;
            Ok(InheritableField::Inherit(field))
        } else {
            let map: BTreeMap<String, BTreeMap<String, String>> =
                toml_spanner::FromToml::from_toml(ctx, item)?;
            Ok(InheritableField::Value(map))
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlDependency {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(TomlDependency::Simple(s.to_owned())),
            toml_spanner::Value::Boolean(&b) => {
                let msg = if b {
                    format!(
                        "invalid type: boolean `true`, expected a version string like \"0.9.8\" or a \
                         detailed dependency like {{ version = \"0.9.8\" }}\n\
                         note: if you meant to use a workspace member, you can write\n \
                         dep.workspace = true"
                    )
                } else {
                    format!(
                        "invalid type: boolean `false`, expected a version string like \"0.9.8\" or a \
                         detailed dependency like {{ version = \"0.9.8\" }}"
                    )
                };
                Err(push_custom_error(ctx, item, msg))
            }
            toml_spanner::Value::Table(_) => {
                let detailed =
                    <TomlDetailedDependency as toml_spanner::FromToml>::from_toml(ctx, item)?;
                Ok(TomlDependency::Detailed(detailed))
            }
            _ => Err(ctx.error_expected_but_found("a version string or dependency table", item)),
        }
    }
}

impl<'de> toml_spanner::FromToml<'de> for InvalidCargoFeatures {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        Err(push_custom_error(
            ctx,
            item,
            "the field `cargo-features` should be set at the top of Cargo.toml before any tables",
        ))
    }
}
