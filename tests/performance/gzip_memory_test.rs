//! Peak memory of parsing a ~100MB asciicast, plain versus gzip.
//!
//! Spawns the agr binary under `/usr/bin/time` (`-l` on macOS, `-v` on Linux) and compares
//! max RSS. Run with: `cargo test --test performance -- --ignored gzip`

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;

use flate2::write::GzEncoder;
use flate2::Compression;
use tempfile::TempDir;

const TARGET_BYTES: usize = 100 * 1024 * 1024;
/// Allowed extra memory for decoder buffers on top of the 10% margin.
const SLACK_BYTES: u64 = 8 * 1024 * 1024;

/// Stream a ~100MB asciicast into `out`, so the generator itself stays small.
fn write_cast(out: impl Write) {
    let mut w = BufWriter::new(out);
    writeln!(w, r#"{{"version":3,"term":{{"cols":80,"rows":24}}}}"#).unwrap();
    let (mut written, mut i) = (0, 0u64);
    while written < TARGET_BYTES {
        let time = if i % 100 == 0 { 5.0 } else { 0.1 };
        let line = format!("[{time},\"o\",\"output line {i}\\r\\n\"]\n");
        w.write_all(line.as_bytes()).unwrap();
        written += line.len();
        i += 1;
    }
    w.flush().unwrap();
}

fn write_plain(path: &Path) {
    write_cast(File::create(path).unwrap());
}

fn write_gzip(path: &Path) {
    let mut enc = GzEncoder::new(File::create(path).unwrap(), Compression::default());
    write_cast(&mut enc);
    enc.finish().unwrap();
}

/// Max RSS in bytes of `agr optimize --remove-silence` reading `input` and writing `output`.
fn peak_rss(input: &Path, output: &Path) -> u64 {
    let (flag, unit) = if cfg!(target_os = "macos") {
        ("-l", 1)
    } else {
        ("-v", 1024)
    };
    let out = Command::new("/usr/bin/time")
        .arg(flag)
        .arg(env!("CARGO_BIN_EXE_agr"))
        .args(["optimize", "--remove-silence", "--output"])
        .arg(output)
        .arg(input)
        .output()
        .expect("/usr/bin/time is required");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "agr failed: {stderr}");
    parse_max_rss(&stderr).expect("max RSS line missing") * unit
}

/// macOS: `<n>  maximum resident set size` (bytes), Linux: `Maximum resident set size (kbytes): <n>`.
fn parse_max_rss(stderr: &str) -> Option<u64> {
    stderr.lines().find_map(|l| {
        let l = l.trim();
        if l.ends_with("maximum resident set size") {
            l.split_whitespace().next()?.parse().ok()
        } else {
            l.strip_prefix("Maximum resident set size (kbytes):")?
                .trim()
                .parse()
                .ok()
        }
    })
}

#[test]
#[ignore]
fn gzip_parse_peak_memory_matches_plain() {
    let dir = TempDir::new().unwrap();
    let (plain, gz) = (dir.path().join("p.cast"), dir.path().join("g.cast.gz"));
    write_plain(&plain);
    write_gzip(&gz);

    let plain_peak = peak_rss(&plain, &dir.path().join("out_plain.cast"));
    let gz_peak = peak_rss(&gz, &dir.path().join("out_gz.cast"));
    println!("peak RSS plain: {plain_peak} bytes, gzip: {gz_peak} bytes");

    assert!(
        gz_peak <= plain_peak + plain_peak / 10 + SLACK_BYTES,
        "gzip peak {gz_peak} exceeds plain {plain_peak} by more than 10% + slack"
    );
}
