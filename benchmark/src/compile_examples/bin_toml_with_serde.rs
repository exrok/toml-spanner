#![allow(unused)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    version: String,
    description: Option<String>,
    settings: Settings,
    dependencies: Vec<Dependency>,
    targets: Vec<Target>,
    metadata: Option<Metadata>,
}

#[derive(Debug, Deserialize)]
struct Settings {
    optimize: bool,
    parallel: Option<i64>,
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    version: Option<String>,
    path: Option<String>,
    #[serde(default)]
    optional: bool,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
    kind: String,
    sources: Vec<String>,
    settings: Option<TargetSettings>,
}

#[derive(Debug, Deserialize)]
struct TargetSettings {
    optimize_level: Option<i64>,
    debug: Option<bool>,
    #[serde(default)]
    extra_flags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    authors: Vec<String>,
    license: Option<String>,
    repository: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
}

fn run(input: &str) {
    let project: Project = toml::from_str(input).unwrap();
    std::hint::black_box(&project);
}
