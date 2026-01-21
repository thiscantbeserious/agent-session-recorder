//! Build script for AGR - embeds git commit hash and build info
//!
//! When the `release` feature is NOT set (default dev builds):
//! - Emits `VERGEN_GIT_SHA` environment variable with the commit hash
//! - Emits `AGR_BUILD_DATE` environment variable with the build date
//!
//! When the `release` feature IS set (CI/official builds):
//! - Emits build date only (clean version string without git hash)

use std::process::Command;

/// Get the current date in YYYY-MM-DD format
fn get_build_date() -> String {
    // Use the date command for cross-platform compatibility
    if let Ok(output) = Command::new("date").args(["+%Y-%m-%d"]).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    // Fallback for systems where date command differs
    "unknown".to_string()
}

/// Get the repository name in "owner/repo" format from git remote
fn get_repo_name() -> String {
    // Try to get the remote URL from git
    if let Ok(output) = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Parse owner/repo from various URL formats:
            // - https://github.com/owner/repo.git
            // - https://github.com/owner/repo
            // - git@github.com:owner/repo.git
            // - git@github.com:owner/repo
            if let Some(repo) = parse_repo_from_url(&url) {
                return repo;
            }
        }
    }
    // Fallback to package repository from Cargo.toml
    "thiscantbeserious/agent-session-recorder".to_string()
}

/// Parse owner/repo from a git remote URL
fn parse_repo_from_url(url: &str) -> Option<String> {
    // Remove .git suffix if present
    let url = url.trim_end_matches(".git");

    if url.contains("github.com") || url.contains("gitlab.com") || url.contains("bitbucket.org") {
        // HTTPS format: https://github.com/owner/repo
        if let Some(path) = url
            .split('/')
            .collect::<Vec<_>>()
            .get(3..)
            .map(|parts| parts.join("/"))
        {
            if !path.is_empty() {
                return Some(path);
            }
        }
        // SSH format: git@github.com:owner/repo
        if let Some(colon_pos) = url.find(':') {
            let path = &url[colon_pos + 1..];
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

fn main() {
    // Always emit repo name and build date
    let repo_name = get_repo_name();
    let build_date = get_build_date();
    println!("cargo:rustc-env=AGR_REPO_NAME={}", repo_name);
    println!("cargo:rustc-env=AGR_BUILD_DATE={}", build_date);

    // Only emit git SHA when NOT building with --features release
    #[cfg(not(feature = "release"))]
    {
        use vergen_gitcl::{Emitter, GitclBuilder};

        // Configure git info - we need the SHA
        // Use graceful fallback if git info is unavailable
        let git_result = GitclBuilder::default().sha(true).build();

        let emit_result = match git_result {
            Ok(git) => Emitter::default()
                .add_instructions(&git)
                .and_then(|emitter| emitter.emit()),
            Err(e) => {
                eprintln!("cargo:warning=Failed to configure git info: {}", e);
                println!("cargo:rustc-env=VERGEN_GIT_SHA=unknown");
                return;
            }
        };

        if let Err(e) = emit_result {
            // If git info fails (e.g., not in a git repo), emit fallback value
            eprintln!("cargo:warning=Failed to get git info: {}", e);
            println!("cargo:rustc-env=VERGEN_GIT_SHA=unknown");
        }
    }

    // For release builds, no git SHA is emitted (clean version string)
}
