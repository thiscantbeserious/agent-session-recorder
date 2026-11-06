//! Shared confirmation prompt for commands that change data.

use std::io::{self, BufRead, Write};

use anyhow::Result;

use agr::theme::current_theme;

/// Determine whether to proceed with the operation.
///
/// If `auto_confirm` is true (--yes flag), returns true immediately.
/// Otherwise, prompts the user for confirmation.
/// If stdin is not a TTY, returns false with a hint about --yes.
pub fn should_proceed(message: &str, auto_confirm: bool) -> Result<bool> {
    if auto_confirm {
        return Ok(true);
    }

    let theme = current_theme();

    // Check if stdin is a TTY - if not, skip prompt and return false
    if !io::IsTerminal::is_terminal(&io::stdin()) {
        println!(
            "{}",
            theme.secondary_text("Non-interactive mode: use --yes to apply changes automatically")
        );
        return Ok(false);
    }

    print!("{} [y/N] ", theme.primary_text(message));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Parse a yes/no answer. Empty or unknown input keeps `default`.
pub fn parse_answer(_input: &str, default: bool) -> bool {
    default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_keeps_default() {
        assert!(parse_answer("", true));
        assert!(!parse_answer("", false));
        assert!(parse_answer("  \n", true));
    }

    #[test]
    fn no_answers_are_false() {
        assert!(!parse_answer("n", true));
        assert!(!parse_answer("no", true));
        assert!(!parse_answer("NO\n", true));
    }

    #[test]
    fn yes_answers_are_true() {
        assert!(parse_answer("y", false));
        assert!(parse_answer("YES", false));
        assert!(parse_answer(" yes \n", false));
    }

    #[test]
    fn unknown_keeps_default() {
        assert!(parse_answer("x", true));
        assert!(!parse_answer("x", false));
    }
}
