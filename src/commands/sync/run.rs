//! status, push, pull and delete, thin layer over `agr::sync::run::Engine`.

use anyhow::Result;

use agr::config::state::State;
use agr::sync::auto;
use agr::sync::backend::{BackendError, SyncBackend};
use agr::sync::format;
use agr::sync::key;
use agr::sync::run::{Engine, Options, Outcome, Report, SyncError};
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

/// A precondition that failed, message plus optional hint line.
struct Failure(String, Option<String>);

impl From<BackendError> for Failure {
    fn from(e: BackendError) -> Self {
        let (message, hint) = explain(&e);
        Failure(message, hint)
    }
}

impl From<SetupError> for Failure {
    fn from(e: SetupError) -> Self {
        match e {
            SetupError::NoKey | SetupError::KeyOnRemote => Failure(
                "No sync key on this machine.".into(),
                Some(IMPORT_HINT.into()),
            ),
            SetupError::Backend(e) => e.into(),
            e => Failure(e.to_string(), None),
        }
    }
}

impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        Failure(format!("{e:#}"), None)
    }
}

/// Config, backend, tool and login checks, then the key. Changes nothing.
fn try_prepare(need: KeyNeed, announce: bool) -> Result<Ctx, Failure> {
    let config = Config::load()?;
    let test_dir: Option<std::path::PathBuf> =
        std::env::var_os("AGR_SYNC_TEST_BACKEND_DIR").map(Into::into);
    if let (Some(dir), true) = (&test_dir, announce) {
        eprintln!("Using test sync backend at {}", dir.display());
    }
    let Some(backend) = open_backend(config.sync.backend, test_dir) else {
        return Err(Failure(
            "Sync is off.".into(),
            Some("Enable it with `[sync] backend = \"gist\"` in ~/.config/agr/config.toml.".into()),
        ));
    };
    backend.check()?;
    let key = resolve_key(&default_key_path()?, backend.as_ref(), need)?;
    let storage = StorageManager::new(config);
    storage.ensure_storage_dir()?;
    Ok(Ctx {
        backend,
        storage,
        key,
    })
}

fn prepare(need: KeyNeed) -> Result<Ctx> {
    Ok(try_prepare(need, true)
        .unwrap_or_else(|Failure(message, hint)| abort(&message, hint.as_deref())))
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
    if let Some(line) = auto::take_error_line(&mut state) {
        say(&[format!("Warning: {line}")]);
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

/// Preview and problems, then the prompt. `false` when there is nothing to do, on
/// `--dry-run` or when declined. `notice` is shown right before the prompt.
fn confirm_planned(plan: &Report, verb: &str, args: &Args, notice: &[String]) -> Result<bool> {
    say(&format::preview_lines(plan, verb));
    say(&format::problem_lines(plan));
    let planned = plan.items.iter().any(|i| i.outcome == Outcome::Planned);
    if args.dry_run || !planned {
        return Ok(false);
    }
    say(notice);
    let question = format!("{}{} these sessions?", verb[..1].to_uppercase(), &verb[1..]);
    let proceed = should_proceed(&question, args.yes)?;
    if !proceed {
        say(&["No changes made.".to_string()]);
    }
    Ok(proceed)
}

fn privacy_notice(plan: &Report, state: &State) -> Vec<String> {
    let creating = plan
        .items
        .iter()
        .filter(|i| i.outcome == Outcome::Planned && !i.on_remote)
        .count();
    if needs_privacy_ack(state.sync_privacy_ack, creating) {
        format::privacy_lines()
    } else {
        Vec::new()
    }
}

pub fn push(args: Args, auto: bool) -> Result<()> {
    if auto {
        return push_auto(&args);
    }
    let ctx = prepare(KeyNeed::MayCreate)?;
    let plan = ctx
        .engine()
        .push(&opts(&args, true))
        .unwrap_or_else(|e| engine_failed(e));
    let mut state = State::load();
    let notice = privacy_notice(&plan, &state);
    if !confirm_planned(&plan, "push", &args, &notice)? {
        return Ok(());
    }
    if let KeyState::Fresh(fresh) = &ctx.key {
        key::save_new(&default_key_path()?, fresh)?;
        say(&["Created a new sync key. Back up your key with agr sync key export.".to_string()]);
    }
    if !notice.is_empty() {
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
    if !confirm_planned(&plan, "pull", &args, &[])? {
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

/// Detached child of a recording, no prompt, no output, a failure is stored for later.
fn push_auto(args: &Args) -> Result<()> {
    let mut state = State::load();
    if !state.sync_privacy_ack {
        return Ok(());
    }
    let result = try_prepare(KeyNeed::Existing, false)
        .map_err(|Failure(message, _)| SyncError::Failed(message))
        .and_then(|ctx| {
            auto::run(
                &ctx.engine(),
                args.sessions.first().map_or("", String::as_str),
            )
        });
    if let Some(line) = auto::error_line(&result) {
        state.last_auto_push_error = Some(line);
        state.save()?;
    }
    Ok(())
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
