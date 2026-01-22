# LRU Cache: Test & Benchmark Summary

## Unit Tests
**Status**: Passing (18/18 tests)
**Coverage**: Basic Logic, Eviction (LFU-approx), Edge Cases, Concurrency (Lock-Free Reads, Sharded Writes), Stress/Chaos.

## Performance Benchmark
**Configuration**: 100k Items, 16 Threads, 90% Read / 10% Write.
**Key Insight**: Using "High Folds" (sharding) effectively turns linear scans into O(1) lookups, yielding massive throughput.

| Implementation | Configuration | Ops/Sec | Note |
| :--- | :--- | :--- | :--- |
| **MyLRU (This Logic)** | **High Folds (25k)** | **~850,989,056** | **Fastest. 0 contention, cache-friendly.** |
| DashMap | Standard | ~328,151,154 | No eviction logic (Raw Map baseline). |
| Moka | Sync | ~14,390,086 | Industry standard concurrent LRU. |
| Mutex<Lru> | Standard | ~14,449,427 | Heavy lock contention at scale. |
| MyLRU | Low Folds (16) | ~1,081,666 | Slower due to linear scanning in large folds. |

## How to Run

### 1. Run Unit Tests
Verifies correctness, concurrency safety, and logic.
```bash
cargo test
```

### 2. Run Benchmarks
Compiles optimized release build and generates an HTML graph (`benchmark_report.html`).
```bash
python3 run_benchmark.py
```
*(Requires Python 3. No external Python libs needed, just standard library)*

---
**System Environment**: Linux, 16 Cores.
**Date**: January 22, 2026.
