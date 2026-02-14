use solution_1::LruCache;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn make_small_cache() -> LruCache<i32, &'static str> {
    LruCache::new(2)
}

#[test]
fn lru_eviction_evicts_least_recent_when_over_capacity() {
    let cache = make_small_cache();
    cache.put(1, "a");
    cache.put(2, "b");
    // Touch 1 so 2 becomes LRU
    assert_eq!(cache.get(&1), Some("a"), "expected hit for key 1");
    cache.put(3, "c"); // evicts 2
    assert_eq!(cache.get(&2), None, "key 2 should have been evicted");
    assert_eq!(cache.get(&1), Some("a"), "key 1 should remain");
    assert_eq!(cache.get(&3), Some("c"), "key 3 should be present");
}

#[test]
fn update_existing_moves_to_front_and_updates_value() {
    let cache = make_small_cache();
    cache.put(1, "x");
    cache.put(2, "y");
    cache.put(1, "z"); // update value and move to MRU
    cache.put(3, "w"); // evicts 2 now
    assert_eq!(
        cache.get(&2),
        None,
        "2 should be evicted after 1 was updated"
    );
    assert_eq!(cache.get(&1), Some("z"), "updated value should be returned");
}

#[test]
fn len_and_is_empty_reflect_state() {
    let c: LruCache<i32, i32> = LruCache::new(1);
    assert!(c.is_empty(), "new cache should be empty");
    c.put(7, 7);
    assert_eq!(c.len(), 1, "len should report 1 after single put");
    assert!(!c.is_empty(), "cache should not be empty after put");
}

#[test]
#[should_panic(expected = "Capacity must be > 0")]
fn zero_capacity_panics() {
    let _ = LruCache::<i32, i32>::new(0);
}

#[test]
fn debug_order_matches_expected_mru_to_lru_sequence() {
    let cache = LruCache::new(3);
    cache.put(1, "a"); // [1]
    cache.put(2, "b"); // [2,1]
    cache.put(3, "c"); // [3,2,1]
    assert_eq!(
        cache.debug_order(),
        vec![3, 2, 1],
        "debug_order returns keys from MRU(head) to LRU(tail)"
    );
    // Re-access 2: becomes MRU
    let _ = cache.get(&2); // [2,3,1]
    assert_eq!(
        cache.debug_order(),
        vec![2, 3, 1],
        "after get(2), 2 should be MRU"
    );
    // Insert 4, evicts LRU (1)
    cache.put(4, "d"); // [4,2,3]
    assert_eq!(
        cache.debug_order(),
        vec![4, 2, 3],
        "after put(4), 1 should be evicted"
    );
    assert_eq!(cache.get(&1), None, "1 should be evicted");
}

#[test]
fn capacity_one_behaves_correctly() {
    let c = LruCache::new(1);
    c.put(10, "x");
    assert_eq!(c.get(&10), Some("x"), "should retrieve the only entry");
    c.put(11, "y"); // evicts 10
    assert_eq!(c.get(&10), None, "previous entry should be evicted");
    assert_eq!(c.get(&11), Some("y"), "new entry should be present");
}

#[test]
fn concurrent_puts_and_gets_remain_bounded() {
    let cache = Arc::new(LruCache::new(16));
    let threads = 6;
    let iters = 400;
    let barrier = Arc::new(Barrier::new(threads));
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = Arc::clone(&cache);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for i in 0..iters {
                let k = (i * 3 + t) % 64;
                c.put(k, (t, i));
                // Introduce a tiny jitter to increase interleavings
                if i % 67 == 0 {
                    thread::sleep(Duration::from_micros(40));
                }
                let _ = c.get(&k);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert!(
        cache.len() <= 16,
        "cache length {} should be <= capacity",
        cache.len()
    );
}
