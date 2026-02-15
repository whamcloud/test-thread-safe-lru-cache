use solution_2::ShardedLruCache;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn sharded_eviction_and_get_put_semantics() {
    let cache = ShardedLruCache::new(4, 2);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");
    assert_eq!(cache.get(&1), Some("a"), "expect hit for 1");
    cache.put(4, "d");
    cache.put(5, "e");
    assert!(
        cache.len() <= cache.total_capacity(),
        "len must be bounded by capacity"
    );
}

#[test]
fn sharded_len_and_empty() {
    let c: ShardedLruCache<i32, i32> = ShardedLruCache::new(2, 4);
    assert!(c.is_empty());
    c.put(1, 1);
    assert_eq!(c.len(), 1);
    c.put(2, 2);
    assert_eq!(c.len(), 2);
}

#[test]
#[should_panic(expected = "Capacity must be > 0")]
fn sharded_zero_capacity_panics() {
    let _ = ShardedLruCache::<i32, i32>::new(0, 2);
}

#[test]
fn sharded_concurrency_bound_and_correctness() {
    let cache = Arc::new(ShardedLruCache::new(32, 8));
    let threads = 8;
    let iters = 2000;
    let barrier = Arc::new(Barrier::new(threads));
    let mut handles = Vec::new();
    for t in 0..threads {
        let c = Arc::clone(&cache);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for i in 0..iters {
                let k = (i * 17 + t) % 256;
                c.put(k, (t, i));
                let _ = c.get(&k);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert!(
        cache.len() <= cache.total_capacity(),
        "cache length {} exceeds capacity {}",
        cache.len(),
        cache.total_capacity()
    );
}

// #[test]
// fn basic_operations_and_eviction() {
//     let cache = ShardedLruCache::new(4, 2);
//     cache.put(1, "a");
//     cache.put(2, "b");
//     cache.put(3, "c");
//     assert_eq!(cache.get(&1), Some("a"));
//     cache.put(4, "d");
//     cache.put(5, "e"); // triggers eviction in a shard
//     assert!(cache.len() <= cache.total_capacity());
// }

// #[test]
// fn zero_capacity_panics() {
//     let res = std::panic::catch_unwind(|| ShardedLruCache::<i32, i32>::new(0, 4));
//     assert!(res.is_err());
// }

// #[test]
// fn concurrent_contention_remains_bounded() {
//     let cache = Arc::new(ShardedLruCache::new(32, 8));
//     let threads = 8;
//     let iters = 1000;
//     let barrier = Arc::new(Barrier::new(threads));
//     let mut handles = Vec::new();

//     for t in 0..threads {
//         let c = Arc::clone(&cache);
//         let b = Arc::clone(&barrier);
//         handles.push(thread::spawn(move || {
//             b.wait();
//             for i in 0..iters {
//                 let k = (i + t) % 256;
//                 c.put(k, (t, i));
//                 let _ = c.get(&k);
//             }
//         }));
//     }

//     for h in handles {
//         h.join().unwrap();
//     }

//     assert!(
//         cache.len() <= cache.total_capacity(),
//         "len {} exceeds capacity {}",
//         cache.len(),
//         cache.total_capacity()
//     );
// }
