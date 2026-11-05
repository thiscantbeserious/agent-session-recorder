//! CLI colour only on a terminal, `NO_COLOR` honoured (assert_cmd style, stdout is a pipe)

use std::fs;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn home() -> TempDir {
    let home = TempDir::new().unwrap();
    let cfg = home.path().join(".config/agr");
    fs::create_dir_all(&cfg).unwrap();
    let dir = home.path().join("sessions");
    fs::create_dir_all(dir.join("claude")).unwrap();
    let toml = format!("[storage]\ndirectory = \"{}\"\n", dir.display());
    fs::write(cfg.join("config.toml"), toml).unwrap();
    home
}

fn run(home: &TempDir, args: &[&str], envs: &[(&str, &str)]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("AGR_FORCE_COLOR")
        .envs(envs.iter().copied())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn piped_commands_have_no_escape_codes() {
    let h = home();
    for args in [&["status"][..], &["list"], &["compress"], &["--help"]] {
        let out = run(&h, args, &[]);
        assert!(!out.contains("\x1b["), "{:?}: {:?}", args, out);
    }
}

#[test]
fn no_color_set_has_no_escape_codes() {
    let h = home();
    let out = run(&h, &["status"], &[("NO_COLOR", "1")]);
    assert!(!out.contains("\x1b["), "{:?}", out);
}

#[test]
fn force_color_override_emits_escape_codes() {
    let h = home();
    let out = run(&h, &["status"], &[("AGR_FORCE_COLOR", "1")]);
    assert!(out.contains("\x1b["), "{:?}", out);
}
