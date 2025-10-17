# Sanakirja Version Overflow Analysis

## Overview

When sanakirja's version cycles back to an old root that still has active readers, the system enters a safe blocking state to prevent data corruption.

## Version Overflow Scenario

Let's analyze what happens with `n_roots = 4` when writes proceed faster than readers finish:

```
Timeline (Current Root Points to Next Available):
=================================================================
Time    Action                      Current Root    Reader States
-----------------------------------------------------------------
T0      Reader starts txn           1               Reader on root 0
T1      Writer 1 commits            2               Reader on root 0
T2      Writer 2 commits            3               Reader on root 0  
T3      Writer 3 commits            0               Reader on root 0
T4      Writer 4 commits            1               Reader on root 0
T5      Writer 5 tries              [BLOCKS]        Reader on root 0
=================================================================
```

## Lock Contention Analysis

### Root Lock States

```rust
// Each root has: parking_lot::RawRwLock + AtomicUsize n_txn
pub(crate) struct RootLock {
    #[cfg(feature = "mmap")] lock_file: Option<std::fs::File>,
    rw: parking_lot::RawRwLock,        // RWLock for coordination
    n_txn: AtomicUsize,                // Reader count
}
```

### State Transitions

```
Root 0: [READER_SHARED_LOCK]❗ (1 reader, prevents exclusive access)
Root 1: [FREE] ← Writer wants this (was reader's root, now cycled)
Root 2: [FREE]
Root 3: [FREE]
```

### Blocking Point

```rust
// In mut_txn_begin() - Writer blocks here:
env_.roots[root].rw.lock_exclusive(); // root = 1, locked by reader
// ↑ This blocks until reader completes
```

## Memory Safety Guarantees

### 1. Version Isolation
- Each root page is completely independent
- Readers see immutable snapshots
- Writers work on isolated copies

### 2. Lock-Based Protection
```rust
// Reader acquisition
env_.roots[root].rw.lock_shared();           // Shared lock
env_.roots[root].n_txn.fetch_add(1, ...);    // Increment count

// Writer acquisition  
env_.roots[root].rw.lock_exclusive();         // Exclusive lock (blocks!)
```

### 3. Page Copy Safety
```rust
// Safe copy (fixed in our earlier patch)
if v0 != root {
    std::ptr::copy_nonoverlapping(page_ptr.add(8), next_page_ptr.add(8), PAGE_SIZE - 8);
}
```

## Performance Implications

### Reader Impact
- ✅ ** unaffected**: Readers continue normally until completion
- ✅ ** consistent snapshot**: No data corruption possible
- ✅ ** no timeouts**: Reader can take as long as needed

### Writer Impact  
- ⚠️ ** blocked**: Must wait for old readers to finish
- ⚠️ ** reduced throughput**: Write rate limited by slowest reader
- ✅ ** no data loss**: Writing resumes when lock available

## Configuration Trade-offs

### `n_roots = 1` (Serial)
```
 Pros: Simple, predictable behavior
 Cons: Readers block writers, writers block readers
 Use: Light read workloads, simple concurrency
```

### `n_roots = 2-4` (Balanced)  
```
 Pros: Good read-write concurrency, reasonable memory
 Cons: Potential blocking under heavy writes
 Use: Most applications (default recommended)
```

### `n_roots = 8+` (High Concurrency)
```
 Pros: Very low blocking probability, high read throughput
 Cons: More memory usage (8KB per additional root)
 Use: Read-heavy workloads with long-running queries
```

## Blocking Probability Calculation

For `n_roots = R` and `W` writers during reader lifetime `T`:

```rust
// Probability of blocking ~ (W * T) / R
// Example: R=4, W=10 writes/sec, T=2s reader lifetime
// P(block) ≈ (10 * 2) / 4 = 5.0 → >100% (certain blocking)
```

## Mitigation Strategies

### 1. Increase `n_roots`
```rust
// From n_roots = 4 to n_roots = 8
let env = Env::new_anon(size, 8)?; // Doubles buffer
```

### 2. Reader Timeouts
```rust
// Monitor and handle slow readers
let start = Instant::now();
let reader = env.txn_begin()?;
// ... use reader ...
let duration = start.elapsed();
if duration > Duration::from_secs(5) {
    warn!("Slow reader detected: {:?}", duration);
}
```

### 3. Write Rate Limiting
```rust
// Throttle writes if approaching limit
if active_readers > n_roots - 1 {
    wait_for_reader_completion();
}
```

## Real-World Example

```rust
use sanakirja::*;
use std::thread;
use std::time::Duration;

// Environment with 4 root pages
let env = Env::new_anon(1024 * 1024, 4)?;

// Slow reader (takes 10 seconds)
let reader_handle = thread::spawn({
    let env = env.clone();
    move || {
        let txn = env.txn_begin().unwrap(); // Locks root 0
        println!("Reader started");
        thread::sleep(Duration::from_secs(10));
        println!("Reader finishing");
        // txn drops here, releases lock
    }
});

// Fast writers (commit every 500ms)
for i in 0.. {
    let env = env.clone();
    thread::spawn(move || {
        let mut txn = env.mut_txn_begin().unwrap(); // May block!
        let mut db = create_db::<_, u64, (), _>(&mut txn).unwrap();
        put(&mut txn, &mut db, &i, &()).unwrap();
        match txn.commit() {
            Ok(_) => println!("Writer {} committed", i),
            Err(_) => println!("Writer {} blocked", i),
        }
    });
    thread::sleep(Duration::from_millis(500));
    
    if i == 15 { break; } // After 8 writers, expect blocking
}

reader_handle.join().unwrap();
```

## Conclusion

Sanakirja's version overflow protection **prevents data corruption** by carefully controlling access to root pages through a combination of:

1. **Root page isolation** (each version is independent)
2. **RWLock coordination** (prevents concurrent access conflicts)  
3. **Atomic reference counting** (tracks active readers)
4. **Writer blocking** (waits for old readers to finish)

The blocking behavior is a **feature, not a bug** - it guarantees memory safety and data consistency at the cost of write throughput for very long-running readers.