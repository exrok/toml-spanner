#![allow(unused)]

use toml_spanner::{Deserialize, Error, Item, ValueMut};

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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let project = Self {
            name: table.required("name")?,
            version: table.required("version")?,
            description: table.optional("description")?,
            settings: table.required("settings")?,
            dependencies: table.required("dependencies")?,
            targets: table.required("targets")?,
            metadata: table.optional("metadata")?,
        };
        table.expect_empty()?;
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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let settings = Self {
            optimize: table.required("optimize")?,
            parallel: table.optional("parallel")?,
            features: table.required("features")?,
        };
        table.expect_empty()?;
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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let dep = Self {
            name: table.required("name")?,
            version: table.optional("version")?,
            path: table.optional("path")?,
            optional: table.required("optional")?,
        };
        table.expect_empty()?;
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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let target = Self {
            name: table.required("name")?,
            kind: table.required("kind")?,
            sources: table.required("sources")?,
            settings: table.optional("settings")?,
        };
        table.expect_empty()?;
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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let settings = Self {
            optimize_level: table.optional("optimize_level")?,
            debug: table.optional("debug")?,
            extra_flags: table.required("extra_flags")?,
        };
        table.expect_empty()?;
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
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        let table = value.expect_table()?;
        let metadata = Self {
            authors: table.required("authors")?,
            license: table.optional("license")?,
            repository: table.optional("repository")?,
            keywords: table.required("keywords")?,
        };
        table.expect_empty()?;
        Ok(metadata)
    }
}

#[inline(never)]
fn run(input: &str) -> Project {
    let arena = toml_spanner::Arena::new();
    let table = toml_spanner::parse(input, &arena).unwrap();
    let mut item = table.into_item();
    Project::deserialize(&mut item).unwrap()
}
