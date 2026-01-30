//! Shell integration management for AGR
//!
//! This module handles installing and uninstalling shell integration
//! to .zshrc and .bashrc files using marked sections.

pub mod completions;
pub mod install;
pub mod minify;
pub mod paths;
pub mod status;

// minify.rs - use as minify::exec() or minify::debug()

// Re-export public items for backward compatibility
// paths.rs
pub use paths::{
    all_shell_rcs, bash_completion_path, default_script_path, detect_shell_rc, zsh_completion_path,
};

// status.rs
pub use status::{
    extract_script_path, find_installed_rc, get_status, is_installed_in, ShellStatus, MARKER_END,
    MARKER_START,
};

// install.rs
pub use install::{
    generate_section, install, install_script, uninstall, Shell, MARKER_WARNING, SHELL_SCRIPT,
};

// completions.rs
pub use completions::{
    cleanup_old_completions, extract_commands, generate_bash_init, generate_zsh_init, CommandInfo,
};
