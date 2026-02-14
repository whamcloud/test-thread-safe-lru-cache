use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

type NodeRef<K> = Arc<Mutex<Node<K>>>;
type Link<K> = Option<NodeRef<K>>;
type CacheEntry<K, V> = (V, NodeRef<K>);
type CacheMap<K, V> = HashMap<K, CacheEntry<K, V>>;

#[derive(Debug)]
struct Node<K> {
    key: K,
    prev: Link<K>,
    next: Link<K>,
}

struct Inner<K, V> {
    map: CacheMap<K, V>, // Key -> (Value, Node pointer)
    head: Link<K>,       // Most Recently Used (MRU)
    tail: Link<K>,       // Least Recently Used (LRU)
    capacity: usize,
}

/// Thread-safe Least Recently Used (LRU) cache with fixed capacity.
///
/// - Stores key-value pairs with O(1) average get/put.
/// - Evicts least recently used item when capacity is exceeded.
/// - Safe for concurrent access via global `Mutex` on internal state.
pub struct LruCache<K, V> {
    inner: Arc<Mutex<Inner<K, V>>>,
}

impl<K: Eq + Hash + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> LruCache<K, V> {
    /// Create a new cache with a fixed positive capacity.
    ///
    /// Panics if `capacity == 0`.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be > 0");

        Self {
            inner: Arc::new(Mutex::new(Inner {
                map: HashMap::new(),
                head: None,
                tail: None,
                capacity,
            })),
        }
    }

    /// Get a value by key, marking it as most recently used on hit.
    ///
    /// Returns `None` if the key is absent.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (value, node) = inner.map.get(key)?.clone();
        Self::move_to_front(&mut inner, &node);
        Some(value)
    }

    /// Insert or update a key with value, moving it to most-recent position.
    ///
    /// On inserting a new key that causes the cache to exceed capacity, the
    /// least recently used key is evicted.
    pub fn put(&self, key: K, value: V) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if let Some((v, node)) = inner.map.get_mut(&key) {
            *v = value;
            let node = node.clone();
            Self::move_to_front(&mut inner, &node);
        } else {
            let node = Arc::new(Mutex::new(Node {
                key: key.clone(),
                prev: None,
                next: None,
            }));
            Self::attach_front(&mut inner, node.clone());
            inner.map.insert(key, (value, node));
            Self::evict_if_needed(&mut inner);
        }
    }

    /// Current number of elements stored in the cache.
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.map.len()
    }

    /// Returns true if the cache contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current LRU order from most-recent (head) to least-recent (tail).
    ///
    /// Intended for debugging/observability and tests.
    pub fn debug_order(&self) -> Vec<K> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut current = inner.head.clone();
        let mut out = Vec::with_capacity(inner.map.len());
        while let Some(n) = current {
            let g = n.lock().unwrap_or_else(|e| e.into_inner());
            out.push(g.key.clone());
            current = g.next.clone();
        }
        out
    }

    fn move_to_front(inner: &mut Inner<K, V>, node: &NodeRef<K>) {
        Self::detach(inner, node);
        Self::attach_front(inner, node.clone());
    }

    fn detach(inner: &mut Inner<K, V>, node: &NodeRef<K>) {
        let mut node_guard = node.lock().unwrap_or_else(|e| e.into_inner());

        let prev = node_guard.prev.take();
        let next = node_guard.next.take();
        drop(node_guard);

        match &prev {
            Some(p) => p.lock().unwrap_or_else(|e| e.into_inner()).next = next.clone(),
            None => inner.head = next.clone(),
        }

        match &next {
            Some(n) => n.lock().unwrap_or_else(|e| e.into_inner()).prev = prev.clone(),
            None => inner.tail = prev.clone(),
        }
    }

    fn attach_front(inner: &mut Inner<K, V>, node: NodeRef<K>) {
        {
            let mut node_guard = node.lock().unwrap_or_else(|e| e.into_inner());
            node_guard.prev = None;
            node_guard.next = inner.head.clone();
        }

        if let Some(ref head) = inner.head {
            head.lock().unwrap_or_else(|e| e.into_inner()).prev = Some(node.clone());
        }

        inner.head = Some(node.clone());

        if inner.tail.is_none() {
            inner.tail = Some(node);
        }
    }

    fn evict_if_needed(inner: &mut Inner<K, V>) {
        if inner.map.len() <= inner.capacity {
            return;
        }

        if let Some(old_tail) = inner.tail.clone() {
            let key = old_tail
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .key
                .clone();
            Self::detach(inner, &old_tail);
            inner.map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LruCache;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn basic_eviction_and_order() {
        let cache = LruCache::new(2);
        cache.put(1, "a");
        cache.put(2, "b");
        assert_eq!(cache.get(&1), Some("a"));
        cache.put(3, "c");
        assert_eq!(cache.get(&2), None, "2 should have been evicted as LRU");
        assert_eq!(cache.get(&1), Some("a"));
        assert_eq!(cache.get(&3), Some("c"));
    }

    #[test]
    fn len_and_is_empty_behave() {
        let cache: LruCache<i32, i32> = LruCache::new(1);
        assert!(cache.is_empty());
        cache.put(7, 9);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[test]
    #[should_panic]
    fn zero_capacity_panics() {
        let _ = LruCache::<i32, i32>::new(0);
    }

    #[test]
    fn concurrent_is_safe_and_bounded() {
        let cache = Arc::new(LruCache::new(32));
        let threads = 8;
        let iters = 300;
        let barrier = Arc::new(Barrier::new(threads));
        let mut handles = Vec::new();

        for t in 0..threads {
            let c = Arc::clone(&cache);
            let b = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                b.wait();
                for i in 0..iters {
                    let k = (i + t) % 128;
                    c.put(k, (t, i));
                    let _ = c.get(&k);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert!(
            cache.len() <= 32,
            "cache size {} exceeds capacity",
            cache.len()
        );
    }
}
