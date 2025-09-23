# AOF2 Validation Tests

Run the targeted validation suite from the repository root:

```
cargo test --manifest-path rust/bop-rs/Cargo.toml --test aof2_failure_tests
```

The suite covers metadata persistence retries (`metadata_retry_increments_metrics`) and Tier 2 upload/delete/fetch retry behaviour (`tier2_*_retry_metrics`).
