//! Source text boundaries of the update feature.

use std::fs;
use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn no_network_outside_update_module() {
    let src = root().join("src");
    let update = src.join("update");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    for f in files.iter().filter(|f| !f.starts_with(&update)) {
        let text = fs::read_to_string(f).unwrap();
        assert!(!text.contains("\"curl\""), "curl in {}", f.display());
        assert!(
            !text.contains("AGR_UPDATE_RELEASE_BASE"),
            "override in {}",
            f.display()
        );
    }
}

#[test]
fn override_not_documented() {
    let mut files = vec![root().join("README.md")];
    let mut stack = vec![root().join("docs")];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                files.push(p);
            }
        }
    }
    for f in files {
        if let Ok(text) = fs::read_to_string(&f) {
            assert!(!text.contains("AGR_UPDATE_RELEASE_BASE"), "{}", f.display());
        }
    }
}
