//! Auto push after a recording, cheap local checks in-process, the network part in a
//! detached `agr sync push --auto <session>` child.

use std::path::Path;

use super::run::{Engine, Options, Outcome, Report, SyncError};
use crate::config::state::State;
use crate::config::Config;
use crate::files::{codec, lock};

/// What to do with a finished recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auto {
    /// Nothing to say, sync or auto push is off.
    Silent,
    /// Not pushed, one hint line for the user.
    Skip(String),
    /// Spawn `agr` with these arguments.
    Spawn(Vec<String>),
}

/// Facts the checks need that are not in config or state.
#[derive(Debug, Clone, Copy)]
pub struct Env {
    pub key_present: bool,
    pub tools_present: bool,
}

/// Decide in-process, no network.
pub fn decide(config: &Config, state: &State, env: Env, path: &Path) -> Auto {
    if config.sync.backend.is_none() || !config.sync.auto_push {
        return Auto::Silent;
    }
    if let Some(hint) = blocker(state, env, path) {
        return Auto::Skip(hint.into());
    }
    match session_name(path) {
        Some(name) => Auto::Spawn(
            ["sync", "push", "--auto"]
                .map(String::from)
                .into_iter()
                .chain([name])
                .collect(),
        ),
        None => Auto::Silent,
    }
}

fn blocker(state: &State, env: Env, path: &Path) -> Option<&'static str> {
    if !state.sync_privacy_ack {
        return Some("Auto push skipped, run `agr sync push` once to confirm the privacy notice.");
    }
    if !env.key_present {
        return Some("Auto push skipped, no sync key, run `agr sync key import`.");
    }
    if !env.tools_present {
        return Some("Auto push skipped, gh or git not found on PATH.");
    }
    if !codec::is_gzip_path(path) {
        return Some("Auto push skipped, session is not compressed, run `agr compress`.");
    }
    if lock::read_lock(path).is_some() {
        return Some("Auto push skipped, session is still recording.");
    }
    None
}

/// `agent/name.cast.gz`
fn session_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let agent = path.parent()?.file_name()?.to_str()?;
    Some(format!("{agent}/{name}"))
}

/// One line for the state file after an `--auto` run, `None` when it went fine or a
/// concurrent push is already running.
pub fn error_line(result: &Result<Report, SyncError>) -> Option<String> {
    let report = match result {
        Err(SyncError::Busy) => return None,
        Err(e) => return Some(format!("auto push failed: {e}")),
        Ok(report) => report,
    };
    let item = report.items.iter().find(|i| {
        matches!(i.outcome, Outcome::Failed(_)) || matches!(i.outcome, Outcome::Skipped(_))
    })?;
    let (Outcome::Failed(why) | Outcome::Skipped(why)) = &item.outcome else {
        return None;
    };
    Some(format!("auto push of {} failed: {why}", item.path))
}

/// True when every program is an executable file in one of the `PATH` directories.
pub fn on_path(programs: &[&str], path_var: &std::ffi::OsStr) -> bool {
    programs
        .iter()
        .all(|program| std::env::split_paths(path_var).any(|dir| dir.join(program).is_file()))
}

/// The stored error as a warning line, clearing it from `state` (caller saves).
pub fn take_error_line(state: &mut State) -> Option<String> {
    let error = state.last_auto_push_error.take()?;
    Some(format!(
        "Last automatic push failed: {error} (see `agr sync status`)"
    ))
}

/// Local checks after a recording. Returns the lines to print (at most one) and the
/// arguments to spawn, the caller prints and spawns.
pub fn after_recording(config: &Config, path: &Path) -> (Vec<String>, Option<Vec<String>>) {
    let mut state = State::load();
    let stored = take_error_line(&mut state);
    if stored.is_some() {
        let _ = state.save();
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let env = Env {
        key_present: key_exists(),
        tools_present: on_path(&["gh", "git"], &path_var),
    };
    let (hint, spawn) = match decide(config, &state, env, path) {
        Auto::Silent => (None, None),
        Auto::Skip(hint) => (Some(hint), None),
        Auto::Spawn(args) => (None, Some(args)),
    };
    (stored.or(hint).into_iter().collect(), spawn)
}

fn key_exists() -> bool {
    super::setup::default_key_path().is_ok_and(|p| p.exists())
}

/// The `--auto` run, pushes only the one session, never pulls or deletes, a conflict is
/// skipped (no overwrite) and reported.
pub fn run(engine: &Engine<'_>, session: &str) -> Result<Report, SyncError> {
    engine.push(&Options {
        sessions: vec![session.to_string()],
        ..Options::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncBackendKind;
    use crate::sync::plan::Decision;
    use crate::sync::run::{Item, Outcome};
    use std::fs;

    fn config(backend: bool, auto_push: bool) -> Config {
        let mut config = Config::default();
        config.sync.backend = backend.then_some(SyncBackendKind::Gist);
        config.sync.auto_push = auto_push;
        config
    }

    fn acked() -> State {
        State {
            sync_privacy_ack: true,
            ..State::default()
        }
    }

    const READY: Env = Env {
        key_present: true,
        tools_present: true,
    };

    fn session(dir: &Path) -> std::path::PathBuf {
        fs::create_dir_all(dir.join("claude")).unwrap();
        let path = dir.join("claude/s.cast.gz");
        fs::write(&path, b"x").unwrap();
        path
    }

    fn decide_in(dir: &Path, config: &Config, state: &State, env: Env) -> Auto {
        let mut config = config.clone();
        config.storage.directory = dir.to_string_lossy().into_owned();
        decide(&config, state, env, &session(dir))
    }

    #[test]
    fn off_states_are_silent() {
        let dir = tempfile::tempdir().unwrap();
        for config in [config(false, true), config(true, false)] {
            assert_eq!(
                decide_in(dir.path(), &config, &acked(), READY),
                Auto::Silent
            );
        }
    }

    #[test]
    fn missing_privacy_ack_hints_to_push_once() {
        let dir = tempfile::tempdir().unwrap();
        let got = decide_in(dir.path(), &config(true, true), &State::default(), READY);
        let Auto::Skip(hint) = got else {
            panic!("expected Skip, got {got:?}")
        };
        assert!(hint.contains("agr sync push"), "{hint}");
    }

    #[test]
    fn other_local_blockers_skip_with_a_hint() {
        let dir = tempfile::tempdir().unwrap();
        let on = config(true, true);
        let no_key = Env {
            key_present: false,
            ..READY
        };
        let no_tools = Env {
            tools_present: false,
            ..READY
        };
        for env in [no_key, no_tools] {
            assert!(matches!(
                decide_in(dir.path(), &on, &acked(), env),
                Auto::Skip(_)
            ));
        }
    }

    #[test]
    fn plain_and_locked_sessions_are_not_pushed() {
        let dir = tempfile::tempdir().unwrap();
        let mut on = config(true, true);
        on.storage.directory = dir.path().to_string_lossy().into_owned();
        let plain = dir.path().join("claude/p.cast");
        fs::create_dir_all(dir.path().join("claude")).unwrap();
        fs::write(&plain, b"x").unwrap();
        assert!(matches!(
            decide(&on, &acked(), READY, &plain),
            Auto::Skip(_)
        ));
        let gz = session(dir.path());
        crate::files::lock::create_lock(&gz).unwrap();
        assert!(matches!(decide(&on, &acked(), READY, &gz), Auto::Skip(_)));
    }

    #[test]
    fn ready_session_spawns_push_auto_with_agent_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let got = decide_in(dir.path(), &config(true, true), &acked(), READY);
        assert_eq!(
            got,
            Auto::Spawn(
                ["sync", "push", "--auto", "claude/s.cast.gz"]
                    .map(String::from)
                    .to_vec()
            )
        );
    }

    fn item(outcome: Outcome, decision: Decision) -> Item {
        Item {
            path: "claude/s.cast.gz".into(),
            decision,
            outcome,
            size: 0,
            on_remote: false,
            on_local: true,
            local_hash: None,
            remote_hash: None,
            remote_size: 0,
            remote_id: None,
            pending: false,
            duplicates: Vec::new(),
        }
    }

    fn report(item: Item, failed: usize, skipped: usize, conflicts: usize) -> Report {
        Report {
            items: vec![item],
            failed,
            skipped,
            conflicts,
            ..Report::default()
        }
    }

    #[test]
    fn auto_run_pushes_only_the_session_never_pulls_and_skips_conflicts() {
        use crate::sync::backend::SyncBackend;
        use crate::sync::crypto::{blob_aad, seal, seal_meta, sha256_hex, Key, Meta};
        use crate::sync::memory::MemoryBackend;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.storage.directory = dir.path().to_string_lossy().into_owned();
        let storage = crate::StorageManager::new(cfg);
        let key = Key::generate();
        let backend = MemoryBackend::new();
        let remote_only = Meta {
            v: 2,
            path: "claude/other.cast.gz".into(),
            sha256: sha256_hex(b"other"),
            size: 5,
            gen: 1,
        };
        backend.put(
            None,
            &seal_meta(&key, &remote_only),
            &seal(&key, &blob_aad(&remote_only.path, 1), b"other"),
        );
        let path = session(dir.path());
        let engine = Engine {
            backend: &backend,
            key: &key,
            storage: &storage,
        };
        let report = run(&engine, "claude/s.cast.gz").unwrap();
        assert_eq!((report.pushed, report.pulled, report.deleted), (1, 0, 0));
        assert!(!dir.path().join("claude/other.cast.gz").exists());
        assert_eq!(backend.len(), 2);
        // changed on both sides is a conflict, skipped, nothing overwritten
        fs::write(&path, b"changed locally").unwrap();
        let moved = Meta {
            v: 2,
            path: "claude/s.cast.gz".into(),
            sha256: sha256_hex(b"changed remotely"),
            size: 16,
            gen: 1,
        };
        let id = backend
            .list()
            .unwrap()
            .into_iter()
            .find(|e| e.id != "mem-1")
            .unwrap()
            .id;
        backend.put(
            Some(&id),
            &seal_meta(&key, &moved),
            &seal(&key, &blob_aad(&moved.path, 1), b"changed remotely"),
        );
        let report = run(&engine, "claude/s.cast.gz").unwrap();
        assert_eq!((report.pushed, report.conflicts), (0, 1));
        assert!(error_line(&Ok(report)).unwrap().contains("conflict"));
        assert_eq!(fs::read(&path).unwrap(), b"changed locally");
    }

    #[test]
    fn on_path_finds_programs_in_path_dirs() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["gh", "git"] {
            let file = dir.path().join(name);
            fs::write(&file, "#!/bin/sh\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let path = std::env::join_paths([Path::new("/nonexistent"), dir.path()]).unwrap();
        assert!(on_path(&["gh", "git"], &path));
        assert!(!on_path(&["gh", "nope"], &path));
        assert!(!on_path(&["gh"], std::ffi::OsStr::new("")));
    }

    #[test]
    fn take_error_line_returns_and_clears_the_stored_error() {
        let mut state = State {
            last_auto_push_error: Some("offline".into()),
            ..State::default()
        };
        let line = take_error_line(&mut state).unwrap();
        assert!(
            line.contains("offline") && line.contains("agr sync status"),
            "{line}"
        );
        assert_eq!(state.last_auto_push_error, None);
        assert_eq!(take_error_line(&mut state), None);
    }

    #[test]
    fn error_line_keeps_the_rate_limit_text_with_sessions_left() {
        let line = error_line(&Err(SyncError::RateLimited { left: 4 })).unwrap();
        assert_eq!(
            line,
            "auto push failed: GitHub rate limit reached, sync stopped, 4 left, please run again later"
        );
    }

    #[test]
    fn error_line_is_none_for_success_and_busy() {
        let ok = report(item(Outcome::Done(1), Decision::Push), 0, 0, 0);
        assert_eq!(error_line(&Ok(ok)), None);
        assert_eq!(error_line(&Err(SyncError::Busy)), None);
    }

    #[test]
    fn error_line_names_failures_conflicts_and_skips_on_one_line() {
        let failed = report(
            item(Outcome::Failed("offline".into()), Decision::Push),
            1,
            0,
            0,
        );
        let line = error_line(&Ok(failed)).unwrap();
        assert!(
            line.contains("claude/s.cast.gz") && line.contains("offline"),
            "{line}"
        );
        assert!(!line.contains('\n'));
        let conflict = report(
            item(
                Outcome::Skipped("conflict, use --overwrite".into()),
                Decision::Conflict,
            ),
            0,
            0,
            1,
        );
        assert!(error_line(&Ok(conflict)).unwrap().contains("conflict"));
        let capped = report(
            item(
                Outcome::Skipped("over the 1 GiB limit".into()),
                Decision::Push,
            ),
            0,
            1,
            0,
        );
        assert!(error_line(&Ok(capped)).unwrap().contains("1 GiB"));
        let err = error_line(&Err(SyncError::Failed("boom".into()))).unwrap();
        assert!(err.contains("boom"), "{err}");
    }
}
