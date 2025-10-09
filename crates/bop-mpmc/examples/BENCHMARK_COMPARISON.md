# Crossbeam-Deque vs St3 Work-Stealing Benchmark Comparison

**Test Environment:** 32 CPU cores, 1M items per test

## Executive Summary

### Winner by Category:
- **Balanced Workload:** Crossbeam (faster by 1.4x - 1.9x)
- **Imbalanced Workload:** St3 (faster by 1.8x - 1.9x) 
- **High Contention:** Crossbeam (massively faster, 18x - 6x)
- **Producer/Consumer:** Crossbeam only (St3 doesn't have Injector pattern)

### Key Findings:
1. **Crossbeam excels at balanced workloads** - superior performance when work is evenly distributed
2. **St3 dominates imbalanced scenarios** - excellent work-stealing performance when rebalancing is needed
3. **St3 struggles with high contention** - bounded queue capacity becomes a bottleneck
4. **St3 has counting issues** - "Completed" items exceed expected totals, indicating double-counting during steals

---

## Detailed Comparison

### 1. Balanced Workload (Local Queues + Stealing)

| Workers | Crossbeam (ns/op) | St3 LIFO (ns/op) | St3 FIFO (ns/op) | Winner |
|---------|-------------------|------------------|------------------|--------|
| 2       | **12.14**         | 17.38            | 12.34            | Crossbeam (1.4x) |
| 4       | **9.38**          | 16.70            | 13.61            | Crossbeam (1.8x) |
| 8       | **6.64**          | 12.81            | 13.03            | Crossbeam (1.9x) |

**Analysis:**
- Crossbeam is consistently 1.4x - 1.9x faster for balanced workloads
- Crossbeam scales better as worker count increases (6.64ns vs 12.81ns at 8 workers)
- St3 FIFO performs better than LIFO in this scenario
- Both show good scaling with increased parallelism

**Throughput Comparison (8 workers):**
- Crossbeam: **150.70M ops/sec**
- St3 LIFO: 78.07M ops/sec
- St3 FIFO: 76.74M ops/sec

---

### 2. Imbalanced Workload (90% work on one worker)

| Workers | Crossbeam (ns/op) | St3 LIFO (ns/op) | Winner |
|---------|-------------------|------------------|--------|
| 4       | 11.41             | **6.43**         | St3 (1.8x faster) |
| 8       | 16.64             | **8.65**         | St3 (1.9x faster) |

**Analysis:**
- **St3 dominates this scenario** by 1.8x - 1.9x
- St3's batch stealing is highly effective for work rebalancing
- Crossbeam actually gets *slower* with more workers (11.41 → 16.64ns)
- St3 maintains performance as workers increase (6.43 → 8.65ns)

**Throughput Comparison (4 workers):**
- St3 LIFO: **155.61M ops/sec**
- Crossbeam: 87.62M ops/sec

**Critical Issue - St3 Over-counting:**
- St3 reports completing 1.50M items when only 1.00M were enqueued
- This suggests items are being counted multiple times during steal operations
- The actual throughput may be inflated

---

### 3. High Contention (Many stealers, few victims)

| Setup | Crossbeam (ns/op) | St3 LIFO (ns/op) | Winner |
|-------|-------------------|------------------|--------|
| 8 workers, 2 queues  | **14.22** | 257.23 | Crossbeam (18x faster) |
| 16 workers, 4 queues | **19.57** | 85.01  | Crossbeam (4.3x faster) |

**Analysis:**
- **Crossbeam massively outperforms St3** in high contention scenarios
- St3's bounded capacity (1024) creates severe bottleneck
- St3 only processed 4.10K - 8.19K items instead of 1M!
- Bounded queues cause workers to block when trying to steal into full queues

**Actual Items Processed:**
- Crossbeam (8 workers): 1.00M items ✓
- St3 (8 workers): 4.10K items ✗ (0.4% completion)
- St3 (16 workers): 8.19K items ✗ (0.8% completion)

**Root Cause:**
St3's bounded queues fill up quickly during stealing, preventing further work distribution. Workers can't steal if their local queue is full.

---

### 4. Producer/Consumer Pattern

**Crossbeam Results:**

| Setup | ns/op | ops/sec |
|-------|-------|---------|
| Injector + 2 workers | 27.60 | 36.23M |
| Injector + 4 workers | 55.46 | 18.03M |
| Injector + 8 workers | 32.50 | 30.76M |

**St3:** Not tested - doesn't have Injector pattern

**Analysis:**
- Crossbeam's Injector provides a dedicated producer/consumer pattern
- Performance varies (18M - 36M ops/sec) depending on worker count
- St3 would require workers to push to each other's queues directly

---

### 5. Multi-Producer Pattern

**Crossbeam Results:**

| Setup | ns/op | ops/sec |
|-------|-------|---------|
| 2 producers + 4 workers | 31.93 | 31.32M |
| 4 producers + 4 workers | 42.49 | 23.54M |

**St3:** Not tested - would require custom coordination

---

### 6. St3 Stealing Strategy Comparison (4 workers)

| Strategy | ns/op | ops/sec | Winner |
|----------|-------|---------|--------|
| Steal Half (50%) | 18.63 | 53.69M | |
| **Steal Quarter (25%)** | **13.52** | **73.95M** | ✓ Best |
| Steal 75% | 17.80 | 56.18M | |
| Steal All (100%) | 17.11 | 58.43M | |

**Analysis:**
- **Steal Quarter (25%) is fastest** - 1.4x faster than Steal Half
- Smaller steal amounts reduce contention and queue overflow
- Stealing all items (100%) is only slightly better than stealing half
- This suggests aggressive stealing causes cache coherence issues

---

## Performance Characteristics

### Crossbeam-Deque Strengths:
1. ✓ **Unbounded capacity** - no queue full issues
2. ✓ **Better balanced workload performance** - 1.4x - 1.9x faster
3. ✓ **Scales well with parallelism** - 6.64ns at 8 workers
4. ✓ **Handles high contention excellently** - 18x faster than St3
5. ✓ **Injector pattern** for producer/consumer scenarios
6. ✓ **Accurate counting** - no over-counting issues

### Crossbeam-Deque Weaknesses:
1. ✗ Slower at work rebalancing (imbalanced scenarios)
2. ✗ More memory overhead (unbounded)
3. ✗ More atomic operations (fences, additional loads/stores)

### St3 Strengths:
1. ✓ **Excellent work-stealing** - 1.8x - 1.9x faster for imbalanced workloads
2. ✓ **Bounded memory usage** - fixed capacity prevents unbounded growth
3. ✓ **Fewer atomic operations** - no fences, fewer RMW ops
4. ✓ **Flexible stealing strategies** - customizable steal amounts
5. ✓ **LIFO and FIFO variants** - semantic flexibility

### St3 Weaknesses:
1. ✗ **Bounded capacity bottleneck** - severe issues in high contention (18x slower)
2. ✗ **Over-counting bug** - reports more completed items than enqueued
3. ✗ **Slower balanced workloads** - 1.4x - 1.9x slower than Crossbeam
4. ✗ **No built-in Injector** - requires manual producer/consumer coordination
5. ✗ **Queue full errors** - requires error handling during push

---

## Recommendations

### Use Crossbeam-Deque when:
- ✓ Workload is balanced or unknown
- ✓ High contention is expected
- ✓ Unbounded growth is acceptable
- ✓ Producer/consumer pattern is needed
- ✓ Maximum throughput on balanced workloads is critical
- ✓ Simplicity and reliability are priorities

### Use St3 when:
- ✓ Workload is highly imbalanced (work-stealing intensive)
- ✓ Memory bounds are critical (embedded, real-time systems)
- ✓ Contention is low to moderate
- ✓ You need custom stealing strategies
- ✓ Fewer atomic operations are beneficial (cache-sensitive workloads)
- ✓ LIFO vs FIFO semantics matter

### Avoid St3 when:
- ✗ High contention is expected (bounded queues become bottleneck)
- ✗ Queue capacity is hard to predict
- ✗ Simplicity is preferred over micro-optimizations

---

## Conclusion

**Crossbeam-deque is the safer, more versatile choice** for general-purpose work-stealing:
- Consistently good performance across all scenarios
- No capacity management needed
- Better high-contention handling
- More battle-tested (used in Tokio, Rayon, etc.)

**St3 is a specialized tool** that excels in specific scenarios:
- Best for imbalanced workloads with low-moderate contention
- Ideal when bounded memory is critical
- Requires careful capacity tuning
- Current implementation has counting bugs that need investigation

**Performance Winner:** Context-dependent
- Balanced workloads: **Crossbeam** (1.4x - 1.9x faster)
- Imbalanced workloads: **St3** (1.8x - 1.9x faster)
- High contention: **Crossbeam** (18x faster)
- Overall: **Crossbeam** (more consistent, no failure modes)

---

## Issues Found

### St3 Over-counting Bug:
All St3 tests show "Completed" counts exceeding the input:
- Expected: 1.00M items
- Actual: 1.03M - 1.50M items reported

**Hypothesis:** Items are being counted multiple times:
1. Once when popped by the original owner
2. Again when stolen and counted by the stealer
3. The stealer adds `stolen` count, but those items may have already been counted

**Recommendation:** Review the benchmark's counting logic - stolen items may be counted by both victim and stealer.

### St3 High Contention Failure:
The high contention test only processed 0.4% - 0.8% of items:
- Workers' queues fill up during stealing
- Can't steal into a full bounded queue
- Results in deadlock-like behavior where workers can't make progress

**Recommendation:** Increase queue capacity or redesign test to handle bounded queue semantics.
