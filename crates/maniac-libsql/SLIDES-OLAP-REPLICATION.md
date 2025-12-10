# OLTP to OLAP Replication

## maniac-libsql → maniac-ducklake

### Real-Time Analytics Pipeline

---

# Slide 1: Vision

## Unified OLTP + OLAP Architecture

**The Problem**: Applications need both:
- **OLTP** (SQLite) - Fast transactional writes, low latency
- **OLAP** (DuckDB) - Complex analytics, columnar performance

**Traditional Solutions**:
- Batch ETL jobs (hours of latency)
- Dual-write (consistency nightmares)
- CDC pipelines (complex infrastructure)

**Our Solution**: Native Raft changeset replication from maniac-libsql to maniac-ducklake

---

# Slide 2: Architecture Overview

## Real-Time OLTP to OLAP Pipeline

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         maniac-libsql (OLTP)                            │
│                                                                         │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────┐     │
│   │   SQLite    │───►│ Virtual WAL │───►│   Raft Log              │     │
│   │   Writes    │    │   Frames    │    │   (Changesets)          │     │
│   └─────────────┘    └─────────────┘    └───────────┬─────────────┘     │
└─────────────────────────────────────────────────────┼───────────────────┘
                                                      │
                              Raft Replication        │
                              (Real-Time Stream)      ▼
┌─────────────────────────────────────────────────────┼───────────────────┐
│                         maniac-ducklake (OLAP)      │                   │
│                                                     ▼                   │
│   ┌─────────────────────────┐    ┌─────────────────────────────────┐    │
│   │   Changeset Applier     │───►│   DuckDB + Ducklake             │    │
│   │   (Transform & Ingest)  │    │   (Columnar Analytics)          │    │
│   └─────────────────────────┘    └─────────────────────────────────┘    │
│                                              │                          │
│                                              ▼                          │
│                                  ┌─────────────────────────┐            │
│                                  │   Iceberg/Parquet       │            │
│                                  │   (S3 Data Lake)        │            │
│                                  └─────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────┘
```

---

# Slide 3: Why This Architecture?

## Benefits of Native Raft Replication

| Approach | Latency | Consistency | Complexity |
|----------|---------|-------------|------------|
| Batch ETL | Hours | Eventual | Medium |
| CDC (Debezium) | Seconds | Eventual | High |
| Dual-Write | Zero | Broken | Low |
| **Raft Changesets** | **Milliseconds** | **Strong** | **Low** |

### Key Advantages

- **Sub-second latency** - Analytics on live data
- **Strong consistency** - Same Raft log = same order
- **Zero infrastructure** - No Kafka, no CDC connectors
- **Transactional guarantees** - Atomic changeset application

---

# Slide 4: maniac-libsql Changesets

## The Replication Unit

### Raft Log Entry Types

```rust
pub enum RaftEntryPayload {
    // WAL segment persisted to S3
    WalMetadata {
        branch_id: u32,
        start_frame: u64,
        end_frame: u64,
        s3_key: String,
    },

    // SQLite session changeset (THE KEY!)
    Changeset {
        branch_id: u32,
        changeset_data: Vec<u8>,  // SQLite changeset format
        schema_version: u64,
    },

    // Checkpoint marker
    Checkpoint { branch_id: u32, frame: u64 },

    // Branch operations
    BranchCreated { branch_id: u32, parent: Option<u32> },
}
```

### Changeset Contents

- **INSERT**: Full row data
- **UPDATE**: Primary key + changed columns (old & new values)
- **DELETE**: Primary key + old row data

---

# Slide 5: SQLite Changeset Format

## Industry-Standard CDC Format

```
┌─────────────────────────────────────────────────────────────┐
│                    SQLite Changeset                         │
├─────────────────────────────────────────────────────────────┤
│  Table: "orders"                                            │
│  ├── INSERT: {id: 1001, customer: "Alice", total: 99.99}    │
│  ├── UPDATE: {id: 1000, total: 149.99 → 199.99}             │
│  └── DELETE: {id: 999, ...old_values...}                    │
├─────────────────────────────────────────────────────────────┤
│  Table: "order_items"                                       │
│  ├── INSERT: {order_id: 1001, product: "Widget", qty: 2}    │
│  └── INSERT: {order_id: 1001, product: "Gadget", qty: 1}    │
└─────────────────────────────────────────────────────────────┘
```

### Why Changesets?

- **Atomic** - All changes from a transaction
- **Ordered** - Maintains causality
- **Compact** - Only changed data, not full rows
- **Invertible** - Can generate undo changesets

---

# Slide 6: maniac-ducklake Architecture

## Embedded DuckDB + Ducklake

```
┌─────────────────────────────────────────────────────────────┐
│                    maniac-ducklake                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌───────────────────┐     ┌───────────────────────────┐    │
│  │ Changeset Decoder │────►│ Schema Mapper             │    │
│  │ (SQLite format)   │     │ (OLTP → OLAP transforms)  │    │
│  └───────────────────┘     └─────────────┬─────────────┘    │
│                                          │                  │
│                                          ▼                  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                    DuckDB Engine                      │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │  │
│  │  │ Vectorized  │  │  Columnar   │  │   Parallel  │    │  │
│  │  │ Execution   │  │  Storage    │  │   Scans     │    │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘    │  │
│  └───────────────────────────────────────────────────────┘  │
│                            │                                │
│                            ▼                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                   Ducklake Layer                      │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │  │
│  │  │   Iceberg   │  │  Parquet    │  │  S3/Object  │    │  │
│  │  │   Catalog   │  │  Files      │  │  Storage    │    │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘    │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

# Slide 7: What is Ducklake?

## DuckDB's Native Data Lake Integration

### Core Capabilities

| Feature | Description |
|---------|-------------|
| **Iceberg Tables** | Open table format with ACID transactions |
| **Parquet Storage** | Columnar format, high compression |
| **S3 Native** | Direct cloud object storage access |
| **Schema Evolution** | Add/rename/drop columns without rewrite |
| **Time Travel** | Query historical versions |
| **Partition Pruning** | Skip irrelevant data automatically |

### Why Ducklake for OLAP?

- **Serverless** - No cluster to manage
- **Scalable** - Petabyte-scale on object storage
- **Fast** - Vectorized columnar execution
- **Open** - Iceberg format, no vendor lock-in

---

# Slide 8: Replication Flow

## From OLTP Commit to OLAP Query

```
Timeline:
─────────────────────────────────────────────────────────────────────►

T+0ms     T+1ms        T+5ms           T+10ms         T+50ms
  │         │            │               │              │
  ▼         ▼            ▼               ▼              ▼
┌─────┐  ┌──────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐
│OLTP │  │ Raft │  │ Changeset │  │  DuckDB   │  │   OLAP    │
│Write│─►│ Log  │─►│  Decoded  │─►│  Ingest   │─►│  Query    │
└─────┘  └──────┘  └───────────┘  └───────────┘  └───────────┘

Total Latency: ~50ms from write to queryable
```

### Latency Breakdown

| Stage | Time | Description |
|-------|------|-------------|
| OLTP Commit | 1ms | SQLite WAL + fsync |
| Raft Replicate | 4ms | Consensus + network |
| Changeset Apply | 5ms | Decode + transform |
| DuckDB Ingest | 40ms | Columnar write + index |
| **Total** | **~50ms** | **Real-time analytics** |

---

# Slide 9: Schema Transformation

## OLTP Schema → OLAP Schema

### Automatic Transformations

```sql
-- OLTP (SQLite - Normalized)
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    status TEXT,
    created_at TEXT
);

CREATE TABLE order_items (
    id INTEGER PRIMARY KEY,
    order_id INTEGER,
    product_id INTEGER,
    quantity INTEGER,
    price REAL
);
```

```sql
-- OLAP (DuckDB - Denormalized + Optimized)
CREATE TABLE orders_facts (
    order_id BIGINT,
    customer_id BIGINT,
    status VARCHAR,
    created_at TIMESTAMP,

    -- Denormalized from order_items
    total_items INTEGER,
    total_amount DECIMAL(10,2),

    -- Partitioning
    order_date DATE
) PARTITION BY (order_date);
```

---

# Slide 10: Transformation Rules

## Configurable OLTP → OLAP Mappings

```rust
pub struct ReplicationConfig {
    pub tables: Vec<TableMapping>,
    pub transforms: Vec<Transform>,
}

pub struct TableMapping {
    pub source_table: String,      // OLTP table
    pub target_table: String,      // OLAP table
    pub column_mappings: Vec<ColumnMapping>,
    pub partition_by: Option<String>,
}

pub enum Transform {
    // Denormalize related tables
    Join {
        source: String,
        target: String,
        on: String,
        columns: Vec<String>,
    },

    // Compute aggregates
    Aggregate {
        group_by: Vec<String>,
        aggregations: Vec<Aggregation>,
    },

    // Type conversions
    Cast {
        column: String,
        to_type: String,
    },

    // Add computed columns
    Computed {
        name: String,
        expression: String,
    },
}
```

---

# Slide 11: Ingestion Strategies

## Micro-Batch vs Streaming

### Micro-Batch Mode (Default)

```
Changesets:  [C1] [C2] [C3] [C4] [C5] [C6] [C7] [C8]
                   │              │              │
Batches:           └──────┬──────┘      ┌───────┘
                          ▼             ▼
DuckDB:              [Batch 1]    [Batch 2]    ...
                     (100ms)      (100ms)
```

**Benefits**: Higher throughput, better compression

### Streaming Mode

```
Changesets:  [C1] [C2] [C3] [C4] [C5] [C6] [C7] [C8]
               │    │    │    │    │    │    │    │
DuckDB:        ▼    ▼    ▼    ▼    ▼    ▼    ▼    ▼
            Immediate ingestion per changeset
```

**Benefits**: Lower latency (~10ms)

### Configuration

```rust
pub enum IngestionMode {
    MicroBatch {
        batch_size: usize,        // Max changesets per batch
        batch_timeout: Duration,   // Max wait time
    },
    Streaming {
        buffer_size: usize,       // In-memory buffer
    },
}
```

---

# Slide 12: Consistency Guarantees

## Strong Consistency via Raft

### The Guarantee

```
If changeset C₁ commits before C₂ in OLTP,
then C₁ is visible before C₂ in OLAP.

Formally: OLTP.commit_order = OLAP.visibility_order
```

### How It Works

```
┌─────────────────────────────────────────────────────────────┐
│                    Raft Consensus                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Leader         Follower 1       Follower 2 (OLAP)         │
│     │                │                  │                   │
│     │  AppendEntries │                  │                   │
│     │───────────────►│                  │                   │
│     │                │  AppendEntries   │                   │
│     │───────────────────────────────────►                   │
│     │                │                  │                   │
│     │◄───────────────│  ACK             │                   │
│     │                │◄─────────────────│  ACK              │
│     │                │                  │                   │
│  Commit              │               Apply to               │
│                      │               DuckDB                 │
└─────────────────────────────────────────────────────────────┘
```

### Consistency Levels

| Level | Latency | Guarantee |
|-------|---------|-----------|
| **Read-Your-Writes** | ~50ms | See your own changes |
| **Monotonic Reads** | ~50ms | No going back in time |
| **Linearizable** | ~100ms | Real-time ordering |

---

# Slide 13: Branch Replication

## OLTP Branches → OLAP Views

### Git-Like Branching for Analytics

```
OLTP (maniac-libsql)              OLAP (maniac-ducklake)
─────────────────────             ──────────────────────

main (v100)                       main_analytics
  │                                 │
  ├── feature-a (v80)             feature_a_analytics
  │     │                           │
  │     └── fix (v85)             fix_analytics
  │
  └── feature-b (v95)             feature_b_analytics
```

### Use Cases

- **A/B Testing**: Analyze different feature branches
- **What-If Analysis**: Query hypothetical data changes
- **Audit**: Compare branch states at any point
- **Staging**: Test analytics on pre-production data

---

# Slide 14: Time Travel Queries

## Query Any Point in Time

### Powered by Ducklake + Iceberg

```sql
-- Query current state
SELECT * FROM orders_facts;

-- Query as of 1 hour ago
SELECT * FROM orders_facts
FOR SYSTEM_TIME AS OF TIMESTAMP '2024-01-15 10:00:00';

-- Query as of specific Raft index
SELECT * FROM orders_facts
FOR VERSION AS OF 12345;

-- Compare two points in time
SELECT
    current.total_revenue - past.total_revenue as revenue_growth
FROM (
    SELECT SUM(amount) as total_revenue
    FROM orders_facts
) current,
(
    SELECT SUM(amount) as total_revenue
    FROM orders_facts
    FOR SYSTEM_TIME AS OF TIMESTAMP '2024-01-01'
) past;
```

---

# Slide 15: Schema Evolution

## Zero-Downtime OLAP Schema Changes

### Evolution Flow

```
┌──────────────────────────────────────────────────────────────┐
│                   Schema Evolution Pipeline                   │
└──────────────────────────────────────────────────────────────┘

Step 1: Create Shadow Table
─────────────────────────────
OLAP:  [orders_facts]  →  [orders_facts_v2] (new schema)

Step 2: Dual-Write (Automatic)
─────────────────────────────
Changesets → [orders_facts]
           → [orders_facts_v2]  (transformed)

Step 3: Backfill Historical Data
─────────────────────────────
[orders_facts] ──backfill──► [orders_facts_v2]

Step 4: Atomic Cutover
─────────────────────────────
[orders_facts_v2] becomes [orders_facts]
```

### Supported Changes

- Add columns (with defaults)
- Rename columns
- Change partitioning
- Add/modify indexes
- Denormalization changes

---

# Slide 16: Query Patterns

## Optimized for Analytics

### Real-Time Dashboards

```sql
-- Live order metrics (sub-second latency)
SELECT
    date_trunc('minute', created_at) as minute,
    COUNT(*) as orders,
    SUM(total_amount) as revenue
FROM orders_facts
WHERE created_at > now() - INTERVAL '1 hour'
GROUP BY 1
ORDER BY 1 DESC;
```

### Historical Analysis

```sql
-- Year-over-year comparison (scan terabytes in seconds)
SELECT
    date_trunc('month', order_date) as month,
    SUM(total_amount) as revenue,
    LAG(SUM(total_amount)) OVER (ORDER BY month) as prev_year
FROM orders_facts
WHERE order_date >= '2023-01-01'
GROUP BY 1;
```

### Funnel Analysis

```sql
-- User conversion funnel
WITH funnel AS (
    SELECT
        user_id,
        MAX(CASE WHEN event = 'view' THEN 1 END) as viewed,
        MAX(CASE WHEN event = 'cart' THEN 1 END) as carted,
        MAX(CASE WHEN event = 'purchase' THEN 1 END) as purchased
    FROM events_facts
    GROUP BY user_id
)
SELECT
    COUNT(*) as total_users,
    SUM(viewed) as viewed,
    SUM(carted) as carted,
    SUM(purchased) as purchased
FROM funnel;
```

---

# Slide 17: Performance Comparison

## OLTP vs OLAP Query Performance

| Query Type | SQLite (OLTP) | DuckDB (OLAP) | Speedup |
|------------|---------------|---------------|---------|
| Point lookup (PK) | **0.1ms** | 5ms | 0.02x |
| Range scan (1K rows) | 10ms | **2ms** | 5x |
| Aggregation (1M rows) | 500ms | **15ms** | 33x |
| Join (10M × 1M) | 30s | **200ms** | 150x |
| Window function | 5s | **50ms** | 100x |

### Why DuckDB is Faster for Analytics

- **Columnar storage** - Only read needed columns
- **Vectorized execution** - SIMD operations on batches
- **Parallel scans** - Multi-core utilization
- **Compression** - Less I/O, more cache hits
- **Late materialization** - Defer row construction

---

# Slide 18: Storage Architecture

## Tiered Storage Strategy

```
┌─────────────────────────────────────────────────────────────┐
│                    Hot Tier (DuckDB Memory)                 │
│   Recent data, frequently queried                           │
│   Retention: Last 24 hours                                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Warm Tier (Local Parquet)                │
│   Recent historical, moderate query frequency               │
│   Retention: Last 30 days                                   │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Cold Tier (S3 Iceberg)                   │
│   Historical archive, infrequent queries                    │
│   Retention: Unlimited                                      │
└─────────────────────────────────────────────────────────────┘
```

### Automatic Tiering

```rust
pub struct TieringConfig {
    pub hot_retention: Duration,      // e.g., 24 hours
    pub warm_retention: Duration,     // e.g., 30 days
    pub cold_storage: String,         // e.g., "s3://bucket/lake"
    pub compaction_interval: Duration,
}
```

---

# Slide 19: Monitoring & Observability

## Replication Health Dashboard

### Key Metrics

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `replication_lag_ms` | Time behind OLTP | > 1000ms |
| `changesets_pending` | Queue depth | > 10000 |
| `apply_rate` | Changesets/sec | < 1000 |
| `transform_errors` | Failed transforms | > 0 |
| `storage_bytes` | OLAP storage size | > quota |

### Replication Status

```rust
pub struct ReplicationStatus {
    pub oltp_commit_index: u64,
    pub olap_applied_index: u64,
    pub lag_changesets: u64,
    pub lag_duration: Duration,
    pub state: ReplicationState,
    pub last_error: Option<String>,
}

pub enum ReplicationState {
    Streaming,      // Normal operation
    CatchingUp,     // Behind, processing backlog
    Paused,         // Manually paused
    Error,          // Needs intervention
}
```

---

# Slide 20: Use Cases

## Where This Architecture Shines

### 1. Real-Time Analytics Dashboard

```
User Action → SQLite Write → Raft → DuckDB → Dashboard Update
                    └────────── < 100ms ──────────┘
```

### 2. Operational Reporting

- Live inventory levels
- Real-time sales metrics
- Active user counts
- System health aggregates

### 3. Machine Learning Features

- Feature stores with fresh data
- Training data pipelines
- Real-time inference features

### 4. Audit & Compliance

- Complete change history
- Point-in-time reconstruction
- Regulatory reporting

### 5. Multi-Tenant Analytics

- Per-tenant OLAP branches
- Isolated query execution
- Tenant-specific transformations

---

# Slide 21: Deployment Topology

## Production Architecture

```
┌───────────────────────────────────────────────────────────────────┐
│                         Raft Cluster                              │
│                                                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐            │
│  │   Node 1    │    │   Node 2    │    │   Node 3    │            │
│  │   (Leader)  │◄──►│  (Follower) │◄──►│  (Follower) │            │
│  │             │    │             │    │             │            │
│  │ libsql OLTP │    │ libsql OLTP │    │ libsql OLTP │            │
│  └─────────────┘    └─────────────┘    └─────────────┘            │
│         │                  │                  │                   │
│         └──────────────────┼──────────────────┘                   │
│                            │                                      │
│                    Raft Log Stream                                │
│                            │                                      │
│                            ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │                    OLAP Replica Pool                        │  │
│  │                                                             │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │  │
│  │  │  ducklake   │  │  ducklake   │  │  ducklake   │          │  │
│  │  │  Replica 1  │  │  Replica 2  │  │  Replica 3  │          │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘          │  │
│  │         │                │                │                 │  │
│  │         └────────────────┼────────────────┘                 │  │
│  │                          │                                  │  │
│  │                          ▼                                  │  │
│  │              ┌─────────────────────┐                        │  │
│  │              │   S3 Data Lake      │                        │  │
│  │              │   (Iceberg/Parquet) │                        │  │
│  │              └─────────────────────┘                        │  │
│  └─────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

---

# Slide 22: API Overview

## maniac-ducklake Interface

```rust
/// Create OLAP replica from OLTP source
pub struct DucklakeReplica {
    config: ReplicaConfig,
    duckdb: DuckDB,
    applier: ChangesetApplier,
}

impl DucklakeReplica {
    /// Connect to Raft cluster and start replication
    pub async fn connect(
        raft_endpoint: &str,
        config: ReplicaConfig,
    ) -> Result<Self>;

    /// Get current replication status
    pub fn status(&self) -> ReplicationStatus;

    /// Execute OLAP query
    pub async fn query(&self, sql: &str) -> Result<QueryResult>;

    /// Query at specific version
    pub async fn query_at_version(
        &self,
        sql: &str,
        version: u64
    ) -> Result<QueryResult>;

    /// Pause/resume replication
    pub fn pause(&self);
    pub fn resume(&self);

    /// Apply schema transformation
    pub async fn evolve_schema(
        &self,
        evolution: SchemaEvolution,
    ) -> Result<()>;
}
```

---

# Slide 23: Configuration Example

## Complete Setup

```rust
let config = ReplicaConfig {
    // Source OLTP cluster
    raft_endpoints: vec![
        "node1:5000".into(),
        "node2:5000".into(),
        "node3:5000".into(),
    ],

    // Ingestion settings
    ingestion: IngestionMode::MicroBatch {
        batch_size: 1000,
        batch_timeout: Duration::from_millis(100),
    },

    // Table mappings
    tables: vec![
        TableMapping {
            source_table: "orders".into(),
            target_table: "orders_facts".into(),
            partition_by: Some("order_date".into()),
            column_mappings: vec![
                ColumnMapping::direct("id", "order_id"),
                ColumnMapping::cast("created_at", "TIMESTAMP"),
            ],
        },
    ],

    // Storage tiering
    tiering: TieringConfig {
        hot_retention: Duration::from_hours(24),
        warm_retention: Duration::from_days(30),
        cold_storage: "s3://analytics/lake".into(),
        compaction_interval: Duration::from_hours(1),
    },

    // DuckDB settings
    duckdb: DuckDBConfig {
        memory_limit: "8GB".into(),
        threads: 8,
        temp_directory: "/tmp/duckdb".into(),
    },
};

let replica = DucklakeReplica::connect(&config).await?;

// Start querying immediately
let result = replica.query("
    SELECT date_trunc('hour', created_at) as hour,
           COUNT(*) as orders
    FROM orders_facts
    GROUP BY 1
").await?;
```

---

# Slide 24: Summary

## maniac-libsql → maniac-ducklake

### The Complete Picture

| Layer | Technology | Purpose |
|-------|------------|---------|
| **OLTP** | maniac-libsql | Fast transactional writes |
| **Replication** | Raft Changesets | Consistent, ordered stream |
| **Transform** | Schema Mapper | OLTP → OLAP adaptation |
| **OLAP Engine** | DuckDB | Vectorized analytics |
| **Storage** | Ducklake/Iceberg | Scalable data lake |

### Key Benefits

- **Real-time**: Sub-100ms replication latency
- **Consistent**: Strong ordering guarantees via Raft
- **Scalable**: Petabyte-scale on object storage
- **Simple**: No Kafka, no CDC connectors, no ETL jobs
- **Flexible**: Configurable transforms and tiering

---

# Slide 25: Roadmap

## Implementation Phases

### Phase 1: Core Replication
- [ ] Changeset decoder for DuckDB
- [ ] Basic table replication
- [ ] Replication monitoring

### Phase 2: Transformations
- [ ] Schema mapping DSL
- [ ] Denormalization transforms
- [ ] Computed columns

### Phase 3: Ducklake Integration
- [ ] Iceberg catalog support
- [ ] Parquet file management
- [ ] S3 storage backend

### Phase 4: Production Features
- [ ] Schema evolution
- [ ] Storage tiering
- [ ] Multi-tenant isolation

### Phase 5: Advanced Analytics
- [ ] Time travel queries
- [ ] Branch-based analytics
- [ ] Incremental materialized views

---

# Slide 26: Thank You

## Questions?

**maniac-libsql → maniac-ducklake**

*Real-time OLTP to OLAP replication*
*powered by Raft changesets*

---

Part of the **bisque-io/bop** ecosystem

*Unified transactional and analytical processing*
