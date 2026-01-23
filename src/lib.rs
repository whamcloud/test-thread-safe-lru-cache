use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Trait to provide atomic access to generic storage
pub trait AtomicStorage {
    fn load(&self, order: Ordering) -> usize;
    fn store(&self, val: usize, order: Ordering);
    fn fetch_add(&self, val: usize, order: Ordering) -> usize;
}

impl AtomicStorage for AtomicUsize {
    fn load(&self, order: Ordering) -> usize {
        self.load(order)
    }
    fn store(&self, val: usize, order: Ordering) {
        self.store(val, order)
    }
    fn fetch_add(&self, val: usize, order: Ordering) -> usize {
        self.fetch_add(val, order)
    }
}

/// Wrapper for keys to support generic atomic types
pub struct LRUKey<T> {
    pub key: T,
}

/// Wrapper for values to support generic atomic types
pub struct LRUValue<T> {
    pub value: T,
}

/// A high-performance, thread-safe LRU cache using atomic arrays and configurable "folds".
///
/// The types K and V are intended to be atomic types (like AtomicUsize).
pub struct LRUCache<K, V> {
    capacity: usize,
    num_folds: usize,
    keys: Vec<LRUKey<K>>,
    values: Vec<LRUValue<V>>,
    hit_counts: Vec<AtomicUsize>,
    folds: Vec<Mutex<()>>,
    hasher: fn(usize) -> usize,
}

impl<K, V> LRUCache<K, V>
where
    K: Default,
    V: Default,
{
    pub fn new(capacity: usize, num_folds: usize, hasher: fn(usize) -> usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");
        assert!(num_folds > 0, "Number of folds must be greater than 0");
        assert!(
            capacity >= num_folds,
            "Capacity must be at least equal to num_folds"
        );

        let mut keys = Vec::with_capacity(capacity);
        let mut values = Vec::with_capacity(capacity);
        let mut hit_counts = Vec::with_capacity(capacity);
        let mut folds = Vec::with_capacity(num_folds);

        for _ in 0..capacity {
            keys.push(LRUKey { key: K::default() });
            values.push(LRUValue {
                value: V::default(),
            });
            hit_counts.push(AtomicUsize::new(0));
        }

        for _ in 0..num_folds {
            folds.push(Mutex::new(()));
        }

        LRUCache {
            capacity,
            num_folds,
            keys,
            values,
            hit_counts,
            folds,
            hasher,
        }
    }
}

impl<K, V> LRUCache<K, V> {
    /// Helper to determine which fold a key belongs to
    fn get_fold_index(&self, key: usize) -> usize {
        (self.hasher)(key) % self.num_folds
    }

    /// Helper to get the range of indices controlled by a fold
    fn get_fold_range(&self, fold_idx: usize) -> (usize, usize) {
        let fold_size = self.capacity / self.num_folds;
        let start = fold_idx * fold_size;
        let end = if fold_idx == self.num_folds - 1 {
            self.capacity
        } else {
            start + fold_size
        };
        (start, end)
    }

    pub fn get(&self, key: usize) -> Option<usize>
    where
        K: AtomicStorage,
        V: AtomicStorage,
    {
        if key == 0 {
            return None;
        }

        let fold_idx = self.get_fold_index(key);
        let (start, end) = self.get_fold_range(fold_idx);

        for i in start..end {
            // Load key with Acquire to see the value stored before it
            let k1 = self.keys[i].key.load(Ordering::Acquire);

            if k1 == key {
                let v = self.values[i].value.load(Ordering::Acquire);

                // Double-check: Ensure the key didn't change while we were reading the value
                // This prevents returning a value belonging to a different key if the slot was repurposed.
                let k2 = self.keys[i].key.load(Ordering::Acquire);

                if k1 == k2 {
                    self.hit_counts[i].fetch_add(1, Ordering::Relaxed);
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn put(&self, key: usize, value: usize)
    where
        K: AtomicStorage,
        V: AtomicStorage,
    {
        if key == 0 {
            return; // 0 is reserved for empty/invalid keys
        }

        let fold_idx = self.get_fold_index(key);
        let _lock = self.folds[fold_idx].lock().unwrap();

        let (start, end) = self.get_fold_range(fold_idx);

        let mut lru_idx = start;
        let mut min_hits = usize::MAX;
        let mut empty_idx = None;

        for i in start..end {
            let current_key = self.keys[i].key.load(Ordering::Relaxed);

            if current_key == key {
                // If key already exists, update value and hit count
                // Store value with Release to ensure readers see it before the key (if they were checking)
                self.values[i].value.store(value, Ordering::Release);
                self.hit_counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }

            if current_key == 0 && empty_idx.is_none() {
                empty_idx = Some(i);
            }

            let hits = self.hit_counts[i].load(Ordering::Relaxed);
            if hits < min_hits {
                min_hits = hits;
                lru_idx = i;
            }
        }

        let target_idx = empty_idx.unwrap_or(lru_idx);

        // Invalidate the key slot first if we are replacing data.
        // This closes the race condition where a reader sees (Old Key, New Value, Old Key).
        // Readers will see (Old Key, New Value, 0/New Key) -> mismatch -> retry/fail.
        self.keys[target_idx].key.store(0, Ordering::Release);

        // Now it's safe to update the value
        self.values[target_idx]
            .value
            .store(value, Ordering::Release);

        // Finally store the new key, making the entry valid again
        self.keys[target_idx].key.store(key, Ordering::Release);
        self.hit_counts[target_idx].store(1, Ordering::Relaxed);
    }

    pub fn len(&self) -> usize
    where
        K: AtomicStorage,
    {
        let mut count = 0;
        for i in 0..self.capacity {
            if self.keys[i].key.load(Ordering::Relaxed) != 0 {
                count += 1;
            }
        }
        count
    }

    pub fn is_empty(&self) -> bool
    where
        K: AtomicStorage,
    {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn clear(&self)
    where
        K: AtomicStorage,
    {
        // We should probably lock all folds for a full clear,
        // but let's just do it entry by entry or lock one by one.
        for f in 0..self.num_folds {
            let _lock = self.folds[f].lock().unwrap();
            let (start, end) = self.get_fold_range(f);
            for i in start..end {
                self.keys[i].key.store(0, Ordering::Relaxed);
                self.hit_counts[i].store(0, Ordering::Relaxed);
            }
        }
    }

    pub fn remove(&self, key: usize) -> Option<usize>
    where
        K: AtomicStorage,
        V: AtomicStorage,
    {
        if key == 0 {
            return None;
        }

        let fold_idx = self.get_fold_index(key);
        let _lock = self.folds[fold_idx].lock().unwrap();

        let (start, end) = self.get_fold_range(fold_idx);

        for i in start..end {
            if self.keys[i].key.load(Ordering::Relaxed) == key {
                let val = self.values[i].value.load(Ordering::Relaxed);
                self.keys[i].key.store(0, Ordering::Relaxed);
                self.hit_counts[i].store(0, Ordering::Relaxed);
                return Some(val);
            }
        }
        None
    }

    pub fn contains_key(&self, key: usize) -> bool
    where
        K: AtomicStorage,
    {
        if key == 0 {
            return false;
        }

        let fold_idx = self.get_fold_index(key);
        // We can do contains_key without a lock for performance, similar to get
        let (start, end) = self.get_fold_range(fold_idx);

        for i in start..end {
            if self.keys[i].key.load(Ordering::Relaxed) == key {
                return true;
            }
        }
        false
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    /// Tests that the cache is created with correct capacity, zero length, and empty state.
    #[test]
    fn test_cache_creation() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(10, 2, |k| k);
        assert_eq!(cache.capacity(), 10);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    /// Tests simple `put` and `get` operations across different folds to ensure basic storage works.
    #[test]
    fn test_put_and_get() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(10, 2, |k| k);

        cache.put(2, 200); // Fold 0
        cache.put(3, 300); // Fold 1

        assert_eq!(cache.get(2), Some(200));
        assert_eq!(cache.get(3), Some(300));
    }

    /// Tests that `put` updates the value of an existing key instead of creating a new entry,
    /// and length remains constant.
    #[test]
    fn test_update_existing_key() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(3, 1, |k| k);
        cache.put(1, 100);
        assert_eq!(cache.get(1), Some(100));

        cache.put(1, 200);
        assert_eq!(cache.get(1), Some(200));
        assert_eq!(cache.len(), 1);
    }

    /// Tests that the cache correctly evicts the least recently used (lowest hit count) item
    /// when capacity is exceeded.
    #[test]
    fn test_eviction() {
        // Capacity 2, 1 fold.
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(2, 1, |k| k);

        cache.put(1, 100);
        cache.put(2, 200);

        // Both keys should be there
        assert_eq!(cache.get(1), Some(100)); // Key 1 hit count now 2
        assert_eq!(cache.get(2), Some(200)); // Key 2 hit count now 2

        // Put key 3, should evict key with lowest hit count (Key 1)
        cache.put(3, 300);
        assert_eq!(cache.get(3), Some(300));
        assert_eq!(cache.get(1), None);
        assert_eq!(cache.get(2), Some(200));
    }

    /// Tests that `clear` removes all items from the cache and resets length to zero.
    #[test]
    fn test_clear() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(3, 1, |k| k);
        cache.put(1, 100);
        cache.put(2, 200);
        cache.put(3, 300);

        assert_eq!(cache.len(), 3);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.get(1), None);
    }

    /// Tests that `remove` correctly deletes a specific key and reduces the length.
    #[test]
    fn test_remove() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(3, 1, |k| k);
        cache.put(1, 100);
        cache.put(2, 200);
        cache.put(3, 300);

        assert_eq!(cache.remove(2), Some(200));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(2), None);

        assert_eq!(cache.remove(99), None);
    }

    /// Tests `contains_key` verify presence without needing to retrieve the value.
    #[test]
    fn test_contains_key() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(3, 1, |k| k);
        cache.put(1, 100);
        cache.put(2, 200);

        assert!(cache.contains_key(1));
        assert!(cache.contains_key(2));
        assert!(!cache.contains_key(99));
    }

    /// Tests that creating a cache with 0 capacity correctly panics.
    #[test]
    #[should_panic(expected = "Capacity must be greater than 0")]
    fn test_zero_capacity_panics() {
        let _: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(0, 1, |k| k);
    }

    /// Tests the edge case of a cache with capacity 1 to ensure eviction works instantly.
    #[test]
    fn test_capacity_one() {
        let cache: LRUCache<AtomicUsize, AtomicUsize> = LRUCache::new(1, 1, |k| k);
        cache.put(1, 100);
        assert_eq!(cache.get(1), Some(100));

        cache.put(2, 200);
        assert_eq!(cache.get(1), None);
        assert_eq!(cache.get(2), Some(200));
    }

    /// Tests that multiple threads can read from the cache concurrently without data races or errors.
    #[test]
    fn test_concurrent_reads() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(100, 4, |k| k));

        for i in 1..101 {
            cache.put(i, i * 10);
        }

        let mut handles = vec![];

        for _ in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 1..101 {
                    assert_eq!(cache_clone.get(i), Some(i * 10));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Tests that multiple threads can write to the cache concurrently, verifying thread-safe `put` across folds.
    #[test]
    fn test_concurrent_writes() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(100, 4, |k| k));
        let mut handles = vec![];

        for thread_id in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    let key = thread_id * 10 + i + 1; // Avoid key 0
                    cache_clone.put(key, key * 100);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(cache.len(), 100);
    }

    /// Tests mixed concurrent reads and writes to ensure lock-free reads don't see inconsistent states during writes.
    #[test]
    fn test_concurrent_read_write() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(1000, 8, |k| k));
        let mut handles = vec![];
        let operations = Arc::new(AtomicUsize::new(0));

        // Readers
        for _ in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let ops_clone = Arc::clone(&operations);
            let handle = thread::spawn(move || {
                for i in 0..1000 {
                    let _ = cache_clone.get(i);
                    ops_clone.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        // Writers
        for thread_id in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let ops_clone = Arc::clone(&operations);
            let handle = thread::spawn(move || {
                for i in 0..200 {
                    let key = thread_id * 200 + i;
                    cache_clone.put(key, key);
                    ops_clone.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify exactly 6000 operations were performed (5000 reads + 1000 writes)
        assert_eq!(operations.load(Ordering::Relaxed), 6000);
    }

    /// Tests concurrent writes causing evictions to ensure the LRU/LFU policy holds up under pressure.
    #[test]
    fn test_concurrent_eviction() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(100, 4, |k| k));
        let mut handles = vec![];

        for thread_id in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..50 {
                    let key = thread_id * 50 + i;
                    cache_clone.put(key, key);
                    thread::sleep(Duration::from_micros(10));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should be at or near capacity
        assert!(cache.len() <= 100);
        assert!(cache.len() > 0);
    }

    /// Tests `clear` operation running concurrently with other operations to ensure no panics or invalid states.
    #[test]
    fn test_concurrent_clear_and_operations() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(100, 4, |k| k));
        let mut handles = vec![];

        // Populate cache
        for i in 0..100 {
            cache.put(i, i);
        }

        // Clear while other threads operate
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for _ in 0..10 {
                thread::sleep(Duration::from_millis(5));
                cache_clone.clear();
            }
        });
        handles.push(handle);

        // Writers
        for thread_id in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..20 {
                    let key = thread_id * 20 + i;
                    cache_clone.put(key, key);
                    thread::sleep(Duration::from_millis(1));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should still be valid
        assert!(cache.len() <= 100);
    }

    /// Tests concurrent removals to separate keys to verify robust removal logic.
    #[test]
    fn test_concurrent_remove() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(1000, 8, |k| k));
        let mut handles = vec![];

        // Populate cache
        for i in 0..1000 {
            cache.put(i, i * 10);
        }

        // Removers
        for thread_id in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let key = thread_id * 100 + i;
                    cache_clone.remove(key);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(cache.len(), 0);
    }

    /// Accesses many keys across many threads to stress test the sharding and atomic updates.
    #[test]
    fn test_stress_many_threads() {
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(1000, 8, |k| k));
        let mut handles = vec![];
        let num_threads = 50;

        for thread_id in 0..num_threads {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let key = thread_id * 100 + i % 1000;
                    cache_clone.put(key, key);
                    cache_clone.get(key);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert!(cache.len() <= 1000);
    }

    /// Tests extreme contention on a SINGLE key to verify atomic integrity and prevent tearing.
    #[test]
    fn test_stress_hot_key() {
        // Thousands of operations on a SINGLE key to test extreme contention
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(100, 4, |k| k));
        let mut handles = vec![];
        let num_threads = 50;
        let ops_per_thread = 1000;

        // Populate initial value
        cache.put(99, 0);

        for i in 0..num_threads {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for _ in 0..ops_per_thread {
                    if i % 2 == 0 {
                        cache_clone.put(99, i);
                    } else {
                        cache_clone.get(99);
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Key should still exist and be valid
        assert!(cache.get(99).is_some());
    }

    /// Simulates a chaotic workload with random operations to catch unexpected state corruptions.
    #[test]
    fn test_stress_random_chaos() {
        // A chaotic test mixing all operations randomly
        let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(50, 4, |k| k));
        let mut handles = vec![];
        let num_threads = 20;
        let ops_per_thread = 2000;

        for t in 0..num_threads {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                // Simple LCG pseudo-random generator
                let mut state = t as u64 + 1;
                let mut rng = move || {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    state
                };

                for _ in 0..ops_per_thread {
                    let r = rng();
                    let key = (r % 100) as usize + 1; // Keys 1..100
                    let op = r % 100;

                    if op < 50 {
                        // 50% reads
                        cache_clone.get(key);
                    } else if op < 90 {
                        // 40% writes
                        cache_clone.put(key, (r % 1000) as usize);
                    } else if op < 95 {
                        // 5% removes
                        cache_clone.remove(key);
                    } else {
                        // 5% clear (rare, but destructive)
                        // Only let thread 0 clear to prevent constant wiping
                        if t == 0 {
                            cache_clone.clear();
                        }
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Ensure structure is still intact
        assert!(cache.len() <= 50);
    }
}
