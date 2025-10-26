//! Multi-threaded benchmark for the standard Worker/Task executor.
//!
//! Creates a configurable number of tasks that cooperatively yield a set
//! number of times. Multiple workers run in parallel until every task
//! completes. Throughput and per-worker statistics are printed for each run.

fn main() {
    println!(
        "The legacy worker benchmark has been superseded. \
        Please run `cargo run --example fast_worker_benchmark_multithread`."
    );
}
