#![allow(dead_code)]

use snapshot_tests::{invalid_de, valid_de};
use toml_spanner::{Context, Deserialize, Failed, Item, Spanned, TableHelper};

#[derive(Debug)]
struct Boop {
    s: String,
    os: Option<u32>,
}

impl<'de> Deserialize<'de> for Boop {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let s = th.required("s")?;
        let os = th.optional("os");
        th.expect_empty()?;
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

impl<'de> Deserialize<'de> for Package {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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
            Err(ctx.error_expected_but_found("a string or table", value))
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

impl<'de> Deserialize<'de> for Array {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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

impl<'de, T> Deserialize<'de> for PackageSpecOrExtended<T>
where
    T: DeserializeTable<'de>,
{
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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

impl<'de> Deserialize<'de> for Reason {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let reason = th.required("reason")?;
        th.expect_empty()?;
        Ok(Self { reason })
    }
}

#[derive(Debug)]
struct Flattened {
    flattened: Vec<PackageSpecOrExtended<Reason>>,
}

impl<'de> Deserialize<'de> for Flattened {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
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

impl<'de> Deserialize<'de> for Ohno {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let year = th.required("year")?;

        if let Some(snbh) = th.optional::<Spanned<std::borrow::Cow<'de, str>>>("this-is-deprecated")
        {
            return Err(th.ctx.push_error(toml_spanner::Error::from((
                toml_spanner::ErrorKind::Custom("this-is-deprecated is deprecated".into()),
                snbh.span,
            ))));
        }

        th.expect_empty()?;
        Ok(Self { year })
    }
}

invalid_de!(
    custom_error,
    Ohno,
    "year = 40_000\nthis-is-deprecated = 'this should not be here'"
);
