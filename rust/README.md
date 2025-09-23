# Legacy Rust Workspace

The Rust crates previously lived under this directory. The workspace has moved to the repository root (`Cargo.toml`) with packages relocated into `crates/`. Existing tooling or IDE configurations should be updated to point at `crates/`.

This folder now only hosts per-tool configuration (for example `.vscode/`) and the Kotlin connector project. Once those consumers are migrated, feel free to delete the empty `bop-rs` placeholder directory that remains here.