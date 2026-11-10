//! `agr sync migrate`: preview, ask, then move and delete. By hand only, never from auto push.

use std::io;
use std::path::PathBuf;

use anyhow::Result;

use agr::sync::confirm::{confirm_migrate, Confirm, Flags};
use agr::sync::format::migrate_lines;
use agr::sync::setup::{keep_writer, legacy_reader, KeyNeed};
use agr::sync::{MigrateEnv, MigrateError};

use super::run::{abort, prepare, say};

fn failed(e: MigrateError) -> ! {
    match e {
        MigrateError::List(e) | MigrateError::Store(e) => abort(&e.to_string(), None),
        MigrateError::Open(e) => abort(&e.to_string(), None),
        MigrateError::Local(m) => abort(&m, None),
    }
}

/// A private directory for the sessions in flight, removed when the command ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("agr-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn migrate(yes: bool, tty: bool) -> Result<()> {
    let ctx = prepare(KeyNeed::Existing)?;
    let test_dir = std::env::var_os("AGR_SYNC_TEST_BACKEND_DIR").map(PathBuf::from);
    let legacy = legacy_reader(test_dir.as_deref());
    let scratch = Scratch::new()?;
    let dir = ctx.storage.storage_dir();
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
        pace: &|| {},
    };
    let preview = agr::sync::migrate_preview(&env).unwrap_or_else(|e| failed(e));
    say(&preview.lines);
    let flags = Flags {
        yes,
        force: false,
        tty,
    };
    let (stdin, mut out) = (io::stdin(), io::stdout());
    if confirm_migrate(&flags, &mut stdin.lock(), &mut out)? != Confirm::Go {
        say(&["No changes made.".to_string()]);
        return Ok(());
    }
    keep_writer(&ctx.writer)?;
    let report = agr::sync::migrate_run(&env).unwrap_or_else(|e| failed(e));
    say(&migrate_lines(&report));
    match &report.failed {
        Some(why) => abort(why, None),
        None => Ok(()),
    }
}
