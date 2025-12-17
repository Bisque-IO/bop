# Multishot Benchmark Documentation

This directory contains a comprehensive benchmark suite comparing `maniac::sync::multishot` against `tokio::sync::oneshot` for single-value asynchronous channels.

## Overview

The benchmark suite measures various aspects of channel performance:

1. **Single-threaded send/receive** - Basic latency when sender and receiver are on the same thread
2. **Cross-thread send** - Performance when sender is on a different thread
3. **Sender reuse** - Multishot's key feature of reusing the same receiver with new senders
4. **Drop without send** - Error handling when sender is dropped before sending
5. **Waker registration** - Overhead of registering a waker when no value is ready
6. **Memory allocation** - Allocation patterns for creating channels
7. **Contention** - Performance with multiple threads accessing different channels
8. **Sender recycling** - Cost of getting a new sender after receiving a value

## Running the Benchmarks

### Prerequisites

Make sure you have Rust and Cargo installed, and that you have the maniac crate available in your workspace.

### Running All Benchmarks

```bash
# From the maniac crate directory
cargo bench --bench multishot_bench

# Or from the workspace root
cargo bench -p maniac --bench multishot_bench
```

### Running Specific Benchmark Groups

```bash
# Only run single-threaded benchmarks
cargo bench --bench multishot_bench single_threaded_send_recv

# Only run reuse benchmarks
cargo bench --bench multishot_bench reuse

# Only run allocation benchmarks
cargo bench --bench multishot_bench allocation
```

### Benchmark Configuration

The benchmarks are configured with:
- Warm-up time: 500ms
- Measurement time: 2 seconds per benchmark
- Sample size: 100 measurements per benchmark

You can modify these settings in the `criterion_group!` macro at the bottom of `multishot_bench.rs`.

## Expected Results

### Key Advantages of Multishot

1. **Reuse Performance**: Multishot should significantly outperform tokio oneshot in scenarios where you need to repeatedly create new senders for the same receiver. The `reuse` benchmarks will show this clearly.

2. **Memory Efficiency**: The `allocation` benchmarks demonstrate that multishot requires fewer allocations in reuse scenarios since the channel structure is reused.

3. **Sender Recycling**: The `sender_recycling` benchmarks show the overhead of getting a new sender after receiving a value, which is essentially zero for multishot.

### Performance Characteristics

- **Single-threaded latency**: Both channels should perform similarly for basic send/receive operations
- **Cross-thread send**: Similar performance, though multishot may have slight overhead due to its more complex state management
- **Waker registration**: Similar overhead for both channels
- **Contention**: Performance scales similarly with thread count

## Interpreting Results

### Benchmark Groups

1. **single_threaded_send_recv**: Measures basic send/receive latency
   - `multishot`: Uses `maniac::sync::multishot::channel()`
   - `tokio_oneshot`: Uses `tokio::sync::oneshot::channel()`

2. **cross_thread_send**: Measures send from different thread
   - Important for understanding thread synchronization overhead

3. **reuse**: Measures repeated use of the same receiver
   - `multishot_reuse_N`: Multishot with N iterations using the same receiver
   - `tokio_oneshot_new_N`: Tokios oneshot creating N new channels
   - Multishot should be significantly faster for reuse scenarios

4. **drop_without_send**: Measures error handling when sender is dropped
   - Both should perform similarly

5. **waker_registration**: Measures overhead of polling without ready value
   - Both should perform similarly

6. **allocation**: Measures memory allocation patterns
   - `multishot_create_N`: Creating N new multishot channels
   - `tokio_oneshot_create_N`: Creating N new tokio oneshot channels

7. **contention**: Measures performance with multiple threads
   - Each thread uses its own channel, so similar performance expected

8. **sender_recycling**: Measures the cost of getting a new sender
   - Multishot should have minimal overhead
   - Tokio oneshot requires creating a new channel

## Customization

You can modify the benchmark parameters by editing `multishot_bench.rs`:

- Change iteration counts in loops (e.g., in `bench_reuse` and `bench_sender_recycling`)
- Add new benchmark scenarios
- Adjust measurement parameters in the `criterion_group!` macro
- Modify the test values (currently using `i32` with values like `42`, `i`)

## Technical Notes

### Dummy Waker Implementation

The benchmarks use a custom no-op waker implementation to avoid the overhead of real waker operations. This focuses the measurements on channel performance rather than waker management.

### Thread Spawning

Cross-thread benchmarks use `std::thread::spawn` to create threads. The benchmarks measure the time for the entire operation including thread spawning overhead, which is representative of real-world usage.

### Batch Size

The benchmarks use `BatchSize::SmallInput` where applicable to ensure consistent measurements. You can adjust this to `BatchSize::LargeInput` if you want to measure throughput rather than latency.

## Contributing

When adding new benchmarks:

1. Follow the existing pattern of comparing multishot vs tokio oneshot
2. Use appropriate benchmark group names
3. Ensure both implementations test the same scenario
4. Include meaningful iteration counts
5. Document the purpose of new benchmarks in comments

## Example Output

When you run the benchmarks, you'll see output like:

```
single_threaded_send_recv/multishot      time:   [X.XX ns Y.XX ns Z.XX ns]
single_threaded_send_recv/tokio_oneshot  time:   [X.XX ns Y.XX ns Z.XX ns]

reuse/multishot/10        time:   [X.XX ns Y.XX ns Z.XX ns]
reuse/tokio_oneshot/10    time:   [X.XX ns Y.XX ns Z.XX ns]
...
```

The results show mean, median, and standard deviation times. Compare the results to understand the performance characteristics of each channel type.