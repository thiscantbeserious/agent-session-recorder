//! status, push and pull, thin layer over `agr::sync::Engine`.

use anyhow::Result;

use agr::config::state::State;
use agr::sync::auto;
use agr::sync::format::{self, Live};
use agr::sync::key;
use agr::sync::setup::{
    default_key_path, explain, keep_writer, needs_privacy_ack, open_store, pack_target,
    resolve_key, writer_id, KeyNeed, KeyState, SetupError, IMPORT_HINT,
};
use agr::sync::testing::{backend_error, PackStore};
use agr::sync::{BackendError, Engine, Item, Options, Outcome, ProgressSink, Report, SyncError};
use agr::theme::current_theme;
use agr::{Config, StorageManager};

use crate::commands::prompt::should_proceed;

pub struct Args {
    pub sessions: Vec<String>,
    pub dry_run: bool,
    pub yes: bool,
    pub overwrite: bool,
    pub verbose: bool,
}

/// Everything a command needs after the preconditions passed.
pub(super) struct Ctx {
    pub store: Box<dyn PackStore>,
    pub storage: StorageManager,
    pub key: KeyState,
    pub writer: String,
    pub target: u64,
}

impl Ctx {
    pub fn engine(&self) -> Engine<'_> {
        let (store, key) = (self.store.as_ref(), self.key.key());
        Engine::new(store, key, &self.storage, &self.writer, self.target)
    }
}

pub(super) fn say(lines: &[String]) {
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
pub(super) fn abort(message: &str, hint: Option<&str>) -> ! {
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
fn try_prepare(
    need: KeyNeed,
    announce: bool,
    progress: Option<ProgressSink>,
) -> Result<Ctx, Failure> {
    let config = Config::load()?;
    let test_dir: Option<std::path::PathBuf> =
        std::env::var_os("AGR_SYNC_TEST_BACKEND_DIR").map(Into::into);
    if let (Some(dir), true) = (&test_dir, announce) {
        eprintln!("Using test sync backend at {}", dir.display());
    }
    let Some(store) = open_store(config.sync.backend, test_dir, progress) else {
        return Err(Failure(
            "Sync is off.".into(),
            Some("Set `[sync] backend = \"gist\"` (or remove `backend = \"off\"`) in ~/.config/agr/config.toml.".into()),
        ));
    };
    store.check().map_err(backend_error)?;
    let key = resolve_key(&default_key_path()?, store.as_ref(), need)?;
    let target = pack_target(config.sync.pack_size_mib).map_err(anyhow::Error::msg)?;
    let storage = StorageManager::new(config);
    storage.ensure_storage_dir()?;
    Ok(Ctx {
        store,
        storage,
        key,
        writer: writer_id(),
        target,
    })
}

pub(super) fn prepare(need: KeyNeed) -> Result<Ctx> {
    prepare_with(need, None)
}

fn prepare_with(need: KeyNeed, progress: Option<ProgressSink>) -> Result<Ctx> {
    Ok(try_prepare(need, true, progress)
        .unwrap_or_else(|Failure(message, hint)| abort(&message, hint.as_deref())))
}

/// The lock file is named so a stale one (empty or garbage, counted as held) can be removed.
pub(super) fn busy_text(lock: &std::path::Path) -> String {
    format!(
        "Another sync is running, try again in a moment. If none is, remove {}",
        lock.display()
    )
}

/// Prints the error and exits, generic so it fits `unwrap_or_else` as a plain function.
pub(super) fn engine_failed<T>(e: SyncError) -> T {
    match e {
        SyncError::Busy(lock) => abort(&busy_text(&lock), None),
        SyncError::Backend(e) => {
            let (message, hint) = explain(&e);
            abort(&message, hint.as_deref())
        }
        SyncError::NeedsMigrate => abort("Run `agr sync migrate` first.", None),
        SyncError::Failed(message) => abort(&message, None),
        SyncError::RateLimited { left } => {
            let [stopped, later] = format::rate_limit_lines(left);
            abort(&stopped, Some(&later))
        }
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
    if State::load().last_auto_push_error.is_none() {
        return;
    }
    if let Ok(Some(line)) = State::update(auto::take_error_line) {
        say(&[format!("Warning: {line}")]);
    }
}

pub fn status(verbose: bool) -> Result<()> {
    let ctx = prepare(KeyNeed::MayCreate)?;
    let live = Live::stderr("", "");
    let report = ctx
        .engine()
        .status_with(&[], &mut live.loader())
        .unwrap_or_else(engine_failed);
    live.end();
    let name = ctx.store.name();
    say(&if verbose {
        format::status_lines(&report, name)
    } else {
        format::compact_status_lines(&report, name)
    });
    show_auto_error();
    Ok(())
}

/// Preview and problems, then the prompt. `false` when there is nothing to do, on
/// `--dry-run` or when declined. `notice` is shown right before the prompt.
fn confirm_planned(plan: &Report, verb: &str, args: &Args, notice: &[String]) -> Result<bool> {
    if args.verbose {
        say(&format::preview_lines(plan, verb));
        say(&format::problem_lines(plan));
    } else {
        say(&format::compact_preview_lines(plan, verb));
    }
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
    let live = Live::stderr("uploading", "pushed");
    let mut ctx = prepare_with(KeyNeed::MayCreate, live.sink())?;
    let plan = ctx
        .engine()
        .push_with(&opts(&args, true), 0, &mut live.loader())
        .unwrap_or_else(engine_failed);
    live.end();
    let notice = privacy_notice(&plan, &State::load());
    if !confirm_planned(&plan, "push", &args, &notice)? {
        return Ok(());
    }
    if let KeyState::Fresh(fresh) = &ctx.key {
        key::save_new(&default_key_path()?, fresh)?;
        say(&["Created a new sync key. Back up your key with agr sync key export.".to_string()]);
    }
    if !notice.is_empty() {
        State::update(|s| s.sync_privacy_ack = true)?;
    }
    ctx.writer = keep_writer(&ctx.writer)?;
    let mut observer = live.observer();
    let result = ctx
        .engine()
        .push_with(&opts(&args, false), planned(&plan), &mut observer);
    live.end();
    finish(result, "push", args.verbose)
}

pub fn pull(args: Args) -> Result<()> {
    let live = Live::stderr("downloading", "pulled");
    let ctx = prepare_with(KeyNeed::Existing, live.sink())?;
    let plan = ctx
        .engine()
        .pull_with(&opts(&args, true), 0, &mut live.loader())
        .unwrap_or_else(engine_failed);
    live.end();
    if !confirm_planned(&plan, "pull", &args, &[])? {
        return Ok(());
    }
    let mut observer = live.observer();
    let result = ctx
        .engine()
        .pull_with(&opts(&args, false), planned(&plan), &mut observer);
    live.end();
    finish(result, "pull", args.verbose)
}

/// Sessions that get a progress line in a real run, planned, skipped and failed ones.
pub(super) fn planned(plan: &Report) -> usize {
    let lined = |i: &&Item| {
        matches!(
            i.outcome,
            Outcome::Planned | Outcome::Skipped(_) | Outcome::Failed(_)
        )
    };
    plan.items.iter().filter(lined).count()
}

/// Detached child of a recording, no prompt, no output, a failure is stored for later.
fn push_auto(args: &Args) -> Result<()> {
    if !State::load().sync_privacy_ack {
        return Ok(());
    }
    let session = args.sessions.first().map_or("", String::as_str);
    let result = try_prepare(KeyNeed::Existing, false, None)
        .map_err(|Failure(message, _)| SyncError::Failed(message))
        .and_then(|mut ctx| {
            ctx.writer = keep_writer(&ctx.writer).map_err(|e| SyncError::Failed(e.to_string()))?;
            auto::run(&ctx.engine(), session)
        });
    if let Some(line) = auto::error_line(&result) {
        State::store_auto_error_default(line)?;
    }
    Ok(())
}

/// Print the summary, exit non-zero when any session failed.
pub(super) fn finish(result: Result<Report, SyncError>, verb: &str, verbose: bool) -> Result<()> {
    let report = result.unwrap_or_else(engine_failed);
    say(&if verbose {
        format::summary_lines(&report)
    } else {
        format::compact_summary_lines(&report, verb)
    });
    if report.run_failed() {
        std::process::exit(1);
    }
    Ok(())
}
