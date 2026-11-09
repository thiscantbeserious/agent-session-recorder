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
    pub forks: Vec<String>,
    pub rejected: Vec<(String, Rejected)>,
    pub duplicates: Vec<(String, String)>,
    pub dead_bytes: BTreeMap<String, u64>,
    frozen: BTreeSet<String>,
}

impl View {
    pub fn get(&self, path: &str) -> Option<&Merged> {
        self.merged.get(path)
    }

    pub fn pack(&self, id: &str) -> Option<&PackMeta> {
        self.packs.get(id)
    }

    pub fn is_frozen(&self, id: &str) -> bool {
        self.frozen.contains(id)
    }
}

pub fn merge(packs: &[PackIn]) -> View {
    let mut view = View::default();
    for p in packs {
        if let Err(err) = &p.index {
            view.rejected.push((p.id.clone(), reject(err)));
        }
        if let Ok(idx) = &p.index {
            let all: u64 = idx.entries.iter().map(|e| e.length).sum();
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
    duplicates(packs, &mut view);
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
fn duplicates(packs: &[PackIn], view: &mut View) {
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
        view.frozen.extend(ids.iter().map(|id| id.to_string()));
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
        if let Some(d) = view.dead_bytes.get_mut(&m.keeper().pack) {
            *d -= m.keeper().length;
        }
        if m.places
            .iter()
            .any(|p| p.identity() != m.places[0].identity())
        {
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
        deleted: e.deleted,
    }
}
