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
        if let Ok(idx) = &p.index {
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
    finish(&mut view);
    view
}

fn finish(view: &mut View) {
    for (path, m) in view.merged.iter_mut() {
        if m.places.iter().any(|p| p.sha256 != m.places[0].sha256) {
            m.top = Top::Fork;
            view.forks.push(path.clone());
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
    }
}
