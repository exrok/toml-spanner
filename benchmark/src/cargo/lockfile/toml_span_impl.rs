use std::collections::BTreeMap;

use toml_span::de_helpers::TableHelper;
use toml_span::value::{Value, ValueInner};
use toml_span::{DeserError, ErrorKind};

use super::{
    TomlLockfile, TomlLockfileDependency, TomlLockfileMetadata, TomlLockfilePackageId,
    TomlLockfilePatch, TomlLockfileSourceId,
};

impl<'de> toml_span::Deserialize<'de> for TomlLockfileSourceId {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let s = value.take_string(None)?;
        TomlLockfileSourceId::new(s.into_owned()).map_err(|err| {
            toml_span::Error {
                kind: ErrorKind::Custom(err.to_string().into()),
                span: value.span,
                line_info: None,
            }
            .into()
        })
    }
}

impl<'de> toml_span::Deserialize<'de> for TomlLockfilePackageId {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let s = value.take_string(None)?;
        s.parse::<TomlLockfilePackageId>().map_err(|err| {
            toml_span::Error {
                kind: ErrorKind::Custom(err.to_string().into()),
                span: value.span,
                line_info: None,
            }
            .into()
        })
    }
}

impl<'de> toml_span::Deserialize<'de> for TomlLockfileDependency {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let lock = TomlLockfileDependency {
            name: th.required("name")?,
            version: th.required("version")?,
            source: th.optional("source"),
            checksum: th.optional("checksum"),
            dependencies: th.optional("dependencies"),
            replace: th.optional("replace"),
        };
        th.finalize(None)?;
        Ok(lock)
    }
}

impl<'de> toml_span::Deserialize<'de> for TomlLockfilePatch {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let unused = th.optional("unused").unwrap_or_default();
        th.finalize(None)?;
        Ok(TomlLockfilePatch { unused })
    }
}

impl<'de> toml_span::Deserialize<'de> for TomlLockfile {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let version: Option<u32> = th.optional("version");
        let package: Option<Vec<TomlLockfileDependency>> = th.optional("package");
        let root: Option<TomlLockfileDependency> = th.optional("root");
        let metadata = match th.take("metadata") {
            Some((_key, mut val)) => Some(deserialize_metadata(&mut val)?),
            None => None,
        };
        let patch: TomlLockfilePatch = th.optional("patch").unwrap_or_default();
        th.finalize(None)?;
        Ok(TomlLockfile {
            version,
            package,
            root,
            metadata,
            patch,
        })
    }
}

/// Deserializes a `BTreeMap<String, String>` from a TOML table value.
fn deserialize_metadata<'de>(value: &mut Value<'de>) -> Result<TomlLockfileMetadata, DeserError> {
    let ValueInner::Table(table) = value.take() else {
        return Err(toml_span::de_helpers::expected("a table", value.take(), value.span).into());
    };
    let mut map = BTreeMap::new();
    let mut errors = Vec::new();
    for (key, val) in &table {
        match val.as_str() {
            Some(s) => {
                map.insert(key.name.to_string(), s.to_owned());
            }
            None => {
                errors.push(toml_span::Error {
                    kind: ErrorKind::Wanted {
                        expected: "a string",
                        found: val.as_ref().type_str(),
                    },
                    span: val.span,
                    line_info: None,
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(map)
    } else {
        Err(DeserError { errors })
    }
}
