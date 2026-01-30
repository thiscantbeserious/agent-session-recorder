//! Shell completion and initialization code generation
//!
//! This module handles generating dynamic shell initialization code with
//! embedded completions. Completions are dynamically generated from clap
//! command definitions rather than static files.

use std::fs;
use std::io;

use clap::CommandFactory;

use super::minify;
use super::paths::{bash_completion_path, zsh_completion_path};
use crate::cli::Cli;

// ============================================================================
// Legacy completion file cleanup
// ============================================================================

/// Clean up old completion files from legacy installation
///
/// This removes static completion files that were installed by earlier
/// versions of agr. Completions are now embedded in the RC file section.
pub fn cleanup_old_completions() -> io::Result<()> {
    // Remove old zsh completion file
    if let Some(path) = zsh_completion_path() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }

    // Remove old bash completion file
    if let Some(path) = bash_completion_path() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }

    Ok(())
}

// ============================================================================
// Dynamic shell init code generation (REQ-1)
// ============================================================================

/// Information about a CLI command for completion
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Command name
    pub name: String,
    /// Short description from clap's `about`
    pub description: String,
    /// Whether this command accepts a file argument
    pub accepts_file: bool,
    /// Subcommands (if any)
    pub subcommands: Vec<CommandInfo>,
}

/// Extract command information from clap definitions
///
/// Uses `CommandFactory` to introspect the CLI and extract all subcommands
/// with their descriptions, including nested subcommands.
pub fn extract_commands() -> Vec<CommandInfo> {
    let cmd = Cli::command();
    extract_subcommands(&cmd)
}

fn extract_subcommands(cmd: &clap::Command) -> Vec<CommandInfo> {
    cmd.get_subcommands()
        .filter(|sub| !sub.is_hide_set()) // Skip hidden commands like "completions"
        .map(|sub| CommandInfo {
            name: sub.get_name().to_string(),
            description: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
            accepts_file: has_file_argument(sub),
            subcommands: extract_subcommands(sub),
        })
        .collect()
}

/// Check if a command has a positional "file" argument (dynamic detection from clap)
fn has_file_argument(cmd: &clap::Command) -> bool {
    cmd.get_positionals().any(|arg| arg.get_id() == "file")
}

/// Generate zsh initialization code with embedded completions
///
/// The generated code includes:
/// - Embedded command list from clap (including subcommands)
/// - Multi-layer completion: commands, subcommands, then files
/// - Zsh-specific completion widgets and compdef
///
/// When `debug` is true, outputs readable formatted code with comments.
/// Note: `_AGR_LOADED=1` marker is set in agr.sh, not here.
/// When `debug` is false (default), outputs aggressively minified code.
pub fn generate_zsh_init(debug: bool) -> String {
    let commands = extract_commands();

    // Build command array entries with descriptions for _describe
    let cmd_entries: Vec<String> = commands
        .iter()
        .map(|c| {
            let desc = c.description.replace('\'', "'\\''"); // Escape single quotes
            format!("'{}:{}'", c.name, desc)
        })
        .collect();
    let cmd_array = cmd_entries.join(" ");

    // Build subcommand arrays for commands that have them
    let mut subcmd_arrays = String::new();
    let mut subcmd_cases = String::new();

    for cmd in &commands {
        if !cmd.subcommands.is_empty() {
            // Generate array for this command's subcommands
            let sub_entries: Vec<String> = cmd
                .subcommands
                .iter()
                .map(|s| {
                    let desc = s.description.replace('\'', "'\\''");
                    format!("'{}:{}'", s.name, desc)
                })
                .collect();
            subcmd_arrays.push_str(&format!(
                "_agr_{}_subcmds=({})\n",
                cmd.name.replace('-', "_"),
                sub_entries.join(" ")
            ));

            // Generate case for this command
            subcmd_cases.push_str(&format!(
                "        {}) _describe 'subcommands' _agr_{}_subcmds ;;\n",
                cmd.name,
                cmd.name.replace('-', "_")
            ));
        }
    }

    // File-accepting commands (space-separated)
    let file_cmds: Vec<&str> = commands
        .iter()
        .filter(|c| c.accepts_file)
        .map(|c| c.name.as_str())
        .collect();
    let file_cmds_space = file_cmds.join(" ");

    let raw_output = format!(
        r#"# AGR Shell Integration - Zsh
# Generated by: agr completions --shell-init zsh
_agr_commands=({cmd_array})
_agr_file_cmds="{file_cmds_space}"
{subcmd_arrays}
# Zsh-specific completion setup (skip if sourced by bash for testing)
if [[ -n "$ZSH_VERSION" ]]; then
    # Enable menu selection for completions (Tab cycles through options)
    zstyle ':completion:*:*:agr:*' menu select
    zstyle ':completion:*:*:agr:*' format '%F{{8}}-- %d --%f'

    # Helper: complete with cast files
    _agr_complete_files() {{
        local cur="$1"
        local -a files
        files=(${{(f)"$(agr completions --files --limit 20 "$cur" 2>/dev/null)"}})
        (( $#files )) && _describe 'recordings' files
    }}

    # Multi-layer completion: commands, subcommands, files
    _agr_complete() {{
        local cur="${{words[CURRENT]}}"
        local cmd="${{words[2]}}"
        local subcmd="${{words[3]}}"

        if (( CURRENT == 2 )); then
            _describe 'commands' _agr_commands
        elif (( CURRENT == 3 )); then
            case "$cmd" in
{subcmd_cases}            *) [[ " $_agr_file_cmds " =~ " $cmd " ]] && _agr_complete_files "$cur" ;;
            esac
        elif (( CURRENT >= 4 )); then
            # Position 4+: files for marker add/list, or other file-accepting contexts
            if [[ "$cmd" == "marker" ]]; then
                _agr_complete_files "$cur"
            fi
        fi
    }}

    compdef _agr_complete agr
fi
"#
    );

    // Apply minification (debug=true skips compression)
    if debug {
        minify::debug(&raw_output)
    } else {
        minify::exec(&raw_output)
    }
}

/// Generate bash initialization code with embedded completions
///
/// The generated code includes:
/// - Embedded command list from clap (including subcommands)
/// - Multi-layer completion: commands, subcommands, then files
/// - Bash-specific completion function and complete command
///
/// When `debug` is true, outputs readable formatted code with comments.
/// Note: `_AGR_LOADED=1` marker is set in agr.sh, not here.
/// When `debug` is false (default), outputs aggressively minified code.
pub fn generate_bash_init(debug: bool) -> String {
    let commands = extract_commands();

    // Build simple space-separated command list
    let cmd_names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    let cmd_list = cmd_names.join(" ");

    // File-accepting commands
    let file_cmds: Vec<&str> = commands
        .iter()
        .filter(|c| c.accepts_file)
        .map(|c| c.name.as_str())
        .collect();
    let file_cmd_pattern = file_cmds.join(" ");

    // Build subcommand variables and case statements
    let mut subcmd_vars = String::new();
    let mut subcmd_cases = String::new();

    for cmd in &commands {
        if !cmd.subcommands.is_empty() {
            // Generate variable for this command's subcommands
            let sub_names: Vec<&str> = cmd.subcommands.iter().map(|s| s.name.as_str()).collect();
            subcmd_vars.push_str(&format!(
                "_agr_{}_subcmds=\"{}\"\n",
                cmd.name.replace('-', "_"),
                sub_names.join(" ")
            ));

            // Generate case for this command
            subcmd_cases.push_str(&format!(
                "        {}) COMPREPLY=($(compgen -W \"$_agr_{}_subcmds\" -- \"$cur\")) ;;\n",
                cmd.name,
                cmd.name.replace('-', "_")
            ));
        }
    }

    let raw_output = format!(
        r#"# AGR Shell Integration - Bash
# Generated by: agr completions --shell-init bash
_agr_commands="{cmd_list}"
_agr_file_cmds="{file_cmd_pattern}"
{subcmd_vars}
# Helper: complete with cast files
_agr_complete_files() {{
    local cur="$1"
    local files
    files=$(agr completions --files --limit 20 "$cur" 2>/dev/null)
    COMPREPLY=($(compgen -W "$files" -- "$cur"))
}}

_agr_complete() {{
    local cur="${{COMP_WORDS[COMP_CWORD]}}"
    local cmd="${{COMP_WORDS[1]}}"
    local subcmd="${{COMP_WORDS[2]}}"

    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$_agr_commands" -- "$cur"))
    elif [[ $COMP_CWORD -eq 2 ]]; then
        case "$cmd" in
{subcmd_cases}        *) [[ " $_agr_file_cmds " =~ " $cmd " ]] && _agr_complete_files "$cur" ;;
        esac
    elif [[ $COMP_CWORD -ge 3 ]]; then
        # Position 3+: files for marker add/list, or other file-accepting contexts
        if [[ "$cmd" == "marker" ]]; then
            _agr_complete_files "$cur"
        fi
    fi
}}

complete -F _agr_complete agr
"#
    );

    // Apply minification (debug=true skips compression)
    if debug {
        minify::debug(&raw_output)
    } else {
        minify::exec(&raw_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_commands_returns_visible_commands() {
        let commands = extract_commands();
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();

        // Should include common commands
        assert!(names.contains(&"record"), "Should contain 'record'");
        assert!(names.contains(&"play"), "Should contain 'play'");
        assert!(names.contains(&"list"), "Should contain 'list'");
        assert!(names.contains(&"status"), "Should contain 'status'");

        // Should NOT include hidden commands
        assert!(
            !names.contains(&"completions"),
            "Should not contain hidden 'completions'"
        );
    }

    #[test]
    fn extract_commands_marks_file_accepting() {
        let commands = extract_commands();

        let play = commands.iter().find(|c| c.name == "play");
        assert!(play.is_some());
        assert!(play.unwrap().accepts_file);

        let analyze = commands.iter().find(|c| c.name == "analyze");
        assert!(analyze.is_some());
        assert!(analyze.unwrap().accepts_file);

        let status = commands.iter().find(|c| c.name == "status");
        assert!(status.is_some());
        assert!(!status.unwrap().accepts_file);
    }

    #[test]
    fn generate_zsh_init_contains_commands_array() {
        // Use debug mode for readable output
        let init = generate_zsh_init(true);
        // _AGR_LOADED is now in agr.sh, not completions
        // Check for commands array instead
        assert!(init.contains("_agr_commands="));
    }

    #[test]
    fn generate_zsh_init_contains_commands() {
        // Use debug mode for readable output
        let init = generate_zsh_init(true);
        assert!(init.contains("record"));
        assert!(init.contains("play"));
        assert!(init.contains("status"));
    }

    #[test]
    fn generate_zsh_init_contains_completion_function() {
        // Use debug mode for readable output
        let init = generate_zsh_init(true);
        assert!(init.contains("_agr_complete()"));
        assert!(init.contains("compdef _agr_complete agr"));
    }

    #[test]
    fn generate_bash_init_contains_commands_var() {
        // Use debug mode for readable output
        let init = generate_bash_init(true);
        // _AGR_LOADED is now in agr.sh, not completions
        // Check for commands variable instead
        assert!(init.contains("_agr_commands="));
    }

    #[test]
    fn generate_bash_init_contains_commands() {
        // Use debug mode for readable output
        let init = generate_bash_init(true);
        assert!(init.contains("record"));
        assert!(init.contains("play"));
        assert!(init.contains("status"));
    }

    #[test]
    fn generate_bash_init_contains_completion_function() {
        // Use debug mode for readable output
        let init = generate_bash_init(true);
        assert!(init.contains("_agr_complete()"));
        assert!(init.contains("complete -F _agr_complete agr"));
    }

    #[test]
    fn zsh_init_is_valid_shell_syntax() {
        // Use debug mode for readable output
        let init = generate_zsh_init(true);
        // Basic syntax checks - verify shell variables are properly formed
        assert!(init.contains("${"), "Should have shell variable syntax");
        assert!(
            !init.contains("{cmd_"),
            "Should not have unescaped format placeholders"
        );
        assert!(
            !init.contains("{{"),
            "Should not have double braces (format! escape artifacts)"
        );
    }

    #[test]
    fn bash_init_is_valid_shell_syntax() {
        // Use debug mode for readable output
        let init = generate_bash_init(true);
        // Basic syntax checks - verify shell variables are properly formed
        assert!(init.contains("${"), "Should have shell variable syntax");
        assert!(
            !init.contains("{cmd_"),
            "Should not have unescaped format placeholders"
        );
        assert!(
            !init.contains("{{"),
            "Should not have double braces (format! escape artifacts)"
        );
    }

    #[test]
    fn zsh_init_enables_menu_selection() {
        // Use debug mode for readable output
        let init = generate_zsh_init(true);
        assert!(
            init.contains("menu select"),
            "Should enable menu selection for Tab cycling"
        );
    }

    #[test]
    fn bash_init_uses_complete() {
        // Use debug mode for readable output
        let init = generate_bash_init(true);
        assert!(
            init.contains("complete -F _agr_complete agr"),
            "Should register completion function"
        );
    }
}
