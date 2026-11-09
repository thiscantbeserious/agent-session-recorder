//! Repo: the merged view of one store and the writes of one machine.

mod push;
#[cfg(test)]
mod tests;

pub use push::{PushItem, PushReport};

use crate::sync::crypto::Key;
use crate::sync::pack::index::{open_index, Index};
use crate::sync::pack::merge::{merge, PackIn, View};
use crate::sync::store::{PackStore, StoreError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
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

pub struct Repo<'a> {
    store: &'a dyn PackStore,
    key: &'a Key,
    writer: String,
    #[allow(dead_code)] // read by the seal rules in the next commit
    target: u64,
    view: View,
    open: Option<OpenPack>,
}

/// One pass over every pack: list, read each index, open, merge.
fn load(store: &dyn PackStore, key: &Key) -> Result<View, StoreError> {
    let mut ins = Vec::new();
    for p in store.list()?.packs {
        let Ok(text) = store.read_index(&p.id) else {
            continue;
        };
        let index = open_index(key, &p.tag, &text);
        ins.push(PackIn {
            id: p.id,
            tag: p.tag,
            index,
        });
    }
    Ok(merge(&ins))
}

impl<'a> Repo<'a> {
    /// `hint` is the open pack id the app saved locally, `target` the pack size in bytes.
    pub fn open(
        store: &'a dyn PackStore,
        key: &'a Key,
        writer: &str,
        target: u64,
        _hint: Option<&str>,
    ) -> Result<Self, RepoError> {
        let view = load(store, key).map_err(RepoError::PartialView)?;
        Ok(Self {
            store,
            key,
            writer: writer.to_string(),
            target,
            view,
            open: None,
        })
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn open_pack(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.id.as_str())
    }
}
