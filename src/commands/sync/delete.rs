//! `agr sync delete`: collect the matches, preview, ask, then one tombstone append.

use std::io;

use anyhow::Result;

use agr::sync::format::{self, Live};
use agr::sync::setup::{keep_writer, KeyNeed};
use agr::sync::{select_remote, Report};

use super::run::{abort, engine_failed, finish, prepare, say};
use agr::sync::confirm::{confirm_delete, Confirm, Flags};

pub struct Args {
    pub patterns: Vec<String>,
    pub all: bool,
    pub force: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub verbose: bool,
}

/// `claude/a.cast` is the same session as `claude/a.cast.gz`, other patterns stay as typed.
fn normalise(pattern: &str) -> String {
    let literal = !pattern.contains(['*', '?']);
    if literal && pattern.ends_with(".cast") {
        format!("{pattern}.gz")
    } else {
        pattern.to_string()
    }
}

/// The remote sessions the patterns select, as a report of only those items.
fn collect(plan: Report, args: &Args) -> Report {
    let remote: Vec<String> = plan
        .items
        .iter()
        .filter(|i| i.on_remote && !i.path.starts_with("remote:"))
        .map(|i| i.path.clone())
        .collect();
    let patterns: Vec<String> = args.patterns.iter().map(|p| normalise(p)).collect();
    let hits =
        select_remote(&patterns, args.all, &remote).unwrap_or_else(|e| abort(&e.to_string(), None));
    let items = plan.items.into_iter().filter(|i| hits.contains(&i.path));
    Report {
        items: items.collect(),
        ..Report::default()
    }
}

/// The prompts, the terminal decision comes from the entry point.
fn confirmed(count: usize, args: &Args, tty: bool) -> Result<bool> {
    let flags = Flags {
        yes: args.yes,
        force: args.force,
        tty,
    };
    let (stdin, mut out) = (io::stdin(), io::stdout());
    match confirm_delete(count, &flags, &mut stdin.lock(), &mut out)? {
        Confirm::Go => Ok(true),
        Confirm::Declined => Ok(false),
        Confirm::NeedForce => abort(
            &format!("{count} sessions need --force without a terminal"),
            None,
        ),
    }
}

pub fn delete(args: &Args, tty: bool) -> Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    let load = Live::stderr("", "deleted");
    let plan = ctx
        .engine()
        .status_with(&[], &mut load.loader())
        .unwrap_or_else(|e| engine_failed(e));
    load.end();
    let matched = collect(plan, args);
    say(&if args.verbose {
        format::delete_preview_lines(&matched)
    } else {
        format::compact_delete_preview_lines(&matched)
    });
    if args.dry_run {
        return Ok(());
    }
    if !confirmed(matched.items.len(), args, tty)? {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    keep_writer(&ctx.writer)?;
    let paths: Vec<String> = matched.items.iter().map(|i| i.path.clone()).collect();
    let live = Live::stderr("", "deleted");
    let mut observer = live.observer();
    let result = ctx.engine().delete_with(&paths, paths.len(), &mut observer);
    live.end();
    finish(result, "delete", args.verbose)
}
