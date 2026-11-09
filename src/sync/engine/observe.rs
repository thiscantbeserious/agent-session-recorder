//! Progress observer of the engine, called per acted-on item, the engine itself never prints.

use super::run::{Item, Outcome};

/// What the engine reports while a push or pull runs, `i` counts from 1 and `n` is the total.
#[derive(Debug)]
pub enum Event<'a> {
    /// A session is about to be uploaded or downloaded, `size` is the plaintext size.
    Started {
        path: &'a str,
        size: u64,
        i: usize,
        n: usize,
    },
    /// A session is done, skipped or failed.
    Finished { item: &'a Item, i: usize, n: usize },
}

/// Called for every event, `Started` always comes before the `Finished` of the same session.
pub type Observer<'a> = dyn FnMut(&Event<'_>) + 'a;

/// What the engine loop hands over, `counted` adds the index and the total.
pub(super) enum Step<'a> {
    Started { path: &'a str, size: u64 },
    Finished(&'a Item),
}

pub(super) fn no_progress(_: &Event<'_>) {}

/// Turns steps into events, `Finished` only for done, skipped and failed items, counting from 1.
pub(super) fn counted<'o>(
    total: usize,
    observer: &'o mut Observer<'o>,
) -> impl FnMut(Step<'_>) + 'o {
    let mut seen = 0;
    move |step: Step<'_>| {
        if let Step::Finished(item) = step {
            if matches!(
                item.outcome,
                Outcome::Done(_) | Outcome::Skipped(_) | Outcome::Failed(_)
            ) {
                seen += 1;
                observer(&Event::Finished {
                    item,
                    i: seen,
                    n: total,
                });
            }
        }
    }
}
