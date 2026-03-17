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
    let mut doc = toml_spanner::parse(s, &arena).map_err(|e| vec![e])?;
    match doc.to::<lockfile::TomlLockfile>() {
        Ok(lockfile) if !doc.has_errors() => Ok(lockfile),
        Ok(_) => Err(doc.ctx.errors.into_iter().collect()),
        Err(_) => Err(doc.ctx.errors.into_iter().collect()),
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
    let mut doc = toml_spanner::parse(s, &arena).map_err(|e| vec![e])?;
    match doc.to::<manifest::TomlManifest>() {
        Ok(manifest) if !doc.has_errors() => Ok(manifest),
        Ok(_) => Err(doc.ctx.errors.into_iter().collect()),
        Err(_) => Err(doc.ctx.errors.into_iter().collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_input;

    #[test]
    fn lock_parsing_all_agree() {
        let serde = parse_lock_serde_toml(static_input::ZED_CARGO_LOCK).unwrap();
        let spanner = parse_lock_toml_spanner(static_input::ZED_CARGO_LOCK).unwrap();
        let span = parse_lock_toml_span(static_input::ZED_CARGO_LOCK).unwrap();
        let span_str = format!("{span:#?}");
        let serde_str = format!("{serde:#?}");
        let spanner_str = format!("{spanner:#?}");
        assert_eq!(serde_str, spanner_str);
        assert_eq!(serde_str, span_str);
    }

    #[test]
    fn manifest_parsing_all_agree() {
        let serde = parse_manifest_serde_toml(static_input::ZED_CARGO_TOML).unwrap();
        let spanner = match parse_manifest_toml_spanner(static_input::ZED_CARGO_TOML) {
            Ok(v) => v,
            Err(errs) => {
                for err in errs {
                    eprintln!(
                        "Error: {err}, {}",
                        &static_input::ZED_CARGO_TOML[err.span().range()]
                    );
                }
                panic!()
            }
        };
        let serde_str = format!("{serde:#?}");
        let spanner_str = format!("{spanner:#?}");
        assert_eq!(serde_str, spanner_str);
    }
}
