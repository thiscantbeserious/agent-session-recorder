//! Stage 1 probe of the pack layer design, runs against real secret gists with the logged in `gh`.
//! Run by hand only: `cargo test --test pack_probe_live -- --ignored --nocapture --test-threads=1`.
//! Every fact is printed as a `PROBE:` line, so the output answers the questions even on a failed assert.

use agr::sync::testing::{GistStore, PackStore};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const MIB: usize = 1024 * 1024;

/// Deletes the gist on drop, also when an assertion panics.
struct Cleanup<'a>(&'a GistStore, String);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(&self.1);
    }
}

/// Pseudo random bytes, different per seed, incompressible enough for a pack check.
fn binary_blob(len: usize, seed: u64) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64 ^ seed.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Line based ASCII text of exactly `len` bytes, like an index file.
fn text_blob(len: usize) -> String {
    let mut s = String::with_capacity(len + 64);
    let mut n = 0u64;
    while s.len() < len {
        s.push_str(&format!(
            "entry {n:08} {:016x}\n",
            n.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ));
        n += 1;
    }
    s.truncate(len);
    s
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn probe(name: &str, ok: bool, detail: &str) {
    eprintln!(
        "PROBE: {name} {} {detail}",
        if ok { "PASS" } else { "FAIL" }
    );
}

fn info(name: &str, detail: &str) {
    eprintln!("PROBE: {name} INFO {detail}");
}

/// Pace every remote call about 1 s apart.
fn pace() {
    std::thread::sleep(Duration::from_secs(1));
}

/// Runs git in `dir` like the backend does, HTTPS with gh as the credential helper. Stdout or stderr on failure.
fn git_try(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git_env(dir, args, &[])
}

fn git_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    pace();
    let out = Command::new("git")
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "credential.helper=!gh auth git-credential",
            "-c",
            "user.name=probe",
            "-c",
            "user.email=probe@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
    git_try(dir, args).unwrap_or_else(|e| panic!("git {args:?} failed: {e}"))
}

fn git_text(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git(dir, args)).trim().to_string()
}

fn clone(id: &str, parent: &Path, name: &str, extra: &[&str]) -> std::path::PathBuf {
    let url = format!("https://gist.github.com/{id}.git");
    let mut args = vec!["clone"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&[url.as_str(), name]);
    git(parent, &args);
    parent.join(name)
}

fn commit_push(dir: &Path, msg: &str) -> Result<Vec<u8>, String> {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-m", msg]);
    git_try(dir, &["push", "--no-verify", "origin", "HEAD"])
}

/// Adds files through a normal full clone and pushes.
fn push_files(id: &str, files: &[(&str, &[u8])]) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = clone(id, tmp.path(), "w", &[]);
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    commit_push(&dir, "probe").expect("push failed");
}

fn api(id: &str) -> serde_json::Value {
    pace();
    let out = Command::new("gh")
        .args(["api", &format!("gists/{id}")])
        .output()
        .unwrap();
    assert!(out.status.success(), "gh api failed");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Downloads `url` with curl (raw urls of secret gists need no token), returns the status and the body.
fn curl(url: &str, range: Option<&str>) -> (String, Vec<u8>) {
    pace();
    let tmp = tempfile::tempdir().unwrap();
    let body = tmp.path().join("body");
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-L", "-o"])
        .arg(&body)
        .args(["-w", "%{http_code}"]);
    if let Some(r) = range {
        cmd.args(["-H", &format!("Range: bytes={r}")]);
    }
    let out = cmd.arg(url).output().unwrap();
    let status = String::from_utf8_lossy(&out.stdout).into_owned();
    (status, std::fs::read(&body).unwrap_or_default())
}

/// Size of the object database in KiB, `size` (loose) plus `size-pack`.
fn odb_kib(dir: &Path) -> (u64, String) {
    let text = git_text(dir, &["count-objects", "-v"]);
    let get = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k))
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    (get("size:") + get("size-pack:"), text.replace('\n', " | "))
}

/// True when the blob `oid` is not in the local object database (rev-list does not lazy fetch).
fn blob_missing(dir: &Path, oid: &str) -> bool {
    git_text(dir, &["rev-list", "--objects", "--missing=print", "HEAD"])
        .lines()
        .any(|l| l == format!("?{oid}"))
}

fn new_gist() -> GistStore {
    let backend = GistStore::new();
    backend.check().unwrap();
    backend
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn append_from_partial_clone() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let d0 = binary_blob(3 * MIB, 1);
    let d1 = binary_blob(3 * MIB, 2);
    push_files(&id, &[("data.000", &d0), ("index", b"v1\n")]);

    let tmp = tempfile::tempdir().unwrap();
    let dir = clone(
        &id,
        tmp.path(),
        "partial",
        &["--depth", "1", "--filter=blob:none", "--no-checkout"],
    );
    let oid0 = git_text(&dir, &["rev-parse", "HEAD:data.000"]);
    let (before, before_raw) = odb_kib(&dir);
    info("append.before", &format!("odb_kib={before} {before_raw}"));
    let missing_before = blob_missing(&dir, &oid0);
    probe(
        "append.data000_not_local_after_clone",
        missing_before,
        &oid0,
    );

    // no checkout, so the index is empty, read the trees (no blobs needed) or the commit would drop data.000
    git(&dir, &["read-tree", "HEAD"]);
    std::fs::write(dir.join("data.001"), &d1).unwrap();
    std::fs::write(dir.join("index"), b"v2\n").unwrap();
    git(&dir, &["add", "data.001", "index"]);
    git(&dir, &["commit", "-m", "append"]);
    let pushed = git_try(&dir, &["push", "--no-verify", "origin", "HEAD"]);
    probe(
        "append.push_from_partial_clone",
        pushed.is_ok(),
        pushed.as_ref().err().map(String::as_str).unwrap_or("ok"),
    );
    let (after, after_raw) = odb_kib(&dir);
    info("append.after", &format!("odb_kib={after} {after_raw}"));
    info(
        "append.downloaded_kib_during_append",
        &format!(
            "{} (data.000 is {} KiB)",
            after.saturating_sub(before),
            3 * MIB / 1024
        ),
    );
    // the new blobs (data.001) are in the odb after the commit, so only a drop below the size of data.000 counts
    probe(
        "append.data000_never_fetched",
        blob_missing(&dir, &oid0),
        "data.000 still missing locally after commit and push",
    );
    pushed.expect("push from partial clone failed");

    let files = &api(&id)["files"];
    let size = |n: &str| files[n]["size"].as_u64();
    probe(
        "append.api_sizes",
        size("data.000") == Some(d0.len() as u64) && size("data.001") == Some(d1.len() as u64),
        &format!(
            "data.000={:?} data.001={:?} index={:?}",
            size("data.000"),
            size("data.001"),
            size("index")
        ),
    );
    let full = clone(&id, tmp.path(), "full", &[]);
    let got0 = std::fs::read(full.join("data.000")).unwrap();
    let got1 = std::fs::read(full.join("data.001")).unwrap();
    probe(
        "append.full_clone_data000_identical",
        got0 == d0,
        &sha(&got0),
    );
    probe(
        "append.full_clone_data001_identical",
        got1 == d1,
        &sha(&got1),
    );
    assert_eq!(size("data.000"), Some(d0.len() as u64));
    assert_eq!(size("data.001"), Some(d1.len() as u64));
    assert!(got0 == d0 && got1 == d1);
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn binary_part_round_trip() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let part = binary_blob(8 * MIB, 3);
    let want = sha(&part);
    push_files(&id, &[("data.000", &part)]);

    let tmp = tempfile::tempdir().unwrap();
    let dir = clone(
        &id,
        tmp.path(),
        "partial",
        &["--depth", "1", "--filter=blob:none", "--no-checkout"],
    );
    let via_clone = git(&dir, &["cat-file", "blob", "HEAD:data.000"]);
    probe(
        "part.partial_clone_cat_file",
        sha(&via_clone) == want,
        &format!("len={} sha={}", via_clone.len(), sha(&via_clone)),
    );

    let meta = api(&id);
    let raw_url = meta["files"]["data.000"]["raw_url"].as_str().unwrap_or("");
    info("part.raw_url", raw_url);
    let (status, body) = curl(raw_url, None);
    probe(
        "part.raw_url_full",
        status == "200" && sha(&body) == want,
        &format!(
            "status={status} len={} sha_match={}",
            body.len(),
            sha(&body) == want
        ),
    );
    let (status, body) = curl(raw_url, Some("0-1023"));
    probe(
        "part.raw_url_range",
        status == "206" && body == part[..1024],
        &format!(
            "status={status} len={} first_1024_match={}",
            body.len(),
            body == part[..1024]
        ),
    );
    assert_eq!(sha(&via_clone), want, "partial clone read differs");
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn index_through_api() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());

    let small = text_blob(700 * 1024);
    push_files(&id, &[("index", small.as_bytes())]);
    let f = api(&id)["files"]["index"].clone();
    let truncated = f["truncated"].as_bool();
    let same = f["content"].as_str() == Some(small.as_str());
    probe(
        "index.700KiB_not_truncated_identical",
        truncated == Some(false) && same,
        &format!(
            "truncated={truncated:?} content_identical={same} size={:?}",
            f["size"].as_u64()
        ),
    );

    let big = text_blob(MIB + 200 * 1024);
    push_files(&id, &[("index", big.as_bytes())]);
    let f = api(&id)["files"]["index"].clone();
    info(
        "index.1.2MiB",
        &format!(
            "truncated={:?} content_len={:?} size={:?}",
            f["truncated"].as_bool(),
            f["content"].as_str().map(str::len),
            f["size"].as_u64()
        ),
    );
    assert_eq!(truncated, Some(false));
    assert!(same);
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn file_cap() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let names: Vec<String> = (0..30).map(|i| format!("f{i:03}")).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), b"x" as &[u8])).collect();
    push_files(&id, &files);
    let meta = api(&id);
    let count = meta["files"].as_object().map_or(0, |o| o.len());
    info(
        "cap.files_listed",
        &format!(
            "pushed=30 (plus the create file) listed={count} truncated={:?}",
            meta["truncated"].as_bool()
        ),
    );
}

/// Label, git args before `--no-verify`, env.
type Variant<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);

/// Partial clone of `id` plus the oid of the existing `data.000`.
fn fresh(id: &str, parent: &Path, name: &str, extra: &[&str]) -> (std::path::PathBuf, String) {
    let mut args = vec!["--depth", "1", "--filter=blob:none"];
    args.extend_from_slice(extra);
    let dir = clone(id, parent, name, &args);
    let oid = git_text(&dir, &["rev-parse", "HEAD:data.000"]);
    (dir, oid)
}

/// PASS means data.000 is still not in the local object store.
fn report(step: &str, dir: &Path, oid: &str) {
    let missing = blob_missing(dir, oid);
    let (kib, _) = odb_kib(dir);
    probe(
        &format!("fetch.{step}"),
        missing,
        &format!("missing={missing} odb_kib={kib}"),
    );
}

/// Writes the next part and the index into the worktree, returns the part name.
fn write_new(dir: &Path, n: u32) -> String {
    let name = format!("data.{n:03}");
    std::fs::write(dir.join(&name), binary_blob(64 * 1024, u64::from(n))).unwrap();
    std::fs::write(dir.join("index"), format!("v{n}\n")).unwrap();
    name
}

/// Commits the new part and index without index file writes of the worktree: hash-object, update-index, write-tree, commit-tree, update-ref.
fn tree_commit(dir: &Path, name: &str) {
    git(dir, &["read-tree", "HEAD"]);
    for f in [name, "index"] {
        let oid = git_text(dir, &["hash-object", "-w", f]);
        git(
            dir,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{oid},{f}"),
            ],
        );
    }
    let tree = git_text(dir, &["write-tree"]);
    let c = git_text(dir, &["commit-tree", &tree, "-p", "HEAD", "-m", "append"]);
    git(dir, &["update-ref", "HEAD", &c]);
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn append_fetch_trigger() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    let version = Command::new("git").arg("--version").output().unwrap();
    info(
        "fetch.git_version",
        String::from_utf8_lossy(&version.stdout).trim(),
    );
    push_files(
        &id,
        &[("data.000", &binary_blob(3 * MIB, 1)), ("index", b"v0\n")],
    );
    let tmp = tempfile::tempdir().unwrap();
    let mut n = 0u32;
    let mut next = || {
        n += 1;
        n
    };

    // staged in one clone: clone, read-tree, add, plain commit
    let (dir, oid) = fresh(&id, tmp.path(), "c0", &["--no-checkout"]);
    report("clone", &dir, &oid);
    git(&dir, &["read-tree", "HEAD"]);
    report("read_tree", &dir, &oid);
    let name = write_new(&dir, next());
    git(&dir, &["add", &name, "index"]);
    report("add", &dir, &oid);
    git(&dir, &["commit", "-m", "append"]);
    report("commit_plain", &dir, &oid);

    let (dir, oid) = fresh(&id, tmp.path(), "c1", &["--no-checkout"]);
    git(&dir, &["read-tree", "HEAD"]);
    let name = write_new(&dir, next());
    git(&dir, &["add", &name, "index"]);
    git(
        &dir,
        &["-c", "core.fsmonitor=false", "commit", "-m", "append"],
    );
    report("commit_fsmonitor_off", &dir, &oid);

    let (dir, oid) = fresh(&id, tmp.path(), "c2", &["--no-checkout"]);
    let name = write_new(&dir, next());
    tree_commit(&dir, &name);
    report("commit_tree", &dir, &oid);

    // push variants, each from a fresh clone, commit via commit-tree so the push is the only step under test
    // label, full push args, env
    let variants: [Variant; 5] = [
        ("plain", &["push"], &[]),
        ("no_thin", &["push", "--no-thin"], &[]),
        ("no_bitmaps", &["-c", "pack.useBitmaps=false", "push"], &[]),
        (
            "noop_negotiation",
            &["-c", "fetch.negotiationAlgorithm=noop", "push"],
            &[],
        ),
        ("no_lazy_fetch", &["push"], &[("GIT_NO_LAZY_FETCH", "1")]),
    ];
    for (i, (label, pre, envs)) in variants.iter().enumerate() {
        let (dir, oid) = fresh(&id, tmp.path(), &format!("p{i}"), &["--no-checkout"]);
        let name = write_new(&dir, next());
        tree_commit(&dir, &name);
        let mut args: Vec<&str> = pre.to_vec();
        args.extend_from_slice(&["--no-verify", "origin", "HEAD"]);
        let pushed = git_env(&dir, &args, envs);
        info(
            &format!("fetch.push_{label}.result"),
            &match &pushed {
                Ok(_) => "ok".to_string(),
                Err(e) => format!("error {}", e.replace('\n', " | ")),
            },
        );
        report(&format!("push_{label}"), &dir, &oid);
    }

    // sparse checkout instead of --no-checkout
    let (dir, oid) = fresh(&id, tmp.path(), "s0", &["--sparse"]);
    report("clone_sparse_cone", &dir, &oid);
    let (dir, oid) = fresh(&id, tmp.path(), "s1", &["--no-checkout"]);
    git(&dir, &["sparse-checkout", "set", "--no-cone", "/index"]);
    git(&dir, &["checkout"]);
    report("sparse_noncone_checkout", &dir, &oid);
    let name = write_new(&dir, next());
    git(&dir, &["add", "--sparse", &name, "index"]);
    git(&dir, &["commit", "-m", "append"]);
    report("sparse_noncone_commit", &dir, &oid);
    let pushed = git_try(&dir, &["push", "--no-verify", "origin", "HEAD"]);
    info(
        "fetch.sparse_noncone_push.result",
        &pushed
            .as_ref()
            .err()
            .map_or("ok", String::as_str)
            .replace('\n', " | "),
    );
    report("sparse_noncone_push", &dir, &oid);
}

/// `updated_at` of gist `id` as the paginated listing shows it, the index cache keys on this value.
fn listing_updated_at(id: &str) -> String {
    pace();
    let jq = format!(".[] | select(.id == \"{id}\") | .updated_at");
    let out = Command::new("gh")
        .args(["api", "gists?per_page=100", "--paginate", "--jq", &jq])
        .output()
        .unwrap();
    assert!(out.status.success(), "gh api gists failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn api_updated_at(id: &str) -> (String, String) {
    let v = api(id);
    let at = v["updated_at"].as_str().unwrap_or_default().to_string();
    let version = v["history"][0]["version"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    (at, version)
}

#[test]
#[ignore = "creates a real secret gist, run by hand"]
fn updated_at_moves_on_git_push() {
    let backend = new_gist();
    let id = backend.create("probe", "v0\n").unwrap();
    let _cleanup = Cleanup(&backend, id.clone());
    push_files(&id, &[("index", b"v1\n")]);
    // updated_at has second resolution, keep the push clearly in a later second.
    std::thread::sleep(Duration::from_secs(2));
    let (api_before, version_before) = api_updated_at(&id);
    let list_before = listing_updated_at(&id);

    push_files(&id, &[("index", b"v2\n")]);
    let (mut api_after, mut version_after) = (api_before.clone(), version_before.clone());
    let mut list_after = list_before.clone();
    for _ in 0..10 {
        (api_after, version_after) = api_updated_at(&id);
        list_after = listing_updated_at(&id);
        if api_after != api_before && list_after != list_before {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let api_ok = !api_before.is_empty() && api_after != api_before;
    let list_ok = !list_before.is_empty() && list_after != list_before;
    probe(
        "updated_at.api",
        api_ok,
        &format!("before={api_before} after={api_after}"),
    );
    probe(
        "updated_at.listing",
        list_ok,
        &format!("before={list_before} after={list_after}"),
    );
    info(
        "updated_at.history_version",
        &format!("before={version_before} after={version_after}"),
    );
    assert!(api_ok && list_ok);
}
