# A Distributed libSQL Architecture: The ASCII Blueprint

This document outlines a high-performance distributed database architecture built upon libsql's Virtual WAL. It progresses from sharding to a multi-layered system for maximum throughput and strict serializability.

## 1. Core Problem & Guiding Principle

**THE PROBLEM:** SQLite, which powers libsql, is fundamentally single-threaded for writes. WAL prevents writers from blocking readers, but not multiple writers from blocking each other.

**THE GUIDING PRINCIPLE:** To achieve high write throughput, we do not fight SQLite's single-threaded nature. We embrace it at a small scale and multiply it horizontally. Concurrency is achieved by multiplicity.

## 2. Layer 1: Sharding - The Foundation

The database is not one entity, but thousands of independent databases called "shards."

**What it is:** Each shard is a complete libsql instance with its own files (.db, .db-wal, etc.) and its own RAFT group for replication.

**Concurrency Solved:** The system achieves massive parallelism by running thousands of these single-threaded SQLite instances across a cluster.

**Local LSNs:** Each shard's RAFT group has its own "Local LSN". This guarantees ordering within the shard only.

**Parallel Checkpoints:** Instead of one huge, slow checkpoint, the system has hundreds of small, staggered checkpoints running in parallel.

### Diagram: Basic Sharded Cluster

```
  [ CLIENT REQUESTS ]
          |
          V
 +------------------+
 |  Router/Proxy    |
 +------------------+
          |
   -------+-------
  /   /   \   \   (Routes to shard owner)
 V   V     V   V
+-----------+ +-----------+ +-----------+ +-----------+
| SHARD A   | | SHARD B   | | SHARD C   | | SHARD ... |
|(RAFT Grp) | |(RAFT Grp) | |(RAFT Grp) | |(RAFT Grp) |
| Leader->F | | Leader->F | | Leader->F | | Leader->F |
+-----------+ +-----------+ +-----------+ +-----------+
```

## 3. Layer 2: Global Consensus - Transaction Conductor

Sharding provides throughput but no global order across shards. A second, higher-level consensus mechanism is introduced for ACID guarantees.

**The Global RAFT Log:** A separate, cluster-wide RAFT group. It does NOT store data payload. It stores lightweight metadata:
```
{ global_lsn: 12345, tx_id: '...', affected_shards: ['A', 'B'] }
```

**Two Levels of LSN:**

- **Local LSN (Data):** Position of a frame in a shard.
- **Global LSN (Order):** Position of a transaction in the cluster.

**The Workflow:** A transaction is not committed until its metadata is committed to this Global RAFT log. This serializes the entire cluster's history.

**The Trade-off:** This provides consistency but creates a bottleneck: the Global RAFT Leader. Every transaction must be sequenced by it.

### Diagram: Global Consensus Model

```
           [ GLOBAL RAFT LEADER ]
               (The Sequencer)
                       |
     ------------------+------------------
    /                  |                  \
   V                   V                   V
+-----------+   +-----------+   +-----------+
| SHARD A   |   | SHARD B   |   | SHARD C   |
|(RAFT Grp) |   |(RAFT Grp) |   |(RAFT Grp) |
+-----------+   +-----------+   +-----------+
```

## 4. Layer 3: Virtual Masters - Parallel Frontend

To mitigate the Global Leader bottleneck, we introduce a pool of worker roles called "Virtual Masters."

**What they are:** Processes/threads spread across the cluster, each responsible for managing a subset of shards.

**Their Job:**

1. Receive SQL, prepare WAL frames in memory.
2. Propose metadata to the single Global RAFT Leader → get LSN.
3. Replicate the actual WAL frames to the correct shard groups.

**Parallelism Achieved:** Multiple VMs do steps 1 & 3 in parallel. Only the request to the Global Leader (step 2) is serial.

**Trade-off:** Latency per-transaction increases, but overall intake throughput of the cluster rises dramatically.

### Diagram: Virtual Master Layer

```
     [ CLIENTS ]
         |
         V
 +-----------------+
 | ROUTER / PROXY  |
 +-----------------+
         |
   ------+------
  /      |      \     (Directs load)
 V       V       V
+-------+ +-------+ +---------+
| VM-1  | | VM-2  | | VM-...N |
|Owns A | |Owns B | | Owns C..|
+-------+ +-------+ +---------+
   \       |       /
    \      |      /
     \     |     /
      \    |    /   (All propose to get Global LSN)
       V   V   V
  [ GLOBAL RAFT LEADER ]
```

## 5. Layer 4: Speculative Execution - Maximizing CPU

The final layer hides latency and keeps CPUs fully saturated using SQLite's BEGIN CONCURRENT feature.

**BEGIN CONCURRENT:** Allows multiple writers to prepare their changes in memory at once. Conflicts are resolved at COMMIT time.

**The Speculative Pipeline:** The Virtual Master becomes a complex pipeline:

- **[ PREPARE (Parallel) ]** Uses a thread pool to begin many transactions at once using BEGIN CONCURRENT, preparing their in-memory changes.

- **[ DISPATCH (Pipelined) ]** Sends proposals to the Global Leader back-to-back, keeping the network path full. Does not wait for LSN response.

- **[ LOGICAL LSN ]** The VM assigns its own local ID to each in-flight transaction to track logical ordering until the official global_lsn arrives.

- **[ ORDERED COMMIT (Serial) ]** Once global_lsns start arriving, the VM commits transactions to SQLite in the order of the global_lsn, not it's local order.

**Final Trade-off:** Achieves maximum TPS by using CPU cycles to hide network/serialization latency. But it's extremely sensitive to write-write conflict rates, and state management is very complex.

## 6. Complete Transaction Flow Summary

A multi-shard transaction (`UPDATE shard_A`, `UPDATE shard_B`):

```
CLIENT   ROUTER   V.MASTER   GLOBAL LEADER   SHARD A LEADER   SHARD B LEADER
  |         |         |              |                |                |
  |--Tx---->|         |              |                |                |
  |         |-- Prep->|              |                |                |
  |         |         |-- Propose ---|-------------->|                |
  |         |         |              |  (Get LSN)     |                |
  |         |         |<-- Global-LSN|---------------|                |
  |         |         |              |  (8001)        |                |
  |         |         |-- Conflict Check ------------>|                |
  |         |         |              |  (SQLite COMMIT)               |
  |         |         |<-- OK -------|----------------|                |
  |         |         |              |                |                |
  |         |         |-- WAL Frame -|--------------->|                |
  |         |         |              |  |-- Replicate ->               |
  |         |         |              |  |<- Committed -|               |
  |         |         |              |                |                |
  |         |         |-- WAL Frame -|----------------|-------------->|
  |         |         |              |                |  |-- Replicate ->
  |         |         |              |                |  |<- Committed -|
  |         |         |<-- Shard B OK ----------------|----------------|
  |         |<-- Success --------------------------------------------|
  |<-- OK -----------------------------------------------------------|
```

## 7. Architectural Trade-offs at a Glance

| Feature       | Design Choice          | Pro                    | Con                      |
|---------------|------------------------|------------------------|--------------------------|
| Scalability   | Horizontal Sharding    | Massive throughput     | High complexity          |
| Consistency   | Global RAFT Log        | True serializability   | Global leader bottleneck |
| Throughput    | Virtual Masters        | Parallel intake        | Higher latency           |
| Latency       | Speculative Execution  | Max CPU utilization    | Sensitive to aborts      |

## Final Conclusion

This architecture is the blueprint for a high-throughput, strongly-consistent, NewSQL database. It combines SQLite's reliability with distributed systems patterns (sharding, multi-level consensus, speculative execution). The result is an incredibly powerful system, but one whose implementation complexity is commensurate with its capabilities.
