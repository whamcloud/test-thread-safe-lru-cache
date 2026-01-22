Least Recently Used Cache

There are several ways to implement least recently used cache mechanism, however here we are going to make use of specific arrangement to optimize the speed.
Key points for design are:
 * Going to use contiguous memory
 * Global lock is easy to implement, but going to stall frequently, unless single thread we must have lock. The idea is to use a sparse lock, with the assumption that the requests will be spread out across keys
 * We are going to make use of Rust specific optimization like use of Arrays instead of Linked lists to maximize the performance and cache hits
 * We assume reads will be more than writes

 Definition of problem:
 * There are N threads that can read or write M data locations at any point of time
 * At any point of time only one thread can modify a single location in M
 * At any point of time all threads including the one that wrote a data should see only the last modified data
 * At best any read of data should come from M locations instead of fetching from source

 Implementation points:
  * Will use keys and values separately, so that cache will be filled optimally. 
  * Will use integer types for both keys and values, for now we will assume value has the actual value but could be made as index for large type of datasets
  * Will break the lock region to 64 bytes or similar, so that general-purpose CPUs' cache hit will match this
  * Will use counter for each keys for access, this would then be used for finding the least used entry
  * To free an entry we will make the keys to some invalid or zero value, this would then be used to find the location along with usage counter, where free entries will have more priority and the least valued counter will be the next priority
  * All keys, values, hit counts will use same indexing and key is the master

Post implementation points:
  * The key idea is that we separate Keys and Values so they can be placed in contiguous memory. Keys (and small metadata) are Atomic where practical to avoid read-side locks.
  * Used hit counters instead of timestamps for compactness and easier atomic updates.
  * Used in-place clearing of entries rather than shifting the data structures; clearing an entry is done by setting the key to zero.
  * Read synchronization: readers are lock-free in the common case — they perform atomic loads (Acquire) of the key/metadata and then read the value. Publication rules guarantee consistency: writers publish the new key (or version) with a Release-store after updating the value, so a reader that observes the published key also observes the updated value.
  * Write lock mechanism: writers take a small, fold-scoped write lock (Mutex) to serialize modifications within that fold. Under the lock a writer updates the value and hit counter, then uses an atomic Release-store to publish the new key/version. This keeps contention local and limits the locked region to the minimal write-critical section.
  * To avoid readers observing partially-updated data, writers update value first (under lock) and only then update the key/version with Release ordering. Readers verify the observed key is non-zero and then re-check or retry once if a mismatch is detected.
  * Atomics plus fold-level locks minimize locking: only writers ever take Mutexes; readers stay lock-free and fast.
  * The structure uses traits, so different Atomic types or ordering strategies can be plugged in.
  * Folds remain configurable at cache creation; fold-scoped locks keep contention low and helped the benchmark exceed standard dashmap performance.

Visualization:

 Keys:    | k1 | k2 | k3 | k4 | ... | kn |
 Values:  | v1 | v2 | v3 | v4 | ... | vn |
 Hits:    | h1 | h2 | h3 | h4 | ... | hn |
 Folds:   <---fold1--><-fold2-> .. <foldX>

 * Folds are configured at cache creation
 * User control which fold to select by user provided function
 * Key = 0 means entry free in all three rows, will set when entry freed, no movement
 * Hits maintains the use count, used for eviction
