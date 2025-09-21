# Platform Overview

BOP provides a collection of modular, reusable components for building high-performance, durable applications in Rust. These components include:

## Raft - Consensus and Replication (NuRaft core)

A distributed consensus algorithm designed for fault-tolerance and high availability. Key features include:

- Leader election and log replication
- Strong consistency guarantees
- Support for dynamic membership changes
- Integration with BOP's tracing and metrics systems
- Pluggable storage backends for log and state machine persistence

## AOF - Tiered Storage and Async Integration

An advanced append-only file (AOF) implementation with built-in support for multi-tiered storage, asynchronous operations, and seamless integration with Tokio. Key features include:

- Synchronous append with backpressure handling
- Automatic data tiering across local disk and S3
- Asynchronous read and hydration of cold data
- Robust crash recovery and replication support
- Structured logging and metrics for observability

## Segmented Raft Log (built on AOF)

- Each Segment maps to an individual AOF instance and an individual FSM for isolation and efficiency
- Segments can be compacted and snapshotted independently to optimize storage and performance
- Isolation of log AOFs for efficient compaction and snapshotting
- Seamless integration with Raft for log replication and consistency

## SQLite - S3

A storage engine that combines SQLite's reliability with S3's scalability, enabling efficient storage and retrieval of large datasets. Key features include:

- Transparent page caching and eviction
- Write-ahead logging with S3 offload
- Support for concurrent readers and writers
- Configurable durability and consistency guarantees
- Integration with BOP's tracing and metrics systems

## SQLite - Encryption

An encryption layer for SQLite databases, providing data-at-rest security with minimal performance overhead. Key features include:

- AES-256 encryption for database files and WAL
- Transparent encryption and decryption of pages
- Configurable key management and rotation
- Compatibility with existing SQLite features and extensions

## SQLite - Replication - Raft WAL

A replication mechanism for SQLite databases using Raft for consensus and log replication. Key features include:

- Reliable log replication with strong consistency guarantees
- Integration with BOP's Raft implementation for fault-tolerance
- Support for dynamic membership changes and leader election
- Seamless integration with SQLite's WAL mode for durability

## SQLite - Snapshots

A snapshotting mechanism for SQLite databases, enabling point-in-time recovery and efficient backups. Key features include:

- Consistent snapshots with minimal performance impact
- Integration with S3 for scalable storage
- Support for incremental and differential backups
- Configurable retention policies and lifecycle management

## SQLite - MySQL Wire Protocol

MySQL-compatible protocol layer for SQLite, enabling seamless integration with existing MySQL clients and tools. Key features include:

- Support for common MySQL commands and data types
- Compatibility with popular MySQL clients and libraries
- Integration with BOP's tracing and metrics systems
- Configurable authentication and access control

## SQLite - Postgres Wire Protocol

Postgres-compatible protocol layer for SQLite, enabling seamless integration with existing Postgres clients and tools. Key features include:

- Support for common Postgres commands and data types
- Compatibility with popular Postgres clients and libraries
- Integration with BOP's tracing and metrics systems
- Configurable authentication and access control

---

## App Lifecycle and Process Management

Manage lifecycle of external services and applications with BOP's built-in process management capabilities. Key features include:

- Start, stop, and monitor external processes
- Automatic restarts and failure handling
- Integration with BOP's logging and metrics systems

### Agentic - Process Management

Automatic Agentic AI context management for external processes for enhanced observability and control. Key features include:
- Automatic context propagation for tracing and logging
- Context-aware process management and monitoring
- Integration with BOP's tracing and metrics systems

### Agentic - Debugging

Enhanced debugging capabilities for external processes, including automatic log collection and context-aware diagnostics. Key features include:

- Automatic log collection and aggregation
- Context-aware error reporting and diagnostics
- Integration with BOP's tracing and metrics systems

BOP's process management capabilities are designed to simplify the deployment and management of external services and applications. Key features include:

- Unified management interface for all external processes
- Support for service discovery and load balancing
- Integration with BOP's tracing and metrics systems

BOP applications can be run using the `bop` command-line tool, which provides a simple interface for starting and managing BOP-based services. The `bop` tool supports configuration via command-line arguments and configuration files, allowing for flexible deployment options.