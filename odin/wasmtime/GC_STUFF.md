| Metric                                          | **ZGC**                                | **snmalloc**           | **mimalloc**                                    |
| ----------------------------------------------- | -------------------------------------- | ---------------------- | ----------------------------------------------- |
| **Throughput (alloc+dealloc/s)**                | \~30–50M/sec                           | \~150–300M/sec         | \~130–250M/sec                                  |
| **Total CPU Time**                              | High (GC threads + mutators)           | Low (no GC, lock-free) | Low to moderate                                 |
| **Max Pause Time (P99)**                        | <2 ms                                  | <50 μs                 | <100 μs                                         |
| **Heap Footprint (working set)**                | \~2–3× live set                        | \~1.1–1.5× live set    | \~1.2–1.6× live set                             |
| **Scalability (16→64 threads)**                 | Good, but GC thread contention appears | Excellent              | Very good (slightly more locking than snmalloc) |
| **Tail Latency (object lifetime finalization)** | Moderate (\~1–2 ms delay)              | Immediate via drop     | Immediate via drop                              |
| **Allocation Latency (P50)**                    | \~200–500 ns                           | \~2–50 ns              | \~4–70 ns                                       |
