//! Embedded AI agent skills
//!
//! This module provides skills that can be installed to various AI agent
//! command directories (Claude, Codex, Gemini) for use during sessions.

use std::path::PathBuf;

/// Embedded skill: agr-analyze.md
pub const SKILL_ANALYZE: &str = include_str!("../agents/agr-analyze.md");

/// Embedded skill: agr-review.md
pub const SKILL_REVIEW: &str = include_str!("../agents/agr-review.md");

/// All available skills as (filename, content) pairs
pub const SKILLS: &[(&str, &str)] = &[
    ("agr-analyze.md", SKILL_ANALYZE),
    ("agr-review.md", SKILL_REVIEW),
];

/// Get skill directories for different AI agents
pub fn skill_directories() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        home.join(".claude/commands"),
        home.join(".codex/commands"),
        home.join(".gemini/commands"),
    ]
}

/// Information about an installed skill
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    /// Skill filename
    pub name: String,
    /// Path where installed
    pub path: PathBuf,
    /// Whether the installed content matches the embedded version
    pub matches_embedded: bool,
}

/// List installed skills across all agent directories
pub fn list_installed_skills() -> Vec<InstalledSkill> {
    let mut installed = Vec::new();

    for dir in skill_directories() {
        for (name, embedded_content) in SKILLS {
            let skill_path = dir.join(name);
            if skill_path.exists() {
                let matches = std::fs::read_to_string(&skill_path)
                    .map(|content| content == *embedded_content)
                    .unwrap_or(false);

                installed.push(InstalledSkill {
                    name: name.to_string(),
                    path: skill_path,
                    matches_embedded: matches,
                });
            }
        }
    }

    installed
}

/// Install skills to all agent command directories
/// Returns a list of paths where skills were installed
pub fn install_skills() -> std::io::Result<Vec<PathBuf>> {
    let mut installed = Vec::new();

    for dir in skill_directories() {
        // Create directory if it doesn't exist
        std::fs::create_dir_all(&dir)?;

        for (name, content) in SKILLS {
            let skill_path = dir.join(name);
            std::fs::write(&skill_path, content)?;
            installed.push(skill_path);
        }
    }

    Ok(installed)
}

/// Uninstall agr skills from all agent command directories
/// Returns a list of paths where skills were removed
pub fn uninstall_skills() -> std::io::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();

    for dir in skill_directories() {
        for (name, _) in SKILLS {
            let skill_path = dir.join(name);
            if skill_path.exists() {
                // Check if it's a symlink or regular file - remove either way
                std::fs::remove_file(&skill_path)?;
                removed.push(skill_path);
            }
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_skills_are_embedded() {
        // Verify skills are not empty
        assert!(!SKILL_ANALYZE.is_empty());
        assert!(!SKILL_REVIEW.is_empty());

        // Verify skills contain expected content
        assert!(SKILL_ANALYZE.contains("agr-analyze"));
        assert!(SKILL_REVIEW.contains("agr-review"));
    }

    #[test]
    fn test_skills_array() {
        assert_eq!(SKILLS.len(), 2);
        assert_eq!(SKILLS[0].0, "agr-analyze.md");
        assert_eq!(SKILLS[1].0, "agr-review.md");
    }

    #[test]
    fn test_skill_directories_returns_paths() {
        let dirs = skill_directories();
        assert_eq!(dirs.len(), 3);

        // All should be under home directory
        let home = dirs::home_dir().unwrap();
        for dir in &dirs {
            assert!(dir.starts_with(&home));
        }
    }

    #[test]
    fn test_install_and_uninstall_skills() -> std::io::Result<()> {
        // Create a temp directory to simulate home
        let temp = TempDir::new()?;
        let temp_path = temp.path();

        // Create test skill directories
        let claude_dir = temp_path.join(".claude/commands");
        let codex_dir = temp_path.join(".codex/commands");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&codex_dir)?;

        // Write a skill file
        let skill_path = claude_dir.join("agr-analyze.md");
        std::fs::write(&skill_path, SKILL_ANALYZE)?;

        // Verify it exists
        assert!(skill_path.exists());

        // Read back and verify content
        let content = std::fs::read_to_string(&skill_path)?;
        assert_eq!(content, SKILL_ANALYZE);

        // Clean up by removing
        std::fs::remove_file(&skill_path)?;
        assert!(!skill_path.exists());

        Ok(())
    }
}
