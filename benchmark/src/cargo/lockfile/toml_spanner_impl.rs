use super::*;
use toml_spanner::{Context, Failed, Item};

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfileSourceId {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        TomlLockfileSourceId::new(item.expect_string(ctx)?.into())
            .map_err(|err| ctx.push_error(toml_spanner::Error::custom(err, item.span_unchecked())))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLockfilePackageId {
    fn deserialize(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        item.parse().map_err(|err| ctx.push_error(err))
    }
}

toml_spanner::deserialize_table! {
    struct TomlLockfileDependency {
        required name: String,
        required version: String,
        optional source: TomlLockfileSourceId,
        optional checksum: String,
        optional dependencies: Vec<TomlLockfilePackageId>,
        optional replace: TomlLockfilePackageId,
    }
}

toml_spanner::deserialize_table! {
    struct TomlLockfilePatch {
        default unused: Vec<TomlLockfileDependency>,
    }
}

toml_spanner::deserialize_table! {
    struct TomlLockfile {
        optional version: u32,
        optional package: Vec<TomlLockfileDependency>,
        optional root: TomlLockfileDependency,
        optional metadata: TomlLockfileMetadata,
        default patch: TomlLockfilePatch,
    }
}
