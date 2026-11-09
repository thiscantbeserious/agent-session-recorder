//! Repo: the merged view of one store and the writes of one machine.

mod push;
mod read;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dir;
#[cfg(test)]
mod tests_fits;
#[cfg(test)]
mod tests_legacy;
#[cfg(test)]
mod tests_read;
#[cfg(test)]
mod tests_retry;
#[cfg(test)]
mod tests_session;

pub use push::{PushItem, PushReport};
pub use read::{read_pack, ReadError};

use crate::sync::crypto::{open_meta, Key};
use crate::sync::pack::index::{open_index, Index};
use crate::sync::pack::merge::{merge, PackIn, View};
use crate::sync::store::{PackStore, StoreError};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("no such session: {0}")]
    Unknown(String),
    #[error("run `agr sync migrate` first")]
    NeedsMigrate,
    #[error("could not read every pack index: {0}")]
    PartialView(StoreError),
}

/// The open pack of this writer with the index text it was planned against.
#[derive(Clone)]
pub(super) struct OpenPack {
    pub id: String,
    pub tag: String,
    pub text: String,
    pub index: Index,
}

/// Index text and opened index per pack id.
type Packs = BTreeMap<String, (String, Index)>;

pub struct Repo<'a> {
    pub(super) store: &'a dyn PackStore,
    pub(super) key: &'a Key,
    writer: String,
    target: u64,
    view: View,
    /// Opened index text and index per pack id.
    packs: Packs,
    open: Option<OpenPack>,
}

/// One pass over every pack: list, read each index, open, merge. One failed read fails the pass,
/// a view of part of the packs would plan against data that is not there.
fn load(store: &dyn PackStore, key: &Key) -> Result<(View, Packs), StoreError> {
    let (mut ins, mut packs) = (Vec::new(), BTreeMap::new());
    for p in store.list()?.packs {
        let text = store.read_index(&p.id)?;
        let index = open_index(key, &p.tag, &text);
        if let Ok(i) = &index {
            packs.insert(p.id.clone(), (text, i.clone()));
        }
        ins.push(PackIn {
            id: p.id,
            tag: p.tag,
            index,
        });
    }
    Ok((merge(&ins), packs))
}

impl<'a> Repo<'a> {
    /// Like `open_for_migrate`, but refuses while an `agr:v1` gist of our key is listed.
    pub fn open(
        store: &'a dyn PackStore,
        key: &'a Key,
        writer: &str,
        target: u64,
        hint: Option<&str>,
    ) -> Result<Self, RepoError> {
        let legacy = store.list()?.legacy;
        let ours = |d: &String| {
            let token = d.strip_prefix("agr:v1 ").unwrap_or_default().trim();
            open_meta(key, token).is_ok()
        };
        if legacy.iter().any(ours) {
            return Err(RepoError::NeedsMigrate);
        }
        Self::open_for_migrate(store, key, writer, target, hint)
    }

    /// `hint` is the open pack id the app saved locally, `target` the pack size in bytes.
    pub fn open_for_migrate(
        store: &'a dyn PackStore,
        key: &'a Key,
        writer: &str,
        target: u64,
        hint: Option<&str>,
    ) -> Result<Self, RepoError> {
        let (view, packs) = load(store, key).map_err(RepoError::PartialView)?;
        let mut repo = Self {
            store,
            key,
            writer: writer.to_string(),
            target,
            view,
            packs,
            open: None,
        };
        repo.open = repo.pick_open(hint);
        Ok(repo)
    }

    /// Reads every index again, the view then shows what the last append wrote.
    pub(super) fn reload(&mut self) -> Result<(), StoreError> {
        (self.view, self.packs) = load(self.store, self.key)?;
        Ok(())
    }

    pub(super) fn packs(&self) -> &Packs {
        &self.packs
    }

    fn usable(&self, id: &str) -> bool {
        let ours = self
            .view
            .pack(id)
            .is_some_and(|m| m.writer == self.writer && !m.sealed);
        ours && !self.view.is_frozen(id)
    }

    /// The saved id when it still is our open pack, else any unsealed pack of our writer.
    /// A pack with a copy is never used, the next push starts a new one.
    fn pick_open(&self, hint: Option<&str>) -> Option<OpenPack> {
        let id = hint.filter(|h| self.usable(h)).or_else(|| {
            self.packs
                .keys()
                .map(String::as_str)
                .find(|id| self.usable(id))
        })?;
        let (text, index) = self.packs.get(id)?.clone();
        let tag = self.view.pack(id)?.tag.clone();
        Some(OpenPack {
            id: id.to_string(),
            tag,
            text,
            index,
        })
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn open_pack(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.id.as_str())
    }
}
