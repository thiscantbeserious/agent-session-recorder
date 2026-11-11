//! The prompts of `agr sync delete`, the terminal decision and the reader are passed in.

use std::io::{self, BufRead, Write};

use crate::theme::current_theme;

/// Above this many matches the count has to be typed, or `--force` given.
pub const BIG: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Go,
    Declined,
    /// Over `BIG` matches without a terminal and without `--force`.
    NeedForce,
}

pub struct Flags {
    pub yes: bool,
    pub force: bool,
    pub tty: bool,
}

fn ask(out: &mut impl Write, input: &mut impl BufRead, text: &str) -> io::Result<String> {
    write!(out, "{} ", current_theme().primary_text(text))?;
    out.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(line.trim().to_lowercase())
}

/// Yes or no (unless `yes`), then on a terminal above `BIG` matches the typed count.
/// `force` only lifts the abort without a terminal, it never skips the typed count.
pub fn confirm_delete(
    count: usize,
    flags: &Flags,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> io::Result<Confirm> {
    let big = count > BIG;
    if big && !flags.force && !flags.tty {
        return Ok(Confirm::NeedForce);
    }
    if !flags.yes {
        if !flags.tty {
            let hint = "Non-interactive mode: use --yes to apply changes automatically";
            writeln!(out, "{}", current_theme().secondary_text(hint))?;
            return Ok(Confirm::Declined);
        }
        let question = "Mark these remote sessions deleted? Their encrypted bytes stay in the remote pack and the local files stay. [y/N]";
        let answer = ask(out, input, question)?;
        if answer != "y" && answer != "yes" {
            return Ok(Confirm::Declined);
        }
    }
    if big
        && flags.tty
        && ask(out, input, &format!("Type {count} to confirm:"))? != count.to_string()
    {
        return Ok(Confirm::Declined);
    }
    Ok(Confirm::Go)
}

/// One yes or no before migrate deletes remote gists, `--yes` skips it, no terminal never asks.
pub fn confirm_migrate(
    flags: &Flags,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> io::Result<Confirm> {
    if flags.yes {
        return Ok(Confirm::Go);
    }
    if !flags.tty {
        let hint = "Non-interactive mode: use --yes to apply changes automatically";
        writeln!(out, "{}", current_theme().secondary_text(hint))?;
        return Ok(Confirm::Declined);
    }
    let question =
        "Move these sessions into packs and delete the old gists? Local files stay. [y/N]";
    let answer = ask(out, input, question)?;
    let go = answer == "y" || answer == "yes";
    Ok(if go { Confirm::Go } else { Confirm::Declined })
}
