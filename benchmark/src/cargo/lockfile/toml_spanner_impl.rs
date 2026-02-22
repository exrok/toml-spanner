use std::collections::BTreeMap;

use toml_spanner::{Context, Failed, Item};

use super::{
    TomlLockfile, TomlLockfileDependency, TomlLockfileMetadata, TomlLockfilePackageId,
    TomlLockfilePatch, TomlLockfileSourceId,
};

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfileSourceId {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        TomlLockfileSourceId::new(item.expect_string(ctx)?.into())
            .map_err(|err| ctx.push_error(toml_spanner::Error::custom(err, item.span())))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfilePackageId {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        item.parse().map_err(|err| ctx.push_error(err))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfileDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        Ok(TomlLockfileDependency {
            name: th.required("name")?,
            version: th.required("version")?,
            source: th.optional("source"),
            checksum: th.optional("checksum"),
            dependencies: th.optional("dependencies"),
            replace: th.optional("replace"),
        })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfilePatch {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        Ok(TomlLockfilePatch {
            unused: th.optional("unused").unwrap_or_default(),
        })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfile {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        Ok(TomlLockfile {
            version: th.optional("version"),
            package: th.optional("package"),
            root: th.optional("root"),
            metadata: th.optional("metadata"),
            patch: th.optional("patch").unwrap_or_default(),
        })
    }
}
