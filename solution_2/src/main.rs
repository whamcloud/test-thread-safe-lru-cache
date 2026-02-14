//! Executable entry point for solution_2.
//! Demonstrates initialization and concurrent use of the sharded LRU cache.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use solution_2::ShardedLruCache;

fn main() {
    // Initialize a sharded cache with total capacity 16 across 4 shards.
    let cache = Arc::new(ShardedLruCache::new(16, 4));

    // Spawn a few threads to exercise concurrent put/get operations.
    let mut handles = Vec::new();
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            println!("[worker-{t}] starting");
            for i in 0..12 {
                let k = (i * 13 + t) % 64;
                c.put(k, (t, i));
                let _ = c.get(&k);
                if i % 3 == 0 {
                    thread::sleep(Duration::from_millis(10));
                }
            }
            println!("[worker-{t}] done");
        }));
    }

    for h in handles {
        h.join().expect("worker thread panicked");
    }

    // Show a concatenated MRU→LRU order across shards (for observability).
    let order = cache.debug_order();
    println!(
        "Final concatenated MRU→LRU order (per shard appended): {:?}",
        order
    );
    println!("Total items: {}", cache.len());
    println!("Total capacity: {}", cache.total_capacity());
}
