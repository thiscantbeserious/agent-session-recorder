//! Auto push after a recording, cheap local checks in-process, the network part in a
//! detached `agr sync push --auto <session>` child.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::state::State;
use crate::config::{Config, SyncBackendKind};
use crate::files::{codec, lock};

mod child;

pub use child::{error_line, run, take_error_line};

/// What to do with a finished recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Auto {
    /// Nothing to say, sync or auto push is off.
    Silent,
    /// Not pushed, one hint line for the user.
    Skip(String),
    /// Spawn `agr` with these arguments.
    Spawn(Vec<String>),
}

/// Facts the checks need that are not in config or state.
#[derive(Debug, Clone, Copy)]
pub(super) struct Env {
    pub key_present: bool,
    pub tools_present: bool,
}

/// Decide in-process, no network.
pub(super) fn decide(config: &Config, state: &State, env: Env, path: &Path) -> Auto {
    if config.sync.backend == SyncBackendKind::Off || !config.sync.auto_push {
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

/// File names to look for, Windows ships `gh.exe`, not `gh`.
pub(super) fn candidates(program: &str, windows: bool) -> Vec<String> {
    let mut names = vec![program.to_string()];
    if windows {
        names.push(format!("{program}.exe"));
    }
    names
}

/// True when every program is an executable file in one of the `PATH` directories.
pub(super) fn on_path(programs: &[&str], path_var: &std::ffi::OsStr) -> bool {
    programs
        .iter()
        .map(|program| candidates(program, cfg!(windows)))
        .all(|names| {
            std::env::split_paths(path_var)
                .any(|dir| names.iter().any(|name| dir.join(name).is_file()))
        })
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
    super::setup::default_key_path()
        .as_deref()
        .is_ok_and(std::path::Path::exists)
}

/// Windows process creation flags, no console of the parent and no shared Ctrl-C group.
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// Start `exe args` detached, null stdio, own group on unix, detached on Windows, never waited on.
pub fn spawn_detached(exe: &Path, args: &[String]) -> std::io::Result<()> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
    );
    command.spawn().map(drop)
}

#[cfg(test)]
mod tests {
    use super::{candidates, decide, on_path, Auto, Config, Env, Path, State};
    use crate::config::SyncBackendKind;
    use std::fs;

    fn config(backend: bool, auto_push: bool) -> Config {
        let mut config = Config::default();
        config.sync.backend = if backend {
            SyncBackendKind::Gist
        } else {
            SyncBackendKind::Off
        };
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

    pub(super) fn session(dir: &Path) -> std::path::PathBuf {
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
    fn default_config_without_ack_does_not_push() {
        let dir = tempfile::tempdir().unwrap();
        let got = decide_in(dir.path(), &Config::default(), &State::default(), READY);
        assert_eq!(got, Auto::Silent);
        let mut on = Config::default();
        on.sync.auto_push = true;
        let got = decide_in(dir.path(), &on, &State::default(), READY);
        assert!(matches!(got, Auto::Skip(_)), "{got:?}");
    }

    #[test]
    fn default_config_without_gh_or_git_stays_silent() {
        let dir = tempfile::tempdir().unwrap();
        let no_tools = Env {
            tools_present: false,
            ..READY
        };
        let got = decide_in(dir.path(), &Config::default(), &acked(), no_tools);
        assert_eq!(got, Auto::Silent);
    }

    #[test]
    fn auto_push_on_without_tools_gives_one_hint() {
        let dir = tempfile::tempdir().unwrap();
        let no_tools = Env {
            tools_present: false,
            ..READY
        };
        let got = decide_in(dir.path(), &config(true, true), &acked(), no_tools);
        assert!(matches!(got, Auto::Skip(_)), "{got:?}");
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

    #[test]
    fn candidates_add_exe_on_windows_only() {
        assert_eq!(candidates("gh", false), ["gh"]);
        assert_eq!(candidates("gh", true), ["gh", "gh.exe"]);
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
}
