use super::*;
use toml_spanner::{Context, Failed};

impl<'de> toml_spanner::FromToml<'de> for TomlLockfileSourceId {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        TomlLockfileSourceId::new(item.require_string(ctx)?.into())
            .map_err(|err| ctx.push_error(toml_spanner::Error::custom(err, item.span())))
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlLockfilePackageId {
    fn from_toml(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        item.parse().map_err(|err| ctx.push_error(err))
    }
}
