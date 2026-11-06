//! `agr sync key export|import`.

use std::io::BufRead;

use anyhow::{bail, Result};

use agr::cli::SyncKeyCommands;
use agr::sync::key::Imported;
use agr::sync::setup::{default_key_path, export_key, import_key};
use agr::theme::current_theme;

#[cfg(not(tarpaulin_include))]
pub fn handle(cmd: SyncKeyCommands) -> Result<()> {
    match cmd {
        SyncKeyCommands::Export => export(),
        SyncKeyCommands::Import { phrase, force } => import(phrase, force),
    }
}

fn export() -> Result<()> {
    let theme = current_theme();
    let Some(phrase) = export_key(&default_key_path()?)? else {
        bail!("No sync key on this machine, run `agr sync push` first or import one.");
    };
    println!(
        "{}",
        theme.primary_text(
            "Recovery phrase, keep it safe, anyone holding it can read your synced sessions:"
        )
    );
    println!("  {}", theme.accent_text(&phrase));
    println!(
        "{}",
        theme.secondary_text("Import it on another machine with `agr sync key import`.")
    );
    Ok(())
}

fn import(phrase: Option<String>, force: bool) -> Result<()> {
    let phrase = match phrase {
        Some(phrase) => phrase,
        None => read_stdin_line()?,
    };
    let done = match import_key(&default_key_path()?, &phrase, force)? {
        Imported::Created => "Sync key imported.",
        Imported::Same => "This sync key is already installed.",
        Imported::Replaced => "Sync key replaced.",
    };
    println!("{}", current_theme().success_text(done));
    Ok(())
}

fn read_stdin_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}
