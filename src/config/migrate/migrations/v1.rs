//! v0 → v1: Restructure analysis config layout.
//!
//! Moves:
//! - `[recording].analysis_agent` → `[analysis].agent`
//! - `[analysis].default_agent`  → `[analysis].agent`
//! - `[analysis.agents.*]`       → `[agents.*]`

use toml_edit::{Item, Table};

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 1;

pub fn migrate(root: &mut Table, result: &mut MigrateResult) {
    move_recording_analysis_agent(root, result);
    move_analysis_default_agent(root, result);
    move_analysis_agents_to_top_level(root, result);
}

/// `[recording].analysis_agent` → `[analysis].agent`
fn move_recording_analysis_agent(root: &mut Table, result: &mut MigrateResult) {
    if let Some(recording) = root.get_mut("recording").and_then(|i| i.as_table_mut()) {
        if let Some(old_value) = recording.remove("analysis_agent") {
            result
                .removed_fields
                .push("recording.analysis_agent".to_string());

            let analysis = root
                .entry("analysis")
                .or_insert_with(|| Item::Table(Table::new()))
                .as_table_mut();
            if let Some(analysis) = analysis {
                if !analysis.contains_key("agent") {
                    analysis.insert("agent", old_value);
                }
            }
        }
    }
}

/// `[analysis].default_agent` → `[analysis].agent`
fn move_analysis_default_agent(root: &mut Table, result: &mut MigrateResult) {
    if let Some(analysis) = root.get_mut("analysis").and_then(|i| i.as_table_mut()) {
        if let Some(old_value) = analysis.remove("default_agent") {
            result
                .removed_fields
                .push("analysis.default_agent".to_string());

            if !analysis.contains_key("agent") {
                analysis.insert("agent", old_value);
            }
        }
    }
}

/// `[analysis.agents.*]` → `[agents.*]`
fn move_analysis_agents_to_top_level(root: &mut Table, result: &mut MigrateResult) {
    if let Some(analysis) = root.get_mut("analysis").and_then(|i| i.as_table_mut()) {
        if let Some(old_agents) = analysis.remove("agents") {
            if let Some(old_agents_table) = old_agents.as_table() {
                let agents = root
                    .entry("agents")
                    .or_insert_with(|| Item::Table(Table::new()))
                    .as_table_mut();
                if let Some(agents) = agents {
                    for (agent_name, agent_config) in old_agents_table.iter() {
                        if !agents.contains_key(agent_name) {
                            agents.insert(agent_name, agent_config.clone());
                        }
                        result
                            .removed_fields
                            .push(format!("analysis.agents.{}", agent_name));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use toml_edit::DocumentMut;

    use crate::config::migrate::migrate_config;
    use crate::Config;

    #[test]
    fn recording_analysis_agent_migrates() {
        let input = r#"
[recording]
auto_analyze = false
analysis_agent = "codex"
"#;
        let result = migrate_config(input).unwrap();

        assert!(result
            .removed_fields
            .contains(&"recording.analysis_agent".to_string()));

        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.analysis.agent, Some("codex".to_string()));
    }

    #[test]
    fn analysis_default_agent_migrates() {
        let input = r#"
[analysis]
default_agent = "gemini"
"#;
        let result = migrate_config(input).unwrap();

        assert!(result
            .removed_fields
            .contains(&"analysis.default_agent".to_string()));

        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.analysis.agent, Some("gemini".to_string()));
    }

    #[test]
    fn analysis_agents_move_to_top_level() {
        let input = r#"
[agents]
enabled = ["claude"]

[analysis.agents.codex]
extra_args = ["--model", "gpt-5.2-codex"]
"#;
        let result = migrate_config(input).unwrap();

        assert!(result
            .removed_fields
            .contains(&"analysis.agents.codex".to_string()));

        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.agents.codex.extra_args,
            vec!["--model", "gpt-5.2-codex"]
        );
    }

    #[test]
    fn existing_target_values_preserved() {
        let input = r#"
[recording]
analysis_agent = "codex"

[analysis]
agent = "gemini"
"#;
        let result = migrate_config(input).unwrap();

        assert!(result
            .removed_fields
            .contains(&"recording.analysis_agent".to_string()));

        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.analysis.agent, Some("gemini".to_string()));
    }

    #[test]
    fn removed_fields_not_in_output() {
        let input = r#"
[recording]
auto_analyze = false
analysis_agent = "codex"

[analysis]
default_agent = "codex"

[analysis.agents.codex]
extra_args = ["--model", "gpt-5.2-codex"]
"#;
        let result = migrate_config(input).unwrap();
        let doc: DocumentMut = result.content.parse().unwrap();

        // recording.analysis_agent should be gone
        let recording = doc.as_table().get("recording").unwrap().as_table().unwrap();
        assert!(!recording.contains_key("analysis_agent"));

        // analysis.default_agent should be gone
        let analysis = doc.as_table().get("analysis").unwrap().as_table().unwrap();
        assert!(!analysis.contains_key("default_agent"));
        assert!(!analysis.contains_key("agents"));
    }
}
