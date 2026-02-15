//! # solution_2 — Sharded, Thread-Safe LRU Cache
//!
//! This crate provides a **thread-safe, sharded LRU (Least Recently Used) cache**
//! designed to reduce lock contention under concurrent workloads.
//!
//! Instead of protecting the entire cache behind a single global lock,
//! the key space is partitioned across independent shards. Each shard:
//!
//! - Maintains its own `HashMap<K, Entry>`
//! - Tracks MRU/LRU ordering using key-linked adjacency
//! - Is protected by a single `Mutex`
//!
//! This significantly reduces contention compared to a monolithic design.
//!
//! ---
//!
//! ## Design Goals
//!
//! - **O(1)** average-time `get` and `put`
//! - **O(1)** move-to-front operations
//! - **O(1)** tail eviction
//! - Fixed, bounded total capacity
//! - Safe Rust only (no `unsafe`)
//! - Lock-poisoning recovery (with `Mutex::try_lock`)
//!
//! Lock poisoning happens when a thread panics while holding a lock.
//! In this case, the lock is poisoned and subsequent acquisitions will fail.
//! The cache will recover by dropping the poisoned lock and allowing other threads to proceed.
//!
//!
//! ---
//!
//! ## Concurrency Model
//!
//! Each shard is protected by an independent `Mutex`.
//! Operations on different shards proceed fully in parallel.
//!
//! The shard index is determined via the default Rust hasher.
//!
//! ---
//!
//! ## Example
//!
//! ```rust
//! use solution_2::ShardedLruCache;
//!
//! let cache = ShardedLruCache::new(8, 2);
//!
//! cache.put(1, "a");
//! cache.put(2, "b");
//!
//! assert_eq!(cache.get(&1), Some("a"));
//! assert_eq!(cache.len(), 2);
//! ```
//!
//! ---
//!
//! ## When To Use
//!
//! This implementation is well-suited for:
//!
//! - Read-heavy concurrent workloads
//! - Moderate write contention
//! - Bounded memory scenarios
//!
//! It is not intended as a fully lock-free or wait-free structure.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

fn distribute_capacity(total: usize, shards: usize) -> Vec<usize> {
    let n = shards.min(total.max(1));
    let base = total / n;
    let rem = total % n;
    (0..n).map(|i| base + usize::from(i < rem)).collect()
}

/// Entry represents a single key's state within a shard.
/// Stores:
/// - the value
/// - links to previous/next keys in the shard's MRU/LRU list
#[derive(Debug, Clone)]
struct Entry<K, V> {
    value: V,
    prev: Option<K>,
    next: Option<K>,
}

/// Shard tracks keys for a subset of the hash space.
/// It maintains:
/// - a HashMap from K -> Entry (value and adjacency)
/// - head (MRU) and tail (LRU) keys
/// - per-shard capacity
#[derive(Debug)]
struct Shard<K, V> {
    map: HashMap<K, Entry<K, V>>,
    head: Option<K>, // MRU
    tail: Option<K>, // LRU
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V> Shard<K, V> {
    /// Create a new shard with given capacity.
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity.max(1)),
            head: None,
            tail: None,
            capacity,
        }
    }

    /// Get a value and move the associated key to MRU.
    fn get(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let value = self.map.get(key)?.value.clone();
        self.move_to_front(key);
        Some(value)
    }

    /// Insert or update a key with value; move to MRU and evict LRU if needed.
    fn put(&mut self, key: K, value: V) {
        if let Some(e) = self.map.get_mut(&key) {
            e.value = value;
            self.move_to_front(&key);
            return;
        }

        // Insert new key at MRU
        let prev_head = self.head.clone();
        self.head = Some(key.clone());
        if let Some(h) = &prev_head
            && let Some(head_entry) = self.map.get_mut(h)
        {
            head_entry.prev = Some(key.clone());
        }
        let entry = Entry {
            value,
            prev: None,
            next: prev_head,
        };
        if self.tail.is_none() {
            self.tail = Some(key.clone());
        }
        self.map.insert(key.clone(), entry);
        self.evict_if_needed();
    }

    /// Current number of keys stored in the shard.
    fn len(&self) -> usize {
        self.map.len()
    }

    /// Move an existing key to MRU, patching adjacency and head/tail as needed.
    fn move_to_front(&mut self, key: &K) {
        if self.head.as_ref() == Some(key) {
            return;
        }
        let (prev, next) = match self.map.get(key) {
            Some(e) => (e.prev.clone(), e.next.clone()),
            None => return,
        };
        if let Some(p) = &prev {
            if let Some(pe) = self.map.get_mut(p) {
                pe.next = next.clone();
            }
        } else {
            self.head = next.clone();
        }
        if let Some(n) = &next {
            if let Some(ne) = self.map.get_mut(n) {
                ne.prev = prev.clone();
            }
        } else {
            self.tail = prev.clone();
        }
        if let Some(e) = self.map.get_mut(key) {
            e.prev = None;
            e.next = self.head.clone();
        }
        if let Some(h) = &self.head
            && let Some(he) = self.map.get_mut(h)
        {
            he.prev = Some(key.clone());
        }
        self.head = Some(key.clone());
        if self.tail.is_none() {
            self.tail = Some(key.clone());
        }
    }

    /// Evict the LRU (tail) entry if the shard exceeds capacity.
    fn evict_if_needed(&mut self) {
        if self.map.len() <= self.capacity {
            return;
        }
        if let Some(lru_key) = self.tail.clone() {
            let (prev_opt, next_opt) = {
                let e = self.map.get(&lru_key).unwrap();
                (e.prev.clone(), e.next.clone())
            };
            if let Some(ref p) = prev_opt {
                if let Some(pe) = self.map.get_mut(p) {
                    pe.next = next_opt.clone();
                }
                self.tail = Some(p.clone());
            } else {
                self.tail = None;
            }
            // Collapse nested conditions: update next's prev when both exist
            if let Some(ref n) = next_opt
                && let Some(ne) = self.map.get_mut(n)
            {
                ne.prev = prev_opt.clone();
            }
            self.map.remove(&lru_key);
        }
    }

    /// Return MRU→LRU key order for this shard (debug/observability).
    fn order(&self) -> Vec<K> {
        let mut out = Vec::with_capacity(self.map.len());
        let mut cur = self.head.clone();
        while let Some(k) = cur {
            out.push(k.clone());
            cur = self.map.get(&k).and_then(|e| e.next.clone());
        }
        out
    }
}

/// A sharded, thread-safe LRU cache that minimizes contention by partitioning
/// the key space across multiple independent shards.
///
/// Each shard uses O(1) HashMap lookups and key-linked adjacency for MRU/LRU
/// list management without per-node heap allocations or extra mutexes.
pub struct ShardedLruCache<K, V> {
    shards: Vec<Mutex<Shard<K, V>>>,
}

impl<K: Eq + Hash + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> ShardedLruCache<K, V> {
    /// Create a new sharded LRU with total capacity and an optional shard hint.
    /// The number of shards will not exceed the capacity to preserve the bound.
    pub fn new(total_capacity: usize, shard_hint: usize) -> Self {
        assert!(total_capacity > 0, "Capacity must be > 0");
        let shard_count = shard_hint.max(1).min(total_capacity);
        let caps = distribute_capacity(total_capacity, shard_count);
        let shards = caps
            .into_iter()
            .map(|c| Mutex::new(Shard::new(c)))
            .collect();
        Self { shards }
    }

    /// Map a key to its shard index via a default hasher.
    fn shard_index(&self, key: &K) -> usize {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        (h.finish() as usize) % self.shards.len()
    }

    /// Get a value by key and move it to MRU within its shard.
    pub fn get(&self, key: &K) -> Option<V> {
        let idx = self.shard_index(key);
        let mut shard = self.shards[idx].lock().unwrap_or_else(|e| e.into_inner());
        shard.get(key)
    }

    /// Put a key/value pair into its shard; move to MRU, evict LRU if needed.
    pub fn put(&self, key: K, value: V) {
        let idx = self.shard_index(&key);
        let mut shard = self.shards[idx].lock().unwrap_or_else(|e| e.into_inner());
        shard.put(key, value);
    }

    /// Total number of elements across all shards.
    pub fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).len())
            .sum()
    }

    /// True if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sum of individual shard capacities.
    pub fn total_capacity(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).capacity)
            .sum()
    }

    /// Returns a concatenation of MRU→LRU orders for each shard, in shard index
    /// order. Intended for debugging and tests only.
    pub fn debug_order(&self) -> Vec<K> {
        let mut out = Vec::new();
        for s in &self.shards {
            let s = s.lock().unwrap_or_else(|e| e.into_inner());
            out.extend(s.order());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::ShardedLruCache;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn basic_operations_and_eviction() {
        let cache = ShardedLruCache::new(4, 2);
        cache.put(1, "a");
        cache.put(2, "b");
        cache.put(3, "c");
        assert_eq!(cache.get(&1), Some("a"));
        cache.put(4, "d");
        cache.put(5, "e"); // triggers eviction in a shard
        assert!(cache.len() <= cache.total_capacity());
    }

    #[test]
    fn zero_capacity_panics() {
        let res = std::panic::catch_unwind(|| ShardedLruCache::<i32, i32>::new(0, 4));
        assert!(res.is_err());
    }

    #[test]
    fn concurrent_contention_remains_bounded() {
        let cache = Arc::new(ShardedLruCache::new(32, 8));
        let threads = 8;
        let iters = 1000;
        let barrier = Arc::new(Barrier::new(threads));
        let mut handles = Vec::new();

        for t in 0..threads {
            let c = Arc::clone(&cache);
            let b = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                b.wait();
                for i in 0..iters {
                    let k = (i + t) % 256;
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
            "len {} exceeds capacity {}",
            cache.len(),
            cache.total_capacity()
        );
    }
}
