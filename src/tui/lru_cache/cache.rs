//! Generic async LRU cache with background loading
//!
//! Provides a bounded cache that loads values on a background thread.
//! Callers request keys and poll for results. LRU eviction keeps
//! the cache within its configured size limit.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use super::worker::{spawn_worker_pool, LoadResult, DEFAULT_POOL_SIZE};

/// Async LRU cache that loads values on a background thread.
///
/// Keys are requested via `request()` or `prefetch()`, loaded
/// asynchronously, and retrieved via `get()` after calling `poll()`.
pub struct AsyncLruCache<K, V> {
    /// Cached entries (key -> value)
    cache: HashMap<K, V>,
    /// LRU order (front = oldest, back = newest)
    lru_order: VecDeque<K>,
    /// Maximum cache size
    max_size: usize,
    /// Keys currently being loaded
    pending: HashSet<K>,
    /// Channel sender for load requests
    request_tx: Sender<K>,
    /// Channel receiver for load results
    result_rx: Receiver<LoadResult<K, V>>,
}

impl<K, V> AsyncLruCache<K, V>
where
    K: Hash + Eq + Clone + Send + 'static,
    V: Send + 'static,
{
    /// Create a new cache with the given max size and loader function.
    ///
    /// The `loader` runs on a pool of background threads for each requested key.
    /// It should return `Some(value)` on success or `None` on failure.
    pub fn new(max_size: usize, loader: impl Fn(&K) -> Option<V> + Send + Sync + 'static) -> Self {
        let (request_tx, request_rx) = channel::<K>();
        let (result_tx, result_rx) = channel::<LoadResult<K, V>>();

        let loader = Arc::new(loader);
        spawn_worker_pool(DEFAULT_POOL_SIZE, request_rx, result_tx, loader);

        Self {
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
            max_size,
            pending: HashSet::new(),
            request_tx,
            result_rx,
        }
    }

    /// Poll for completed loads and add them to cache
    pub fn poll(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&result.key);
            if let Some(value) = result.value {
                self.insert(result.key, value);
            }
        }
    }

    /// Get a value from cache, returning None if not cached yet
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.cache.contains_key(key) {
            self.touch(key);
            self.cache.get(key)
        } else {
            None
        }
    }

    /// Check if a key is currently being loaded
    pub fn is_pending(&self, key: &K) -> bool {
        self.pending.contains(key)
    }

    /// Request a value to be loaded (non-blocking).
    ///
    /// Skips the request if the key is already cached or pending.
    pub fn request(&mut self, key: K) {
        if self.cache.contains_key(&key) || self.pending.contains(&key) {
            return;
        }

        self.pending.insert(key.clone());
        // Ignore send errors (worker may have exited)
        let _ = self.request_tx.send(key);
    }

    /// Request loading for multiple keys at once
    pub fn prefetch(&mut self, keys: &[K]) {
        for key in keys {
            self.request(key.clone());
        }
    }

    /// Remove a key from cache, pending set, and LRU order.
    ///
    /// The key will be reloaded on next access.
    pub fn invalidate(&mut self, key: &K) {
        self.cache.remove(key);
        self.lru_order.retain(|k| k != key);
        self.pending.remove(key);
    }

    /// Insert a pre-loaded value directly into the cache.
    ///
    /// Useful for synchronously seeding the cache (e.g. the first preview)
    /// so it's available immediately without waiting for a worker round-trip.
    pub fn insert(&mut self, key: K, value: V) {
        if self.cache.contains_key(&key) {
            self.touch(&key);
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

        self.cache.insert(key.clone(), value);
        self.lru_order.push_back(key);
    }

    /// Move a key to the back of the LRU queue (most recently used)
    fn touch(&mut self, key: &K) {
        self.lru_order.retain(|k| k != key);
        self.lru_order.push_back(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    /// Create a test cache with a simple string->usize loader
    fn test_cache(max_size: usize) -> AsyncLruCache<String, usize> {
        AsyncLruCache::new(max_size, |key: &String| {
            // Simulate loading: return the string length as the value
            Some(key.len())
        })
    }

    #[test]
    fn cache_starts_empty() {
        let mut cache = test_cache(10);
        assert!(cache.get(&"nonexistent".to_string()).is_none());
    }

    #[test]
    fn request_marks_as_pending() {
        let mut cache = test_cache(10);
        let key = "/test/path".to_string();
        assert!(!cache.is_pending(&key));
        cache.request(key.clone());
        assert!(cache.is_pending(&key));
    }

    #[test]
    fn duplicate_request_is_ignored() {
        let mut cache = test_cache(10);
        let key = "/test/path".to_string();
        cache.request(key.clone());
        cache.request(key.clone()); // Should not panic or duplicate
        assert!(cache.is_pending(&key));
    }

    #[test]
    fn invalidate_removes_from_pending() {
        let mut cache = test_cache(10);
        let key = "/test/path".to_string();
        cache.request(key.clone());
        assert!(cache.is_pending(&key));

        cache.invalidate(&key);
        assert!(!cache.is_pending(&key));
    }

    #[test]
    fn invalidate_nonexistent_key_is_safe() {
        let mut cache = test_cache(10);
        let key = "/nonexistent/path".to_string();
        // Should not panic
        cache.invalidate(&key);
        assert!(!cache.is_pending(&key));
    }

    #[test]
    fn poll_receives_loaded_values() {
        let mut cache = test_cache(10);
        let key = "hello".to_string();
        cache.request(key.clone());

        // Give worker thread time to process
        sleep(Duration::from_millis(50));
        cache.poll();

        assert!(!cache.is_pending(&key));
        assert_eq!(cache.get(&key), Some(&5));
    }

    #[test]
    fn lru_eviction_removes_oldest() {
        let mut cache = test_cache(2);

        // Load two entries
        cache.request("aa".to_string());
        cache.request("bb".to_string());
        sleep(Duration::from_millis(50));
        cache.poll();

        // Both should be cached
        assert!(cache.get(&"aa".to_string()).is_some());
        assert!(cache.get(&"bb".to_string()).is_some());

        // Request a third -- should evict "aa" (oldest after "bb" was touched by get)
        cache.request("ccc".to_string());
        sleep(Duration::from_millis(50));
        cache.poll();

        assert!(cache.get(&"aa".to_string()).is_none());
        assert!(cache.get(&"bb".to_string()).is_some());
        assert!(cache.get(&"ccc".to_string()).is_some());
    }

    #[test]
    fn prefetch_requests_multiple_keys() {
        let mut cache = test_cache(10);
        let keys = vec!["a".to_string(), "bb".to_string(), "ccc".to_string()];
        cache.prefetch(&keys);

        sleep(Duration::from_millis(50));
        cache.poll();

        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"bb".to_string()), Some(&2));
        assert_eq!(cache.get(&"ccc".to_string()), Some(&3));
    }
}
