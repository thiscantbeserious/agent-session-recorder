//! Individual transforms for content cleaning.
//!
//! These transforms implement the [`crate::asciicast::Transform`] trait and
//! can be composed into a pipeline for cleaning asciicast event data.
//!
//! - [`ContentCleaner`] - Single-pass ANSI/control/spinner stripping
//! - [`DeduplicateProgressLines`] - Keeps only final state of `\r`-rewritten lines
//! - [`NormalizeWhitespace`] - Collapses excessive whitespace
//! - [`FilterEmptyEvents`] - Removes events with no remaining content
//! - [`SimilarityFilter`] - Collapses consecutive lines that are highly similar
//! - [`BlockTruncator`] - Truncates large contiguous blocks of output
//! - [`EventCoalescer`] - Merges rapid, similar consecutive events
//! - [`GlobalDeduplicator`] - Caps global line frequency and hashes redundant redraws

mod aggressive;
mod cleaner;
mod dedupe;
mod noise;
mod normalize;
mod terminal;

pub use aggressive::{
    BlockTruncator, EventCoalescer, FileDumpFilter, GlobalDeduplicator, SimilarityFilter,
    WindowedLineDeduplicator,
};
pub use cleaner::ContentCleaner;
pub use dedupe::DeduplicateProgressLines;
pub use normalize::{EmptyLineFilter, FilterEmptyEvents, NormalizeWhitespace};
pub use terminal::TerminalTransform;
