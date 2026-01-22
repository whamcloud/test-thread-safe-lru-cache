use dashmap::DashMap;
use lru::LruCache as StdLruCache;
use lru_rs::LRUCache as MyLRUCache;
use moka::sync::Cache as MokaCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const CACHE_CAPACITY: usize = 100_000;
const KEY_SPACE: usize = 200_000;
const DURATION_SECS: u64 = 2; // Short duration for quick comparison

fn main() {
    println!("Implementation,Threads,Throughput (Ops/sec)");

    let thread_counts = vec![1, 4, 8, 16, 32];

    for &num_threads in &thread_counts {
        // 1. My LRU (Atomic Sharded) - Standard Config
        let my_lru = Arc::new(MyLRUCache::<AtomicUsize, AtomicUsize>::new(
            CACHE_CAPACITY,
            16,
            |k| k,
        ));
        bench(
            "MyLRU (Low Folds)",
            num_threads,
            my_lru.clone(),
            |c, k| {
                c.get(k);
            },
            |c, k, v| {
                c.put(k, v);
            },
        );

        // 1b. My LRU (Atomic Sharded) - High Folds (Approximating HashMap Buckets)
        // Capacity 100k, 25k folds -> 4 items per fold (Fast scan)
        let my_lru_optimized = Arc::new(MyLRUCache::<AtomicUsize, AtomicUsize>::new(
            CACHE_CAPACITY,
            CACHE_CAPACITY / 4,
            |k| k,
        ));
        bench(
            "MyLRU (High Folds)",
            num_threads,
            my_lru_optimized.clone(),
            |c, k| {
                c.get(k);
            },
            |c, k, v| {
                c.put(k, v);
            },
        );

        // 2. DashMap (Concurrent Map, No Eviction - Baseline Speed)
        let dash = Arc::new(DashMap::new());
        bench(
            "DashMap*",
            num_threads,
            dash.clone(),
            |c, k| {
                c.get(&k);
            },
            |c, k, v| {
                c.insert(k, v);
            },
        );

        // 3. Moka (High Performance Concurrent LRU)
        let moka = MokaCache::new(CACHE_CAPACITY as u64);
        bench(
            "Moka",
            num_threads,
            moka.clone(),
            |c, k| {
                c.get(&k);
            },
            |c, k, v| {
                c.insert(k, v);
            },
        );

        // 4. Mutex<Lru> (Standard Protected LRU)
        let std_lru = Arc::new(Mutex::new(StdLruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).unwrap(),
        )));
        bench(
            "MutexLRU",
            num_threads,
            std_lru.clone(),
            |c, k| {
                c.lock().get(&k);
            },
            |c, k, v| {
                c.lock().put(k, v);
            },
        );
    }
}

fn bench<T, FGet, FPut>(name: &str, num_threads: usize, cache: T, read_op: FGet, write_op: FPut)
where
    T: Clone + Send + Sync + 'static,
    FGet: Fn(&T, usize) + Copy + Send + Sync + 'static,
    FPut: Fn(&T, usize, usize) + Copy + Send + Sync + 'static,
{
    // Pre-fill
    // We can't easily pre-fill generically enough without complicating the signature,
    // but the benchmark loop runs enough ops that cold start is negligible for 2s run.

    let start_signal = Arc::new(AtomicUsize::new(0));
    let total_ops = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    for t in 0..num_threads {
        let cache = cache.clone();
        let start_signal = start_signal.clone();
        let total_ops = total_ops.clone();

        handles.push(thread::spawn(move || {
            let mut state = (t as u64).wrapping_add(123456789);
            let mut rng = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as usize
            };

            while start_signal.load(Ordering::Relaxed) == 0 {
                std::hint::spin_loop();
            }

            let mut ops = 0;
            while start_signal.load(Ordering::Relaxed) == 1 {
                for _ in 0..100 {
                    let r = rng();
                    let key = (r % KEY_SPACE) + 1;

                    if r % 100 < 90 {
                        read_op(&cache, key);
                    } else {
                        write_op(&cache, key, r);
                    }
                }
                ops += 100;
            }
            total_ops.fetch_add(ops, Ordering::Relaxed);
        }));
    }

    thread::sleep(std::time::Duration::from_millis(50));
    start_signal.store(1, Ordering::Release);
    let start_time = Instant::now();
    thread::sleep(std::time::Duration::from_secs(DURATION_SECS));
    start_signal.store(2, Ordering::Release);
    let elapsed = start_time.elapsed();

    for h in handles {
        h.join().unwrap();
    }

    let ops = total_ops.load(Ordering::Acquire);
    let throughput = ops as f64 / elapsed.as_secs_f64();
    println!("{},{},{:.2}", name, num_threads, throughput);
}
