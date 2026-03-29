#![allow(dead_code)]

use snapshot_tests::{invalid_de, valid_de, warnings_de};
use toml_spanner::{Context, Failed, FromToml, Spanned, TableHelper, Toml};

#[derive(Debug)]
struct Boop {
    s: String,
    os: Option<u32>,
}

impl<'de> FromToml<'de> for Boop {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let s = th.required("s")?;
        let os = th.optional("os");
        th.require_empty()?;
        Ok(Self { s, os })
    }
}

valid_de!(basic_table, Boop, "s = 'boop string'\nos = 20");
invalid_de!(missing_required, Boop, "os = 20");
invalid_de!(
    unknown_field,
    Boop,
    "s = 'val'\nthis-field-is-not-known = 20"
);

#[derive(Debug)]
struct Package {
    name: String,
    version: Option<String>,
}

impl Package {
    fn from_str(s: &str) -> (String, Option<String>) {
        if let Some((name, version)) = s.split_once(':') {
            (name.to_owned(), Some(version.to_owned()))
        } else {
            (s.to_owned(), None)
        }
    }
}

impl<'de> FromToml<'de> for Package {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        if let Some(s) = value.as_str() {
            let (name, version) = Self::from_str(s);
            Ok(Self { name, version })
        } else if value.as_table().is_some() {
            let mut th = value.table_helper(ctx)?;
            if let Some(crate_str) = th.optional::<String>("crate") {
                let (name, version) = Self::from_str(&crate_str);
                Ok(Self { name, version })
            } else {
                let name = th.required("name")?;
                let version = th.optional("version");
                Ok(Self { name, version })
            }
        } else {
            Err(ctx.report_expected_but_found(&"a string or table", value))
        }
    }
}

/// Trait for types that can be deserialized from a shared [`TableHelper`],
/// allowing multiple types to extract fields from the same table (flattening).
trait DeserializeTable<'de>: Sized {
    fn deserialize_table(th: &mut TableHelper<'_, '_, 'de>) -> Result<Self, Failed>;
}

impl<'de> DeserializeTable<'de> for Package {
    fn deserialize_table(th: &mut TableHelper<'_, '_, 'de>) -> Result<Self, Failed> {
        if let Some(crate_str) = th.optional::<String>("crate") {
            let (name, version) = Self::from_str(&crate_str);
            Ok(Self { name, version })
        } else {
            let name = th.required("name")?;
            let version = th.optional("version");
            Ok(Self { name, version })
        }
    }
}

#[derive(Debug)]
struct Array {
    packages: Vec<Package>,
}

impl<'de> FromToml<'de> for Array {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let packages = th.required("packages")?;
        Ok(Self { packages })
    }
}

valid_de!(basic_arrays, Array);

#[derive(Debug)]
enum UntaggedPackage {
    Simple {
        spec: Package,
    },
    Split {
        name: Spanned<String>,
        version: Option<String>,
    },
}

#[derive(Debug)]
pub struct PackageSpecOrExtended<T> {
    spec: Package,
    inner: Option<T>,
}

impl<'de, T> FromToml<'de> for PackageSpecOrExtended<T>
where
    T: DeserializeTable<'de>,
{
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        if let Some(s) = value.as_str() {
            let (name, version) = Package::from_str(s);
            return Ok(Self {
                spec: Package { name, version },
                inner: None,
            });
        }
        let mut th = value.table_helper(ctx)?;
        let spec = Package::deserialize_table(&mut th)?;
        let inner = if th.remaining_count() > 0 {
            Some(T::deserialize_table(&mut th)?)
        } else {
            None
        };
        Ok(Self { spec, inner })
    }
}

#[derive(Debug)]
struct Reason {
    reason: String,
}

impl<'de> DeserializeTable<'de> for Reason {
    fn deserialize_table(th: &mut TableHelper<'_, '_, 'de>) -> Result<Self, Failed> {
        let reason = th.required("reason")?;
        Ok(Self { reason })
    }
}

impl<'de> FromToml<'de> for Reason {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let reason = th.required("reason")?;
        th.require_empty()?;
        Ok(Self { reason })
    }
}

#[derive(Debug)]
struct Flattened {
    flattened: Vec<PackageSpecOrExtended<Reason>>,
}

impl<'de> FromToml<'de> for Flattened {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let flattened = th.required("flattened")?;
        Ok(Self { flattened })
    }
}

valid_de!(flattened, Flattened);

#[derive(Debug)]
struct Ohno {
    year: u8,
}

impl<'de> FromToml<'de> for Ohno {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let year = th.required("year")?;

        if let Some(snbh) = th.optional::<Spanned<std::borrow::Cow<'de, str>>>("this-is-deprecated")
        {
            return Err(th.ctx.push_error(toml_spanner::Error::custom(
                "this-is-deprecated is deprecated",
                snbh.span,
            )));
        }

        th.require_empty()?;
        Ok(Self { year })
    }
}

invalid_de!(
    custom_error,
    Ohno,
    "year = 40_000\nthis-is-deprecated = 'this should not be here'"
);

#[derive(Debug)]
struct ServerConfig {
    timeout: u32,
}

impl<'de> FromToml<'de> for ServerConfig {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let timeout = th.required("timeout")?;
        th.require_empty()?;
        Ok(Self { timeout })
    }
}

#[derive(Debug)]
struct Server {
    host: String,
    config: ServerConfig,
}

impl<'de> FromToml<'de> for Server {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let host = th.required("host")?;
        let config = th.required("config")?;
        th.require_empty()?;
        Ok(Self { host, config })
    }
}

#[derive(Debug)]
struct Component {
    servers: Vec<Server>,
}

impl<'de> FromToml<'de> for Component {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let servers = th.required("servers")?;
        th.require_empty()?;
        Ok(Self { servers })
    }
}

#[derive(Debug)]
struct Deployment {
    component: Component,
}

impl<'de> FromToml<'de> for Deployment {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let component = th.required("component")?;
        th.require_empty()?;
        Ok(Self { component })
    }
}

invalid_de!(
    nested_unexpected_key,
    Deployment,
    "\
[component]
[[component.servers]]
host = 'alpha'
[component.servers.config]
timeout = 30

[[component.servers]]
host = 'beta'
[component.servers.config]
timeout = 60
bogus = true"
);

// --- DuplicateField: alias collision ---

#[derive(Toml, Debug)]
struct WithAlias {
    #[toml(alias = "server_name")]
    name: String,
}

invalid_de!(
    duplicate_field_alias,
    WithAlias,
    "name = 'a'\nserver_name = 'b'"
);

// --- Deprecated field ---

#[derive(Toml, Debug)]
struct WithDeprecated {
    #[toml(deprecated_alias = "old_name")]
    new_name: String,
}

warnings_de!(deprecated_alias, WithDeprecated, "old_name = 'val'");

// --- UnexpectedVariant: string enum with unknown variant ---

#[derive(Toml, Debug)]
#[toml(FromToml)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug)]
struct Palette {
    color: Color,
}

impl<'de> FromToml<'de> for Palette {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let color = th.required("color")?;
        th.require_empty()?;
        Ok(Self { color })
    }
}

invalid_de!(unexpected_variant, Palette, "color = 'Purple'");

// --- Custom error: wrong array size ---

#[derive(Debug)]
struct Pair {
    items: [String; 2],
}

impl<'de> FromToml<'de> for Pair {
    fn from_toml(ctx: &mut Context<'de>, value: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let items = th.required("items")?;
        th.require_empty()?;
        Ok(Self { items })
    }
}

invalid_de!(custom_wrong_array_size, Pair, "items = ['a', 'b', 'c']");
