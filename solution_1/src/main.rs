use solution_1::LruCache;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let cache = Arc::new(LruCache::new(3));

    let mut handles = vec![];

    // Demonstration:
    // - Spawn multiple threads that interleave puts/gets
    // - Print LRU order to show how entries move (MRU -> LRU)
    // The cache uses internal locking to ensure correctness under concurrency.
    for thread_number in 0..3 {
        let cache_clone = cache.clone();

        handles.push(thread::spawn(move || {
            println!("[T{thread_number}] starting");
            for i in 0..4 {
                // Insert or update a key; should move to MRU
                cache_clone.put(i, thread_number);
                let order = cache_clone.debug_order();
                println!("[T{thread_number}] put {i} -> order(MRU→LRU) = {:?}", order);

                // Small sleep to encourage interleavings
                thread::sleep(Duration::from_millis(40));

                // Access key; should move to MRU again
                let got = cache_clone.get(&i);
                let order = cache_clone.debug_order();
                println!(
                    "[T{thread_number}] get {i} = {:?} -> order(MRU→LRU) = {:?}",
                    got, order
                );
            }
            println!("[T{thread_number}] done");
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Finished concurrent operations safely.");
}
