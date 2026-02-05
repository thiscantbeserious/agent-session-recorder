//! Individual transforms for content cleaning.
//!
//! These transforms implement the [`crate::asciicast::Transform`] trait and
//! can be composed into a pipeline for cleaning asciicast event data.
//!
//! - [`ContentCleaner`] - Single-pass ANSI/control/spinner stripping
//! - [`DeduplicateProgressLines`] - Keeps only final state of `\r`-rewritten lines
//! - [`NormalizeWhitespace`] - Collapses excessive whitespace
//! - [`FilterEmptyEvents`] - Removes events with no remaining content

mod cleaner;
mod dedupe;
mod normalize;
mod aggressive;

pub use cleaner::ContentCleaner;
pub use dedupe::DeduplicateProgressLines;
pub use normalize::{FilterEmptyEvents, NormalizeWhitespace};
pub use aggressive::{BlockTruncator, EventCoalescer, GlobalDeduplicator, SimilarityFilter};
