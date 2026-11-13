//! Peak memory of `agr sync push` and `pull` for a 64 MiB and a 512 MiB session.
//!
//! Spawns the release binary under `/usr/bin/time` (`-l` on macOS, `-v` on Linux) against the
//! file backend. Run with: `cargo test --release --test performance -- --ignored sync_memory --nocapture`

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::helpers::parse_max_rss;

const MIB: u64 = 1024 * 1024;
/// Allowed growth of peak RSS between the small and the large session.
const GROWTH_BOUND: u64 = 16 * MIB;
/// Absolute peak RSS bound for every run.
const PEAK_BOUND: u64 = 64 * MIB;

/// Stored (uncompressed) gzip member of `size` pseudo random bytes, streamed to `path`.
/// The file ends up a few bytes larger than `size`, which is fine.
fn write_stored_gzip(path: &Path, size: u64) {
    let mut out = BufWriter::new(File::create(path).unwrap());
    out.write_all(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255])
        .unwrap();
    let (mut crc, mut state, mut left) = (flate2::Crc::new(), 0x9e37_79b9_7f4a_7c15u64, size);
    while left > 0 {
        let len = left.min(65535) as usize;
        let block: Vec<u8> = (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        left -= len as u64;
        out.write_all(&[(left == 0) as u8]).unwrap();
        out.write_all(&(len as u16).to_le_bytes()).unwrap();
        out.write_all(&(!(len as u16)).to_le_bytes()).unwrap();
        out.write_all(&block).unwrap();
        crc.update(&block);
    }
    out.write_all(&crc.sum().to_le_bytes()).unwrap();
    out.write_all(&(size as u32).to_le_bytes()).unwrap();
    out.flush().unwrap();
}

struct Machine {
    home: TempDir,
    remote: PathBuf,
}

impl Machine {
    fn new(remote: &Path) -> Self {
        let home = TempDir::new_in(remote.parent().unwrap()).unwrap();
        let cfg = home.path().join(".config/agr");
        fs::create_dir_all(&cfg).unwrap();
        let toml = format!(
            "[storage]\ndirectory = \"{}\"\n[sync]\nbackend = \"gist\"\n",
            home.path().join("sessions").display()
        );
        fs::write(cfg.join("config.toml"), toml).unwrap();
        Machine {
            home,
            remote: remote.to_path_buf(),
        }
    }

    fn session(&self) -> PathBuf {
        self.home.path().join("sessions/claude/big.cast.gz")
    }

    /// Run `agr sync <args>` under `/usr/bin/time`, returns (stdout, peak RSS in bytes).
    fn run(&self, args: &[&str]) -> (String, u64) {
        let (flag, unit) = if cfg!(target_os = "macos") {
            ("-l", 1)
        } else {
            ("-v", 1024)
        };
        let out = Command::new("/usr/bin/time")
            .arg(flag)
            .arg(env!("CARGO_BIN_EXE_agr"))
            .arg("sync")
            .args(args)
            .env("NO_COLOR", "1")
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("AGR_SYNC_TEST_BACKEND_DIR", &self.remote)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("/usr/bin/time is required");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "agr sync {args:?} failed: {stderr}");
        let rss = parse_max_rss(&stderr).expect("max RSS line missing") * unit;
        (String::from_utf8_lossy(&out.stdout).into_owned(), rss)
    }
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let (mut a, mut b) = (
        BufReader::new(File::open(a).unwrap()),
        BufReader::new(File::open(b).unwrap()),
    );
    let (mut x, mut y) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = a.read(&mut x).unwrap();
        if n == 0 {
            return b.read(&mut y).unwrap() == 0;
        }
        if b.read_exact(&mut y[..n]).is_err() || x[..n] != y[..n] {
            return false;
        }
    }
}

/// Push on machine 1, key export and import and pull on machine 2, returns (push, pull) peak RSS.
fn round_trip(root: &Path, size: u64) -> (u64, u64) {
    let remote = root.join(format!("remote_{size}"));
    fs::create_dir_all(&remote).unwrap();
    let (m1, m2) = (Machine::new(&remote), Machine::new(&remote));
    fs::create_dir_all(m1.session().parent().unwrap()).unwrap();
    write_stored_gzip(&m1.session(), size);

    let (_, push) = m1.run(&["push", "--yes"]);
    let (export, _) = m1.run(&["key", "export"]);
    let phrase = export
        .split_whitespace()
        .find(|w| w.starts_with("agr1-"))
        .expect("phrase in key export");
    m2.run(&["key", "import", phrase]);
    let (_, pull) = m2.run(&["pull", "--yes"]);

    assert!(
        same_bytes(&m1.session(), &m2.session()),
        "pulled file differs ({size} bytes)"
    );
    // free the disk before the next size
    fs::remove_dir_all(&remote).unwrap();
    (push, pull)
}

#[test]
#[ignore]
fn sync_memory_stays_flat_for_large_sessions() {
    let root = TempDir::new().unwrap();
    let work = root.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let (push_small, pull_small) = round_trip(&work, 64 * MIB);
    let (push_big, pull_big) = round_trip(&work, 512 * MIB);
    println!("peak RSS push 64 MiB: {push_small} bytes, push 512 MiB: {push_big} bytes");
    println!("peak RSS pull 64 MiB: {pull_small} bytes, pull 512 MiB: {pull_big} bytes");

    assert!(
        push_big <= push_small + GROWTH_BOUND,
        "push grew {push_small} -> {push_big}"
    );
    assert!(
        pull_big <= pull_small + GROWTH_BOUND,
        "pull grew {pull_small} -> {pull_big}"
    );
    for (name, rss) in [
        ("push 64", push_small),
        ("push 512", push_big),
        ("pull 64", pull_small),
        ("pull 512", pull_big),
    ] {
        assert!(rss < PEAK_BOUND, "{name} MiB peak {rss} over {PEAK_BOUND}");
    }
}
