//! The last place rule: a pack goes only when everything live in it has a place elsewhere.

use crate::sync::pack::merge::View;
use std::collections::BTreeSet;

/// `planned` without the packs that still hold the last place of a session in `fresh`. Places
/// of one identity (same hash, or tombstones) are one group, a fork has one group per hash.
pub(super) fn deletable(fresh: &View, planned: &[String]) -> Vec<String> {
    let mut gone: BTreeSet<&str> = planned
        .iter()
        .map(String::as_str)
        .filter(|id| fresh.pack(id).is_some())
        .collect();
    loop {
        let mut keep: BTreeSet<&str> = BTreeSet::new();
        for (_, m) in fresh.iter() {
            let mut groups: Vec<(bool, &str, Vec<&str>)> = Vec::new();
            for p in &m.places {
                let id = if p.deleted { "" } else { p.sha256.as_str() };
                match groups.iter_mut().find(|g| g.0 == p.deleted && g.1 == id) {
                    Some(g) => g.2.push(&p.pack),
                    None => groups.push((p.deleted, id, vec![&p.pack])),
                }
            }
            for (_, _, packs) in groups {
                if packs.iter().all(|p| gone.contains(p)) {
                    keep.extend(packs);
                }
            }
        }
        if keep.is_empty() {
            break;
        }
        gone.retain(|id| !keep.contains(id));
    }
    planned
        .iter()
        .filter(|id| gone.contains(id.as_str()))
        .cloned()
        .collect()
}
