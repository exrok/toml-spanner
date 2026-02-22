use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use toml_spanner::{Context, Failed};

use super::*;

/// Convert a `toml_spanner::Item` into a `toml::Value`.
fn item_to_toml_value(item: &toml_spanner::Item<'_>) -> toml::Value {
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
        impl<'de> toml_spanner::Deserialize<'de> for $name {
            fn deserialize(
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

impl<'de> toml_spanner::Deserialize<'de> for StringOrVec {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(StringOrVec(vec![s.to_owned()])),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(StringOrVec(v))
            }
            _ => Err(ctx.error_expected_but_found("a string or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for StringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(StringOrBool::Bool(b)),
            toml_spanner::Value::String(&s) => Ok(StringOrBool::String(s.to_owned())),
            _ => Err(ctx.error_expected_but_found("a string or bool", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for VecStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(VecStringOrBool::Bool(b)),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(VecStringOrBool::VecString(v))
            }
            _ => Err(ctx.error_expected_but_found("a boolean or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for PathValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s: String = toml_spanner::Deserialize::deserialize(ctx, item)?;
        Ok(PathValue(s.into()))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlPackageBuild {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(TomlPackageBuild::Auto(b)),
            toml_spanner::Value::String(&s) => Ok(TomlPackageBuild::SingleScript(s.to_owned())),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(TomlPackageBuild::MultipleScript(v))
            }
            _ => Err(ctx.error_expected_but_found("a bool, string, or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlOptLevel {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
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

impl<'de> toml_spanner::Deserialize<'de> for TomlDebugInfo {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
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

impl<'de> toml_spanner::Deserialize<'de> for TomlTrimPathsValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        match s {
            "diagnostics" => Ok(TomlTrimPathsValue::Diagnostics),
            "macro" => Ok(TomlTrimPathsValue::Macro),
            "object" => Ok(TomlTrimPathsValue::Object),
            _ => Err(push_custom_error(
                ctx,
                item,
                format_args!("expected \"diagnostics\", \"macro\", or \"object\", found \"{s}\""),
            )),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlTrimPaths {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
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
                    let val =
                        <TomlTrimPathsValue as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                    Ok(val.into())
                }
            },
            toml_spanner::Value::Array(_) => {
                let v: Vec<TomlTrimPathsValue> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(v.into())
            }
            _ => {
                Err(ctx
                    .error_expected_but_found("a boolean, string, or array for trim-paths", item))
            }
        }
    }
}

toml_spanner::deserialize_table! {
    struct TomlProfile {
        optional "opt-level" opt_level: TomlOptLevel,
        optional lto: StringOrBool,
        optional "codegen-backend" codegen_backend: String,
        optional "codegen-units" codegen_units: u32,
        optional debug: TomlDebugInfo,
        optional "split-debuginfo" split_debuginfo: String,
        optional "debug-assertions" debug_assertions: bool,
        optional rpath: bool,
        optional panic: String,
        optional "overflow-checks" overflow_checks: bool,
        optional incremental: bool,
        optional "dir-name" dir_name: String,
        optional inherits: String,
        optional strip: StringOrBool,
        optional rustflags: Vec<String>,
        optional package: BTreeMap<ProfilePackageSpec, TomlProfile>,
        optional "build-override" build_override: Box<TomlProfile>,
        optional "trim-paths" trim_paths: TomlTrimPaths,
        optional "hint-mostly-unused" hint_mostly_unused: bool,
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlProfiles {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let map: BTreeMap<ProfileName, TomlProfile> =
            toml_spanner::Deserialize::deserialize(ctx, item)?;
        Ok(TomlProfiles(map))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLintLevel {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        match s {
            "forbid" => Ok(TomlLintLevel::Forbid),
            "deny" => Ok(TomlLintLevel::Deny),
            "warn" => Ok(TomlLintLevel::Warn),
            "allow" => Ok(TomlLintLevel::Allow),
            _ => Err(push_custom_error(
                ctx,
                item,
                format_args!(
                    "expected \"forbid\", \"deny\", \"warn\", or \"allow\", found \"{s}\""
                ),
            )),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLintConfig {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let level: TomlLintLevel = th.required("level")?;
        let priority: i8 = th.optional("priority").unwrap_or(0);
        // Remaining fields go into the config table
        let mut config = toml::Table::new();
        for (key, val) in th.into_remaining() {
            config.insert(key.name.to_owned(), item_to_toml_value(val));
        }
        Ok(TomlLintConfig {
            level,
            priority,
            config,
        })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLint {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(_) => {
                let level = <TomlLintLevel as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlLint::Level(level))
            }
            toml_spanner::Value::Table(_) => {
                let config = <TomlLintConfig as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlLint::Config(config))
            }
            _ => Err(ctx.error_expected_but_found("a lint level string or config table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableLints {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
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
                match <BTreeMap<String, TomlLint> as toml_spanner::Deserialize>::deserialize(
                    ctx, val,
                ) {
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

impl<'de> toml_spanner::Deserialize<'de> for WorkspaceValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.as_bool() {
            Some(true) => Ok(WorkspaceValue),
            Some(false) => Err(push_custom_error(ctx, item, "`workspace` cannot be false")),
            None => Err(ctx.error_expected_but_found("a boolean", item)),
        }
    }
}

toml_spanner::deserialize_table! {
    #[deny_unknown_fields]
    struct TomlInheritedField {
        required workspace: WorkspaceValue,
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

impl<'de> toml_spanner::Deserialize<'de> for InheritableSemverVersion {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.trim().parse::<semver::Version>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableString {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(InheritableField::Value(s.to_owned())),
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableRustVersion {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.parse::<RustVersion>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableVecString {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(InheritableField::Value(v))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("an array of strings or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(InheritableField::Value(StringOrBool::Bool(b))),
            toml_spanner::Value::String(&s) => {
                Ok(InheritableField::Value(StringOrBool::String(s.to_owned())))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string, bool, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableVecStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => {
                Ok(InheritableField::Value(VecStringOrBool::Bool(b)))
            }
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(InheritableField::Value(VecStringOrBool::VecString(v)))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx
                .error_expected_but_found("a boolean, array of strings, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableBtreeMap {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        if is_workspace_inherit(item) {
            let field = <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
            Ok(InheritableField::Inherit(field))
        } else {
            let map: BTreeMap<String, BTreeMap<String, String>> =
                toml_spanner::Deserialize::deserialize(ctx, item)?;
            Ok(InheritableField::Value(map))
        }
    }
}

toml_spanner::deserialize_table! {
    struct TomlDetailedDependency {
        optional version: String,
        optional registry: RegistryName,
        optional "registry-index" registry_index: String,
        optional path: String,
        optional base: PathBaseName,
        optional git: String,
        optional branch: String,
        optional tag: String,
        optional rev: String,
        optional features: Vec<String>,
        optional optional: bool,
        optional "default-features" default_features: bool,
        optional "default_features" default_features2: bool,
        optional package: PackageName,
        optional public: bool,
        optional artifact: StringOrVec,
        optional lib: bool,
        optional target: String,
    }
    flatten _unused_keys: BTreeMap<String, toml::Value> = |key, value| {
        _unused_keys.insert(key.name.to_owned(), item_to_toml_value(value));
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
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
                    <TomlDetailedDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlDependency::Detailed(detailed))
            }
            _ => Err(ctx.error_expected_but_found("a version string or dependency table", item)),
        }
    }
}

toml_spanner::deserialize_table! {
    struct TomlInheritedDependency {
        required workspace: bool,
        optional features: Vec<String>,
        optional "default-features" default_features: bool,
        optional "default_features" default_features2: bool,
        optional optional: bool,
        optional public: bool,
    }
    flatten _unused_keys: BTreeMap<String, toml::Value> = |key, value| {
        _unused_keys.insert(key.name.to_owned(), item_to_toml_value(value));
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        // If it's a table with `workspace` key, try to parse as inherited
        if is_workspace_inherit(item) {
            let dep =
                <TomlInheritedDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
            if dep.workspace {
                return Ok(InheritableDependency::Inherit(dep));
            } else {
                return Err(push_custom_error(ctx, item, "`workspace` cannot be false"));
            }
        }
        // Check for workspace = false case
        if let Some(table) = item.as_table() {
            if let Some(ws) = table.get("workspace") {
                if ws.as_bool() == Some(false) {
                    return Err(push_custom_error(ctx, item, "`workspace` cannot be false"));
                }
            }
        }
        let dep = <TomlDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
        Ok(InheritableDependency::Value(dep))
    }
}

toml_spanner::deserialize_table! {
    #[deny_unknown_fields]
    struct TomlTarget {
        optional name: String,
        optional "crate-type" crate_type: Vec<String>,
        optional "crate_type" crate_type2: Vec<String>,
        optional path: PathValue,
        optional filename: String,
        optional test: bool,
        optional doctest: bool,
        optional bench: bool,
        optional doc: bool,
        optional "doc-scrape-examples" doc_scrape_examples: bool,
        optional "proc-macro" proc_macro: bool,
        optional "proc_macro" proc_macro2: bool,
        optional harness: bool,
        optional "required-features" required_features: Vec<String>,
        optional edition: String,
    }
}

toml_spanner::deserialize_table! {
    #[deny_unknown_fields]
    struct TomlPlatform {
        optional dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "build-dependencies" build_dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "build_dependencies" build_dependencies2: BTreeMap<PackageName, InheritableDependency>,
        optional "dev-dependencies" dev_dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "dev_dependencies" dev_dependencies2: BTreeMap<PackageName, InheritableDependency>,
    }
}

toml_spanner::deserialize_table! {
    #[deny_unknown_fields]
    struct Hints {
        optional "mostly-unused" mostly_unused: toml::Value = |item| {
            Ok(item_to_toml_value(item))
        },
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InvalidCargoFeatures {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        Err(push_custom_error(
            ctx,
            item,
            "the field `cargo-features` should be set at the top of Cargo.toml before any tables",
        ))
    }
}

toml_spanner::deserialize_table! {
    struct TomlPackage {
        optional edition: InheritableString,
        optional "rust-version" rust_version: InheritableRustVersion,
        optional name: PackageName,
        optional version: InheritableSemverVersion,
        optional authors: InheritableVecString,
        optional build: TomlPackageBuild,
        optional metabuild: StringOrVec,
        optional "default-target" default_target: String,
        optional "forced-target" forced_target: String,
        optional links: String,
        optional exclude: InheritableVecString,
        optional include: InheritableVecString,
        optional publish: InheritableVecStringOrBool,
        optional workspace: String,
        optional "im-a-teapot" im_a_teapot: bool,
        optional autolib: bool,
        optional autobins: bool,
        optional autoexamples: bool,
        optional autotests: bool,
        optional autobenches: bool,
        optional "default-run" default_run: String,
        optional description: InheritableString,
        optional homepage: InheritableString,
        optional documentation: InheritableString,
        optional readme: InheritableStringOrBool,
        optional keywords: InheritableVecString,
        optional categories: InheritableVecString,
        optional license: InheritableString,
        optional "license-file" license_file: InheritableString,
        optional repository: InheritableString,
        optional resolver: String,
        optional "cargo-features" _invalid_cargo_features: InvalidCargoFeatures,
        optional metadata: toml::Value = |item| {
            Ok(item_to_toml_value(item))
        },
    }
}

toml_spanner::deserialize_table! {
    struct InheritablePackage {
        optional authors: Vec<String>,
        optional description: String,
        optional homepage: String,
        optional documentation: String,
        optional readme: StringOrBool,
        optional keywords: Vec<String>,
        optional categories: Vec<String>,
        optional license: String,
        optional "license-file" license_file: String,
        optional repository: String,
        optional publish: VecStringOrBool,
        optional edition: String,
        optional badges: BTreeMap<String, BTreeMap<String, String>>,
        optional exclude: Vec<String>,
        optional include: Vec<String>,
        optional "version" version: semver::Version = |item| {
            let s = item.as_str().ok_or_else(|| item.expected("a version string"))?;
            s.trim().parse::<semver::Version>()
                .map_err(|err| toml_spanner::Error::custom(err, item.span()))
        },
        optional "rust-version" rust_version: RustVersion = |item| {
            let s = item.as_str().ok_or_else(|| item.expected("a rust version string"))?;
            s.parse::<RustVersion>()
                .map_err(|err| toml_spanner::Error::custom(err, item.span()))
        },
    }
}

toml_spanner::deserialize_table! {
    struct TomlWorkspace {
        optional members: Vec<String>,
        optional exclude: Vec<String>,
        optional "default-members" default_members: Vec<String>,
        optional resolver: String,
        optional package: InheritablePackage,
        optional dependencies: BTreeMap<PackageName, TomlDependency>,
        optional lints: TomlLints,
        optional metadata: toml::Value = |item| {
            Ok(item_to_toml_value(item))
        },
    }
}

toml_spanner::deserialize_table! {
    struct TomlManifest {
        optional "cargo-features" cargo_features: Vec<String>,
        optional package: Box<TomlPackage>,
        optional project: Box<TomlPackage>,
        optional badges: BTreeMap<String, BTreeMap<String, String>>,
        optional features: BTreeMap<FeatureName, Vec<String>>,
        optional lib: TomlLibTarget,
        optional bin: Vec<TomlBinTarget>,
        optional example: Vec<TomlExampleTarget>,
        optional test: Vec<TomlTestTarget>,
        optional bench: Vec<TomlTestTarget>,
        optional dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "dev-dependencies" dev_dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "dev_dependencies" dev_dependencies2: BTreeMap<PackageName, InheritableDependency>,
        optional "build-dependencies" build_dependencies: BTreeMap<PackageName, InheritableDependency>,
        optional "build_dependencies" build_dependencies2: BTreeMap<PackageName, InheritableDependency>,
        optional target: BTreeMap<String, TomlPlatform>,
        optional lints: InheritableLints,
        optional hints: Hints,
        optional workspace: TomlWorkspace,
        optional profile: TomlProfiles,
        optional patch: BTreeMap<String, BTreeMap<PackageName, TomlDependency>>,
        optional replace: BTreeMap<String, TomlDependency>,
    }
    flatten _unused_keys: BTreeSet<String> = |key, _value| {
        _unused_keys.insert(key.name.to_owned());
    }
}
