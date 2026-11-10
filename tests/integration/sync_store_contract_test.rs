//! The pack store contract against the memory, dir and gist store.

use crate::helpers::fake_remote::Handle;
use crate::helpers::gist_rig::Rig;
use agr::sync::testing::{DirStore, GistStore, MemoryStore};

mod memory {
    use super::MemoryStore;

    contract_tests!(|| (MemoryStore::new(), ()));
}

mod dir {
    use super::DirStore;

    contract_tests!(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        (DirStore::new(dir.path()), dir)
    });
}

mod gist {
    use super::{GistStore, Handle, Rig};

    contract_tests!(|| {
        let rig = Rig::new();
        let store: GistStore<Handle> = rig.store();
        (store, rig)
    });
}
