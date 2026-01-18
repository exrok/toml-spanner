#![allow(dead_code)]

use integ_tests::{invalid_de, valid_de};
use toml_spanner::{
    DeserError, Deserialize,
    de_helpers::*,
    span::Spanned,
    value::{Value, ValueInner},
};

#[derive(Debug)]
struct Boop {
    s: String,
    os: Option<u32>,
}

impl<'de> Deserialize<'de> for Boop {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut mh = TableHelper::new(value)?;

        let s = mh.required("s")?;
        let os = mh.optional("os");

        mh.finalize(None)?;

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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        fn from_str(s: std::borrow::Cow<'_, str>) -> (String, Option<String>) {
            if let Some((name, version)) = s.split_once(':') {
                (name.to_owned(), Some(version.to_owned()))
            } else {
                (s.into(), None)
            }
        }

        match value.take() {
            ValueInner::String(s) => {
                let (name, version) = from_str(s);

                Ok(Self { name, version })
            }
            ValueInner::Table(tab) => {
                let mut th = TableHelper::from((tab, value.span));

                if let Some(mut val) = th.table.remove("crate") {
                    let (name, version) = match val.take() {
                        ValueInner::String(s) => from_str(s),
                        found => {
                            th.errors
                                .push(expected("a package string", found, val.span));
                            th.finalize(Some(value))?;
                            unreachable!();
                        }
                    };

                    th.finalize(Some(value))?;

                    Ok(Self { name, version })
                } else {
                    let name = th.required_s("name")?;
                    let version = th.optional("version");

                    th.finalize(Some(value))?;

                    Ok(Self {
                        name: name.value,
                        version,
                    })
                }
            }
            other => Err(expected("a string or table", other, value.span).into()),
        }
    }
}

#[derive(Debug)]
struct Array {
    packages: Vec<Package>,
}

impl<'de> Deserialize<'de> for Array {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let packages = th.required("packages")?;
        th.finalize(Some(value))?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let reason = th.required("reason")?;
        th.finalize(None)?;
        Ok(Self { reason })
    }
}

#[derive(Debug)]
struct Flattened {
    flattened: Vec<PackageSpecOrExtended<Reason>>,
}

impl<'de> Deserialize<'de> for Flattened {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let flattened = th.required("flattened")?;
        th.finalize(Some(value))?;
        Ok(Self { flattened })
    }
}

valid_de!(flattened, Flattened);

/// Validates the `Value::pointer/_mut` methods work as expected
#[test]
fn pointers() {
    let mut ba = toml_spanner::parse(include_str!("../data/basic_arrays.toml")).unwrap();

    assert_eq!(
        ba.pointer("/packages/2/version").unwrap().as_str().unwrap(),
        "3.0.0"
    );

    assert_eq!(
        ba.pointer_mut("/packages/3/crate")
            .unwrap()
            .take()
            .as_str()
            .unwrap(),
        "fourth:0.1"
    );

    // After take(), the value is replaced with a Boolean(false) placeholder
    assert_eq!(
        ba.pointer("/packages/3/crate").unwrap().as_bool(),
        Some(false)
    );
}

#[derive(Debug)]
struct Ohno {
    year: u8,
}

impl<'de> Deserialize<'de> for Ohno {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let year = th.required("year").ok();

        if let Some(snbh) = th.optional_s::<std::borrow::Cow<'de, str>>("this-is-deprecated") {
            th.errors.push(
                (
                    toml_spanner::ErrorKind::Custom("this-is-deprecated is deprecated".into()),
                    snbh.span,
                )
                    .into(),
            );
        }

        th.finalize(Some(value))?;
        Ok(Self {
            year: year.unwrap(),
        })
    }
}

invalid_de!(
    custom_error,
    Ohno,
    "year = 40_000\nthis-is-deprecated = 'this should not be here'"
);
