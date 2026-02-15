use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use std::thread;

use lru_cache::LruCache;

fn bench_concurrent(c: &mut Criterion) {
    c.bench_function("concurrent_4_threads", |b| {
        b.iter(|| {
            let cache = Arc::new(LruCache::new(1000));
            let mut handles = vec![];

            for t in 0..4 {
                let c = Arc::clone(&cache);
                handles.push(thread::spawn(move || {
                    for i in 0..1000 {
                        c.put(i, t);
                        black_box(c.get(&i));
                    }
                }));
            }

            for h in handles {
                h.join().unwrap();
            }
        })
    });
}

criterion_group!(benches, bench_concurrent);
criterion_main!(benches);
