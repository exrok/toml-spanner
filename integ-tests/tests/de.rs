#![allow(dead_code)]

use integ_tests::{invalid_de, valid_de};
use toml_spanner::{Deserialize, Error, Item, Spanned};

#[derive(Debug)]
struct Boop {
    s: String,
    os: Option<u32>,
}

impl<'de> Deserialize<'de> for Boop {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
            return Err(value.expected("a table").into());
        };

        let s = table.required("s")?;
        let os = table.optional("os")?;

        table.expect_empty()?;

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

impl<'de> Deserialize<'de> for Package {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        fn from_str(s: &str) -> (String, Option<String>) {
            if let Some((name, version)) = s.split_once(':') {
                (name.to_owned(), Some(version.to_owned()))
            } else {
                (s.to_owned(), None)
            }
        }

        if value.as_str().is_some() {
            let s = value.take_string(None)?;
            let (name, version) = from_str(&s);
            Ok(Self { name, version })
        } else if value.as_table().is_some() {
            let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
                unreachable!()
            };

            if let Some(mut val) = table.remove("crate") {
                let s = val.take_string(Some("a package string"))?;
                let (name, version) = from_str(&s);

                Ok(Self { name, version })
            } else {
                let name = table.required_s("name")?;
                let version = table.optional("version")?;

                Ok(Self {
                    name: name.value,
                    version,
                })
            }
        } else {
            Err(value.expected("a string or table").into())
        }
    }
}

#[derive(Debug)]
struct Array {
    packages: Vec<Package>,
}

impl<'de> Deserialize<'de> for Array {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
            return Err(value.expected("a table").into());
        };
        let packages = table.required("packages")?;
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
    T: Deserialize<'de>,
{
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let spec = Package::deserialize(value)?;

        let inner = if value.has_keys() {
            Some(T::deserialize(value)?)
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

impl<'de> Deserialize<'de> for Reason {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
            return Err(value.expected("a table").into());
        };
        let reason = table.required("reason")?;
        table.expect_empty()?;
        Ok(Self { reason })
    }
}

#[derive(Debug)]
struct Flattened {
    flattened: Vec<PackageSpecOrExtended<Reason>>,
}

impl<'de> Deserialize<'de> for Flattened {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
            return Err(value.expected("a table").into());
        };
        let flattened = table.required("flattened")?;
        Ok(Self { flattened })
    }
}

valid_de!(flattened, Flattened);

#[derive(Debug)]
struct Ohno {
    year: u8,
}

impl<'de> Deserialize<'de> for Ohno {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let toml_spanner::ValueMut::Table(table) = value.as_mut() else {
            return Err(value.expected("a table").into());
        };
        let year = table.required("year")?;

        if let Some(snbh) = table.optional_s::<std::borrow::Cow<'de, str>>("this-is-deprecated")? {
            return Err(toml_spanner::Error::from((
                toml_spanner::ErrorKind::Custom("this-is-deprecated is deprecated".into()),
                snbh.span,
            )));
        }

        table.expect_empty()?;
        Ok(Self { year })
    }
}

invalid_de!(
    custom_error,
    Ohno,
    "year = 40_000\nthis-is-deprecated = 'this should not be here'"
);
