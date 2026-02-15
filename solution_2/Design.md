# DESIGN.md

## Overview

This project implements a sharded, thread-safe Least Recently Used (LRU) cache in Rust.
It aims to reduce lock contention under concurrent workloads by partitioning the key space
across multiple shards, each protected by a single `Mutex`.

Goals:
- Maintain O(1) average-time `get`/`put`.
- Enforce a strict, fixed total capacity across shards.
- Preserve correct LRU behavior under contention.
- Use only safe Rust with straightforward, maintainable code.

---

## Components

### ShardedLruCache<K, V>
- Entry point API used by applications.
- Holds a `Vec<Mutex<Shard<K, V>>>`. Each shard is independent.
- Responsibilities:
  - Map keys to shard indexes via a hash function.
  - Delegate `get`/`put` to the appropriate shard.
  - Aggregate derived properties (`len`, `total_capacity`, `debug_order`).

### Shard<K, V>
- Maintains a per-shard LRU list and a `HashMap` of entries.
- Each `Entry` stores:
  - `value: V`
  - `prev: Option<K>` and `next: Option<K>` links (key-based adjacency)
- Tracks:
  - `head` (Most Recently Used)
  - `tail` (Least Recently Used)
  - `capacity`

### Entry<K, V>
- Lightweight record for value and adjacency within a shard.
- Adjacency via keys avoids per-node heap allocations and extra mutexes.

---

## Data Flow and Execution

1) `put(key, value)`
- Compute shard index using a default hasher.
- Lock that shard's mutex.
- If key exists: update value, move to MRU.
- If new key: insert at MRU, fix head/tail pointers.
- If shard exceeds capacity: evict the tail (LRU) in O(1).
- Unlock the shard.

2) `get(key)`
- Compute shard index; lock shard.
- Lookup entry; if present, clone value and move to MRU.
- Unlock shard and return value (or `None`).

3) Concurrency
- Shards operate independently; only the target shard’s mutex is held during operations.
- This significantly reduces contention relative to a single global lock.

ASCII LRU list (per shard):

```
head (MRU) -> [K] <-> [K] <-> ... <-> [K] <- tail (LRU)
```

---

## Synchronization Strategy

- Each shard uses a single `Mutex<Shard>`.
- No cross-shard locking is required for a single operation.
- Lock poisoning recovery: the code uses `unwrap_or_else(|e| e.into_inner())`
  to continue operation if a previous panic poisoned the mutex.

---

## Trade-offs

Pros:
- Lower lock contention than a single global lock.
- O(1) LRU operations using key-based adjacency.
- Bounded memory per shard with bounded total capacity.

Cons:
- Keys that map to the same shard still contend on that shard’s mutex.
- Capacity must be distributed across shards; balancing is even and may not match key skew.
- `V: Clone` is required for `get`, which can be expensive for large values.

---

## Known Limitations

1) Shard balancing is even; hot keys in a single shard may still contend.
2) Values are cloned on `get` to return ownership.
3) Lock poisoning recovery assumes invariants remain satisfied.

---

## Correctness Notes

- Invariants maintained within each shard:
  - Every key in the map appears exactly once in the LRU adjacency.
  - `head` is MRU; `tail` is LRU.
  - Moving to MRU and evicting LRU update adjacency in O(1).
- All shard mutations occur while holding the shard mutex.

---

## Running

Build and run the binary:

```bash
cargo run
```

Run tests:

```bash
cargo test
```

Lint with warnings as errors:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Format check:

```bash
cargo fmt --all -- --check
```

Docgen:

```bash
cargo doc --open
```

---

## Implementation Details

- `distribute_capacity(total, shards)` splits total capacity evenly across shards,
  ensuring the number of shards never exceeds the total capacity to preserve bounds.
- `move_to_front` carefully patches `prev`/`next` links and adjusts head/tail where needed.
- `evict_if_needed` removes the tail in O(1) time and updates adjacency.

---

## Example Output

Running `cargo run` prints a final concatenated MRU→LRU order (each shard appended in index order),
along with total size and capacity, demonstrating a healthy, bounded cache under concurrent use.

