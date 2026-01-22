use lru_rs::LRUCache;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const CACHE_CAPACITY: usize = 100_000;
const KEY_SPACE: usize = 200_000; // 2x capacity to ensure churn
const DURATION_SECS: u64 = 2; // Run each test for 2 seconds

fn main() {
    println!("Threads,Throughput (Ops/sec)");

    let thread_counts = vec![1, 2, 4, 8, 16, 24, 32];

    for &num_threads in &thread_counts {
        run_benchmark(num_threads);
    }
}

fn run_benchmark(num_threads: usize) {
    let cache = Arc::new(LRUCache::<AtomicUsize, AtomicUsize>::new(
        CACHE_CAPACITY,
        16, // 16 folds to reduce contention
        |k| k,
    ));

    // Pre-fill slightly to avoid initial empty cache effects (optional, but good for stability)
    for i in 1..CACHE_CAPACITY / 2 {
        cache.put(i, i);
    }

    let start_signal = Arc::new(AtomicUsize::new(0));
    let total_ops = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    for t in 0..num_threads {
        let cache = cache.clone();
        let start_signal = start_signal.clone();
        let total_ops = total_ops.clone();

        handles.push(thread::spawn(move || {
            // Simple LCG PRNG
            let mut state = (t as u64).wrapping_add(123456789);
            let mut rng = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as usize
            };

            // Wait for signal
            while start_signal.load(Ordering::Relaxed) == 0 {
                std::hint::spin_loop();
            }

            let mut ops = 0;
            // Run until signal turns off (we use time-based approximation in the main thread)
            // OR simpler: just run specifically for a duration loop.
            // Actually, checking time in hot loop is expensive.
            // Let's run in batches.

            while start_signal.load(Ordering::Relaxed) == 1 {
                for _ in 0..100 {
                    let r = rng();
                    let key = (r % KEY_SPACE) + 1; // 1 to KEY_SPACE
                    let action = r % 100;

                    if action < 90 {
                        // 90% GET
                        let _ = cache.get(key);
                    } else {
                        // 10% PUT
                        cache.put(key, r);
                    }
                }
                ops += 100;
            }
            total_ops.fetch_add(ops, Ordering::Relaxed);
        }));
    }

    // Warmup
    thread::sleep(std::time::Duration::from_millis(100));

    // START
    start_signal.store(1, Ordering::Release);
    let start_time = Instant::now();

    // SLEEP for Duration
    thread::sleep(std::time::Duration::from_secs(DURATION_SECS));

    // STOP
    start_signal.store(2, Ordering::Release);
    let elapsed = start_time.elapsed();

    for h in handles {
        h.join().unwrap();
    }

    let ops = total_ops.load(Ordering::Acquire);
    let ops_per_sec = ops as f64 / elapsed.as_secs_f64();

    println!("{},{:.2}", num_threads, ops_per_sec);
}
