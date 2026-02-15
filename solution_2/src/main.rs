//! # solution_2 – Concurrent Sharded LRU Cache Demo
//!
//! This binary demonstrates initialization and concurrent usage of
//! [`ShardedLruCache`] from the `solution_2` crate.
//!
//! # Overview
//!
//! The example:
//!
//! - Creates a sharded LRU cache with a total capacity of 16
//! - Splits storage across 4 shards
//! - Spawns multiple worker threads
//! - Performs concurrent `put` and `get` operations
//! - Prints final cache ordering and statistics
//!
//! This serves as a simple concurrency stress test and usage example.
//!
//! # Running
//!
//! ```bash
//! cargo run
//! ```
//!
//! # Related
//!
//! See [`ShardedLruCache`] for implementation details.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use solution_2::ShardedLruCache;

/// Entry point for the concurrent cache demonstration.
///
/// This function:
///
/// 1. Creates a shared [`ShardedLruCache`]
/// 2. Spawns multiple worker threads
/// 3. Executes concurrent `put`/`get` operations
/// 4. Waits for all threads to complete
/// 5. Prints final cache statistics
/// 6. Prints final cache ordering (MRU->LRU per shard)
fn main() {
    // Initialize a sharded cache with total capacity 16 across 4 shards.
    let cache = Arc::new(ShardedLruCache::new(16, 4));

    // Spawn a few worker threads to exercise concurrent put/get operations.
    let mut handles = Vec::new();
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            println!("[worker-{t}] starting");
            for i in 0..12 {
                let k = (i * 13 + t) % 64;

                // Insert value into cache and immediately retrieve it to verify correctness
                c.put(k, (t, i));

                // Attempt to read the same key to verify correctness and move it to MRU position
                let _ = c.get(&k);

                // Simulate staggered workload
                if i % 3 == 0 {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            println!("[worker-{t}] done");
        }));
    }

    // Wait for all threads to finish.
    for h in handles {
        h.join().expect("worker thread panicked");
    }

    // Show a concatenated MRU->LRU order across shards (for observability).
    let order = cache.debug_order();
    println!(
        "Final concatenated MRU->LRU order (per shard appended): {:?}",
        order
    );
    println!("Total items: {}", cache.len());
    println!("Total capacity: {}", cache.total_capacity());
}
