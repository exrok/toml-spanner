use proc_macro2 as proc_macro;
include!("../../toml-spanner-macros/src/case.rs");

#[cfg(feature = "debug")]
impl std::fmt::Debug for RenameRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            RenameRule::None => "None",
            RenameRule::LowerCase => "LowerCase",
            RenameRule::UpperCase => "UpperCase",
            RenameRule::PascalCase => "PascalCase",
            RenameRule::CamelCase => "CamelCase",
            RenameRule::SnakeCase => "SnakeCase",
            RenameRule::ScreamingSnakeCase => "ScreamingSnakeCase",
            RenameRule::KebabCase => "KebabCase",
            RenameRule::ScreamingKebabCase => "ScreamingKebabCase",
        };
        f.write_str(name)
    }
}
