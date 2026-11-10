//! The merged pack view as scan rows: what the remote holds for each path.

use std::collections::BTreeMap;

use super::index::Index;
use super::plan::{Local, Remote};
use super::scan::{row, valid_path, Row};
use crate::sync::crypto::Meta;
use crate::sync::pack::merge::{Merged, Rejected, Top, View};

/// Joins the view into `rows` (local sessions keyed by path). A path that cannot be a session
/// path or a pack that is rejected becomes an opaque `remote:<pack>` row that is never acted on.
pub(super) fn attach_view(rows: &mut BTreeMap<String, Row>, view: &View, index: &Index) {
    for (path, merged) in view.iter() {
        if !valid_path(path) {
            let name = format!("remote:{}", merged.keeper().pack);
            opaque(rows, name, Remote::UnknownVersion, index);
            continue;
        }
        let entry = rows
            .entry(path.clone())
            .or_insert_with(|| row(path, Local::Absent, index));
        entry.input.remote = remote_of(merged);
        entry.gen = merged.gen;
        entry.meta = meta_of(path, merged);
    }
    for (id, why) in view.rejected() {
        let remote = match why {
            Rejected::OtherKey => Remote::OtherKey,
            _ => Remote::UnknownVersion,
        };
        opaque(rows, format!("remote:{id}"), remote, index);
    }
}

fn opaque(rows: &mut BTreeMap<String, Row>, path: String, remote: Remote, index: &Index) {
    let mut opaque = row(&path, Local::Absent, index);
    opaque.input.remote = remote;
    rows.insert(path, opaque);
}

fn remote_of(merged: &Merged) -> Remote {
    match &merged.top {
        Top::Live { sha256 } => Remote::Hash(sha256.clone()),
        Top::Deleted { gen } => Remote::Deleted { gen: *gen },
        Top::Fork => Remote::Fork,
    }
}

/// What the keeper place says about the live session, `None` for a tombstone or a fork.
fn meta_of(path: &str, merged: &Merged) -> Option<Meta> {
    let Top::Live { sha256 } = &merged.top else {
        return None;
    };
    Some(Meta {
        v: 2,
        path: path.to_string(),
        sha256: sha256.clone(),
        size: merged.keeper().size,
        gen: merged.gen,
    })
}
