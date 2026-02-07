//! Background worker pool for async cache loading
//!
//! Processes load requests across multiple worker threads and sends results
//! back via channels. The workers are generic over key and value types.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Result of a background load operation
pub struct LoadResult<K, V> {
    /// The key that was requested
    pub key: K,
    /// The loaded value, or None if loading failed
    pub value: Option<V>,
}

/// Default number of worker threads in the pool
pub const DEFAULT_POOL_SIZE: usize = 4;

/// A thread-safe, shared loader function.
pub type SharedLoader<K, V> = Arc<dyn Fn(&K) -> Option<V> + Send + Sync>;

/// Spawn a pool of worker threads that process load requests in parallel.
///
/// Each worker pulls keys from the shared `request_rx` channel, calls `loader`
/// for each key, and sends `LoadResult` back via its clone of `result_tx`.
/// Workers exit when the request channel is closed (all senders dropped).
pub fn spawn_worker_pool<K, V>(
    num_threads: usize,
    request_rx: Receiver<K>,
    result_tx: Sender<LoadResult<K, V>>,
    loader: SharedLoader<K, V>,
) where
    K: Send + 'static,
    V: Send + 'static,
{
    let request_rx = Arc::new(Mutex::new(request_rx));

    for _ in 0..num_threads {
        let rx = Arc::clone(&request_rx);
        let tx = result_tx.clone();
        let loader = Arc::clone(&loader);

        thread::spawn(move || {
            loop {
                let key = {
                    let rx = rx.lock().unwrap();
                    match rx.recv() {
                        Ok(key) => key,
                        Err(_) => return, // channel closed
                    }
                };
                let value = loader(&key);
                // Ignore send errors (main thread may have exited)
                let _ = tx.send(LoadResult { key, value });
            }
        });
    }
    // Drop the original result_tx so the channel closes only when all workers exit
    drop(result_tx);
}
