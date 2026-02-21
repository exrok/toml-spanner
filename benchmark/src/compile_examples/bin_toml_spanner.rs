#![allow(unused)]

use toml_spanner::{Context, Deserialize, Failed, Item, TableHelper};

#[derive(Debug)]
struct Project {
    name: String,
    version: String,
    description: Option<String>,
    settings: Settings,
    dependencies: Vec<Dependency>,
    targets: Vec<Target>,
    metadata: Option<Metadata>,
}

impl<'de> Deserialize<'de> for Project {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let project = Self {
            name: th.required("name")?,
            version: th.required("version")?,
            description: th.optional("description"),
            settings: th.required("settings")?,
            dependencies: th.required("dependencies")?,
            targets: th.required("targets")?,
            metadata: th.optional("metadata"),
        };
        th.expect_empty()?;
        Ok(project)
    }
}

#[derive(Debug)]
struct Settings {
    optimize: bool,
    parallel: Option<i64>,
    features: Vec<String>,
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let settings = Self {
            optimize: th.required("optimize")?,
            parallel: th.optional("parallel"),
            features: th.required("features")?,
        };
        th.expect_empty()?;
        Ok(settings)
    }
}

#[derive(Debug)]
struct Dependency {
    name: String,
    version: Option<String>,
    path: Option<String>,
    optional: bool,
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let dep = Self {
            name: th.required("name")?,
            version: th.optional("version"),
            path: th.optional("path"),
            optional: th.required("optional")?,
        };
        th.expect_empty()?;
        Ok(dep)
    }
}

#[derive(Debug)]
struct Target {
    name: String,
    kind: String,
    sources: Vec<String>,
    settings: Option<TargetSettings>,
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let target = Self {
            name: th.required("name")?,
            kind: th.required("kind")?,
            sources: th.required("sources")?,
            settings: th.optional("settings"),
        };
        th.expect_empty()?;
        Ok(target)
    }
}

#[derive(Debug)]
struct TargetSettings {
    optimize_level: Option<i64>,
    debug: Option<bool>,
    extra_flags: Vec<String>,
}

impl<'de> Deserialize<'de> for TargetSettings {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let settings = Self {
            optimize_level: th.optional("optimize_level"),
            debug: th.optional("debug"),
            extra_flags: th.required("extra_flags")?,
        };
        th.expect_empty()?;
        Ok(settings)
    }
}

#[derive(Debug)]
struct Metadata {
    authors: Vec<String>,
    license: Option<String>,
    repository: Option<String>,
    keywords: Vec<String>,
}

impl<'de> Deserialize<'de> for Metadata {
    fn deserialize(ctx: &mut Context<'de>, value: &Item<'de>) -> Result<Self, Failed> {
        let mut th = value.table_helper(ctx)?;
        let metadata = Self {
            authors: th.required("authors")?,
            license: th.optional("license"),
            repository: th.optional("repository"),
            keywords: th.required("keywords")?,
        };
        th.expect_empty()?;
        Ok(metadata)
    }
}

#[inline(never)]
fn run(input: &str) -> Project {
    let arena = toml_spanner::Arena::new();
    let mut root = toml_spanner::parse(input, &arena).unwrap();
    // Need to match the behaviour of toml-span, panic with errors not just
    // sentinel.
    match root.deserialize::<Project>() {
        Ok(project) => project,
        Err(_) => panic!("{:?}", root.errors()),
    }
}
