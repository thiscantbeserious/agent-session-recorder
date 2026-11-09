//! Progress observer of the engine, called per acted-on item, the engine itself never prints.

use super::run::{Item, Outcome};

/// Called after each acted-on item with the item, its 1-based index and the total.
pub type Observer<'a> = dyn FnMut(&Item, usize, usize) + 'a;

pub(super) fn no_progress(_: &Item, _: usize, _: usize) {}

/// Calls `observer` for done, skipped and failed items only, counting from 1.
pub(super) fn counted<'o>(total: usize, observer: &'o mut Observer<'o>) -> impl FnMut(&Item) + 'o {
    let mut seen = 0;
    move |item: &Item| {
        if matches!(
            item.outcome,
            Outcome::Done(_) | Outcome::Skipped(_) | Outcome::Failed(_)
        ) {
            seen += 1;
            observer(item, seen, total);
        }
    }
}
