//! Repo: the merged view of one store and the writes of one machine.

mod push;
mod read;

pub use push::{OpenRef, Outcome, PushError, PushItem, PushReport};
pub use read::{read_pack, ReadError};

use crate::sync::crypto::{open_meta, Key};
use crate::sync::pack::index::{open_index, Index};
use crate::sync::pack::merge::{merge, PackIn, View};
use crate::sync::pack::rules::MAX_TARGET;
use crate::sync::store::{PackInfo, PackStore, StoreError};
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
    #[error("pack target of {0} bytes is above the {max} byte limit", max = MAX_TARGET)]
    Target(u64),
    #[error("could not list the packs: {0}")]
    List(StoreError),
}

/// The open pack of this writer with the index text it was planned against.
#[derive(Clone)]
pub(super) struct OpenPack {
    pub id: String,
    pub tag: String,
    /// Data files of the pack, as listed and grown by this run's appends.
    pub files: usize,
    pub text: String,
    pub index: Index,
}

impl OpenPack {
    /// The pack as a listing would show it, for `fits`.
    pub(super) fn info(&self) -> PackInfo {
        PackInfo {
            id: self.id.clone(),
            tag: self.tag.clone(),
            len: self.index.len,
            updated_at: String::new(),
            files: self.files,
        }
    }
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
    files: Files,
    open: Option<OpenPack>,
    /// Prune copies: no append seals its pack, ADR "Last place rule".
    keep_open: bool,
}

/// Data file count per pack id.
type Files = BTreeMap<String, usize>;

/// One pass over the listed packs: read each index, open, merge. One failed read fails the pass,
/// a view of part of the packs would plan against data that is not there.
fn load(
    store: &dyn PackStore,
    key: &Key,
    listed: Vec<PackInfo>,
) -> Result<(View, Packs, Files), StoreError> {
    let (mut ins, mut packs, mut files) = (Vec::new(), BTreeMap::new(), BTreeMap::new());
    for p in listed {
        let text = store.read_index(&p.id)?;
        files.insert(p.id.clone(), p.files);
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
    Ok((merge(&ins), packs, files))
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
        Self::open_with(store, key, (writer, target, hint), true)
    }

    /// `open`, `sweep` false leaves the local clones alone, for runs that do not hold the lock.
    pub fn open_with(
        store: &'a dyn PackStore,
        key: &'a Key,
        (writer, target, hint): (&str, u64, Option<&str>),
        sweep: bool,
    ) -> Result<Self, RepoError> {
        let listing = store.list().map_err(RepoError::List)?;
        let legacy = &listing.legacy;
        let ours = |d: &String| {
            let token = d.strip_prefix("agr:v1 ").unwrap_or_default().trim();
            open_meta(key, token).is_ok()
        };
        if legacy.iter().any(ours) {
            return Err(RepoError::NeedsMigrate);
        }
        Self::from_listing(store, key, (writer, target, hint), listing.packs, sweep)
    }

    /// `hint` is the open pack id the app saved locally, `target` the pack size in bytes.
    pub fn open_for_migrate(
        store: &'a dyn PackStore,
        key: &'a Key,
        writer: &str,
        target: u64,
        hint: Option<&str>,
    ) -> Result<Self, RepoError> {
        let listing = store.list_for_migrate().map_err(RepoError::List)?;
        Self::from_listing(store, key, (writer, target, hint), listing.packs, true)
    }

    fn from_listing(
        store: &'a dyn PackStore,
        key: &'a Key,
        (writer, target, hint): (&str, u64, Option<&str>),
        listed: Vec<PackInfo>,
        sweep: bool,
    ) -> Result<Self, RepoError> {
        if target > MAX_TARGET {
            return Err(RepoError::Target(target));
        }
        let (view, packs, files) = load(store, key, listed).map_err(RepoError::PartialView)?;
        let mut repo = Self {
            store,
            key,
            writer: writer.to_string(),
            target,
            view,
            packs,
            files,
            open: None,
            keep_open: false,
        };
        repo.open = repo.pick_open(hint);
        // once per open: a crash before `sealed` or a frozen pack leaves clones nobody drops
        if sweep {
            store.keep_only(repo.open_pack());
        }
        Ok(repo)
    }

    /// Reads every index again, the view then shows what the last append wrote.
    pub(super) fn reload(&mut self) -> Result<(), StoreError> {
        let listed = self.store.list()?.packs;
        (self.view, self.packs, self.files) = load(self.store, self.key, listed)?;
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
            files: self.files.get(id).copied().unwrap_or(0),
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
