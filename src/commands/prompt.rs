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
    if !atty::is(atty::Stream::Stdin) {
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
