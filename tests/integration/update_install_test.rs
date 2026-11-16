//! Install discovery, verification and the atomic replace.
#![cfg(unix)]

use crate::helpers::update_fake::{build, Spec};
use agr::update::fetch::Source;
use agr::update::install::{check_managed, managed_install, run_at, run_hooked, Hooks, Managed};
use agr::update::version::Version;
use agr::update::Outcome;
use agr::utils::process_guard::ProcessGuard;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

const OLD: &[u8] = b"#!/bin/sh\necho old\n";

fn target(dir: &Path, mode: u32) -> PathBuf {
    let path = dir.join("agr");
    fs::write(&path, OLD).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

fn src(spec: &Spec) -> (crate::helpers::update_fake::FakeRelease, Source) {
    let fake = build(spec);
    let source = Source::from_override(&fake.base()).unwrap();
    (fake, source)
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".agr.update-"))
        .collect()
}

fn fails_untouched(spec: &Spec, want: &str) {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o755);
    let (_fake, source) = src(spec);
    let err = run_at(&source, &t, Version(0, 4, 0)).unwrap_err();
    assert!(format!("{err:#}").contains(want), "{err:#}");
    assert_eq!(fs::read(&t).unwrap(), OLD);
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn managed_cargo() {
    let home = Path::new("/home/u");
    let bin = home.join(".cargo/bin/agr");
    assert_eq!(
        managed_install(&bin, Some(home), None, None),
        Some(Managed::Cargo)
    );
    let ch = Path::new("/opt/ch");
    assert_eq!(
        managed_install(&ch.join("bin/agr"), Some(home), Some(ch), None),
        Some(Managed::Cargo)
    );
    let msg = Managed::Cargo.message(&bin);
    assert!(
        msg.contains(
            "cargo install --git https://github.com/thiscantbeserious/agent-session-recorder"
        ) && msg.contains("/home/u/.cargo/bin/agr"),
        "{msg}"
    );
    assert_eq!(
        managed_install(Path::new("/usr/local/bin/agr"), Some(home), None, None),
        None
    );
}

#[test]
fn managed_brew() {
    let p = Path::new("/opt/homebrew/Cellar/agr/0.4.0/bin/agr");
    assert_eq!(
        managed_install(p, None, None, None),
        Some(Managed::Homebrew)
    );
    assert!(Managed::Homebrew.message(p).contains("brew upgrade"));
}

#[test]
fn shared_prefix_standalone_binary_is_not_managed() {
    let p = Path::new("/usr/local/bin/agr");
    assert_eq!(
        managed_install(p, None, None, Some(Path::new("/usr/local"))),
        None
    );
}

#[test]
fn brew_symlink_into_cellar_is_managed() {
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("Cellar/agr/0.4.0/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("agr"), OLD).unwrap();
    fs::create_dir_all(tmp.path().join("bin")).unwrap();
    let link = tmp.path().join("bin/agr");
    symlink(bin.join("agr"), &link).unwrap();
    let err = check_managed(&link, None, None, Some(tmp.path()))
        .unwrap_err()
        .to_string();
    assert!(err.contains("brew upgrade"), "{err}");
}

#[test]
fn managed_through_symlink() {
    let tmp = TempDir::new().unwrap();
    let real_dir = tmp.path().join(".cargo/bin");
    fs::create_dir_all(&real_dir).unwrap();
    let real = real_dir.join("agr");
    fs::write(&real, OLD).unwrap();
    let link_dir = TempDir::new().unwrap();
    let link = link_dir.path().join("agr");
    symlink(&real, &link).unwrap();
    let err = check_managed(&link, Some(tmp.path()), None, None)
        .unwrap_err()
        .to_string();
    let real = fs::canonicalize(&real).unwrap();
    assert!(
        err.contains(
            "cargo install --git https://github.com/thiscantbeserious/agent-session-recorder"
        ) && err.contains(real.to_str().unwrap()),
        "{err}"
    );
}

#[test]
fn readonly_dir_refused_before_download() {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o755);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    // A source that cannot answer: if a request were made the error would be about curl.
    let source = Source::from_override("file:///nonexistent-agr-fake").unwrap();
    let result = run_at(&source, &t, Version(0, 4, 0));
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains(dir.path().to_str().unwrap()) && err.contains("not writable"),
        "{err}"
    );
}

#[test]
fn staged_file_never_at_final_path_before_rename() {
    fails_untouched(
        &Spec {
            wrong_sum: true,
            ..Spec::default()
        },
        "checksum mismatch",
    );
}

#[test]
fn truncated_download_rejected() {
    fails_untouched(
        &Spec {
            size_extra: 10,
            ..Spec::default()
        },
        "bytes",
    );
}

#[test]
fn tampered_asset_rejected() {
    fails_untouched(
        &Spec {
            flip_byte: true,
            ..Spec::default()
        },
        "checksum mismatch",
    );
}

#[test]
fn wrong_magic_rejected() {
    fails_untouched(
        &Spec {
            script_asset: true,
            ..Spec::default()
        },
        "not an executable",
    );
}

#[test]
fn smoke_run_version_mismatch_rejected() {
    let spec = Spec {
        stub_prints: "0.4.9".into(),
        ..Spec::default()
    };
    fails_untouched(&spec, "reports 0.4.9");
}

#[test]
fn smoke_run_nonzero_rejected() {
    fails_untouched(
        &Spec {
            stub_exit: 3,
            ..Spec::default()
        },
        "failed",
    );
}

#[test]
fn success_replaces_and_keeps_mode() {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o750);
    let (_fake, source) = src(&Spec::default());
    let out = run_at(&source, &t, Version(0, 4, 0)).unwrap();
    assert_eq!(
        out,
        Outcome::Updated {
            from: Version(0, 4, 0),
            to: Version(0, 5, 0)
        }
    );
    assert_eq!(
        fs::metadata(&t).unwrap().permissions().mode() & 0o7777,
        0o750
    );
    let version = Command::new(&t).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&version.stdout).contains("agr 0.5.0"));
    assert!(leftovers(dir.path()).is_empty());
}

/// A supplementary group that differs from the effective one, if the user has any.
fn other_group() -> Option<u32> {
    let mut gids = [0 as libc::gid_t; 256];
    let n = unsafe { libc::getgroups(256, gids.as_mut_ptr()) };
    let egid = unsafe { libc::getegid() };
    gids[..n.max(0) as usize]
        .iter()
        .copied()
        .find(|g| *g != egid)
}

#[test]
fn success_keeps_group() {
    use std::os::unix::fs::MetadataExt;
    let Some(gid) = other_group() else {
        eprintln!("skipped, no supplementary group in this environment");
        return;
    };
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o750);
    std::os::unix::fs::chown(&t, None, Some(gid)).unwrap();
    let (_fake, source) = src(&Spec::default());
    run_at(&source, &t, Version(0, 4, 0)).unwrap();
    assert_eq!(fs::metadata(&t).unwrap().gid(), gid);
}

#[test]
fn rename_failure_leaves_original() {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o755);
    let (_fake, source) = src(&Spec::default());
    // The hook removes the staged file, so the rename has nothing to move.
    let hook = |staged: &Path| fs::remove_file(staged).unwrap();
    let hooks = Hooks {
        guard: ProcessGuard::new(),
        before_rename: Some(&hook),
    };
    let err = run_hooked(&source, &t, Version(0, 4, 0), &hooks).unwrap_err();
    assert!(format!("{err:#}").contains("unchanged"), "{err:#}");
    assert_eq!(fs::read(&t).unwrap(), OLD);
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn second_update_fails_while_locked() {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o755);
    let held = fs::File::open(&t).unwrap();
    use std::os::unix::io::AsRawFd;
    assert_eq!(
        unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let (_fake, source) = src(&Spec::default());
    let err = run_at(&source, &t, Version(0, 4, 0))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("another agr update is running") && err.contains(t.to_str().unwrap()),
        "{err}"
    );
}

#[test]
fn cargo_dir_behind_symlink_is_managed() {
    let tmp = TempDir::new().unwrap();
    let real = tmp.path().join("real");
    fs::create_dir_all(real.join("bin")).unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    symlink(&real, home.join(".cargo")).unwrap();
    let bin = real.join("bin/agr");
    fs::write(&bin, OLD).unwrap();
    let err = check_managed(&bin, Some(&home), None, None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("Cargo"), "{err}");
}

#[test]
fn old_inode_keeps_running() {
    let dir = TempDir::new().unwrap();
    let t = target(dir.path(), 0o755);
    let flag = dir.path().join("go");
    let started = dir.path().join("started");
    let script =
        "#!/bin/sh\ntouch \"$1\"\nwhile [ ! -f \"$2\" ]; do sleep 0.05; done\necho finished\n";
    fs::write(&t, script).unwrap();
    // A parallel test can leak the write fd of the script, retry only ETXTBSY.
    let mut tries = 0;
    let child = loop {
        match Command::new(&t)
            .arg(&started)
            .arg(&flag)
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(c) => break c,
            Err(e) if e.raw_os_error() == Some(26) && tries < 50 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => panic!("spawn failed: {e}"),
        }
    };
    let mut waits = 0;
    while !started.exists() {
        waits += 1;
        assert!(waits < 200, "the script never created the started file");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (_fake, source) = src(&Spec::default());
    run_at(&source, &t, Version(0, 4, 0)).unwrap();
    fs::write(&flag, "").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "finished");
}
