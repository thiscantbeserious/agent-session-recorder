//! Config field documentation — single source of truth for descriptions.
//!
//! Used by:
//! - `agr config show` to annotate TOML output with inline comments
//! - `cargo xtask gen-docs` to generate `wiki/Configuration.md`

mod annotate;
mod fields;
mod markdown;
mod templates;

pub use annotate::annotate_config;
pub use fields::{AGENT_FIELDS, CONFIG_SECTIONS};
pub use markdown::generate_config_markdown;
pub use templates::insert_optional_field_templates;

/// Documentation for a config section.
pub struct SectionDoc {
    /// TOML section name (e.g., "shell", "agents")
    pub name: &'static str,
    /// Human-readable description of the section
    pub description: &'static str,
    /// Fields in this section
    pub fields: &'static [FieldDoc],
}

/// Documentation for a config field.
pub struct FieldDoc {
    /// Field name as it appears in TOML
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Default value as a display string
    pub default_display: &'static str,
    /// Valid TOML value for the commented template line (falls back to `default_display`)
    pub example: Option<&'static str>,
}
