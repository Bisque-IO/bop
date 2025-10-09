# Work-Stealing Integration Strategies for Executor Worker Loop

## Current Architecture Analysis

### Current Setup:
- **Global Queue**: `Mpmc<TaskPtr, 8, 12>` - shared MPMC queue
- **Worker Loop**: Pulls batches of up to 512 tasks from global queue
- **Processing**: Tasks execute locally in the batch array
- **Re-enqueue**: Tasks that yield or need rescheduling go back to global queue

### Current Flow:
```
1. Pop batch (up to 512 tasks) from global MPMC queue
2. Process each task in batch locally
3. If task yields/pending → add to enqueue list
4. Push enqueue list back to global queue
5. Repeat
```

### Performance Characteristics:
- ✅ Fast local processing (array iteration)
- ✅ Batch operations reduce queue contention
- ❌ All tasks go through global queue (contention point)
- ❌ No work-stealing between workers

---

## Strategy 1: Hybrid Local Worker + Global Queue with Crossbeam Stealing

### Concept:
Each worker has its own local `crossbeam::Worker` queue. Workers primarily work from their local queue, but can steal from others when idle.

### Architecture:
```rust
struct ExecutorInner {
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,          // Fallback/overflow
    local_workers: Vec<Worker<TaskPtr>>,               // Per-worker local queues
    stealers: Vec<Stealer<TaskPtr>>,                   // For work-stealing
    tasks: dashmap::DashMap<usize, TaskPtr>,
}
```

### Worker Loop Logic:
```rust
fn worker_loop(
    worker_id: usize, 
    local_worker: Worker<TaskPtr>,
    stealers: Arc<Vec<Stealer<TaskPtr>>>,
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    shutdown: Arc<AtomicBool>
) {
    let mut batch: [TaskPtr; 512] = [...];
    let mut enqueue = Vec::with_capacity(1024);
    
    loop {
        // 1. Try local queue first (LIFO - best cache locality)
        let mut size = 0;
        while size < 512 {
            match local_worker.pop() {
                Some(task) => { batch[size] = task; size += 1; }
                None => break,
            }
        }
        
        // 2. If local queue empty, try stealing from others
        if size == 0 {
            for (i, stealer) in stealers.iter().enumerate() {
                if i == worker_id { continue; }
                match stealer.steal_batch_and_pop(&local_worker) {
                    Steal::Success(task) => { batch[0] = task; size = 1; break; }
                    Steal::Empty => continue,
                    Steal::Retry => continue,
                }
            }
        }
        
        // 3. If still nothing, check global queue
        if size == 0 {
            size = global_queue.try_pop_n(&mut batch);
        }
        
        if size == 0 { spin_loop(); continue; }
        
        // 4. Process batch (same as current logic)
        for i in 0..size {
            let task_ptr = batch[i];
            // ... poll task ...
            if needs_reschedule {
                enqueue.push(task_ptr);
            }
        }
        
        // 5. Re-enqueue to LOCAL queue first
        for task in enqueue.drain(..) {
            if local_worker.push(task).is_err() {
                // Local queue full, push to global
                global_queue.push(task);
            }
        }
    }
}
```

### Pros:
- ✅ Minimal contention - most work stays local
- ✅ Excellent cache locality (LIFO local queue)
- ✅ Work-stealing only when needed
- ✅ Global queue as overflow/balancing mechanism
- ✅ Crossbeam unbounded queues prevent deadlock

### Cons:
- ❌ More complex - two queue systems
- ❌ Slight overhead from checking multiple sources
- ❌ Memory overhead (unbounded crossbeam queues)

### Performance Impact:
- **Best case (balanced load)**: ~5-10% faster (less global queue contention)
- **Imbalanced load**: 20-50% faster (work-stealing rebalances)
- **High contention**: 30-40% faster (local queues reduce contention)

---

## Strategy 2: Per-Worker Local Batch with Stealable Tail

### Concept:
Keep the current batch-based processing, but make the unprocessed portion of the batch stealable via a lock-free mechanism.

### Architecture:
```rust
struct StealableBatch {
    tasks: [AtomicPtr<TaskInner>; 512],
    head: AtomicUsize,    // Next task to process
    tail: AtomicUsize,    // Last valid task
}

struct ExecutorInner {
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    worker_batches: Vec<Arc<StealableBatch>>,  // One per worker
    tasks: dashmap::DashMap<usize, TaskPtr>,
}
```

### Worker Loop Logic:
```rust
fn worker_loop(
    worker_id: usize,
    my_batch: Arc<StealableBatch>,
    other_batches: Arc<Vec<Arc<StealableBatch>>>,
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    shutdown: Arc<AtomicBool>
) {
    let mut local_batch: [TaskPtr; 512] = [...];
    let mut enqueue = Vec::with_capacity(1024);
    
    loop {
        // 1. Pop from global queue into local batch
        let size = global_queue.try_pop_n(&mut local_batch);
        
        if size > 0 {
            // 2. Store batch in shared location for stealing
            let tail = size;
            for i in 0..size {
                my_batch.tasks[i].store(local_batch[i].0, Ordering::Release);
            }
            my_batch.head.store(0, Ordering::Release);
            my_batch.tail.store(tail, Ordering::Release);
            
            // 3. Process batch, allowing concurrent stealing
            let mut head = 0;
            while head < tail {
                // Try to claim next task atomically
                let current = my_batch.head.load(Ordering::Acquire);
                if current >= tail { break; }
                
                if my_batch.head.compare_exchange(
                    current, 
                    current + 1, 
                    Ordering::AcqRel, 
                    Ordering::Acquire
                ).is_ok() {
                    let task_ptr = TaskPtr(my_batch.tasks[current].load(Ordering::Acquire));
                    
                    // Process task
                    // ... poll task ...
                    if needs_reschedule {
                        enqueue.push(task_ptr);
                    }
                    
                    head = current + 1;
                }
            }
        } else {
            // 4. Try stealing from other workers' batches
            for (i, other) in other_batches.iter().enumerate() {
                if i == worker_id { continue; }
                
                let tail = other.tail.load(Ordering::Acquire);
                let head = other.head.load(Ordering::Acquire);
                let available = tail.saturating_sub(head);
                
                if available > 0 {
                    // Try to steal half
                    let steal_count = (available / 2).max(1);
                    let steal_start = tail - steal_count;
                    
                    // Atomic update tail to claim tasks
                    if other.tail.compare_exchange(
                        tail,
                        steal_start,
                        Ordering::AcqRel,
                        Ordering::Acquire
                    ).is_ok() {
                        // Successfully stole tasks
                        for idx in steal_start..tail {
                            let task = TaskPtr(other.tasks[idx].load(Ordering::Acquire));
                            local_batch[idx - steal_start] = task;
                        }
                        // Process stolen tasks...
                        break;
                    }
                }
            }
        }
        
        // 5. Re-enqueue
        if !enqueue.is_empty() {
            global_queue.try_push_n(&enqueue);
            enqueue.clear();
        }
    }
}
```

### Pros:
- ✅ Keeps batch-based processing model
- ✅ Lock-free stealing mechanism
- ✅ Minimal changes to existing architecture
- ✅ Stealing from tail = better cache locality for victim

### Cons:
- ❌ Complex atomic coordination
- ❌ CAS contention when multiple thieves target same victim
- ❌ Array of atomic pointers has memory overhead
- ❌ Stealing disrupts victim's linear iteration

### Performance Impact:
- **Best case (no stealing)**: ~2-3% slower (atomic overhead)
- **Imbalanced load**: 15-25% faster (work-stealing helps)
- **High contention**: May degrade due to CAS storms

---

## Strategy 3: St3 Bounded Local Queues (Memory-Bounded)

### Concept:
Use St3's bounded LIFO queues for local workers with work-stealing, ensuring predictable memory usage.

### Architecture:
```rust
use st3::lifo::Worker as St3Worker;

struct ExecutorInner {
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    local_workers: Vec<St3Worker<TaskPtr>>,     // Fixed capacity
    stealers: Vec<st3::lifo::Stealer<TaskPtr>>,
    tasks: dashmap::DashMap<usize, TaskPtr>,
}
```

### Worker Loop Logic:
```rust
fn worker_loop(
    worker_id: usize,
    local_worker: St3Worker<TaskPtr>,  // Capacity: 1024
    stealers: Arc<Vec<st3::lifo::Stealer<TaskPtr>>>,
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    shutdown: Arc<AtomicBool>
) {
    let mut batch: [TaskPtr; 512] = [...];
    let mut enqueue = Vec::with_capacity(1024);
    
    loop {
        // 1. Try local queue (LIFO)
        let mut size = 0;
        while size < 512 {
            match local_worker.pop() {
                Some(task) => { batch[size] = task; size += 1; }
                None => break,
            }
        }
        
        // 2. Steal from others if local is empty
        if size == 0 {
            for (i, stealer) in stealers.iter().enumerate() {
                if i == worker_id { continue; }
                match stealer.steal(&local_worker, |n| n / 2) {
                    Ok(stolen) if stolen > 0 => {
                        // Items now in local_worker, pop them
                        while size < 512 {
                            match local_worker.pop() {
                                Some(task) => { batch[size] = task; size += 1; }
                                None => break,
                            }
                        }
                        break;
                    }
                    _ => continue,
                }
            }
        }
        
        // 3. Check global queue
        if size == 0 {
            size = global_queue.try_pop_n(&mut batch);
        }
        
        if size == 0 { spin_loop(); continue; }
        
        // 4. Process batch
        for i in 0..size {
            // ... process task ...
            if needs_reschedule {
                enqueue.push(task_ptr);
            }
        }
        
        // 5. Re-enqueue to local first, overflow to global
        for task in enqueue.drain(..) {
            if local_worker.push(task).is_err() {
                // Queue full, must use global
                loop {
                    if global_queue.try_push(task).is_ok() {
                        break;
                    }
                    // If global full too, process some local work first
                    if let Some(local_task) = local_worker.pop() {
                        // Process immediately...
                    }
                }
            }
        }
    }
}
```

### Pros:
- ✅ Bounded memory usage (critical for production)
- ✅ Excellent performance (from benchmark: 78M ops/sec)
- ✅ Fewer atomic operations than Crossbeam
- ✅ Flexible stealing strategies

### Cons:
- ❌ Queue full errors require handling
- ❌ Not as fast as Crossbeam for balanced loads
- ❌ Capacity tuning required
- ❌ High contention can cause issues (from benchmark)

### Performance Impact:
- **Best case (balanced)**: ~10-15% slower than Strategy 1
- **Imbalanced load**: Similar to Strategy 1 (work-stealing works well)
- **Memory footprint**: Much better (bounded)

---

## Strategy 4: Hybrid Batch + Work-Stealing Queue

### Concept:
Process batches locally, but maintain a separate small work-stealing queue for overflow/stealing.

### Architecture:
```rust
struct ExecutorInner {
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    local_queues: Vec<Worker<TaskPtr>>,        // Small capacity for overflow
    stealers: Vec<Stealer<TaskPtr>>,
    tasks: dashmap::DashMap<usize, TaskPtr>,
}
```

### Worker Loop Logic:
```rust
fn worker_loop(
    worker_id: usize,
    local_queue: Worker<TaskPtr>,  // For overflow/stealing only
    stealers: Arc<Vec<Stealer<TaskPtr>>>,
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    shutdown: Arc<AtomicBool>
) {
    let mut batch: [TaskPtr; 512] = [...];
    let mut enqueue = Vec::with_capacity(1024);
    
    loop {
        let mut size = 0;
        
        // 1. Check local overflow queue first (should be small)
        while size < 64 {  // Only take a few from local
            match local_queue.pop() {
                Some(task) => { batch[size] = task; size += 1; }
                None => break,
            }
        }
        
        // 2. Fill rest from global queue
        if size < 512 {
            let global_size = global_queue.try_pop_n(&mut batch[size..]);
            size += global_size;
        }
        
        // 3. If still empty, try stealing
        if size == 0 {
            for (i, stealer) in stealers.iter().enumerate() {
                if i == worker_id { continue; }
                match stealer.steal_batch(&local_queue) {
                    Steal::Success(_) => {
                        // Pop stolen items into batch
                        while size < 512 {
                            match local_queue.pop() {
                                Some(task) => { batch[size] = task; size += 1; }
                                None => break,
                            }
                        }
                        break;
                    }
                    _ => continue,
                }
            }
        }
        
        if size == 0 { spin_loop(); continue; }
        
        // 4. Process batch
        for i in 0..size {
            // ... process task ...
            if needs_reschedule {
                enqueue.push(task_ptr);
            }
        }
        
        // 5. Re-enqueue: small items to local, bulk to global
        if enqueue.len() <= 32 {
            // Small re-enqueue → local queue
            for task in enqueue.drain(..) {
                local_queue.push(task).ok();  // Ignore full errors
            }
        } else {
            // Large re-enqueue → global queue
            global_queue.try_push_n(&enqueue);
            enqueue.clear();
        }
    }
}
```

### Pros:
- ✅ Keeps batch processing model
- ✅ Local queue only for hot/recent tasks (good cache)
- ✅ Most bulk work through global (tested code path)
- ✅ Work-stealing for load balancing

### Cons:
- ❌ Still two queue systems to manage
- ❌ Heuristic for local vs global may be tricky
- ❌ Overhead from checking multiple sources

### Performance Impact:
- **Best case**: ~3-5% faster (hot tasks stay local)
- **Imbalanced load**: 10-20% faster (work-stealing helps)
- **Complexity**: Low (minimal changes)

---

## Strategy 5: Lock-Free Linked List with Batch Draining

### Concept:
Each worker maintains a lock-free linked list of tasks. Workers can drain batches for processing and steal entire segments.

### Architecture:
```rust
use crossbeam::queue::SegQueue;  // Lock-free unbounded queue

struct ExecutorInner {
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    local_queues: Vec<Arc<SegQueue<TaskPtr>>>,
    tasks: dashmap::DashMap<usize, TaskPtr>,
}
```

### Worker Loop Logic:
```rust
fn worker_loop(
    worker_id: usize,
    local_queue: Arc<SegQueue<TaskPtr>>,
    other_queues: Arc<Vec<Arc<SegQueue<TaskPtr>>>>,
    global_queue: Arc<Mpmc<TaskPtr, 8, 12>>,
    shutdown: Arc<AtomicBool>
) {
    let mut batch: [TaskPtr; 512] = [...];
    let mut enqueue = Vec::with_capacity(1024);
    
    loop {
        // 1. Drain local queue into batch
        let mut size = 0;
        while size < 512 {
            match local_queue.pop() {
                Some(task) => { batch[size] = task; size += 1; }
                None => break,
            }
        }
        
        // 2. Try stealing from others
        if size == 0 {
            for (i, other) in other_queues.iter().enumerate() {
                if i == worker_id { continue; }
                
                // Steal up to 256 items
                let mut stolen = 0;
                while stolen < 256 && size < 512 {
                    match other.pop() {
                        Some(task) => { 
                            batch[size] = task; 
                            size += 1; 
                            stolen += 1;
                        }
                        None => break,
                    }
                }
                if stolen > 0 { break; }
            }
        }
        
        // 3. Check global
        if size == 0 {
            size = global_queue.try_pop_n(&mut batch);
        }
        
        if size == 0 { spin_loop(); continue; }
        
        // 4. Process batch
        for i in 0..size {
            // ... process ...
            if needs_reschedule {
                enqueue.push(task_ptr);
            }
        }
        
        // 5. Re-enqueue to local
        for task in enqueue.drain(..) {
            local_queue.push(task);
        }
    }
}
```

### Pros:
- ✅ Simple API (just push/pop)
- ✅ Lock-free and unbounded
- ✅ Easy stealing (just pop from other queues)
- ✅ No CAS storms

### Cons:
- ❌ SegQueue not optimized for work-stealing
- ❌ Stealing from head = poor cache locality
- ❌ No batch stealing primitive
- ❌ Memory overhead (linked list nodes)

### Performance Impact:
- **Best case**: ~5-10% slower (linked list overhead)
- **Imbalanced load**: 15-25% faster (stealing works)
- **High contention**: May degrade (head contention)

---

## Recommendation Matrix

| Strategy | Performance | Complexity | Memory | Best For |
|----------|------------|------------|--------|----------|
| **#1: Crossbeam Hybrid** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐ | General-purpose, high performance |
| **#2: Stealable Batch** | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | Minimal changes, experimental |
| **#3: St3 Bounded** | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | Memory-constrained, production |
| **#4: Hybrid Batch** | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ | Conservative upgrade |
| **#5: SegQueue** | ⭐⭐⭐ | ⭐ | ⭐⭐ | Simplicity over performance |

---

## Implementation Recommendation

### **Start with Strategy #1 (Crossbeam Hybrid)**

**Why:**
1. Best overall performance (from benchmarks: 150M ops/sec balanced, 88M imbalanced)
2. Proven technology (used in Tokio, Rayon)
3. No queue full errors to handle
4. Excellent work-stealing performance

**Implementation Steps:**
1. Add `crossbeam-deque` dependency
2. Create per-worker `Worker` queues in executor initialization
3. Share `Stealer` handles among workers
4. Modify `worker_loop` to check: local → steal → global
5. Re-enqueue to local first, overflow to global

**Fallback to Strategy #4** if complexity is concern - simpler, still gets most benefits.

**Use Strategy #3** if memory bounds are critical requirement.

---

## Performance Expectations

Based on crossbeam benchmarks:

### Current (Global MPMC only):
- Estimated: 40-60M ops/sec (moderate contention on global queue)

### With Strategy #1 (Crossbeam Hybrid):
- **Balanced load**: 100-150M ops/sec (2-3x faster)
- **Imbalanced load**: 80-120M ops/sec (1.5-2x faster)
- **High contention**: 50-100M ops/sec (1-2x faster)

### With Strategy #3 (St3):
- **Balanced load**: 60-80M ops/sec (1-1.5x faster)
- **Imbalanced load**: 100-160M ops/sec (2-2.5x faster)
- **High contention**: May degrade (capacity issues)

---

## Next Steps

1. **Prototype Strategy #1** in a separate example file
2. **Benchmark** against current implementation
3. **Tune parameters**: batch size, stealing strategy, queue capacity
4. **Measure**:
   - Throughput (tasks/sec)
   - Latency (p50, p99)
   - Cache misses
   - Context switches
5. **Compare** with Strategy #4 if #1 is too complex
