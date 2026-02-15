#![allow(unused)]

use toml_span::de_helpers::TableHelper;
use toml_span::{DeserError, Deserialize, Value};

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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let project = Self {
            name: th.required("name")?,
            version: th.required("version")?,
            description: th.optional("description"),
            settings: th.required("settings")?,
            dependencies: th.required("dependencies")?,
            targets: th.required("targets")?,
            metadata: th.optional("metadata"),
        };
        th.finalize(None)?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let settings = Self {
            optimize: th.required("optimize")?,
            parallel: th.optional("parallel"),
            features: th.required("features")?,
        };
        th.finalize(None)?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let dep = Self {
            name: th.required("name")?,
            version: th.optional("version"),
            path: th.optional("path"),
            optional: th.optional("optional").unwrap_or(false),
        };
        th.finalize(None)?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let target = Self {
            name: th.required("name")?,
            kind: th.required("kind")?,
            sources: th.required("sources")?,
            settings: th.optional("settings"),
        };
        th.finalize(None)?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let settings = Self {
            optimize_level: th.optional("optimize_level"),
            debug: th.optional("debug"),
            extra_flags: th.optional("extra_flags").unwrap_or_default(),
        };
        th.finalize(None)?;
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
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut th = TableHelper::new(value)?;
        let metadata = Self {
            authors: th.required("authors")?,
            license: th.optional("license"),
            repository: th.optional("repository"),
            keywords: th.optional("keywords").unwrap_or_default(),
        };
        th.finalize(None)?;
        Ok(metadata)
    }
}

fn run(input: &str) {
    let mut value = toml_span::parse(input).unwrap();
    let project = Project::deserialize(&mut value).unwrap();
    std::hint::black_box(&project);
}
