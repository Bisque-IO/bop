//! Signal Find Nearest Benchmark
//!
//! Benchmarks different implementations of find_nearest functions for signal bit selection.

use bop_mpmc::signal::{
    find_nearest_by_distance, find_nearest_by_distance_branchless, find_nearest_by_distance0,
    find_nearest_set_bit,
};
use std::time::Instant;

/// Format a large number with thousand separators for readability
fn humanize_number(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }

    let mut result = String::new();
    let mut count = 0;

    while n > 0 {
        if count > 0 && count % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, char::from_digit((n % 10) as u32, 10).unwrap());
        n /= 10;
        count += 1;
    }

    result
}

/// Test patterns representing different signal bitfield scenarios
fn get_test_patterns() -> Vec<(&'static str, u64)> {
    vec![
        ("Empty", 0x0000_0000_0000_0000),
        ("Single bit (LSB)", 0x0000_0000_0000_0001),
        ("Single bit (MSB)", 0x8000_0000_0000_0000),
        ("Single bit (middle)", 0x0000_0000_8000_0000),
        ("Two bits (edges)", 0x8000_0000_0000_0001),
        ("Two bits (adjacent)", 0x0000_0000_0000_0003),
        ("Sparse pattern", 0x0100_0200_0400_0800),
        ("Dense pattern", 0x00FF_00FF_00FF_00FF),
        ("Alternating", 0xAAAA_AAAA_AAAA_AAAA),
        ("All set", 0xFFFF_FFFF_FFFF_FFFF),
        ("Lower half", 0x0000_0000_FFFF_FFFF),
        ("Upper half", 0xFFFF_FFFF_0000_0000),
        ("Random 1", 0x123E_4567_89AB_CDEF),
        ("Random 2", 0xFEDC_BA98_7654_3210),
    ]
}

fn benchmark_find_nearest_set_bit() {
    println!("\n=== Benchmarking find_nearest_set_bit ===");

    let patterns = get_test_patterns();
    let iterations = 10_000_000u64;

    for (name, pattern) in &patterns {
        let start = Instant::now();
        let mut result = 0u64;

        for _ in 0..iterations {
            for start_idx in 0..64 {
                result ^= find_nearest_set_bit(*pattern, start_idx);
            }
        }

        let duration = start.elapsed();
        let total_ops = iterations * 64;
        let ns_per_op = duration.as_nanos() as f64 / total_ops as f64;
        let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

        println!(
            "{:<20} {:>8.2} ns/op    {:>15} ops/sec    (result: {})",
            name,
            ns_per_op,
            humanize_number(ops_per_sec),
            result
        );
    }
}

fn benchmark_find_nearest_by_distance() {
    println!("\n=== Benchmarking find_nearest_by_distance ===");

    let patterns = get_test_patterns();
    let iterations = 10_000_000u64;

    for (name, pattern) in &patterns {
        let start = Instant::now();
        let mut result = 0u64;

        for _ in 0..iterations {
            for start_idx in 0..64 {
                result ^= find_nearest_by_distance(*pattern, start_idx);
            }
        }

        let duration = start.elapsed();
        let total_ops = iterations * 64;
        let ns_per_op = duration.as_nanos() as f64 / total_ops as f64;
        let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

        println!(
            "{:<20} {:>8.2} ns/op    {:>15} ops/sec    (result: {})",
            name,
            ns_per_op,
            humanize_number(ops_per_sec),
            result
        );
    }
}

fn benchmark_find_nearest_by_distance0() {
    println!("\n=== Benchmarking find_nearest_by_distance0 ===");

    let patterns = get_test_patterns();
    let iterations = 10_000_000u64;

    for (name, pattern) in &patterns {
        let start = Instant::now();
        let mut result = 0u64;

        for _ in 0..iterations {
            for start_idx in 0..64 {
                result ^= find_nearest_by_distance0(*pattern, start_idx);
            }
        }

        let duration = start.elapsed();
        let total_ops = iterations * 64;
        let ns_per_op = duration.as_nanos() as f64 / total_ops as f64;
        let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

        println!(
            "{:<20} {:>8.2} ns/op    {:>15} ops/sec    (result: {})",
            name,
            ns_per_op,
            humanize_number(ops_per_sec),
            result
        );
    }
}

fn benchmark_find_nearest_by_distance_branchless() {
    println!("\n=== Benchmarking find_nearest_by_distance_branchless ===");

    let patterns = get_test_patterns();
    let iterations = 10_000_000u64;

    for (name, pattern) in &patterns {
        let start = Instant::now();
        let mut result = 0u64;

        for _ in 0..iterations {
            for start_idx in 0..64 {
                result ^= find_nearest_by_distance_branchless(*pattern, start_idx);
            }
        }

        let duration = start.elapsed();
        let total_ops = iterations * 64;
        let ns_per_op = duration.as_nanos() as f64 / total_ops as f64;
        let ops_per_sec = (1_000_000_000.0 / ns_per_op) as u64;

        println!(
            "{:<20} {:>8.2} ns/op    {:>15} ops/sec    (result: {})",
            name,
            ns_per_op,
            humanize_number(ops_per_sec),
            result
        );
    }
}

fn verify_implementations_match() {
    println!("\n=== Verifying implementations produce identical results ===");

    let patterns = get_test_patterns();
    let mut all_match = true;

    for (name, pattern) in &patterns {
        for start_idx in 0..64 {
            let r1 = find_nearest_set_bit(*pattern, start_idx);
            let r2 = find_nearest_by_distance(*pattern, start_idx);
            let r3 = find_nearest_by_distance0(*pattern, start_idx);
            let r4 = find_nearest_by_distance_branchless(*pattern, start_idx);

            if r1 != r2 || r1 != r3 || r1 != r4 {
                println!(
                    "MISMATCH: {} pattern={:#018x} idx={} -> set_bit={} dist={} dist0={} branchless={}",
                    name, pattern, start_idx, r1, r2, r3, r4
                );
                all_match = false;
            }
        }
    }

    if all_match {
        println!("✓ All implementations match across all test patterns");
    } else {
        println!("✗ Some implementations produced different results");
    }
}

fn main() {
    println!("{}", "=".repeat(80));
    println!("Signal Find Nearest Benchmark Suite");
    println!("{}", "=".repeat(80));

    // First verify all implementations match
    verify_implementations_match();

    // Run benchmarks
    benchmark_find_nearest_set_bit();
    benchmark_find_nearest_by_distance();
    benchmark_find_nearest_by_distance0();
    benchmark_find_nearest_by_distance_branchless();

    println!("\n{}", "=".repeat(80));
    println!("Benchmark complete!");
    println!("{}", "=".repeat(80));
}
