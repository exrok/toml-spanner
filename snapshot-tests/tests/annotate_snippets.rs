#![allow(dead_code)]

use snapshot_tests::{invalid_de_snippet, invalid_snippet};
use toml_spanner::{Context, Failed, FromToml, Spanned};

// Parse errors
invalid_snippet!(newline_string, "a = \"\n\"");
invalid_snippet!(duplicate_key, "a = {a=1,a=1}");
invalid_snippet!(bad_codepoint, "foo = \"\\uD800\"");
invalid_snippet!(unterminated_string, r#"foo = "\"#);
invalid_snippet!(table_redefinition, "[a.b]\n[a.\"b\"]");
invalid_snippet!(eof, "key =");

// Deserialization errors

#[derive(Debug)]
struct Boop {
    s: String,
    os: Option<u32>,
}

impl<'de> FromToml<'de> for Boop {
    fn from_toml(
        ctx: &mut Context<'de>,
        value: &toml_spanner::Item<'de>,
    ) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let s = th.required("s")?;
        let os = th.optional("os");
        th.expect_empty()?;
        Ok(Self { s, os })
    }
}

invalid_de_snippet!(missing_required, Boop, "os = 20");
invalid_de_snippet!(
    unknown_field,
    Boop,
    "s = 'val'\nthis-field-is-not-known = 20"
);

#[derive(Debug)]
struct Ohno {
    year: u8,
}

impl<'de> FromToml<'de> for Ohno {
    fn from_toml(
        ctx: &mut Context<'de>,
        value: &toml_spanner::Item<'de>,
    ) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let year = th.required("year")?;

        if let Some(snbh) = th.optional::<Spanned<std::borrow::Cow<'de, str>>>("this-is-deprecated")
        {
            return Err(th.ctx.push_error(toml_spanner::Error::custom(
                "this-is-deprecated is deprecated",
                snbh.span,
            )));
        }

        th.expect_empty()?;
        Ok(Self { year })
    }
}

invalid_de_snippet!(
    custom_error,
    Ohno,
    "year = 40_000\nthis-is-deprecated = 'this should not be here'"
);
