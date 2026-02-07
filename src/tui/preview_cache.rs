//! Async preview cache with LRU eviction and pre-fetching
//!
//! Provides non-blocking preview loading for TUI applications.
//! Features:
//! - LRU cache to avoid re-parsing files
//! - Background thread loading
//! - Pre-fetching of adjacent items

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;

use super::widgets::SessionPreview;

/// Result of a background preview load
struct LoadResult {
    path: String,
    preview: Option<SessionPreview>,
}

/// Async preview cache with LRU eviction
pub struct PreviewCache {
    /// Cached previews (path -> preview)
    cache: HashMap<String, SessionPreview>,
    /// LRU order (front = oldest, back = newest)
    lru_order: VecDeque<String>,
    /// Maximum cache size
    max_size: usize,
    /// Paths currently being loaded
    pending: std::collections::HashSet<String>,
    /// Channel sender for load requests (path)
    request_tx: Sender<String>,
    /// Channel receiver for load results
    result_rx: Receiver<LoadResult>,
}

impl PreviewCache {
    /// Create a new preview cache with the given max size
    pub fn new(max_size: usize) -> Self {
        let (request_tx, request_rx) = channel::<String>();
        let (result_tx, result_rx) = channel::<LoadResult>();

        // Spawn worker thread
        thread::spawn(move || {
            Self::worker_loop(request_rx, result_tx);
        });

        Self {
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
            max_size,
            pending: std::collections::HashSet::new(),
            request_tx,
            result_rx,
        }
    }

    /// Worker thread that processes load requests
    fn worker_loop(request_rx: Receiver<String>, result_tx: Sender<LoadResult>) {
        while let Ok(path) = request_rx.recv() {
            let preview = SessionPreview::load(&path);
            // Ignore send errors (main thread may have exited)
            let _ = result_tx.send(LoadResult { path, preview });
        }
    }

    /// Poll for completed loads and add them to cache
    pub fn poll(&mut self) {
        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.pending.remove(&result.path);
                    if let Some(preview) = result.preview {
                        self.insert(result.path, preview);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Get a preview from cache (returns None if not cached yet)
    pub fn get(&mut self, path: &str) -> Option<&SessionPreview> {
        if self.cache.contains_key(path) {
            // Update LRU order
            self.touch(path);
            self.cache.get(path)
        } else {
            None
        }
    }

    /// Check if a path is currently being loaded
    pub fn is_pending(&self, path: &str) -> bool {
        self.pending.contains(path)
    }

    /// Request a preview to be loaded (non-blocking)
    pub fn request(&mut self, path: &str) {
        // Skip if already cached or pending
        if self.cache.contains_key(path) || self.pending.contains(path) {
            return;
        }

        self.pending.insert(path.to_string());
        // Ignore send errors (worker may have exited)
        let _ = self.request_tx.send(path.to_string());
    }

    /// Request previews for current, previous, and next items
    pub fn prefetch<P: AsRef<Path>>(&mut self, paths: &[P]) {
        for path in paths {
            self.request(path.as_ref().to_string_lossy().as_ref());
        }
    }

    /// Invalidate a cached preview (e.g., after file modification)
    ///
    /// Removes the preview from cache so it will be reloaded on next access.
    pub fn invalidate<P: AsRef<Path>>(&mut self, path: P) {
        let path_str = path.as_ref().to_string_lossy().to_string();
        self.cache.remove(&path_str);
        self.lru_order.retain(|p| p != &path_str);
        self.pending.remove(&path_str);
    }

    /// Insert a preview into the cache
    fn insert(&mut self, path: String, preview: SessionPreview) {
        // If already in cache, just update LRU
        if self.cache.contains_key(&path) {
            self.touch(&path);
            return;
        }

        // Evict oldest if at capacity
        while self.cache.len() >= self.max_size {
            if let Some(oldest) = self.lru_order.pop_front() {
                self.cache.remove(&oldest);
            } else {
                break;
            }
        }

        // Insert new entry
        self.cache.insert(path.clone(), preview);
        self.lru_order.push_back(path);
    }

    /// Move a path to the back of the LRU queue (most recently used)
    fn touch(&mut self, path: &str) {
        // Remove from current position
        self.lru_order.retain(|p| p != path);
        // Add to back (most recent)
        self.lru_order.push_back(path.to_string());
    }
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new(20) // Default to 20 cached previews
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_starts_empty() {
        let mut cache = PreviewCache::new(10);
        assert!(cache.get("/nonexistent").is_none());
    }

    #[test]
    fn request_marks_as_pending() {
        let mut cache = PreviewCache::new(10);
        assert!(!cache.is_pending("/test/path"));
        cache.request("/test/path");
        assert!(cache.is_pending("/test/path"));
    }

    #[test]
    fn duplicate_request_is_ignored() {
        let mut cache = PreviewCache::new(10);
        cache.request("/test/path");
        cache.request("/test/path"); // Should not panic or duplicate
        assert!(cache.is_pending("/test/path"));
    }

    #[test]
    fn invalidate_removes_from_pending() {
        let mut cache = PreviewCache::new(10);
        cache.request("/test/path");
        assert!(cache.is_pending("/test/path"));

        cache.invalidate("/test/path");
        assert!(!cache.is_pending("/test/path"));
    }

    #[test]
    fn invalidate_nonexistent_path_is_safe() {
        let mut cache = PreviewCache::new(10);
        // Should not panic
        cache.invalidate("/nonexistent/path");
        assert!(!cache.is_pending("/nonexistent/path"));
    }
}
