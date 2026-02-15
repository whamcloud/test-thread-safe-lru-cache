use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::RwLock;

// cache struct
pub struct LruCache<K, V> {
    capacity: usize,
    inner: RwLock<CacheState<K, V>>,
}

// structure to keep state of the cache
struct CacheState<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

// our implementation of get and put
impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);

        Self {
            capacity,
            inner: RwLock::new(CacheState {
                map: HashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
            }),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut state = self.inner.write().unwrap();

        if let Some(value) = state.map.get(key).cloned() {
            // most recently used
            if let Some(pos) = state.order.iter().position(|k| k == key) {
                state.order.remove(pos);
            }
            state.order.push_back(key.clone());

            Some(value)
        } else {
            None
        }
    }

    pub fn put(&self, key: K, value: V) {
        let mut state = self.inner.write().unwrap();

        if state.map.contains_key(&key) {
            state.map.insert(key.clone(), value);

            if let Some(pos) = state.order.iter().position(|k| k == &key) {
                state.order.remove(pos);
            }

            state.order.push_back(key);
            return;
        }

        if state.map.len() == self.capacity
            && let Some(lru_key) = state.order.pop_front()
        {
            state.map.remove(&lru_key);
        }

        state.map.insert(key.clone(), value);
        state.order.push_back(key);
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// Our generic unit test cases to test insertion, eviction and concurrency
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn simple_insert_and_get() {
        let cache = LruCache::new(2);

        cache.put(1, "one");
        cache.put(2, "two");

        assert_eq!(cache.get(&1), Some("one"));
        assert_eq!(cache.get(&2), Some("two"));
    }

    #[test]
    fn should_evict_oldest() {
        let cache = LruCache::new(2);

        cache.put(1, "a");
        cache.put(2, "b");
        cache.put(3, "c"); // this should evict 1

        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some("b"));
        assert_eq!(cache.get(&3), Some("c"));
    }

    #[test]
    fn update_value() {
        let cache = LruCache::new(2);

        cache.put(1, "a");
        cache.put(1, "b");

        assert_eq!(cache.get(&1), Some("b"));
    }

    #[test]
    fn basic_concurrent_usage() {
        let cache = Arc::new(LruCache::new(3));
        let mut handles = vec![];

        for i in 0..5 {
            let c = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for j in 0..50 {
                    c.put(j, i);
                    let _ = c.get(&j);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // not super strict, just making sure nothing exploded
        assert!(cache.len() <= 3);
    }
}
