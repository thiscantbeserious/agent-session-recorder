//! Pure merge of opened pack indexes into one view, no store and no IO.

use crate::sync::pack::index::{Entry, Index, IndexError};
use std::collections::{BTreeMap, BTreeSet};

/// One pack as the store listed it plus the result of opening its index.
#[derive(Debug)]
pub struct PackIn {
    pub id: String,
    pub tag: String,
    pub index: Result<Index, IndexError>,
}

/// Why a pack index is reported and never merged.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rejected {
    OtherKey,
    OtherPack,
    UnknownVersion(u8),
    Bad,
}

/// What sits on top of a path: a live hash, a tombstone or several hashes at the top gen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Top {
    Live { sha256: String },
    Deleted { gen: u64 },
    Fork,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub pack: String,
    pub offset: u64,
    pub length: u64,
    pub sha256: String,
    /// Plaintext size of the session, 0 for a tombstone.
    pub size: u64,
    pub deleted: bool,
}

impl Place {
    /// Tombstones are all alike, live places differ by hash.
    fn identity(&self) -> Option<&str> {
        (!self.deleted).then_some(self.sha256.as_str())
    }
}

/// One path at its highest gen with every place that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub gen: u64,
    pub top: Top,
    pub places: Vec<Place>,
}

impl Merged {
    pub fn is_deleted(&self) -> bool {
        matches!(self.top, Top::Deleted { .. })
    }

    /// The place in the highest pack id, `finish` sorts the places that way.
    pub fn keeper(&self) -> &Place {
        &self.places[0]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMeta {
    pub tag: String,
    pub writer: String,
    pub sealed: bool,
    pub len: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct View {
    merged: BTreeMap<String, Merged>,
    packs: BTreeMap<String, PackMeta>,
    forks: Vec<String>,
    rejected: Vec<(String, Rejected)>,
    duplicates: Vec<(String, String)>,
    dead_bytes: BTreeMap<String, u64>,
    frozen: BTreeSet<String>,
}

impl View {
    pub fn get(&self, path: &str) -> Option<&Merged> {
        self.merged.get(path)
    }

    /// Every path of the view with its merged entry, sorted by path.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Merged)> {
        self.merged.iter()
    }

    pub fn pack(&self, id: &str) -> Option<&PackMeta> {
        self.packs.get(id)
    }

    pub fn forks(&self) -> &[String] {
        &self.forks
    }

    pub fn rejected(&self) -> &[(String, Rejected)] {
        &self.rejected
    }

    pub fn duplicates(&self) -> &[(String, String)] {
        &self.duplicates
    }

    /// Bytes per pack id that no path needs any more, a fork keeps all its top gen versions out.
    pub fn dead_bytes(&self) -> &BTreeMap<String, u64> {
        &self.dead_bytes
    }

    pub fn is_frozen(&self, id: &str) -> bool {
        self.frozen.contains(id)
    }
}

/// Per pack id only the newest index counts, a pack only grows, so the longest one is the newest.
fn newest(packs: &[PackIn]) -> Vec<&PackIn> {
    let mut by_id: BTreeMap<&str, &PackIn> = BTreeMap::new();
    for p in packs {
        let best = by_id.entry(&p.id).or_insert(p);
        if rank(p) > rank(best) {
            *best = p;
        }
    }
    by_id.into_values().collect()
}

/// A total order, a zero byte seal ties on len and entries, so sealed ranks above unsealed and
/// the index itself breaks what is left, the pick never depends on the listing order.
fn rank(p: &PackIn) -> Option<(u64, usize, bool, &Index)> {
    let i = p.index.as_ref().ok()?;
    Some((i.len, i.entries.len(), i.sealed, i))
}

pub fn merge(packs: &[PackIn]) -> View {
    let mut view = View::default();
    let packs = newest(packs);
    for p in packs.iter().copied() {
        if let Err(err) = &p.index {
            view.rejected.push((p.id.clone(), reject(err)));
        }
        if let Ok(idx) = &p.index {
            let all = idx
                .entries
                .iter()
                .fold(0u64, |a, e| a.saturating_add(e.length));
            view.dead_bytes.insert(p.id.clone(), all);
            view.packs.insert(p.id.clone(), meta(&p.tag, idx));
            for e in &idx.entries {
                match view.merged.get_mut(&e.path) {
                    None => {
                        view.merged.insert(e.path.clone(), first(&p.id, e));
                    }
                    Some(cur) if e.gen > cur.gen => *cur = first(&p.id, e),
                    Some(cur) if e.gen == cur.gen => cur.places.push(place(&p.id, e)),
                    Some(_) => {}
                }
            }
        }
    }
    duplicates(&packs, &mut view);
    finish(&mut view);
    view
}

fn meta(tag: &str, idx: &Index) -> PackMeta {
    PackMeta {
        tag: tag.to_string(),
        writer: idx.writer.clone(),
        sealed: idx.sealed,
        len: idx.len,
    }
}

/// Packs with one tag are copies of each other, every pair is reported and both ids are frozen.
fn duplicates(packs: &[&PackIn], view: &mut View) {
    let mut by_tag: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for p in packs.iter().filter(|p| p.index.is_ok()) {
        by_tag.entry(&p.tag).or_default().insert(&p.id);
    }
    for ids in by_tag.values().filter(|ids| ids.len() > 1) {
        let ids: Vec<&str> = ids.iter().copied().collect();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                view.duplicates.push((a.to_string(), b.to_string()));
            }
        }
        view.frozen.extend(ids.iter().map(ToString::to_string));
    }
}

fn reject(err: &IndexError) -> Rejected {
    match err {
        IndexError::OtherKey => Rejected::OtherKey,
        IndexError::OtherPack => Rejected::OtherPack,
        IndexError::UnknownVersion(v) => Rejected::UnknownVersion(*v),
        _ => Rejected::Bad,
    }
}

fn finish(view: &mut View) {
    for (path, m) in view.merged.iter_mut() {
        m.places.sort_by(|a, b| b.pack.cmp(&a.pack));
        let fork = m
            .places
            .iter()
            .any(|p| p.identity() != m.places[0].identity());
        // a fork keeps every top gen version live until it is resolved, else only the keeper
        let live = if fork { &m.places[..] } else { &m.places[..1] };
        for p in live {
            if let Some(d) = view.dead_bytes.get_mut(&p.pack) {
                // remote data decides the lengths, a saturated sum must not panic
                *d = d.saturating_sub(p.length);
            }
        }
        if fork {
            m.top = Top::Fork;
            view.forks.push(path.clone());
        } else if m.places[0].deleted {
            m.top = Top::Deleted { gen: m.gen };
        }
    }
}

fn first(pack: &str, e: &Entry) -> Merged {
    Merged {
        gen: e.gen,
        top: Top::Live {
            sha256: e.sha256.clone(),
        },
        places: vec![place(pack, e)],
    }
}

fn place(pack: &str, e: &Entry) -> Place {
    Place {
        pack: pack.to_string(),
        offset: e.offset,
        length: e.length,
        sha256: e.sha256.clone(),
        size: e.size,
        deleted: e.deleted,
    }
}
