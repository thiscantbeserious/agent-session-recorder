//! Generic async LRU cache with background loading
//!
//! Re-exports `AsyncLruCache<K, V>` and provides a `PreviewCache`
//! type alias for session preview loading.

pub mod cache;
pub mod worker;

pub use cache::AsyncLruCache;

use super::widgets::SessionPreview;

/// Preview cache specialized for session preview loading.
///
/// Drop-in replacement for the former `preview_cache::PreviewCache`.
pub type PreviewCache = AsyncLruCache<String, SessionPreview>;

/// Create a new `PreviewCache` with the default capacity (20 entries).
///
/// Uses `SessionPreview::load` as the background loader.
pub fn new_preview_cache() -> PreviewCache {
    AsyncLruCache::new(20, |path| SessionPreview::load(path))
}
