#![allow(unused)]

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

#[derive(Debug)]
struct Settings {
    optimize: bool,
    parallel: Option<i64>,
    features: Vec<String>,
}

#[derive(Debug)]
struct Dependency {
    name: String,
    version: Option<String>,
    path: Option<String>,
    optional: bool,
}

#[derive(Debug)]
struct Target {
    name: String,
    kind: String,
    sources: Vec<String>,
    settings: Option<TargetSettings>,
}

#[derive(Debug)]
struct TargetSettings {
    optimize_level: Option<i64>,
    debug: Option<bool>,
    extra_flags: Vec<String>,
}

#[derive(Debug)]
struct Metadata {
    authors: Vec<String>,
    license: Option<String>,
    repository: Option<String>,
    keywords: Vec<String>,
}

#[allow(clippy::todo)]
fn run(_input: &str) {
    todo!()
}
