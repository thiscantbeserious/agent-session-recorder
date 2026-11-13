//! `agr update` handler: prints, and maps the check result to the exit code.

use agr::update::{self, Outcome, RELEASES_URL};
use anyhow::Result;

/// Exit code of `agr update --check` when an update is available.
const EXIT_UPDATE_AVAILABLE: i32 = 10;

#[cfg(not(tarpaulin_include))]
pub fn handle(check: bool) -> Result<()> {
    if check {
        return handle_check();
    }
    match update::run()? {
        Outcome::UpToDate(v) => println!("agr {v} is up to date."),
        Outcome::Updated { from, to } => println!("Updated agr {from} to {to}."),
    }
    Ok(())
}

fn handle_check() -> Result<()> {
    let r = update::check()?;
    println!("Current: {}\nLatest:  {}", r.current, r.latest);
    if !r.update_available() {
        if r.latest < r.current {
            println!("Running build is newer than the latest release.");
        } else {
            println!("agr is up to date.");
        }
        return Ok(());
    }
    println!("update available");
    if !r.verifiable {
        println!(
            "Release {} cannot be applied automatically because it has no checksums. \
             Install it manually from {RELEASES_URL}.",
            r.tag
        );
    }
    std::process::exit(EXIT_UPDATE_AVAILABLE);
}
