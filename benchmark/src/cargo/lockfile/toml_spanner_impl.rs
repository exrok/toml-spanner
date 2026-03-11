use super::*;
use toml_spanner::{Context, Failed, Item};

impl<'de> toml_spanner::FromToml<'de> for TomlLockfileSourceId {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        TomlLockfileSourceId::new(item.expect_string(ctx)?.into())
            .map_err(|err| ctx.push_error(toml_spanner::Error::custom(err, item.span_unchecked())))
    }
}

impl<'de> toml_spanner::FromToml<'de> for TomlLockfilePackageId {
    fn from_toml(ctx: &mut Context<'de>, item: &Item<'de>) -> Result<Self, Failed> {
        item.parse().map_err(|err| ctx.push_error(err))
    }
}
