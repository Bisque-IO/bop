# bop-sys Native Build Notes

This crate wraps the NuRaft-based C++ shim stored under lib/. The native artefacts are built with Forge (an xmake wrapper). Cargo does not invoke xmake yet, so build the C++ layer before running cargo build -p bop-sys.

## Building libbop

    ./forge b bop

Forge detects the host platform/architecture, runs xmake f / xmake, and drops libbop.a beneath crates/bop-sys/libs/<platform>/<arch>/. That is now the canonical location used by build.rs.

Cross-compiling example:

    ./forge b bop --platform linux --arch arm64

The command above emits crates/bop-sys/libs/linux/arm64/libbop.a when run on the host.

## Using the artefacts

When cargo build -p bop-sys runs, build.rs scans those directories, picks the freshest archive, and links it automatically. If you need to vendor a specific build, copy or symlink it into the corresponding crates/bop-sys/libs/<platform>/<arch>/ folder.

## Cleaning

    ./forge c bop

forge c removes the generated archives under crates/bop-sys/libs, so the next build starts clean.
