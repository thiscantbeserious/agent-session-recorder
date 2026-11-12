//! `agr sync prune`: frees dead bytes of sealed packs under the local sync lock.

use agr::files::scratch::Scratch;
use agr::sync::format::prune_lines;
use agr::sync::setup::{keep_writer, KeyNeed};
use agr::sync::{PruneEnv, PruneError, SyncError, SyncLock, SEAL_BUDGET};

use super::run::{abort, busy_text, prepare, say, Ctx};

fn describe(e: PruneError) -> String {
    match e {
        PruneError::Open(e) => e.to_string(),
        PruneError::Store(e) => e.to_string(),
        PruneError::Local(m) => m,
    }
}

fn attempt(ctx: &Ctx, dry_run: bool, seal_orphans: bool) -> Result<(), String> {
    let dir = ctx.storage.storage_dir();
    let _lock = SyncLock::acquire(&dir).map_err(|e| match e {
        SyncError::Busy(lock) => format!("Sync is busy. {}", busy_text(&lock)),
        other => other.to_string(),
    })?;
    let name = format!("agr-prune-{}", std::process::id());
    let scratch =
        Scratch::at(&std::env::temp_dir().join(name)).map_err(|e| format!("temp dir: {e}"))?;
    let writer = if dry_run {
        ctx.writer.clone()
    } else {
        keep_writer(&ctx.writer).map_err(|e| e.to_string())?
    };
    let env = PruneEnv {
        store: ctx.store.as_ref(),
        key: ctx.key.key(),
        writer: &writer,
        target: ctx.target,
        hint: None,
        scratch: &scratch.0,
        dry_run,
        seal_orphans,
        now: chrono::Utc::now().timestamp(),
        seal_budget: SEAL_BUDGET,
        pace: &|| {},
    };
    let report = agr::sync::prune_run(&env).map_err(describe)?;
    say(&prune_lines(&report, dry_run));
    report.failed.map_or(Ok(()), Err)
}

pub fn prune(dry_run: bool, seal_orphans: bool) -> anyhow::Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    match attempt(&ctx, dry_run, seal_orphans) {
        Ok(()) => Ok(()),
        Err(why) => abort(&why, None),
    }
}
