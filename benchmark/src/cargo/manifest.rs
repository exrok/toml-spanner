//! Extracted from https://github.com/rust-lang/cargo for benchmarking the
//! snapshot was taking at 2026-02-22 from 3ca292befbc3585084922c1592ea3d17e423f035
//!
//! References Files:
//! crates/cargo-toml-files/src/manifest.rs
//!
//! Copyright remains with the original authors of Cargo, licensed under the MIT License or Apache License (at your option).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::{self, Display, Write};
use std::path::PathBuf;
use std::str;

use serde::de::{self, IntoDeserializer as _, Unexpected};
use serde::ser;
use serde::{Deserialize, Serialize};
use serde_untagged::UntaggedEnumVisitor;

use super::package_id_spec::PackageIdSpec;
use super::restricted_names;
use super::rust_version::RustVersion;

pub use super::restricted_names::NameValidationError;

/// This type is used to deserialize `Cargo.toml` files.
#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlManifest {
    pub cargo_features: Option<Vec<String>>,

    // Update `requires_package` when adding new package-specific fields
    pub package: Option<Box<TomlPackage>>,
    pub project: Option<Box<TomlPackage>>,
    pub badges: Option<BTreeMap<String, BTreeMap<String, String>>>,
    pub features: Option<BTreeMap<FeatureName, Vec<String>>>,
    pub lib: Option<TomlLibTarget>,
    pub bin: Option<Vec<TomlBinTarget>>,
    pub example: Option<Vec<TomlExampleTarget>>,
    pub test: Option<Vec<TomlTestTarget>>,
    pub bench: Option<Vec<TomlTestTarget>>,
    pub dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    pub dev_dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    #[serde(rename = "dev_dependencies")]
    pub dev_dependencies2: Option<BTreeMap<PackageName, InheritableDependency>>,
    pub build_dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    #[serde(rename = "build_dependencies")]
    pub build_dependencies2: Option<BTreeMap<PackageName, InheritableDependency>>,
    pub target: Option<BTreeMap<String, TomlPlatform>>,
    pub lints: Option<InheritableLints>,
    pub hints: Option<Hints>,

    pub workspace: Option<TomlWorkspace>,
    pub profile: Option<TomlProfiles>,
    pub patch: Option<BTreeMap<String, BTreeMap<PackageName, TomlDependency>>>,
    pub replace: Option<BTreeMap<String, TomlDependency>>,

    /// Report unused keys (see also nested `_unused_keys`)
    /// Note: this is populated by the caller, rather than automatically
    #[serde(skip)]
    pub _unused_keys: BTreeSet<String>,
}

impl TomlManifest {
    pub fn requires_package(&self) -> impl Iterator<Item = &'static str> {
        [
            self.badges.as_ref().map(|_| "badges"),
            self.features.as_ref().map(|_| "features"),
            self.lib.as_ref().map(|_| "lib"),
            self.bin.as_ref().map(|_| "bin"),
            self.example.as_ref().map(|_| "example"),
            self.test.as_ref().map(|_| "test"),
            self.bench.as_ref().map(|_| "bench"),
            self.dependencies.as_ref().map(|_| "dependencies"),
            self.dev_dependencies().as_ref().map(|_| "dev-dependencies"),
            self.build_dependencies()
                .as_ref()
                .map(|_| "build-dependencies"),
            self.target.as_ref().map(|_| "target"),
            self.lints.as_ref().map(|_| "lints"),
            self.hints.as_ref().map(|_| "hints"),
        ]
        .into_iter()
        .flatten()
    }

    pub fn has_profiles(&self) -> bool {
        self.profile.is_some()
    }

    pub fn package(&self) -> Option<&Box<TomlPackage>> {
        self.package.as_ref().or(self.project.as_ref())
    }

    pub fn dev_dependencies(&self) -> Option<&BTreeMap<PackageName, InheritableDependency>> {
        self.dev_dependencies
            .as_ref()
            .or(self.dev_dependencies2.as_ref())
    }

    pub fn build_dependencies(&self) -> Option<&BTreeMap<PackageName, InheritableDependency>> {
        self.build_dependencies
            .as_ref()
            .or(self.build_dependencies2.as_ref())
    }

    pub fn features(&self) -> Option<&BTreeMap<FeatureName, Vec<String>>> {
        self.features.as_ref()
    }

    pub fn normalized_lints(&self) -> Result<Option<&TomlLints>, UnresolvedError> {
        self.lints.as_ref().map(|l| l.normalized()).transpose()
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TomlWorkspace {
    pub members: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub default_members: Option<Vec<String>>,
    pub resolver: Option<String>,

    pub metadata: Option<toml::Value>,

    // Properties that can be inherited by members.
    pub package: Option<InheritablePackage>,
    pub dependencies: Option<BTreeMap<PackageName, TomlDependency>>,
    pub lints: Option<TomlLints>,
}

/// A group of fields that are inheritable by members of the workspace
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct InheritablePackage {
    pub version: Option<semver::Version>,
    pub authors: Option<Vec<String>>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub readme: Option<StringOrBool>,
    pub keywords: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub license: Option<String>,
    pub license_file: Option<String>,
    pub repository: Option<String>,
    pub publish: Option<VecStringOrBool>,
    pub edition: Option<String>,
    pub badges: Option<BTreeMap<String, BTreeMap<String, String>>>,
    pub exclude: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub rust_version: Option<RustVersion>,
}

/// Represents the `package`/`project` sections of a `Cargo.toml`.
///
/// Note that the order of the fields matters, since this is the order they
/// are serialized to a TOML file. For example, you cannot have values after
/// the field `metadata`, since it is a table and values cannot appear after
/// tables.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub struct TomlPackage {
    pub edition: Option<InheritableString>,
    pub rust_version: Option<InheritableRustVersion>,
    pub name: Option<PackageName>,
    pub version: Option<InheritableSemverVersion>,
    pub authors: Option<InheritableVecString>,
    pub build: Option<TomlPackageBuild>,
    pub metabuild: Option<StringOrVec>,
    pub default_target: Option<String>,
    pub forced_target: Option<String>,
    pub links: Option<String>,
    pub exclude: Option<InheritableVecString>,
    pub include: Option<InheritableVecString>,
    pub publish: Option<InheritableVecStringOrBool>,
    pub workspace: Option<String>,
    pub im_a_teapot: Option<bool>,
    pub autolib: Option<bool>,
    pub autobins: Option<bool>,
    pub autoexamples: Option<bool>,
    pub autotests: Option<bool>,
    pub autobenches: Option<bool>,
    pub default_run: Option<String>,

    // Package metadata.
    pub description: Option<InheritableString>,
    pub homepage: Option<InheritableString>,
    pub documentation: Option<InheritableString>,
    pub readme: Option<InheritableStringOrBool>,
    pub keywords: Option<InheritableVecString>,
    pub categories: Option<InheritableVecString>,
    pub license: Option<InheritableString>,
    pub license_file: Option<InheritableString>,
    pub repository: Option<InheritableString>,
    pub resolver: Option<String>,

    pub metadata: Option<toml::Value>,

    /// Provide a helpful error message for a common user error.
    #[serde(rename = "cargo-features", skip_serializing)]
    pub _invalid_cargo_features: Option<InvalidCargoFeatures>,
}

impl TomlPackage {
    pub fn new(name: PackageName) -> Self {
        Self {
            name: Some(name),
            ..Default::default()
        }
    }

    pub fn normalized_name(&self) -> Result<&PackageName, UnresolvedError> {
        self.name.as_ref().ok_or(UnresolvedError)
    }

    pub fn normalized_edition(&self) -> Result<Option<&String>, UnresolvedError> {
        self.edition.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_rust_version(&self) -> Result<Option<&RustVersion>, UnresolvedError> {
        self.rust_version
            .as_ref()
            .map(|v| v.normalized())
            .transpose()
    }

    pub fn normalized_version(&self) -> Result<Option<&semver::Version>, UnresolvedError> {
        self.version.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_authors(&self) -> Result<Option<&Vec<String>>, UnresolvedError> {
        self.authors.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_build(&self) -> Result<Option<&[String]>, UnresolvedError> {
        let build = self.build.as_ref().ok_or(UnresolvedError)?;
        match build {
            TomlPackageBuild::Auto(false) => Ok(None),
            TomlPackageBuild::Auto(true) => Err(UnresolvedError),
            TomlPackageBuild::SingleScript(value) => Ok(Some(std::slice::from_ref(value))),
            TomlPackageBuild::MultipleScript(scripts) => Ok(Some(scripts)),
        }
    }

    pub fn normalized_exclude(&self) -> Result<Option<&Vec<String>>, UnresolvedError> {
        self.exclude.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_include(&self) -> Result<Option<&Vec<String>>, UnresolvedError> {
        self.include.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_publish(&self) -> Result<Option<&VecStringOrBool>, UnresolvedError> {
        self.publish.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_description(&self) -> Result<Option<&String>, UnresolvedError> {
        self.description
            .as_ref()
            .map(|v| v.normalized())
            .transpose()
    }

    pub fn normalized_homepage(&self) -> Result<Option<&String>, UnresolvedError> {
        self.homepage.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_documentation(&self) -> Result<Option<&String>, UnresolvedError> {
        self.documentation
            .as_ref()
            .map(|v| v.normalized())
            .transpose()
    }

    pub fn normalized_readme(&self) -> Result<Option<&String>, UnresolvedError> {
        let readme = self.readme.as_ref().ok_or(UnresolvedError)?;
        readme.normalized().and_then(|sb| match sb {
            StringOrBool::Bool(false) => Ok(None),
            StringOrBool::Bool(true) => Err(UnresolvedError),
            StringOrBool::String(value) => Ok(Some(value)),
        })
    }

    pub fn normalized_keywords(&self) -> Result<Option<&Vec<String>>, UnresolvedError> {
        self.keywords.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_categories(&self) -> Result<Option<&Vec<String>>, UnresolvedError> {
        self.categories.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_license(&self) -> Result<Option<&String>, UnresolvedError> {
        self.license.as_ref().map(|v| v.normalized()).transpose()
    }

    pub fn normalized_license_file(&self) -> Result<Option<&String>, UnresolvedError> {
        self.license_file
            .as_ref()
            .map(|v| v.normalized())
            .transpose()
    }

    pub fn normalized_repository(&self) -> Result<Option<&String>, UnresolvedError> {
        self.repository.as_ref().map(|v| v.normalized()).transpose()
    }
}

/// An enum that allows for inheriting keys from a workspace in a Cargo.toml.
#[derive(Serialize, Copy, Clone, Debug)]
#[serde(untagged)]
pub enum InheritableField<T> {
    /// The type that is used when not inheriting from a workspace.
    Value(T),
    /// The type when inheriting from a workspace.
    Inherit(TomlInheritedField),
}

impl<T> InheritableField<T> {
    pub fn normalized(&self) -> Result<&T, UnresolvedError> {
        self.as_value().ok_or(UnresolvedError)
    }

    pub fn as_value(&self) -> Option<&T> {
        match self {
            InheritableField::Inherit(_) => None,
            InheritableField::Value(defined) => Some(defined),
        }
    }

    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Inherit(_) => None,
            Self::Value(defined) => Some(defined),
        }
    }

    pub fn is_inherited(&self) -> bool {
        matches!(self, Self::Inherit(_))
    }
}

//. This already has a `Deserialize` impl from version_trim_whitespace
pub type InheritableSemverVersion = InheritableField<semver::Version>;
impl<'de> de::Deserialize<'de> for InheritableSemverVersion {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("SemVer version")
            .string(
                |value| match value.trim().parse().map_err(de::Error::custom) {
                    Ok(parsed) => Ok(InheritableField::Value(parsed)),
                    Err(e) => Err(e),
                },
            )
            .map(|value| value.deserialize().map(InheritableField::Inherit))
            .deserialize(d)
    }
}

pub type InheritableString = InheritableField<String>;
impl<'de> de::Deserialize<'de> for InheritableString {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = InheritableString;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
                f.write_str("a string or workspace")
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(InheritableString::Value(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_string(value.to_owned())
            }

            fn visit_map<V>(self, map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mvd = de::value::MapAccessDeserializer::new(map);
                TomlInheritedField::deserialize(mvd).map(InheritableField::Inherit)
            }
        }

        d.deserialize_any(Visitor)
    }
}

pub type InheritableRustVersion = InheritableField<RustVersion>;
impl<'de> de::Deserialize<'de> for InheritableRustVersion {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = InheritableRustVersion;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
                f.write_str("a semver or workspace")
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let value = value.parse::<RustVersion>().map_err(|e| E::custom(e))?;
                Ok(InheritableRustVersion::Value(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_string(value.to_owned())
            }

            fn visit_map<V>(self, map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mvd = de::value::MapAccessDeserializer::new(map);
                TomlInheritedField::deserialize(mvd).map(InheritableField::Inherit)
            }
        }

        d.deserialize_any(Visitor)
    }
}

pub type InheritableVecString = InheritableField<Vec<String>>;
impl<'de> de::Deserialize<'de> for InheritableVecString {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = InheritableVecString;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
                f.write_str("a vector of strings or workspace")
            }
            fn visit_seq<A>(self, v: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let seq = de::value::SeqAccessDeserializer::new(v);
                Vec::deserialize(seq).map(InheritableField::Value)
            }

            fn visit_map<V>(self, map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mvd = de::value::MapAccessDeserializer::new(map);
                TomlInheritedField::deserialize(mvd).map(InheritableField::Inherit)
            }
        }

        d.deserialize_any(Visitor)
    }
}

pub type InheritableStringOrBool = InheritableField<StringOrBool>;
impl<'de> de::Deserialize<'de> for InheritableStringOrBool {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = InheritableStringOrBool;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
                f.write_str("a string, a bool, or workspace")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let b = de::value::BoolDeserializer::new(v);
                StringOrBool::deserialize(b).map(InheritableField::Value)
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let string = de::value::StringDeserializer::new(v);
                StringOrBool::deserialize(string).map(InheritableField::Value)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_string(value.to_owned())
            }

            fn visit_map<V>(self, map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mvd = de::value::MapAccessDeserializer::new(map);
                TomlInheritedField::deserialize(mvd).map(InheritableField::Inherit)
            }
        }

        d.deserialize_any(Visitor)
    }
}

pub type InheritableVecStringOrBool = InheritableField<VecStringOrBool>;
impl<'de> de::Deserialize<'de> for InheritableVecStringOrBool {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = InheritableVecStringOrBool;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
                f.write_str("a boolean, a vector of strings, or workspace")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let b = de::value::BoolDeserializer::new(v);
                VecStringOrBool::deserialize(b).map(InheritableField::Value)
            }

            fn visit_seq<A>(self, v: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let seq = de::value::SeqAccessDeserializer::new(v);
                VecStringOrBool::deserialize(seq).map(InheritableField::Value)
            }

            fn visit_map<V>(self, map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mvd = de::value::MapAccessDeserializer::new(map);
                TomlInheritedField::deserialize(mvd).map(InheritableField::Inherit)
            }
        }

        d.deserialize_any(Visitor)
    }
}

pub type InheritableBtreeMap = InheritableField<BTreeMap<String, BTreeMap<String, String>>>;

impl<'de> de::Deserialize<'de> for InheritableBtreeMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let value = serde_value::Value::deserialize(deserializer)?;

        if let Ok(w) = TomlInheritedField::deserialize(
            serde_value::ValueDeserializer::<D::Error>::new(value.clone()),
        ) {
            return Ok(InheritableField::Inherit(w));
        }
        BTreeMap::deserialize(serde_value::ValueDeserializer::<D::Error>::new(value))
            .map(InheritableField::Value)
    }
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct TomlInheritedField {
    workspace: WorkspaceValue,
}

impl TomlInheritedField {
    pub fn new() -> Self {
        TomlInheritedField {
            workspace: WorkspaceValue,
        }
    }
}

impl Default for TomlInheritedField {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(try_from = "bool")]
#[serde(into = "bool")]
struct WorkspaceValue;

impl TryFrom<bool> for WorkspaceValue {
    type Error = String;
    fn try_from(other: bool) -> Result<WorkspaceValue, Self::Error> {
        if other {
            Ok(WorkspaceValue)
        } else {
            Err("`workspace` cannot be false".to_owned())
        }
    }
}

impl From<WorkspaceValue> for bool {
    fn from(_: WorkspaceValue) -> bool {
        true
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum InheritableDependency {
    /// The type that is used when not inheriting from a workspace.
    Value(TomlDependency),
    /// The type when inheriting from a workspace.
    Inherit(TomlInheritedDependency),
}

impl InheritableDependency {
    pub fn unused_keys(&self) -> Vec<String> {
        match self {
            InheritableDependency::Value(d) => d.unused_keys(),
            InheritableDependency::Inherit(w) => w._unused_keys.keys().cloned().collect(),
        }
    }

    pub fn normalized(&self) -> Result<&TomlDependency, UnresolvedError> {
        match self {
            InheritableDependency::Value(d) => Ok(d),
            InheritableDependency::Inherit(_) => Err(UnresolvedError),
        }
    }

    pub fn is_inherited(&self) -> bool {
        matches!(self, InheritableDependency::Inherit(_))
    }
}

impl<'de> de::Deserialize<'de> for InheritableDependency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let value = serde_value::Value::deserialize(deserializer)?;

        if let Ok(w) = TomlInheritedDependency::deserialize(serde_value::ValueDeserializer::<
            D::Error,
        >::new(value.clone()))
        {
            return if w.workspace {
                Ok(InheritableDependency::Inherit(w))
            } else {
                Err(de::Error::custom("`workspace` cannot be false"))
            };
        }
        TomlDependency::deserialize(serde_value::ValueDeserializer::<D::Error>::new(value))
            .map(InheritableDependency::Value)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct TomlInheritedDependency {
    pub workspace: bool,
    pub features: Option<Vec<String>>,
    pub default_features: Option<bool>,
    #[serde(rename = "default_features")]
    pub default_features2: Option<bool>,
    pub optional: Option<bool>,
    pub public: Option<bool>,

    /// This is here to provide a way to see the "unused manifest keys" when deserializing
    #[serde(skip_serializing)]
    #[serde(flatten)]
    pub _unused_keys: BTreeMap<String, toml::Value>,
}

impl TomlInheritedDependency {
    pub fn default_features(&self) -> Option<bool> {
        self.default_features.or(self.default_features2)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum TomlDependency<P: Clone = String> {
    /// In the simple format, only a version is specified, eg.
    /// `package = "<version>"`
    Simple(String),
    /// The simple format is equivalent to a detailed dependency
    /// specifying only a version, eg.
    /// `package = { version = "<version>" }`
    Detailed(TomlDetailedDependency<P>),
}

impl TomlDependency {
    pub fn is_version_specified(&self) -> bool {
        match self {
            TomlDependency::Detailed(d) => d.version.is_some(),
            TomlDependency::Simple(..) => true,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            TomlDependency::Detailed(d) => d.optional.unwrap_or(false),
            TomlDependency::Simple(..) => false,
        }
    }

    pub fn is_public(&self) -> bool {
        match self {
            TomlDependency::Detailed(d) => d.public.unwrap_or(false),
            TomlDependency::Simple(..) => false,
        }
    }

    pub fn default_features(&self) -> Option<bool> {
        match self {
            TomlDependency::Detailed(d) => d.default_features(),
            TomlDependency::Simple(..) => None,
        }
    }

    pub fn unused_keys(&self) -> Vec<String> {
        match self {
            TomlDependency::Simple(_) => vec![],
            TomlDependency::Detailed(detailed) => detailed._unused_keys.keys().cloned().collect(),
        }
    }
}

impl<'de, P: Deserialize<'de> + Clone> de::Deserialize<'de> for TomlDependency<P> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let expected = "a version string like \"0.9.8\" or a \
                     detailed dependency like { version = \"0.9.8\" }";
        UntaggedEnumVisitor::new()
            .expecting(expected)
            .string(|value| Ok(TomlDependency::Simple(value.to_owned())))
            .bool(|value| {
                let expected = format!("invalid type: boolean `{value}`, expected {expected}");
                let err = if value {
                    format!(
                        "{expected}\n\
                    note: if you meant to use a workspace member, you can write\n \
                      dep.workspace = {value}"
                    )
                } else {
                    expected
                };

                Err(serde_untagged::de::Error::custom(err))
            })
            .map(|value| value.deserialize().map(TomlDependency::Detailed))
            .deserialize(deserializer)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct TomlDetailedDependency<P: Clone = String> {
    pub version: Option<String>,

    pub registry: Option<RegistryName>,
    /// The URL of the `registry` field.
    /// This is an internal implementation detail. When Cargo creates a
    /// package, it replaces `registry` with `registry-index` so that the
    /// manifest contains the correct URL. All users won't have the same
    /// registry names configured, so Cargo can't rely on just the name for
    /// crates published by other users.
    pub registry_index: Option<String>,
    // `path` is relative to the file it appears in. If that's a `Cargo.toml`, it'll be relative to
    // that TOML file, and if it's a `.cargo/config` file, it'll be relative to that file.
    pub path: Option<P>,
    pub base: Option<PathBaseName>,
    pub git: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub rev: Option<String>,
    pub features: Option<Vec<String>>,
    pub optional: Option<bool>,
    pub default_features: Option<bool>,
    #[serde(rename = "default_features")]
    pub default_features2: Option<bool>,
    pub package: Option<PackageName>,
    pub public: Option<bool>,

    /// One or more of `bin`, `cdylib`, `staticlib`, `bin:<name>`.
    pub artifact: Option<StringOrVec>,
    /// If set, the artifact should also be a dependency
    pub lib: Option<bool>,
    /// A platform name, like `x86_64-apple-darwin`
    pub target: Option<String>,

    /// This is here to provide a way to see the "unused manifest keys" when deserializing
    #[serde(skip_serializing)]
    #[serde(flatten)]
    pub _unused_keys: BTreeMap<String, toml::Value>,
}

impl<P: Clone> TomlDetailedDependency<P> {
    pub fn default_features(&self) -> Option<bool> {
        self.default_features.or(self.default_features2)
    }
}

// Explicit implementation so we avoid pulling in P: Default
impl<P: Clone> Default for TomlDetailedDependency<P> {
    fn default() -> Self {
        Self {
            version: Default::default(),
            registry: Default::default(),
            registry_index: Default::default(),
            path: Default::default(),
            base: Default::default(),
            git: Default::default(),
            branch: Default::default(),
            tag: Default::default(),
            rev: Default::default(),
            features: Default::default(),
            optional: Default::default(),
            default_features: Default::default(),
            default_features2: Default::default(),
            package: Default::default(),
            public: Default::default(),
            artifact: Default::default(),
            lib: Default::default(),
            target: Default::default(),
            _unused_keys: Default::default(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct TomlProfiles(pub BTreeMap<ProfileName, TomlProfile>);

impl TomlProfiles {
    pub fn get_all(&self) -> &BTreeMap<ProfileName, TomlProfile> {
        &self.0
    }

    pub fn get(&self, name: &str) -> Option<&TomlProfile> {
        self.0.get(name)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, Eq, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct TomlProfile {
    pub opt_level: Option<TomlOptLevel>,
    pub lto: Option<StringOrBool>,
    pub codegen_backend: Option<String>,
    pub codegen_units: Option<u32>,
    pub debug: Option<TomlDebugInfo>,
    pub split_debuginfo: Option<String>,
    pub debug_assertions: Option<bool>,
    pub rpath: Option<bool>,
    pub panic: Option<String>,
    pub overflow_checks: Option<bool>,
    pub incremental: Option<bool>,
    pub dir_name: Option<String>,
    pub inherits: Option<String>,
    pub strip: Option<StringOrBool>,
    // Note that `rustflags` is used for the cargo-feature `profile_rustflags`
    pub rustflags: Option<Vec<String>>,
    // These two fields must be last because they are sub-tables, and TOML
    // requires all non-tables to be listed first.
    pub package: Option<BTreeMap<ProfilePackageSpec, TomlProfile>>,
    pub build_override: Option<Box<TomlProfile>>,
    /// Unstable feature `-Ztrim-paths`.
    pub trim_paths: Option<TomlTrimPaths>,
    /// Unstable feature `hint-mostly-unused`
    pub hint_mostly_unused: Option<bool>,
}

impl TomlProfile {
    /// Overwrite self's values with the given profile.
    pub fn merge(&mut self, profile: &Self) {
        if let Some(v) = &profile.opt_level {
            self.opt_level = Some(v.clone());
        }

        if let Some(v) = &profile.lto {
            self.lto = Some(v.clone());
        }

        if let Some(v) = &profile.codegen_backend {
            self.codegen_backend = Some(v.clone());
        }

        if let Some(v) = profile.codegen_units {
            self.codegen_units = Some(v);
        }

        if let Some(v) = profile.debug {
            self.debug = Some(v);
        }

        if let Some(v) = profile.debug_assertions {
            self.debug_assertions = Some(v);
        }

        if let Some(v) = &profile.split_debuginfo {
            self.split_debuginfo = Some(v.clone());
        }

        if let Some(v) = profile.rpath {
            self.rpath = Some(v);
        }

        if let Some(v) = &profile.panic {
            self.panic = Some(v.clone());
        }

        if let Some(v) = profile.overflow_checks {
            self.overflow_checks = Some(v);
        }

        if let Some(v) = profile.incremental {
            self.incremental = Some(v);
        }

        if let Some(v) = &profile.rustflags {
            self.rustflags = Some(v.clone());
        }

        if let Some(other_package) = &profile.package {
            match &mut self.package {
                Some(self_package) => {
                    for (spec, other_pkg_profile) in other_package {
                        match self_package.get_mut(spec) {
                            Some(p) => p.merge(other_pkg_profile),
                            None => {
                                self_package.insert(spec.clone(), other_pkg_profile.clone());
                            }
                        }
                    }
                }
                None => self.package = Some(other_package.clone()),
            }
        }

        if let Some(other_bo) = &profile.build_override {
            match &mut self.build_override {
                Some(self_bo) => self_bo.merge(other_bo),
                None => self.build_override = Some(other_bo.clone()),
            }
        }

        if let Some(v) = &profile.inherits {
            self.inherits = Some(v.clone());
        }

        if let Some(v) = &profile.dir_name {
            self.dir_name = Some(v.clone());
        }

        if let Some(v) = &profile.strip {
            self.strip = Some(v.clone());
        }

        if let Some(v) = &profile.trim_paths {
            self.trim_paths = Some(v.clone())
        }

        if let Some(v) = profile.hint_mostly_unused {
            self.hint_mostly_unused = Some(v);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum ProfilePackageSpec {
    Spec(PackageIdSpec),
    All,
}

impl fmt::Display for ProfilePackageSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfilePackageSpec::Spec(spec) => spec.fmt(f),
            ProfilePackageSpec::All => f.write_str("*"),
        }
    }
}

impl ser::Serialize for ProfilePackageSpec {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.to_string().serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for ProfilePackageSpec {
    fn deserialize<D>(d: D) -> Result<ProfilePackageSpec, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        if string == "*" {
            Ok(ProfilePackageSpec::All)
        } else {
            PackageIdSpec::parse(&string)
                .map_err(de::Error::custom)
                .map(ProfilePackageSpec::Spec)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TomlOptLevel(pub String);

impl ser::Serialize for TomlOptLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self.0.parse::<u32>() {
            Ok(n) => n.serialize(serializer),
            Err(_) => self.0.serialize(serializer),
        }
    }
}

impl<'de> de::Deserialize<'de> for TomlOptLevel {
    fn deserialize<D>(d: D) -> Result<TomlOptLevel, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use serde::de::Error as _;
        UntaggedEnumVisitor::new()
            .expecting("an optimization level")
            .i64(|value| Ok(TomlOptLevel(value.to_string())))
            .string(|value| {
                if value == "s" || value == "z" {
                    Ok(TomlOptLevel(value.to_string()))
                } else {
                    Err(serde_untagged::de::Error::custom(format!(
                        "must be `0`, `1`, `2`, `3`, `s` or `z`, \
                         but found the string: \"{}\"",
                        value
                    )))
                }
            })
            .deserialize(d)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum TomlDebugInfo {
    None,
    LineDirectivesOnly,
    LineTablesOnly,
    Limited,
    Full,
}

impl Display for TomlDebugInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TomlDebugInfo::None => f.write_char('0'),
            TomlDebugInfo::Limited => f.write_char('1'),
            TomlDebugInfo::Full => f.write_char('2'),
            TomlDebugInfo::LineDirectivesOnly => f.write_str("line-directives-only"),
            TomlDebugInfo::LineTablesOnly => f.write_str("line-tables-only"),
        }
    }
}

impl ser::Serialize for TomlDebugInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            Self::None => 0.serialize(serializer),
            Self::LineDirectivesOnly => "line-directives-only".serialize(serializer),
            Self::LineTablesOnly => "line-tables-only".serialize(serializer),
            Self::Limited => 1.serialize(serializer),
            Self::Full => 2.serialize(serializer),
        }
    }
}

impl<'de> de::Deserialize<'de> for TomlDebugInfo {
    fn deserialize<D>(d: D) -> Result<TomlDebugInfo, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let expecting = "a boolean, 0, 1, 2, \"none\", \"limited\", \"full\", \"line-tables-only\", or \"line-directives-only\"";
        UntaggedEnumVisitor::new()
            .expecting(expecting)
            .bool(|value| {
                Ok(if value {
                    TomlDebugInfo::Full
                } else {
                    TomlDebugInfo::None
                })
            })
            .i64(|value| {
                let debuginfo = match value {
                    0 => TomlDebugInfo::None,
                    1 => TomlDebugInfo::Limited,
                    2 => TomlDebugInfo::Full,
                    _ => {
                        return Err(serde_untagged::de::Error::invalid_value(
                            Unexpected::Signed(value),
                            &expecting,
                        ));
                    }
                };
                Ok(debuginfo)
            })
            .string(|value| {
                let debuginfo = match value {
                    "none" => TomlDebugInfo::None,
                    "limited" => TomlDebugInfo::Limited,
                    "full" => TomlDebugInfo::Full,
                    "line-directives-only" => TomlDebugInfo::LineDirectivesOnly,
                    "line-tables-only" => TomlDebugInfo::LineTablesOnly,
                    _ => {
                        return Err(serde_untagged::de::Error::invalid_value(
                            Unexpected::Str(value),
                            &expecting,
                        ));
                    }
                };
                Ok(debuginfo)
            })
            .deserialize(d)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize)]
#[serde(untagged, rename_all = "kebab-case")]
pub enum TomlTrimPaths {
    Values(Vec<TomlTrimPathsValue>),
    All,
}

impl TomlTrimPaths {
    pub fn none() -> Self {
        TomlTrimPaths::Values(Vec::new())
    }

    pub fn is_none(&self) -> bool {
        match self {
            TomlTrimPaths::Values(v) => v.is_empty(),
            TomlTrimPaths::All => false,
        }
    }
}

impl<'de> de::Deserialize<'de> for TomlTrimPaths {
    fn deserialize<D>(d: D) -> Result<TomlTrimPaths, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let expecting = r#"a boolean, "none", "diagnostics", "macro", "object", "all", or an array with these options"#;
        UntaggedEnumVisitor::new()
            .expecting(expecting)
            .bool(|value| {
                Ok(if value {
                    TomlTrimPaths::All
                } else {
                    TomlTrimPaths::none()
                })
            })
            .string(|v| match v {
                "none" => Ok(TomlTrimPaths::none()),
                "all" => Ok(TomlTrimPaths::All),
                v => {
                    let d = v.into_deserializer();
                    let err = |_: D::Error| {
                        serde_untagged::de::Error::custom(format!("expected {expecting}"))
                    };
                    TomlTrimPathsValue::deserialize(d)
                        .map_err(err)
                        .map(|v| v.into())
                }
            })
            .seq(|seq| {
                let seq: Vec<String> = seq.deserialize()?;
                let seq: Vec<_> = seq
                    .into_iter()
                    .map(|s| TomlTrimPathsValue::deserialize(s.into_deserializer()))
                    .collect::<Result<_, _>>()?;
                Ok(seq.into())
            })
            .deserialize(d)
    }
}

impl fmt::Display for TomlTrimPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TomlTrimPaths::All => write!(f, "all"),
            TomlTrimPaths::Values(v) if v.is_empty() => write!(f, "none"),
            TomlTrimPaths::Values(v) => {
                let mut iter = v.iter();
                if let Some(value) = iter.next() {
                    write!(f, "{value}")?;
                }
                for value in iter {
                    write!(f, ",{value}")?;
                }
                Ok(())
            }
        }
    }
}

impl From<TomlTrimPathsValue> for TomlTrimPaths {
    fn from(value: TomlTrimPathsValue) -> Self {
        TomlTrimPaths::Values(vec![value])
    }
}

impl From<Vec<TomlTrimPathsValue>> for TomlTrimPaths {
    fn from(value: Vec<TomlTrimPathsValue>) -> Self {
        TomlTrimPaths::Values(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TomlTrimPathsValue {
    Diagnostics,
    Macro,
    Object,
}

impl TomlTrimPathsValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            TomlTrimPathsValue::Diagnostics => "diagnostics",
            TomlTrimPathsValue::Macro => "macro",
            TomlTrimPathsValue::Object => "object",
        }
    }
}

impl fmt::Display for TomlTrimPathsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub type TomlLibTarget = TomlTarget;
pub type TomlBinTarget = TomlTarget;
pub type TomlExampleTarget = TomlTarget;
pub type TomlTestTarget = TomlTarget;
pub type TomlBenchTarget = TomlTarget;

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TomlTarget {
    pub name: Option<String>,

    // The intention was to only accept `crate-type` here but historical
    // versions of Cargo also accepted `crate_type`, so look for both.
    pub crate_type: Option<Vec<String>>,
    #[serde(rename = "crate_type")]
    pub crate_type2: Option<Vec<String>>,

    pub path: Option<PathValue>,
    // Note that `filename` is used for the cargo-feature `different_binary_name`
    pub filename: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub doc_scrape_examples: Option<bool>,
    pub proc_macro: Option<bool>,
    #[serde(rename = "proc_macro")]
    pub proc_macro2: Option<bool>,
    pub harness: Option<bool>,
    pub required_features: Option<Vec<String>>,
    pub edition: Option<String>,
}

impl TomlTarget {
    pub fn new() -> TomlTarget {
        TomlTarget::default()
    }

    pub fn proc_macro(&self) -> Option<bool> {
        self.proc_macro.or(self.proc_macro2).or_else(|| {
            if let Some(types) = self.crate_types() {
                if types.contains(&"proc-macro".to_string()) {
                    return Some(true);
                }
            }
            None
        })
    }

    pub fn crate_types(&self) -> Option<&Vec<String>> {
        self.crate_type
            .as_ref()
            .or_else(|| self.crate_type2.as_ref())
    }
}

macro_rules! str_newtype {
    ($name:ident) => {
        /// Verified string newtype
        #[derive(Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[serde(transparent)]
        pub struct $name<T: AsRef<str> = String>(T);

        impl<T: AsRef<str>> $name<T> {
            pub fn into_inner(self) -> T {
                self.0
            }
        }

        impl<T: AsRef<str>> AsRef<str> for $name<T> {
            fn as_ref(&self) -> &str {
                self.0.as_ref()
            }
        }

        impl<T: AsRef<str>> std::ops::Deref for $name<T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<T: AsRef<str>> std::borrow::Borrow<str> for $name<T> {
            fn borrow(&self) -> &str {
                self.0.as_ref()
            }
        }

        impl<'a> std::str::FromStr for $name<String> {
            type Err = restricted_names::NameValidationError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_owned())
            }
        }

        impl<'de, T: AsRef<str> + serde::Deserialize<'de>> serde::Deserialize<'de> for $name<T> {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let inner = T::deserialize(deserializer)?;
                Self::new(inner).map_err(serde::de::Error::custom)
            }
        }

        impl<T: AsRef<str>> Display for $name<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.as_ref().fmt(f)
            }
        }
    };
}

str_newtype!(PackageName);

impl<T: AsRef<str>> PackageName<T> {
    /// Validated package name
    pub fn new(name: T) -> Result<Self, NameValidationError> {
        restricted_names::validate_package_name(name.as_ref())?;
        Ok(Self(name))
    }
}

impl PackageName {
    /// Coerce a value to be a validate package name
    ///
    /// Replaces invalid values with `placeholder`
    pub fn sanitize(name: impl AsRef<str>, placeholder: char) -> Self {
        PackageName(restricted_names::sanitize_package_name(
            name.as_ref(),
            placeholder,
        ))
    }
}

str_newtype!(RegistryName);

impl<T: AsRef<str>> RegistryName<T> {
    /// Validated registry name
    pub fn new(name: T) -> Result<Self, NameValidationError> {
        restricted_names::validate_registry_name(name.as_ref())?;
        Ok(Self(name))
    }
}

str_newtype!(ProfileName);

impl<T: AsRef<str>> ProfileName<T> {
    /// Validated profile name
    pub fn new(name: T) -> Result<Self, NameValidationError> {
        restricted_names::validate_profile_name(name.as_ref())?;
        Ok(Self(name))
    }
}

str_newtype!(FeatureName);

impl<T: AsRef<str>> FeatureName<T> {
    /// Validated feature name
    pub fn new(name: T) -> Result<Self, NameValidationError> {
        restricted_names::validate_feature_name(name.as_ref())?;
        Ok(Self(name))
    }
}

str_newtype!(PathBaseName);

impl<T: AsRef<str>> PathBaseName<T> {
    /// Validated path base name
    pub fn new(name: T) -> Result<Self, NameValidationError> {
        restricted_names::validate_path_base_name(name.as_ref())?;
        Ok(Self(name))
    }
}

/// Corresponds to a `target` entry, but `TomlTarget` is already used.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TomlPlatform {
    pub dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    pub build_dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    #[serde(rename = "build_dependencies")]
    pub build_dependencies2: Option<BTreeMap<PackageName, InheritableDependency>>,
    pub dev_dependencies: Option<BTreeMap<PackageName, InheritableDependency>>,
    #[serde(rename = "dev_dependencies")]
    pub dev_dependencies2: Option<BTreeMap<PackageName, InheritableDependency>>,
}

impl TomlPlatform {
    pub fn dev_dependencies(&self) -> Option<&BTreeMap<PackageName, InheritableDependency>> {
        self.dev_dependencies
            .as_ref()
            .or(self.dev_dependencies2.as_ref())
    }

    pub fn build_dependencies(&self) -> Option<&BTreeMap<PackageName, InheritableDependency>> {
        self.build_dependencies
            .as_ref()
            .or(self.build_dependencies2.as_ref())
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct InheritableLints {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub workspace: bool,
    #[serde(flatten)]
    pub lints: TomlLints,
}

impl InheritableLints {
    pub fn normalized(&self) -> Result<&TomlLints, UnresolvedError> {
        if self.workspace {
            Err(UnresolvedError)
        } else {
            Ok(&self.lints)
        }
    }
}

impl<'de> Deserialize<'de> for InheritableLints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct InheritableLintsVisitor;

        impl<'de> de::Visitor<'de> for InheritableLintsVisitor {
            // The type that our Visitor is going to produce.
            type Value = InheritableLints;

            // Format a message stating what data this Visitor expects to receive.
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a lints table")
            }

            // Deserialize MyMap from an abstract "map" provided by the
            // Deserializer. The MapAccess input is a callback provided by
            // the Deserializer to let us see each entry in the map.
            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut lints = TomlLints::new();
                let mut workspace = false;

                // While there are entries remaining in the input, add them
                // into our map.
                while let Some(key) = access.next_key()? {
                    if key == "workspace" {
                        workspace = match access.next_value()? {
                            Some(WorkspaceValue) => true,
                            None => false,
                        };
                    } else {
                        let value = access.next_value()?;
                        lints.insert(key, value);
                    }
                }

                Ok(InheritableLints { workspace, lints })
            }
        }

        deserializer.deserialize_map(InheritableLintsVisitor)
    }
}

pub type TomlLints = BTreeMap<String, TomlToolLints>;

pub type TomlToolLints = BTreeMap<String, TomlLint>;

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TomlLint {
    Level(TomlLintLevel),
    Config(TomlLintConfig),
}

impl<'de> Deserialize<'de> for TomlLint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .string(|string| {
                TomlLintLevel::deserialize(string.into_deserializer()).map(TomlLint::Level)
            })
            .map(|map| map.deserialize().map(TomlLint::Config))
            .deserialize(deserializer)
    }
}

impl TomlLint {
    pub fn level(&self) -> TomlLintLevel {
        match self {
            Self::Level(level) => *level,
            Self::Config(config) => config.level,
        }
    }

    pub fn priority(&self) -> i8 {
        match self {
            Self::Level(_) => 0,
            Self::Config(config) => config.priority,
        }
    }

    pub fn config(&self) -> Option<&toml::Table> {
        match self {
            Self::Level(_) => None,
            Self::Config(config) => Some(&config.config),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TomlLintConfig {
    pub level: TomlLintLevel,
    #[serde(default)]
    pub priority: i8,
    #[serde(flatten)]
    pub config: toml::Table,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TomlLintLevel {
    Forbid,
    Deny,
    Warn,
    Allow,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Hints {
    pub mostly_unused: Option<toml::Value>,
}

#[derive(Copy, Clone, Debug)]
pub struct InvalidCargoFeatures {}

impl<'de> de::Deserialize<'de> for InvalidCargoFeatures {
    fn deserialize<D>(_d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use serde::de::Error as _;

        Err(D::Error::custom(
            "the field `cargo-features` should be set at the top of Cargo.toml before any tables",
        ))
    }
}

/// This can be parsed from either a TOML string or array,
/// but is always stored as a vector.
#[derive(Clone, Debug, Serialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct StringOrVec(pub Vec<String>);

impl StringOrVec {
    pub fn iter<'a>(&'a self) -> std::slice::Iter<'a, String> {
        self.0.iter()
    }
}

impl<'de> de::Deserialize<'de> for StringOrVec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("string or list of strings")
            .string(|value| Ok(StringOrVec(vec![value.to_owned()])))
            .seq(|value| value.deserialize().map(StringOrVec))
            .deserialize(deserializer)
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum StringOrBool {
    String(String),
    Bool(bool),
}

impl<'de> Deserialize<'de> for StringOrBool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .bool(|b| Ok(StringOrBool::Bool(b)))
            .string(|s| Ok(StringOrBool::String(s.to_owned())))
            .deserialize(deserializer)
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum TomlPackageBuild {
    /// If build scripts are disabled or enabled.
    /// If true, `build.rs` in the root folder will be the build script.
    Auto(bool),

    /// Path of Build Script if there's just one script.
    SingleScript(String),

    /// Vector of paths if multiple build script are to be used.
    MultipleScript(Vec<String>),
}

impl<'de> Deserialize<'de> for TomlPackageBuild {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .bool(|b| Ok(TomlPackageBuild::Auto(b)))
            .string(|s| Ok(TomlPackageBuild::SingleScript(s.to_owned())))
            .seq(|value| value.deserialize().map(TomlPackageBuild::MultipleScript))
            .deserialize(deserializer)
    }
}

#[derive(PartialEq, Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum VecStringOrBool {
    VecString(Vec<String>),
    Bool(bool),
}

impl<'de> de::Deserialize<'de> for VecStringOrBool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("a boolean or vector of strings")
            .bool(|value| Ok(VecStringOrBool::Bool(value)))
            .seq(|value| value.deserialize().map(VecStringOrBool::VecString))
            .deserialize(deserializer)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PathValue(pub PathBuf);

impl fmt::Debug for PathValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ser::Serialize for PathValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for PathValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Ok(PathValue(String::deserialize(deserializer)?.into()))
    }
}

/// Error validating names in Cargo.
#[derive(Debug, thiserror::Error)]
#[error("manifest field was not resolved")]
#[non_exhaustive]
pub struct UnresolvedError;

use toml_spanner::{Context, Failed};

/// Convert a `toml_spanner::Item` into a `toml::Value`.
fn item_to_toml_value(item: &toml_spanner::Item<'_>) -> toml::Value {
    match item.value() {
        toml_spanner::Value::String(&s) => toml::Value::String(s.to_owned()),
        toml_spanner::Value::Integer(&i) => toml::Value::Integer(i),
        toml_spanner::Value::Float(&f) => toml::Value::Float(f),
        toml_spanner::Value::Boolean(&b) => toml::Value::Boolean(b),
        toml_spanner::Value::Array(arr) => {
            toml::Value::Array(arr.iter().map(item_to_toml_value).collect())
        }
        toml_spanner::Value::Table(table) => {
            let mut map = toml::map::Map::new();
            for (key, val) in table {
                map.insert(key.name.to_owned(), item_to_toml_value(val));
            }
            toml::Value::Table(map)
        }
        toml_spanner::Value::DateTime(dt) => {
            let mut buf = std::mem::MaybeUninit::uninit();
            let s = dt.format(&mut buf);
            toml::Value::String(s.to_owned())
        }
    }
}

/// Helper to push a custom error from a Display-able error value.
fn push_custom_error(
    ctx: &mut Context<'_>,
    item: &toml_spanner::Item<'_>,
    err: impl fmt::Display,
) -> Failed {
    ctx.push_error(toml_spanner::Error {
        kind: toml_spanner::ErrorKind::Custom(err.to_string().into()),
        span: item.span(),
    })
}

macro_rules! impl_spanner_deserialize_str_newtype {
    ($name:ident) => {
        impl<'de> toml_spanner::Deserialize<'de> for $name {
            fn deserialize(
                ctx: &mut Context<'de>,
                item: &toml_spanner::Item<'de>,
            ) -> Result<Self, Failed> {
                let s = item.expect_string(ctx)?;
                $name::new(s.to_owned()).map_err(|err| push_custom_error(ctx, item, err))
            }
        }
    };
}

impl_spanner_deserialize_str_newtype!(PackageName);
impl_spanner_deserialize_str_newtype!(RegistryName);
impl_spanner_deserialize_str_newtype!(ProfileName);
impl_spanner_deserialize_str_newtype!(FeatureName);
impl_spanner_deserialize_str_newtype!(PathBaseName);

impl<'de> toml_spanner::Deserialize<'de> for StringOrVec {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(StringOrVec(vec![s.to_owned()])),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(StringOrVec(v))
            }
            _ => Err(ctx.error_expected_but_found("a string or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for StringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(StringOrBool::Bool(b)),
            toml_spanner::Value::String(&s) => Ok(StringOrBool::String(s.to_owned())),
            _ => Err(ctx.error_expected_but_found("a string or bool", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for VecStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(VecStringOrBool::Bool(b)),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(VecStringOrBool::VecString(v))
            }
            _ => Err(ctx.error_expected_but_found("a boolean or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for PathValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s: String = toml_spanner::Deserialize::deserialize(ctx, item)?;
        Ok(PathValue(s.into()))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlPackageBuild {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(TomlPackageBuild::Auto(b)),
            toml_spanner::Value::String(&s) => Ok(TomlPackageBuild::SingleScript(s.to_owned())),
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(TomlPackageBuild::MultipleScript(v))
            }
            _ => Err(ctx.error_expected_but_found("a bool, string, or array of strings", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlOptLevel {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Integer(&i) => Ok(TomlOptLevel(i.to_string())),
            toml_spanner::Value::String(&s) => {
                if s == "s" || s == "z" {
                    Ok(TomlOptLevel(s.to_owned()))
                } else {
                    Err(push_custom_error(
                        ctx,
                        item,
                        format_args!(
                            "must be `0`, `1`, `2`, `3`, `s` or `z`, but found the string: \"{s}\""
                        ),
                    ))
                }
            }
            _ => {
                Err(ctx.error_expected_but_found("an optimization level (integer or string)", item))
            }
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlDebugInfo {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(if b {
                TomlDebugInfo::Full
            } else {
                TomlDebugInfo::None
            }),
            toml_spanner::Value::Integer(&i) => match i {
                0 => Ok(TomlDebugInfo::None),
                1 => Ok(TomlDebugInfo::Limited),
                2 => Ok(TomlDebugInfo::Full),
                _ => Err(push_custom_error(
                    ctx,
                    item,
                    "expected a boolean, 0, 1, 2, \"none\", \"limited\", \"full\", \"line-tables-only\", or \"line-directives-only\"",
                )),
            },
            toml_spanner::Value::String(&s) => match s {
                "none" => Ok(TomlDebugInfo::None),
                "limited" => Ok(TomlDebugInfo::Limited),
                "full" => Ok(TomlDebugInfo::Full),
                "line-directives-only" => Ok(TomlDebugInfo::LineDirectivesOnly),
                "line-tables-only" => Ok(TomlDebugInfo::LineTablesOnly),
                _ => Err(push_custom_error(
                    ctx,
                    item,
                    format_args!(
                        "expected a boolean, 0, 1, 2, \"none\", \"limited\", \"full\", \"line-tables-only\", or \"line-directives-only\", found \"{s}\""
                    ),
                )),
            },
            _ => {
                Err(ctx
                    .error_expected_but_found("a boolean, integer, or string for debug info", item))
            }
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlTrimPathsValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        match s {
            "diagnostics" => Ok(TomlTrimPathsValue::Diagnostics),
            "macro" => Ok(TomlTrimPathsValue::Macro),
            "object" => Ok(TomlTrimPathsValue::Object),
            _ => Err(push_custom_error(
                ctx,
                item,
                format_args!("expected \"diagnostics\", \"macro\", or \"object\", found \"{s}\""),
            )),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlTrimPaths {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(if b {
                TomlTrimPaths::All
            } else {
                TomlTrimPaths::none()
            }),
            toml_spanner::Value::String(&s) => match s {
                "none" => Ok(TomlTrimPaths::none()),
                "all" => Ok(TomlTrimPaths::All),
                _ => {
                    let val =
                        <TomlTrimPathsValue as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                    Ok(val.into())
                }
            },
            toml_spanner::Value::Array(_) => {
                let v: Vec<TomlTrimPathsValue> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(v.into())
            }
            _ => {
                Err(ctx
                    .error_expected_but_found("a boolean, string, or array for trim-paths", item))
            }
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for ProfilePackageSpec {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        if s == "*" {
            Ok(ProfilePackageSpec::All)
        } else {
            PackageIdSpec::parse(s)
                .map(ProfilePackageSpec::Spec)
                .map_err(|err| push_custom_error(ctx, item, err))
        }
    }
}

/// Deserialize `BTreeMap<ProfilePackageSpec, TomlProfile>` since `ProfilePackageSpec`
/// doesn't implement `FromStr`.
fn deserialize_profile_package_map<'de>(
    ctx: &mut Context<'de>,
    item: &toml_spanner::Item<'de>,
) -> Result<BTreeMap<ProfilePackageSpec, TomlProfile>, Failed> {
    let table = item.expect_table(ctx)?;
    let mut map = BTreeMap::new();
    let mut had_error = false;
    for (key, val) in table {
        let spec = if key.name == "*" {
            ProfilePackageSpec::All
        } else {
            match PackageIdSpec::parse(key.name) {
                Ok(s) => ProfilePackageSpec::Spec(s),
                Err(err) => {
                    ctx.push_error(toml_spanner::Error {
                        kind: toml_spanner::ErrorKind::Custom(err.to_string().into()),
                        span: key.span,
                    });
                    had_error = true;
                    continue;
                }
            }
        };
        match <TomlProfile as toml_spanner::Deserialize>::deserialize(ctx, val) {
            Ok(profile) => {
                map.insert(spec, profile);
            }
            Err(_) => had_error = true,
        }
    }
    if had_error { Err(Failed) } else { Ok(map) }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlProfile {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let profile = TomlProfile {
            opt_level: th.optional("opt-level"),
            lto: th.optional("lto"),
            codegen_backend: th.optional("codegen-backend"),
            codegen_units: th.optional("codegen-units"),
            debug: th.optional("debug"),
            split_debuginfo: th.optional("split-debuginfo"),
            debug_assertions: th.optional("debug-assertions"),
            rpath: th.optional("rpath"),
            panic: th.optional("panic"),
            overflow_checks: th.optional("overflow-checks"),
            incremental: th.optional("incremental"),
            dir_name: th.optional("dir-name"),
            inherits: th.optional("inherits"),
            strip: th.optional("strip"),
            rustflags: th.optional("rustflags"),
            package: match th.optional_item("package") {
                Some(pkg_item) => match deserialize_profile_package_map(th.ctx, pkg_item) {
                    Ok(map) => Some(map),
                    Err(_) => None,
                },
                None => None,
            },
            build_override: th.optional("build-override"),
            trim_paths: th.optional("trim-paths"),
            hint_mostly_unused: th.optional("hint-mostly-unused"),
        };
        th.expect_empty()?;
        Ok(profile)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlProfiles {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let map: BTreeMap<ProfileName, TomlProfile> =
            toml_spanner::Deserialize::deserialize(ctx, item)?;
        Ok(TomlProfiles(map))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLintLevel {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let s = item.expect_string(ctx)?;
        match s {
            "forbid" => Ok(TomlLintLevel::Forbid),
            "deny" => Ok(TomlLintLevel::Deny),
            "warn" => Ok(TomlLintLevel::Warn),
            "allow" => Ok(TomlLintLevel::Allow),
            _ => Err(push_custom_error(
                ctx,
                item,
                format_args!(
                    "expected \"forbid\", \"deny\", \"warn\", or \"allow\", found \"{s}\""
                ),
            )),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLintConfig {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let level: TomlLintLevel = th.required("level")?;
        let priority: i8 = th.optional("priority").unwrap_or(0);
        // Remaining fields go into the config table
        let mut config = toml::Table::new();
        for (key, val) in th.into_remaining() {
            config.insert(key.name.to_owned(), item_to_toml_value(val));
        }
        Ok(TomlLintConfig {
            level,
            priority,
            config,
        })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlLint {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(_) => {
                let level = <TomlLintLevel as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlLint::Level(level))
            }
            toml_spanner::Value::Table(_) => {
                let config = <TomlLintConfig as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlLint::Config(config))
            }
            _ => Err(ctx.error_expected_but_found("a lint level string or config table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableLints {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let table = item.expect_table(ctx)?;
        let mut workspace = false;
        let mut lints = TomlLints::new();
        for (key, val) in table {
            if key.name == "workspace" {
                match val.as_bool() {
                    Some(true) => workspace = true,
                    Some(false) => workspace = false,
                    None => {
                        ctx.error_expected_but_found("a boolean for workspace", val);
                    }
                }
            } else {
                match <BTreeMap<String, TomlLint> as toml_spanner::Deserialize>::deserialize(
                    ctx, val,
                ) {
                    Ok(tool_lints) => {
                        lints.insert(key.name.to_owned(), tool_lints);
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(InheritableLints { workspace, lints })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for WorkspaceValue {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.as_bool() {
            Some(true) => Ok(WorkspaceValue),
            Some(false) => Err(push_custom_error(ctx, item, "`workspace` cannot be false")),
            None => Err(ctx.error_expected_but_found("a boolean", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlInheritedField {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let _workspace: WorkspaceValue = th.required("workspace")?;
        th.expect_empty()?;
        Ok(TomlInheritedField {
            workspace: WorkspaceValue,
        })
    }
}

/// Check if a table item has a `workspace = true` key.
fn is_workspace_inherit(item: &toml_spanner::Item<'_>) -> bool {
    if let Some(table) = item.as_table() {
        if let Some(ws) = table.get("workspace") {
            return ws.as_bool() == Some(true);
        }
    }
    false
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableSemverVersion {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.trim().parse::<semver::Version>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableString {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(InheritableField::Value(s.to_owned())),
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableRustVersion {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => match s.parse::<RustVersion>() {
                Ok(v) => Ok(InheritableField::Value(v)),
                Err(err) => Err(push_custom_error(ctx, item, err)),
            },
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a version string or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableVecString {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(InheritableField::Value(v))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("an array of strings or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => Ok(InheritableField::Value(StringOrBool::Bool(b))),
            toml_spanner::Value::String(&s) => {
                Ok(InheritableField::Value(StringOrBool::String(s.to_owned())))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx.error_expected_but_found("a string, bool, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableVecStringOrBool {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::Boolean(&b) => {
                Ok(InheritableField::Value(VecStringOrBool::Bool(b)))
            }
            toml_spanner::Value::Array(_) => {
                let v: Vec<String> = toml_spanner::Deserialize::deserialize(ctx, item)?;
                Ok(InheritableField::Value(VecStringOrBool::VecString(v)))
            }
            toml_spanner::Value::Table(_) => {
                let field =
                    <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(InheritableField::Inherit(field))
            }
            _ => Err(ctx
                .error_expected_but_found("a boolean, array of strings, or workspace table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableBtreeMap {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        if is_workspace_inherit(item) {
            let field = <TomlInheritedField as toml_spanner::Deserialize>::deserialize(ctx, item)?;
            Ok(InheritableField::Inherit(field))
        } else {
            let map: BTreeMap<String, BTreeMap<String, String>> =
                toml_spanner::Deserialize::deserialize(ctx, item)?;
            Ok(InheritableField::Value(map))
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlDetailedDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let dep = TomlDetailedDependency {
            version: th.optional("version"),
            registry: th.optional("registry"),
            registry_index: th.optional("registry-index"),
            path: th.optional("path"),
            base: th.optional("base"),
            git: th.optional("git"),
            branch: th.optional("branch"),
            tag: th.optional("tag"),
            rev: th.optional("rev"),
            features: th.optional("features"),
            optional: th.optional("optional"),
            default_features: th.optional("default-features"),
            default_features2: th.optional("default_features"),
            package: th.optional("package"),
            public: th.optional("public"),
            artifact: th.optional("artifact"),
            lib: th.optional("lib"),
            target: th.optional("target"),
            _unused_keys: {
                let mut map = BTreeMap::new();
                for (key, val) in th.into_remaining() {
                    map.insert(key.name.to_owned(), item_to_toml_value(val));
                }
                map
            },
        };
        Ok(dep)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        match item.value() {
            toml_spanner::Value::String(&s) => Ok(TomlDependency::Simple(s.to_owned())),
            toml_spanner::Value::Boolean(&b) => {
                let msg = if b {
                    format!(
                        "invalid type: boolean `true`, expected a version string like \"0.9.8\" or a \
                         detailed dependency like {{ version = \"0.9.8\" }}\n\
                         note: if you meant to use a workspace member, you can write\n \
                         dep.workspace = true"
                    )
                } else {
                    format!(
                        "invalid type: boolean `false`, expected a version string like \"0.9.8\" or a \
                         detailed dependency like {{ version = \"0.9.8\" }}"
                    )
                };
                Err(push_custom_error(ctx, item, msg))
            }
            toml_spanner::Value::Table(_) => {
                let detailed =
                    <TomlDetailedDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
                Ok(TomlDependency::Detailed(detailed))
            }
            _ => Err(ctx.error_expected_but_found("a version string or dependency table", item)),
        }
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlInheritedDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let dep = TomlInheritedDependency {
            workspace: th.required("workspace")?,
            features: th.optional("features"),
            default_features: th.optional("default-features"),
            default_features2: th.optional("default_features"),
            optional: th.optional("optional"),
            public: th.optional("public"),
            _unused_keys: {
                let mut map = BTreeMap::new();
                for (key, val) in th.into_remaining() {
                    map.insert(key.name.to_owned(), item_to_toml_value(val));
                }
                map
            },
        };
        Ok(dep)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritableDependency {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        // If it's a table with `workspace` key, try to parse as inherited
        if is_workspace_inherit(item) {
            let dep =
                <TomlInheritedDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
            if dep.workspace {
                return Ok(InheritableDependency::Inherit(dep));
            } else {
                return Err(push_custom_error(ctx, item, "`workspace` cannot be false"));
            }
        }
        // Check for workspace = false case
        if let Some(table) = item.as_table() {
            if let Some(ws) = table.get("workspace") {
                if ws.as_bool() == Some(false) {
                    return Err(push_custom_error(ctx, item, "`workspace` cannot be false"));
                }
            }
        }
        let dep = <TomlDependency as toml_spanner::Deserialize>::deserialize(ctx, item)?;
        Ok(InheritableDependency::Value(dep))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlTarget {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let target = TomlTarget {
            name: th.optional("name"),
            crate_type: th.optional("crate-type"),
            crate_type2: th.optional("crate_type"),
            path: th.optional("path"),
            filename: th.optional("filename"),
            test: th.optional("test"),
            doctest: th.optional("doctest"),
            bench: th.optional("bench"),
            doc: th.optional("doc"),
            doc_scrape_examples: th.optional("doc-scrape-examples"),
            proc_macro: th.optional("proc-macro"),
            proc_macro2: th.optional("proc_macro"),
            harness: th.optional("harness"),
            required_features: th.optional("required-features"),
            edition: th.optional("edition"),
        };
        th.expect_empty()?;
        Ok(target)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlPlatform {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let platform = TomlPlatform {
            dependencies: th.optional("dependencies"),
            build_dependencies: th.optional("build-dependencies"),
            build_dependencies2: th.optional("build_dependencies"),
            dev_dependencies: th.optional("dev-dependencies"),
            dev_dependencies2: th.optional("dev_dependencies"),
        };
        th.expect_empty()?;
        Ok(platform)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for Hints {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let mostly_unused = th.optional_item("mostly-unused").map(item_to_toml_value);
        th.expect_empty()?;
        Ok(Hints { mostly_unused })
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InvalidCargoFeatures {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        Err(push_custom_error(
            ctx,
            item,
            "the field `cargo-features` should be set at the top of Cargo.toml before any tables",
        ))
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlPackage {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let pkg = TomlPackage {
            edition: th.optional("edition"),
            rust_version: th.optional("rust-version"),
            name: th.optional("name"),
            version: th.optional("version"),
            authors: th.optional("authors"),
            build: th.optional("build"),
            metabuild: th.optional("metabuild"),
            default_target: th.optional("default-target"),
            forced_target: th.optional("forced-target"),
            links: th.optional("links"),
            exclude: th.optional("exclude"),
            include: th.optional("include"),
            publish: th.optional("publish"),
            workspace: th.optional("workspace"),
            im_a_teapot: th.optional("im-a-teapot"),
            autolib: th.optional("autolib"),
            autobins: th.optional("autobins"),
            autoexamples: th.optional("autoexamples"),
            autotests: th.optional("autotests"),
            autobenches: th.optional("autobenches"),
            default_run: th.optional("default-run"),
            description: th.optional("description"),
            homepage: th.optional("homepage"),
            documentation: th.optional("documentation"),
            readme: th.optional("readme"),
            keywords: th.optional("keywords"),
            categories: th.optional("categories"),
            license: th.optional("license"),
            license_file: th.optional("license-file"),
            repository: th.optional("repository"),
            resolver: th.optional("resolver"),
            metadata: th.optional_item("metadata").map(item_to_toml_value),
            _invalid_cargo_features: th.optional("cargo-features"),
        };
        th.expect_empty()?;
        Ok(pkg)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for InheritablePackage {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let pkg = InheritablePackage {
            version: th.optional_mapped("version", |item| {
                let s = item
                    .as_str()
                    .ok_or_else(|| item.expected("a version string"))?;
                s.trim()
                    .parse::<semver::Version>()
                    .map_err(|err| toml_spanner::Error::custom(err, item.span()))
            }),
            authors: th.optional("authors"),
            description: th.optional("description"),
            homepage: th.optional("homepage"),
            documentation: th.optional("documentation"),
            readme: th.optional("readme"),
            keywords: th.optional("keywords"),
            categories: th.optional("categories"),
            license: th.optional("license"),
            license_file: th.optional("license-file"),
            repository: th.optional("repository"),
            publish: th.optional("publish"),
            edition: th.optional("edition"),
            badges: th.optional("badges"),
            exclude: th.optional("exclude"),
            include: th.optional("include"),
            rust_version: th.optional_mapped("rust-version", |item| {
                let s = item
                    .as_str()
                    .ok_or_else(|| item.expected("a rust version string"))?;
                s.parse::<RustVersion>()
                    .map_err(|err| toml_spanner::Error::custom(err, item.span()))
            }),
        };
        th.expect_empty()?;
        Ok(pkg)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlWorkspace {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let ws = TomlWorkspace {
            members: th.optional("members"),
            exclude: th.optional("exclude"),
            default_members: th.optional("default-members"),
            resolver: th.optional("resolver"),
            metadata: th.optional_item("metadata").map(item_to_toml_value),
            package: th.optional("package"),
            dependencies: th.optional("dependencies"),
            lints: th.optional("lints"),
        };
        th.expect_empty()?;
        Ok(ws)
    }
}

impl<'de> toml_spanner::Deserialize<'de> for TomlManifest {
    fn deserialize(ctx: &mut Context<'de>, item: &toml_spanner::Item<'de>) -> Result<Self, Failed> {
        let mut th = item.table_helper(ctx)?;
        let manifest = TomlManifest {
            cargo_features: th.optional("cargo-features"),
            package: th.optional("package"),
            project: th.optional("project"),
            badges: th.optional("badges"),
            features: th.optional("features"),
            lib: th.optional("lib"),
            bin: th.optional("bin"),
            example: th.optional("example"),
            test: th.optional("test"),
            bench: th.optional("bench"),
            dependencies: th.optional("dependencies"),
            dev_dependencies: th.optional("dev-dependencies"),
            dev_dependencies2: th.optional("dev_dependencies"),
            build_dependencies: th.optional("build-dependencies"),
            build_dependencies2: th.optional("build_dependencies"),
            target: th.optional("target"),
            lints: th.optional("lints"),
            hints: th.optional("hints"),
            workspace: th.optional("workspace"),
            profile: th.optional("profile"),
            patch: th.optional("patch"),
            replace: th.optional("replace"),
            _unused_keys: {
                let mut keys = BTreeSet::new();
                for (key, _) in th.into_remaining() {
                    keys.insert(key.name.to_owned());
                }
                keys
            },
        };
        Ok(manifest)
    }
}
