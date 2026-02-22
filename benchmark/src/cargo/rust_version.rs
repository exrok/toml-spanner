//! Extracted from https://github.com/rust-lang/cargo for benchmarking the
//! snapshot was taking at 2026-02-22 from 3ca292befbc3585084922c1592ea3d17e423f035
//!
//! References Files:
//! crates/cargo-util-schemas/src/manifest/rust_version.rs
//!
//! Copyright remains with the original authors of Cargo, licensed under the MIT License or Apache License (at your option).

use std::fmt;
use std::fmt::Display;

use serde_untagged::UntaggedEnumVisitor;

use super::partial_version::PartialVersion;
use super::partial_version::PartialVersionError;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct RustVersion {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

impl RustVersion {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor: Some(minor),
            patch: Some(patch),
        }
    }

    pub fn is_compatible_with(&self, rustc: &PartialVersion) -> bool {
        let msrv = self.to_partial().to_caret_req();
        // Remove any pre-release identifiers for easier comparison
        let rustc = semver::Version {
            major: rustc.major,
            minor: rustc.minor.unwrap_or_default(),
            patch: rustc.patch.unwrap_or_default(),
            pre: Default::default(),
            build: Default::default(),
        };
        msrv.matches(&rustc)
    }

    pub fn to_partial(&self) -> PartialVersion {
        let Self {
            major,
            minor,
            patch,
        } = *self;
        PartialVersion {
            major,
            minor,
            patch,
            pre: None,
            build: None,
        }
    }
}

impl std::str::FromStr for RustVersion {
    type Err = RustVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let partial = value.parse::<PartialVersion>();
        let partial = partial.map_err(RustVersionErrorKind::PartialVersion)?;
        partial.try_into()
    }
}

impl TryFrom<semver::Version> for RustVersion {
    type Error = RustVersionError;

    fn try_from(version: semver::Version) -> Result<Self, Self::Error> {
        let version = PartialVersion::from(version);
        Self::try_from(version)
    }
}

impl TryFrom<PartialVersion> for RustVersion {
    type Error = RustVersionError;

    fn try_from(partial: PartialVersion) -> Result<Self, Self::Error> {
        let PartialVersion {
            major,
            minor,
            patch,
            pre,
            build,
        } = partial;
        if pre.is_some() {
            return Err(RustVersionErrorKind::Prerelease.into());
        }
        if build.is_some() {
            return Err(RustVersionErrorKind::BuildMetadata.into());
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl serde::Serialize for RustVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for RustVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        UntaggedEnumVisitor::new()
            .expecting("SemVer version")
            .string(|value| value.parse().map_err(serde::de::Error::custom))
            .deserialize(deserializer)
    }
}

impl Display for RustVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_partial().fmt(f)
    }
}

/// Error parsing a [`RustVersion`].
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct RustVersionError(#[from] RustVersionErrorKind);

/// Non-public error kind for [`RustVersionError`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
enum RustVersionErrorKind {
    #[error("unexpected prerelease field, expected a version like \"1.32\"")]
    Prerelease,

    #[error("unexpected build field, expected a version like \"1.32\"")]
    BuildMetadata,

    #[error(transparent)]
    PartialVersion(#[from] PartialVersionError),
}
