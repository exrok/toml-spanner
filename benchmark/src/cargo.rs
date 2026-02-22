//! Standalone `Cargo.lock` deserialization.
//!
//! This crate provides the core TOML deserialization types for `Cargo.lock` files,
//! duplicated from `cargo-util-schemas` for external testing purposes.
//!
//! # Example
//!
//! ```rust
//! let lockfile_toml = r#"
//! version = 4
//!
//! [[package]]
//! name = "my-package"
//! version = "0.1.0"
//! "#;
//!
//! let lockfile: cargo_toml_files::lockfile::TomlLockfile = toml::from_str(lockfile_toml).unwrap();
//! assert_eq!(lockfile.version, Some(4));
//! ```
#![allow(
    unused,
    reason = "A lot of code is copied almost directly out of the cargo source, lets keep it that way"
)]
pub mod lockfile;
pub mod manifest;
pub mod package_id_spec;
pub mod partial_version;
pub mod restricted_names;
pub mod rust_version;
pub mod source_kind;

/// Parse a `Cargo.lock` TOML string into a [`lockfile::TomlLockfile`].
pub fn parse_lock_serde_toml(s: &str) -> Result<lockfile::TomlLockfile, toml::de::Error> {
    toml::from_str(s)
}

/// Parse a `Cargo.lock` TOML string into a [`lockfile::TomlLockfile`] using `toml-spanner`.
pub fn parse_lock_toml_spanner(
    s: &str,
) -> Result<lockfile::TomlLockfile, Vec<toml_spanner::Error>> {
    let arena = toml_spanner::Arena::new();
    let mut root = toml_spanner::parse(s, &arena).map_err(|e| vec![e])?;
    match root.deserialize::<lockfile::TomlLockfile>() {
        Ok(lockfile) if !root.has_errors() => Ok(lockfile),
        Ok(_) => Err(root.ctx.errors),
        Err(_) => Err(root.ctx.errors),
    }
}

/// Parse a `Cargo.lock` TOML string into a [`lockfile::TomlLockfile`] using `toml-span`.
pub fn parse_lock_toml_span(s: &str) -> Result<lockfile::TomlLockfile, toml_span::DeserError> {
    let mut value = toml_span::parse(s).map_err(|e| toml_span::DeserError { errors: vec![e] })?;
    toml_span::Deserialize::deserialize(&mut value)
}

/// Parse a `Cargo.toml` TOML string into a [`manifest::TomlManifest`].
pub fn parse_manifest_serde_toml(s: &str) -> Result<manifest::TomlManifest, toml::de::Error> {
    toml::from_str(s)
}

/// Parse a `Cargo.toml` TOML string into a [`manifest::TomlManifest`] using `toml-spanner`.
pub fn parse_manifest_toml_spanner(
    s: &str,
) -> Result<manifest::TomlManifest, Vec<toml_spanner::Error>> {
    let arena = toml_spanner::Arena::new();
    let mut root = toml_spanner::parse(s, &arena).map_err(|e| vec![e])?;
    match root.deserialize::<manifest::TomlManifest>() {
        Ok(manifest) if !root.has_errors() => Ok(manifest),
        Ok(_) => Err(root.ctx.errors),
        Err(_) => Err(root.ctx.errors),
    }
}
