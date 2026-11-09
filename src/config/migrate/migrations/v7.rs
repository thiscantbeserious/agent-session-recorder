//! v6 -> v7: `sync.backend` defaults to `gist`, `"off"` disables sync.
//!
//! A missing `backend` key is filled from `Config::default()` after migrations run,
//! an existing `"gist"` or `"off"` is left alone. The only rewrite is dropping the
//! commented `# backend = "gist"` template that v6 wrote.

use toml_edit::Table;

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 7;

pub fn migrate(_root: &mut Table, _result: &mut MigrateResult) {}

/// Drop the `# backend = "gist"` line v6 wrote. Comments are not reachable through the
/// table, so this works on text, and only runs for configs older than v7.
pub fn strip_v6_template(content: &str) -> String {
    let mut out: String = content
        .lines()
        .filter(|line| line.trim() != "# backend = \"gist\"")
        .collect::<Vec<_>>()
        .join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::Config;

    const V6: &str = "config_version = 6\n[sync]\nauto_push = true\n# backend = \"gist\"\n";

    #[test]
    fn v7_adds_backend_gist_and_drops_the_template() {
        let result = migrate_config(V6).unwrap();
        assert!(result.content.contains("backend = \"gist\""));
        assert!(!result.content.contains("# backend"), "{}", result.content);
        assert!(result.added_fields.contains(&"sync.backend".to_string()));
    }

    #[test]
    fn v7_keeps_off_and_drops_the_template() {
        let input = "config_version = 6\n[sync]\nbackend = \"off\"\n";
        let result = migrate_config(input).unwrap();
        assert!(result.content.contains("backend = \"off\""));
        assert!(!result.content.contains("\"gist\""), "{}", result.content);
        assert!(!result.added_fields.contains(&"sync.backend".to_string()));
    }

    #[test]
    fn v7_template_in_a_middle_section_is_dropped() {
        let input = "config_version = 6\n[sync]\nauto_push = true\n# backend = \"gist\"\n\n[shell]\nauto_wrap = true\n";
        let result = migrate_config(input).unwrap();
        assert!(!result.content.contains("# backend"), "{}", result.content);
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert!(parsed.shell.auto_wrap);
    }

    #[test]
    fn v7_is_idempotent() {
        let once = migrate_config(V6).unwrap();
        let twice = migrate_config(&once.content).unwrap();
        assert_eq!(once.content, twice.content);
        assert!(!twice.has_changes());
    }
}
