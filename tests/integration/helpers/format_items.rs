//! Hand built sync items and reports for the formatter tests.

use agr::sync::{Decision, Item, Outcome, Report};

pub fn item(path: &str, decision: Decision, outcome: Outcome) -> Item {
    Item {
        path: path.into(),
        decision,
        outcome,
        size: 2048,
        on_remote: true,
        on_local: true,
        local_hash: Some("a".repeat(64)),
        remote_hash: Some("b".repeat(64)),
        remote_size: 4096,
    }
}

pub fn sync() -> Decision {
    Decision::InSync {
        refresh_index: false,
    }
}

pub fn report(items: Vec<Item>) -> Report {
    Report {
        items,
        ..Report::default()
    }
}
