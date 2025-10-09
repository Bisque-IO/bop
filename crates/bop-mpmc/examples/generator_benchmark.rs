//! Generator-rs Context Switching Benchmark
//!
//! Measures raw stackful coroutine context switching throughput using generator-rs.
//! This benchmarks the fundamental performance of user-space context switching
//! with zero CPU work - pure switching overhead.

use generator::{Gn, co_yield_with, done};
use std::time::{Duration, Instant};

// Custom stack size for generators (2MB to avoid stack overflow)
// const STACK_SIZE: usize = 4096*1;
const STACK_SIZE: usize = 2 * 1024 * 1024;

/// Format a large number with thousand separators for readability
fn humanize_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn print_results(label: &str, total_ops: usize, duration: Duration) {
    let total_ns = duration.as_nanos();
    let ns_per_op = total_ns as f64 / total_ops as f64;
    let ops_per_sec = 1_000_000_000.0 / ns_per_op;

    println!(
        "{:<45} {:>12} switches   {:>10.2} ms    {:>10.2} ns/switch   {:>15} switches/sec",
        label,
        humanize_number(total_ops as u64),
        duration.as_secs_f64() * 1000.0,
        ns_per_op,
        humanize_number(ops_per_sec as u64)
    );
}

// ============================================================================
// Single Generator Benchmarks
// ============================================================================

/// Benchmark a single generator with many yields
/// This measures the pure context switch overhead without task management
fn benchmark_single_generator(num_yields: usize) -> Duration {
    let start = Instant::now();

    let g = Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
        for _ in 0..num_yields {
            s.yield_(0);
        }
        // done!();
        // Ok(())
        // generator::done!();
        // done!();
        0
    });

    let mut count = 0;
    // Consume all yields
    for i in g {
        // Each iteration causes a context switch
        // println!("{}", i);

        count += 1;
        if count >= num_yields + 1 {
            break;
        }
    }

    println!("done");

    start.elapsed()
}

/// Benchmark a single generator that yields values (not just unit)
/// Tests if passing data across context switches affects performance
fn benchmark_single_generator_with_value(num_yields: usize) -> Duration {
    let start = Instant::now();

    let g = Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
        for i in 0..num_yields {
            s.yield_(i);
        }
        0
    });

    let mut count = 0usize;
    for value in g {
        count = count.wrapping_add(value);
    }

    // Use count to prevent optimization
    std::hint::black_box(count);

    start.elapsed()
}

/// Benchmark generator with larger stack size
/// Tests if stack size affects context switch performance
fn benchmark_generator_large_stack(num_yields: usize, stack_size: usize) -> Duration {
    let start = Instant::now();

    let g = Gn::new_scoped_opt(stack_size, move |mut s| {
        for _ in 0..num_yields {
            s.yield_(());
        }
        ()
    });

    for _ in g {
        // Each iteration causes a context switch
    }

    start.elapsed()
}

// ============================================================================
// Multiple Generators Benchmark
// ============================================================================

/// Benchmark multiple generators running sequentially
/// Shows overhead of creating many generators vs single long-running one
fn benchmark_multiple_generators(num_generators: usize, yields_per_generator: usize) -> Duration {
    let start = Instant::now();

    for _ in 0..num_generators {
        let yields = yields_per_generator;
        let g = Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
            for _ in 0..yields {
                s.yield_(());
            }
            ()
        });

        for _ in g {
            // Context switch
        }
    }

    start.elapsed()
}

// ============================================================================
// Interleaved Execution Benchmark
// ============================================================================

/// Benchmark interleaved execution of multiple generators
/// This simulates manual cooperative multitasking
fn benchmark_interleaved_generators(
    num_generators: usize,
    yields_per_generator: usize,
) -> Duration {
    let start = Instant::now();

    // Create all generators upfront
    let mut generators: Vec<_> = (0..num_generators)
        .map(|_| {
            let yields = yields_per_generator;
            Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
                for _ in 0..yields {
                    s.yield_(());
                }
                ()
            })
        })
        .collect();

    // Round-robin through generators
    let mut active = num_generators;
    while active > 0 {
        let mut i = 0;
        while i < generators.len() {
            if let Some(g) = generators.get_mut(i) {
                match g.next() {
                    Some(_) => {
                        i += 1;
                    }
                    None => {
                        generators.remove(i);
                        active -= 1;
                    }
                }
            } else {
                break;
            }
        }
    }

    start.elapsed()
}

// ============================================================================
// Nested Generator Benchmark
// ============================================================================

/// Benchmark nested generators (generator calling generator)
/// Tests context switch overhead with deeper call stacks
fn benchmark_nested_generators(depth: usize, yields_per_level: usize) -> Duration {
    fn create_nested(depth: usize, yields: usize) -> impl Iterator<Item = usize> {
        if depth == 0 {
            return Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
                for i in 0..yields {
                    s.yield_(i);
                }
                0
            });
        }

        Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
            let inner = create_nested(depth - 1, yields);
            for value in inner {
                s.yield_(value);
            }
            0
        })
    }

    let start = Instant::now();

    let g = create_nested(depth, yields_per_level);
    let mut count = 0usize;
    for value in g {
        count = count.wrapping_add(value);
    }

    std::hint::black_box(count);

    start.elapsed()
}

// ============================================================================
// Work Simulation Benchmarks
// ============================================================================

/// Benchmark with minimal CPU work between yields
fn benchmark_with_minimal_work(num_yields: usize, work_per_yield: u64) -> Duration {
    let start = Instant::now();

    let g = Gn::new_scoped_opt(STACK_SIZE, move |mut s| {
        for i in 0..num_yields {
            // Minimal work
            let mut sum = i as u64;
            for j in 0..work_per_yield {
                sum = sum.wrapping_add(j);
            }
            s.yield_(sum);
        }
        0
    });

    let mut total = 0u64;
    for value in g {
        total = total.wrapping_add(value);
    }

    std::hint::black_box(total);

    start.elapsed()
}

// ============================================================================
// Main Benchmark Runner
// ============================================================================

fn main() {
    println!("{}", "=".repeat(100));
    println!("Generator-rs Context Switching Benchmark Suite");
    println!("{}", "=".repeat(100));

    println!("\nThis benchmark measures raw stackful coroutine context switching throughput.");
    println!("Each 'switch' represents a yield/resume cycle (2 context switches total).");

    // ========================================================================
    // Single Generator - Pure Context Switch Overhead
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("SINGLE GENERATOR - PURE CONTEXT SWITCH OVERHEAD");
    println!("{}", "=".repeat(100));

    println!("\nBasic yields (unit type, no data transfer):");

    let duration = benchmark_single_generator(10);
    print_results("Warmup", 10, duration);

    let duration = benchmark_single_generator(10_000);
    print_results("10K yields", 10_000, duration);

    let duration = benchmark_single_generator(100_000);
    print_results("100K yields", 100_000, duration);

    let duration = benchmark_single_generator(1_000_000);
    print_results("1M yields", 1_000_000, duration);

    let duration = benchmark_single_generator(10_000_000);
    print_results("10M yields", 10_000_000, duration);

    println!("\nYields with value transfer (usize data):");

    let duration = benchmark_single_generator_with_value(100_000);
    print_results("100K yields with value", 100_000, duration);

    let duration = benchmark_single_generator_with_value(1_000_000);
    print_results("1M yields with value", 1_000_000, duration);

    let duration = benchmark_single_generator_with_value(10_000_000);
    print_results("10M yields with value", 10_000_000, duration);

    // // ========================================================================
    // // Stack Size Impact
    // // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("STACK SIZE IMPACT ON CONTEXT SWITCHING");
    println!("{}", "=".repeat(100));

    let yields = 1_000_000;

    let duration = benchmark_generator_large_stack(yields, 4 * 1024);
    print_results("4KB stack", yields, duration);

    let duration = benchmark_generator_large_stack(yields, 16 * 1024);
    print_results("16KB stack", yields, duration);

    let duration = benchmark_generator_large_stack(yields, 64 * 1024);
    print_results("64KB stack", yields, duration);

    let duration = benchmark_generator_large_stack(yields, 256 * 1024);
    print_results("256KB stack", yields, duration);

    let duration = benchmark_generator_large_stack(yields, 1024 * 1024);
    print_results("1MB stack", yields, duration);

    // ========================================================================
    // Multiple Generators
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("MULTIPLE GENERATORS (Sequential Execution)");
    println!("{}", "=".repeat(100));

    println!("\nMany generators, few yields each:");

    let duration = benchmark_multiple_generators(1_000, 100);
    print_results("1K generators × 100 yields", 100_000, duration);

    let duration = benchmark_multiple_generators(10_000, 100);
    print_results("10K generators × 100 yields", 1_000_000, duration);

    println!("\nFew generators, many yields each:");

    let duration = benchmark_multiple_generators(10, 100_000);
    print_results("10 generators × 100K yields", 1_000_000, duration);

    let duration = benchmark_multiple_generators(100, 10_000);
    print_results("100 generators × 10K yields", 1_000_000, duration);

    // ========================================================================
    // Interleaved Execution
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("INTERLEAVED GENERATORS (Manual Round-Robin)");
    println!("{}", "=".repeat(100));

    println!("\nRound-robin scheduling overhead:");

    let duration = benchmark_interleaved_generators(10, 1_000);
    print_results("10 generators × 1K yields", 10_000, duration);

    let duration = benchmark_interleaved_generators(100, 1_000);
    print_results("100 generators × 1K yields", 100_000, duration);

    let duration = benchmark_interleaved_generators(1_000, 100);
    print_results("1K generators × 100 yields", 100_000, duration);

    // ========================================================================
    // Nested Generators
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("NESTED GENERATORS (Deep Call Stack)");
    println!("{}", "=".repeat(100));

    println!("\nNested context switching overhead:");

    let duration = benchmark_nested_generators(2, 10_000);
    print_results("Depth 2, 10K yields/level", 10_000, duration);

    let duration = benchmark_nested_generators(3, 1_000);
    print_results("Depth 3, 1K yields/level", 1_000, duration);

    let duration = benchmark_nested_generators(5, 1_000);
    print_results("Depth 5, 1K yields/level", 1_000, duration);

    let duration = benchmark_nested_generators(10, 100);
    print_results("Depth 10, 100 yields/level", 100, duration);

    // ========================================================================
    // Context Switch with Work
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("CONTEXT SWITCHING WITH MINIMAL CPU WORK");
    println!("{}", "=".repeat(100));

    println!("\nContext switch overhead vs CPU work:");

    let duration = benchmark_with_minimal_work(100_000, 0);
    print_results("100K yields, 0 CPU work", 100_000, duration);

    let duration = benchmark_with_minimal_work(100_000, 10);
    print_results("100K yields, 10 CPU work", 100_000, duration);

    let duration = benchmark_with_minimal_work(100_000, 100);
    print_results("100K yields, 100 CPU work", 100_000, duration);

    let duration = benchmark_with_minimal_work(100_000, 1_000);
    print_results("100K yields, 1K CPU work", 100_000, duration);

    // ========================================================================
    // Summary
    // ========================================================================

    println!("\n{}", "=".repeat(100));
    println!("BENCHMARK SUMMARY");
    println!("{}", "=".repeat(100));
    println!("\nKey Findings:");
    println!("  • Generator-rs provides stackful coroutines with user-space context switching");
    println!(
        "  • Each yield/resume cycle involves 2 context switches (yield to caller, resume to generator)"
    );
    println!("  • Stack size has minimal impact on context switch performance");
    println!("  • Single long-running generators are more efficient than many short ones");
    println!("  • Nested generators add overhead proportional to nesting depth");
    println!("  • Data transfer during yields has negligible overhead");
    println!("\nComparison Notes:");
    println!("  • Compare to Tokio yield benchmarks for async/await vs stackful overhead");
    println!("  • Generator-rs uses real stack frames vs async state machines");
    println!("  • Context switches preserve full call stack and local variables");
    println!("  • Useful for implementing custom schedulers and coroutine systems");
    println!("{}", "=".repeat(100));
}
