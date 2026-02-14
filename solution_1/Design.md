# DESIGN.md

## Overview

This project implements a thread-safe LRU (Least Recently Used) cache in Rust.

The goal was to:

* Support concurrent access from multiple threads
* Keep `get` and `put` operations O(1)
* Maintain correct LRU ordering
* Keep the design simple and easy to reason about

To achieve this, the cache combines a `HashMap`, a doubly linked list, and a single global `Mutex`.

---

## 1. Data Structures Used

### 1. HashMap (Fast Lookup)

We use:

```rust
HashMap<K, (V, NodeRef<K>)>
```

The `HashMap` stores:

* The value (`V`)
* A pointer to the node in the linked list

This allows:

* O(1) lookup by key
* O(1) access to the corresponding linked list node for reordering

The value is stored only in the `HashMap`, not in the linked list node.
The linked list node stores only the key.

This avoids duplication and keeps lookups efficient.

---

### 2. Doubly Linked List (LRU Order)

We maintain a doubly linked list to track usage order:

* `head` → Most Recently Used (MRU)
* `tail` → Least Recently Used (LRU)

Each node contains:

```rust
struct Node<K> {
    key: K,
    prev: Link<K>,
    next: Link<K>,
}
```

When a key is accessed or inserted:

* Its node is moved to the front (MRU position)

When capacity is exceeded:

* We remove the node at the tail (LRU position)

Because it’s a doubly linked list, moving or removing a node takes O(1) time.

---

### 3. Shared Ownership with Arc

Nodes are wrapped in:

```rust
Arc<Mutex<Node<K>>>
```

`Arc` allows multiple owners:

* The `HashMap`
* The linked list

This ensures nodes can be safely shared across threads.

---

## 2. Synchronization Strategy

### Single Global Mutex

All mutable cache state is stored inside:

```rust
Arc<Mutex<Inner<K, V>>>
```

Every `get` and `put` operation:

1. Locks the mutex
2. Performs all necessary changes
3. Releases the lock

This is called **coarse-grained locking**.

### Why Use a Single Mutex?

Because:

* It keeps the design simple
* It prevents race conditions
* It avoids complicated lock ordering
* It makes the implementation easier to reason about

Only one thread can modify the cache at a time, which guarantees consistency.

---

## 3. How LRU Ordering Is Maintained Under Concurrency

Even though multiple threads may call `get` or `put` at the same time:

* Only one thread can access the internal state due to the mutex.
* All list modifications happen while the lock is held.

### On `get(key)`:

* Lookup key in `HashMap`
* Move its node to the front (MRU)

### On `put(key, value)`:

* If key exists:

  * Update value
  * Move node to front
* If key is new:

  * Insert node at front
  * Add to `HashMap`
  * If capacity exceeded → remove tail

Because all of this happens inside one locked section, the linked list can never become inconsistent.

---

## 4. Trade-offs

### Simplicity vs Performance

This design favors **simplicity and correctness** over maximum scalability.

#### Pros

* Easy to understand
* Easy to maintain
* No deadlocks
* Strong correctness guarantees
* Clean invariants

#### Cons

* All operations are serialized
* Only one thread can access the cache at a time
* Does not scale well under very high contention

---

### Why Not Use RwLock?

Even `get()` modifies the LRU order.

That means:

* Reads are actually writes
* A read-write lock would not help much
* Most operations would still require exclusive access

So a single `Mutex` is simpler and just as effective here.

---

## 5. Known Limitations

1. **Limited scalability**

   Since we use one global lock, performance may degrade under heavy multi-threaded workloads.

2. **Memory overhead**

   Each entry includes:

   * A `HashMap` entry
   * A linked list node
   * An `Arc`
   * A `Mutex`

   This is heavier than a single-threaded implementation.

3. **Cloning required**

   `get()` returns a cloned value (`V: Clone`).
   For large values, this could be expensive.

4. **Debug printing inside the lock**

   The implementation avoids debug printing while holding the lock to prevent
   unnecessary blocking and contention.

5. **Lock poisoning recovery**

   All internal mutex acquisitions recover from poisoning by taking the inner
   state (`unwrap_or_else(|e| e.into_inner())`). This prevents panics after a
   prior panic in another thread at the cost of potentially skipping some
   invariants established by that panicking thread. The cache continues
   operating under the assumption that invariants are preserved by coarse-grained
   locking and immediate structural updates.

---

## 6. Why This Design Is Correct

The following are always true:

* Every key in the `HashMap` has exactly one node in the list.
* The list contains exactly the keys in the map.
* `head` is always the most recently used.
* `tail` is always the least recently used.
* All structural changes happen while holding the mutex.

Because of the single global lock, no two threads can modify the list at the same time. This prevents race conditions and ensures the cache remains consistent.

---

## Running Clippy Test


```bash
cargo clippy --all-targets --all-features -- -D warnings
```

```bash
cargo test --all --quiet
```

```bash
cargo run
```

---
