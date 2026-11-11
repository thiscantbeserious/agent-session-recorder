//! `agr sync migrate`: preview, ask, then move and delete. By hand only, never from auto push.

use std::io;
use std::path::PathBuf;

use agr::files::scratch::Scratch;
use agr::sync::confirm::{confirm_migrate, Confirm, Flags};
use agr::sync::format::migrate_lines;
use agr::sync::setup::{keep_writer, legacy_reader, KeyNeed};
use agr::sync::{MigrateEnv, MigrateError, SyncError, SyncLock};

use super::run::{abort, busy_text, prepare, say, Ctx};

fn describe(e: MigrateError) -> String {
    match e {
        MigrateError::List(e) | MigrateError::Store(e) => e.to_string(),
        MigrateError::Open(e) => e.to_string(),
        MigrateError::Local(m) => m,
    }
}

/// Everything after the preconditions. The scratch dir is dropped when this returns, so an error
/// never reaches `abort` (which exits without running any `Drop`) while plain sessions are on disk.
fn attempt(ctx: &Ctx, yes: bool, tty: bool) -> Result<(), String> {
    let dir = ctx.storage.storage_dir();
    let _lock = SyncLock::acquire(&dir).map_err(|e| match e {
        SyncError::Busy(lock) => busy_text(&lock),
        other => other.to_string(),
    })?;
    let test_dir = std::env::var_os("AGR_SYNC_TEST_BACKEND_DIR").map(PathBuf::from);
    let legacy = legacy_reader(test_dir.as_deref());
    let name = format!("agr-migrate-{}", std::process::id());
    let scratch =
        Scratch::at(&std::env::temp_dir().join(name)).map_err(|e| format!("temp dir: {e}"))?;
    let env = MigrateEnv {
        legacy: legacy.as_ref(),
        store: ctx.store.as_ref(),
        key: ctx.key.key(),
        writer: &ctx.writer,
        target: ctx.target,
        hint: None,
        dir: &dir,
        scratch: &scratch.0,
        now: chrono::Utc::now().timestamp(),
        convert_index: &agr::sync::convert_local_index,
        pace: &|| {},
    };
    let preview = agr::sync::migrate_preview(&env).map_err(describe)?;
    say(&preview.lines);
    let flags = Flags {
        yes,
        force: false,
        tty,
    };
    let (stdin, mut out) = (io::stdin(), io::stdout());
    let go = confirm_migrate(&flags, &mut stdin.lock(), &mut out).map_err(|e| e.to_string())?;
    if go != Confirm::Go {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    let writer = keep_writer(&ctx.writer).map_err(|e| e.to_string())?;
    let env = MigrateEnv {
        writer: &writer,
        ..env
    };
    let report = agr::sync::migrate_run(&env).map_err(describe)?;
    say(&migrate_lines(&report));
    report.failed.map_or(Ok(()), Err)
}

pub fn migrate(yes: bool, tty: bool) -> anyhow::Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    match attempt(&ctx, yes, tty) {
        Ok(()) => Ok(()),
        Err(why) => abort(&why, None),
    }
}
