//! status, push, pull and delete, thin layer over `agr::sync::run::Engine`.

use anyhow::Result;

use agr::config::state::State;
use agr::sync::backend::SyncBackend;
use agr::sync::format;
use agr::sync::key;
use agr::sync::run::{Engine, Options, Report, SyncError};
use agr::sync::setup::{
    default_key_path, explain, needs_privacy_ack, open_backend, resolve_key, KeyNeed, KeyState,
    SetupError, IMPORT_HINT,
};
use agr::theme::current_theme;
use agr::{Config, StorageManager};

use crate::commands::prompt::should_proceed;

pub struct Args {
    pub sessions: Vec<String>,
    pub dry_run: bool,
    pub yes: bool,
    pub overwrite: bool,
}

/// Everything a command needs after the preconditions passed.
struct Ctx {
    backend: Box<dyn SyncBackend>,
    storage: StorageManager,
    key: KeyState,
}

impl Ctx {
    fn engine(&self) -> Engine<'_> {
        Engine {
            backend: self.backend.as_ref(),
            key: self.key.key(),
            storage: &self.storage,
        }
    }
}

fn say(lines: &[String]) {
    let theme = current_theme();
    for line in lines {
        let failing = ["Failed", "Warning", "Conflicts"]
            .iter()
            .any(|p| line.starts_with(p));
        let text = if failing {
            theme.error_text(line)
        } else {
            theme.primary_text(line)
        };
        println!("{text}");
    }
}

/// Print an error line and an optional hint on stderr, then exit non-zero.
fn abort(message: &str, hint: Option<&str>) -> ! {
    let theme = current_theme();
    eprintln!("{}", theme.error_text(message));
    if let Some(hint) = hint {
        eprintln!("{}", theme.secondary_text(hint));
    }
    std::process::exit(1)
}

/// Config, backend, tool and login checks, then the key. Changes nothing.
fn prepare(need: KeyNeed) -> Result<Ctx> {
    let config = Config::load()?;
    let test_dir = std::env::var_os("AGR_SYNC_TEST_BACKEND_DIR").map(Into::into);
    let Some(backend) = open_backend(config.sync.backend, test_dir) else {
        abort(
            "Sync is off.",
            Some("Enable it with `[sync] backend = \"gist\"` in ~/.config/agr/config.toml."),
        )
    };
    if let Err(e) = backend.check() {
        let (message, hint) = explain(&e);
        abort(&message, hint.as_deref());
    }
    let key = match resolve_key(&default_key_path()?, backend.as_ref(), need) {
        Ok(key) => key,
        Err(SetupError::NoKey | SetupError::KeyOnRemote) => {
            abort("No sync key on this machine.", Some(IMPORT_HINT))
        }
        Err(SetupError::Backend(e)) => {
            let (message, hint) = explain(&e);
            abort(&message, hint.as_deref())
        }
        Err(e) => abort(&e.to_string(), None),
    };
    let storage = StorageManager::new(config);
    storage.ensure_storage_dir()?;
    Ok(Ctx {
        backend,
        storage,
        key,
    })
}

fn engine_failed(e: SyncError) -> ! {
    match e {
        SyncError::Busy => abort("Another sync is running, try again in a moment.", None),
        SyncError::Backend(e) => {
            let (message, hint) = explain(&e);
            abort(&message, hint.as_deref())
        }
        SyncError::Failed(message) => abort(&message, None),
    }
}

fn opts(args: &Args, dry_run: bool) -> Options {
    Options {
        overwrite: args.overwrite,
        dry_run,
        sessions: args.sessions.clone(),
    }
}

/// Last failed auto push, shown once and cleared.
fn show_auto_error() {
    let mut state = State::load();
    if let Some(error) = state.last_auto_push_error.take() {
        say(&[format!("Warning: Last automatic push failed: {error}")]);
        let _ = state.save();
    }
}

pub fn status() -> Result<()> {
    let ctx = prepare(KeyNeed::MayCreate)?;
    let report = ctx
        .engine()
        .status(&[])
        .unwrap_or_else(|e| engine_failed(e));
    say(&format::status_lines(&report, ctx.backend.name()));
    show_auto_error();
    Ok(())
}

pub fn push(args: Args, _auto: bool) -> Result<()> {
    let ctx = prepare(KeyNeed::MayCreate)?;
    let plan = ctx
        .engine()
        .push(&opts(&args, true))
        .unwrap_or_else(|e| engine_failed(e));
    say(&format::preview_lines(&plan, "push"));
    say(&format::problem_lines(&plan));
    if args.dry_run
        || !plan
            .items
            .iter()
            .any(|i| i.outcome == agr::sync::run::Outcome::Planned)
    {
        return Ok(());
    }
    let mut state = State::load();
    let creating = plan
        .items
        .iter()
        .filter(|i| i.outcome == agr::sync::run::Outcome::Planned && !i.on_remote)
        .count();
    let notice = needs_privacy_ack(state.sync_privacy_ack, creating);
    if notice {
        say(&format::privacy_lines());
    }
    if !should_proceed("Push these sessions?", args.yes)? {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    if let KeyState::Fresh(fresh) = &ctx.key {
        key::save_new(&default_key_path()?, fresh)?;
        say(&["Created a new sync key. Back up your key with agr sync key export.".to_string()]);
    }
    if notice {
        state.sync_privacy_ack = true;
        state.save()?;
    }
    finish(ctx.engine().push(&opts(&args, false)))
}

pub fn pull(args: Args) -> Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    let plan = ctx
        .engine()
        .pull(&opts(&args, true))
        .unwrap_or_else(|e| engine_failed(e));
    say(&format::preview_lines(&plan, "pull"));
    say(&format::problem_lines(&plan));
    if args.dry_run
        || !plan
            .items
            .iter()
            .any(|i| i.outcome == agr::sync::run::Outcome::Planned)
    {
        return Ok(());
    }
    if !should_proceed("Pull these sessions?", args.yes)? {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    finish(ctx.engine().pull(&opts(&args, false)))
}

pub fn delete(args: Args) -> Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    let plan = ctx
        .engine()
        .status(&args.sessions)
        .unwrap_or_else(|e| engine_failed(e));
    say(&format::delete_preview_lines(&plan));
    if args.dry_run || !plan.items.iter().any(|i| i.on_remote) {
        return Ok(());
    }
    if !should_proceed(
        "Delete these remote sessions? The local files stay.",
        args.yes,
    )? {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    finish(ctx.engine().delete(&args.sessions))
}

/// Print the summary, exit non-zero when any session failed.
fn finish(result: Result<Report, SyncError>) -> Result<()> {
    let report = result.unwrap_or_else(|e| engine_failed(e));
    say(&format::summary_lines(&report));
    if report.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
