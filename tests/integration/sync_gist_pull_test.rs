use crate::helpers::gist_fake::{backend, remote_parts, scrub, Bytes, Fake};
use agr::sync::testing::gist::part_name;
use agr::sync::{BackendError, SyncBackend};
use std::io::Read;

#[test]
fn pull_clones_and_reads_the_blob() {
    let fake = Fake {
        remote: vec![(part_name(0), vec![0, 159, 146, 150])],
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).pull_b("abc123");
    assert_eq!(got, Ok((None, vec![0, 159, 146, 150])));
    assert_eq!(fake.calls().len(), 1, "one clone, nothing else");
    let calls = scrub(&fake.calls(), base.path());
    assert_eq!(
        calls,
        ["git -c credential.helper= -c credential.helper=!gh auth git-credential clone --depth 1 --progress https://gist.github.com/abc123.git <tmp>/agr-sync-N/clone"]
    );
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn pull_without_data_files_errors() {
    let fake = Fake::default();
    let base = tempfile::tempdir().unwrap();
    assert!(matches!(
        backend(&fake, base.path()).pull_b("abc123"),
        Err(BackendError::Other(_))
    ));
}

#[test]
fn pull_joins_all_parts_in_order() {
    let fake = Fake {
        remote: vec![
            (part_name(1), b"world".to_vec()),
            (part_name(0), b"hello ".to_vec()),
        ],
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).pull_b("abc123");
    assert_eq!(got, Ok((None, b"hello world".to_vec())));
}

#[test]
fn pull_with_a_missing_part_errors() {
    let mut remote = remote_parts(3);
    remote.remove(1);
    let fake = Fake {
        remote,
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).pull_b("abc123");
    assert!(
        matches!(&got, Err(e) if e.to_string().contains("data.001")),
        "{got:?}"
    );
}

#[test]
fn the_pull_reader_owns_the_clone_until_it_is_dropped() {
    let fake = Fake {
        remote: remote_parts(2),
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let pulled = backend(&fake, base.path()).pull("abc123");
    let Ok((_, mut reader)) = pulled else {
        panic!("pull failed: {:?}", pulled.err());
    };
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 1);
    let mut all = Vec::new();
    reader.read_to_end(&mut all).unwrap();
    assert_eq!(all, [0, 0, 0, 1, 1, 1]);
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 1);
    drop(reader);
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}

#[test]
fn a_pull_with_a_missing_part_errors_before_any_read_and_cleans_up() {
    let mut remote = remote_parts(3);
    remote.remove(1);
    let fake = Fake {
        remote,
        ..Fake::default()
    };
    let base = tempfile::tempdir().unwrap();
    let got = backend(&fake, base.path()).pull("abc123");
    assert!(got.is_err());
    assert_eq!(std::fs::read_dir(base.path()).unwrap().count(), 0);
}
