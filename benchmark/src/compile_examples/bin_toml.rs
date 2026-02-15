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

fn get_string(table: &toml::Table, key: &str) -> String {
    table[key].as_str().unwrap().to_owned()
}

fn get_string_opt(table: &toml::Table, key: &str) -> Option<String> {
    table.get(key).and_then(|v| v.as_str()).map(|s| s.to_owned())
}

fn get_bool(table: &toml::Table, key: &str) -> bool {
    table[key].as_bool().unwrap()
}

fn get_bool_opt(table: &toml::Table, key: &str) -> Option<bool> {
    table.get(key).and_then(|v| v.as_bool())
}

fn get_int_opt(table: &toml::Table, key: &str) -> Option<i64> {
    table.get(key).and_then(|v| v.as_integer())
}

fn get_string_array(table: &toml::Table, key: &str) -> Vec<String> {
    table[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn get_string_array_or_default(table: &toml::Table, key: &str) -> Vec<String> {
    let Some(val) = table.get(key) else {
        return Vec::new();
    };
    val.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

impl Project {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            name: get_string(table, "name"),
            version: get_string(table, "version"),
            description: get_string_opt(table, "description"),
            settings: Settings::from_toml(table["settings"].as_table().unwrap()),
            dependencies: table["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Dependency::from_toml(v.as_table().unwrap()))
                .collect(),
            targets: table["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Target::from_toml(v.as_table().unwrap()))
                .collect(),
            metadata: table
                .get("metadata")
                .and_then(|v| v.as_table())
                .map(Metadata::from_toml),
        }
    }
}

impl Settings {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            optimize: get_bool(table, "optimize"),
            parallel: get_int_opt(table, "parallel"),
            features: get_string_array(table, "features"),
        }
    }
}

impl Dependency {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            name: get_string(table, "name"),
            version: get_string_opt(table, "version"),
            path: get_string_opt(table, "path"),
            optional: table
                .get("optional")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }
}

impl Target {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            name: get_string(table, "name"),
            kind: get_string(table, "kind"),
            sources: get_string_array(table, "sources"),
            settings: table
                .get("settings")
                .and_then(|v| v.as_table())
                .map(TargetSettings::from_toml),
        }
    }
}

impl TargetSettings {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            optimize_level: get_int_opt(table, "optimize_level"),
            debug: get_bool_opt(table, "debug"),
            extra_flags: get_string_array_or_default(table, "extra_flags"),
        }
    }
}

impl Metadata {
    fn from_toml(table: &toml::Table) -> Self {
        Self {
            authors: get_string_array(table, "authors"),
            license: get_string_opt(table, "license"),
            repository: get_string_opt(table, "repository"),
            keywords: get_string_array_or_default(table, "keywords"),
        }
    }
}

fn run(input: &str) {
    let table: toml::Table = input.parse().unwrap();
    let project = Project::from_toml(&table);
    std::hint::black_box(&project);
}
