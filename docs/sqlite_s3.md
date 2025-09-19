# SQLite S3 Large‑Page VFS — Log‑Structured Segments + PageMap (Design v1)

> **Status:** Draft v1  
> **Author:** —  
> **Scope:** A production‑ready SQLite VFS that stores data in **append‑only segments on S3** with a **purpose‑built PageMap‑LSM**, **local‑only WAL**, **NVMe/RAM cache**, **CRC‑64/NVMe** integrity, and **stackful coroutine** execution using `generator-rs` to bridge synchronous VFS APIs to async I/O.

---

## 0. Goals & Non‑Goals

### Goals
- Support *very large* SQLite databases backed by S3 with high read throughput and scalable write ingestion.
- Keep SQLite semantics: **single writer, many readers**, WAL mode, ACID at checkpoint boundaries.
- **Local‑only WAL**: publishing happens on checkpoint; WAL is never uploaded.
- **Append‑only segments** (32–128 MiB) with immutable content suitable for CDN/S3.
- **Custom LSM PageMap** for page→location pointers (no RocksDB dependency).
- **CRC‑64/NVMe** everywhere (pages, tables, headers, footers, objects, multipart validation).
- **Async internals** using **stackful coroutines (`generator-rs`)**; VFS API remains synchronous to SQLite.
- Cheap **snapshots/time‑travel** by pinning root; efficient **GC**.
- First‑class observability and disaster‑recovery story.

### Non‑Goals
- Multi‑writer concurrency (beyond a single writer lease).
- Transactional semantics across **remote** readers prior to publish.
- Full SQL layer changes — this is a pure VFS/storage swap.

---

## 1. Architecture Overview

```
            ┌──────────────────────────────── SQLite ────────────────────────────────┐
            │                                                                        │
            │  WAL(local) ──► Checkpoint ──► VFS.xWrite/xSync (sync façade)          │
            │                                                                        │
            └────────────────────────────────────────────────────────────────────────┘
                                        │
                         (stackful coroutine via generator-rs)
                                        │ yields
                                        ▼
        ┌───────────────┐     ┌───────────────────┐      ┌──────────────────────┐
        │ PageMap‑LSM   │◄───►│ NVMe / RAM Caches │◄────►│  S3 (segments/pm)    │
        │  (mem+SST)    │     │  (PM blocks, seg) │      │  + Root CAS pointer  │
        └───────────────┘     └───────────────────┘      └──────────────────────┘
                ▲                         ▲                         ▲
                │ (pointers)              │ (slices/blocks)         │ (upload/CAS)
                └───────────── Compactor (record‑aware) ────────────┘
```

- **Segments**: immutable append‑only blobs of `(page_no, payload)` with footer index + Bloom.
- **PageMap‑LSM**: key = `page_no (u64)` → value = `(segment_epoch, seq, offset, len, crc64, lsn)`.
- **Root**: lists PageMap files per level + metadata; updated atomically with CAS (If‑Match ETag).
- **Lease**: external fencing token for the single writer (Raft/DynamoDB).

---

## 2. Data Model & Formats

### 2.1 Common
- **Endianness:** little‑endian for fixed fields.
- **Varint:** unsigned LEB128 for packed integers.
- **Integrity:** **CRC‑64/NVMe** (`crc64nvme`) for all payloads and structural footers/headers.

### 2.2 Segment file (`segments/<epoch>/<seq>.seg`)
- **Target size:** 64 MiB (tunable 32–128 MiB). Multipart parts 8–16 MiB.
- **Header (52B):** magic `LPSE`, version, `page_size`, `created_lsn`, `n_records`, `header_crc64`.
- **Records (repeated):** `delta_page_no varint`, `payload_len varint`, `payload_crc64 u64`, `flags varint` (bit0 compressed), optional `uncompressed_len varint`, then bytes.
- **Footer (72B):** magic `LPFT`, `index_off/len`, `bloom_off/len`, `fence_min/max_page`, `n_records`, `footer_crc64`, `seg_crc64` (whole file, except field itself).
- **Index entries (sorted):** `page_no u64, file_off u64, payload_len u32, payload_crc64 u64`.
- **Bloom:** k=7, ~1% FP over page_no.
- **Compression:** optional zstd per page; skip if benefit < 10%.
- **S3 checksum:** set `x-amz-checksum-algorithm: CRC64NVME` (SDK validates on CompleteMultipart).

### 2.3 PageMap SSTable (`pagemap/<level>/<file_id>.pm`)
- **Blocks:** aligned to `2^block_size_log2` (default 4 KiB). Prefix‑compressed keys with restarts every 16 entries.
- **Value encoding:** `epoch varint, seq varint, offset varint, len varint, crc64 u64, lsn varint, flags varint`.
- **Footer:** magic `LPPF`, `index_off/len`, `min_key`, `max_key`, `file_lsn_highwater`, `footer_crc64`.
- **Optional per‑block Bloom** + `block_crc64` for quick reject.

### 2.4 Root object (`root`)
- CBOR/Cap’nProto blob with:
  - `schema, dbid, page_size, lsn, filesize_pages`
  - `pagemap.levels = [ {level, files:[...]} ]`
  - `epochs.active`, `gc.low_watermark_lsn`
- Updated via **CAS** (If‑Match on previous ETag). Readers poll lazily.

---

## 3. PageMap‑LSM

- **Memtable:** lock‑free/sharded RH‑hash; holds newest pointers.
- **Flush:** memtable → L0 `.pm` (8 MiB default). Multiple overlapping L0 files allowed.
- **Levels L1+:** non‑overlapping ranges, size ratio T=10.
- **Lookup order:** mem → immu‑mem → L0 newest→oldest → L1+ binary search. Each miss may fetch one PM block from S3 and cache it.
- **Merge rule:** last‑writer‑wins by `lsn`. No tombstones (truncate handled via `filesize_pages`).

---

## 4. Write/Publish Semantics (WAL Local‑Only)

1. **SQLite in WAL mode.** WAL and SHM are local files managed by underlying OS VFS.
2. **`xWrite(off,n)`** (4 KiB aligned): append pages to **local SegmentWriter** (NVMe), upsert **memtable**, append to **local WAL** (our recovery log). Compute & store `crc64` for each page payload.
3. **Checkpoint → `xSync()`**:
   - Seal local segments; fsync.
   - Upload sealed segments (multipart PUT; server‑validated CRC64NVME).
   - Flush immu‑memtables → build L0 `.pm`; upload.
   - Build new **root** (`lsn` to checkpoint LSN; update `filesize_pages`).
   - **CAS** publish root. On success, truncate local WAL to `root.lsn`.

**Durability:** Once `xSync()` returns OK, either the new root is live or replay after crash produces same state.

---

## 5. Async Execution with Stackful Coroutines

- Each VFS call runs in a **stackful coroutine** (`generator-rs`).
- When remote I/O is needed, the coroutine **spawns** a Tokio future and **yields** until a oneshot delivers the result. No OS thread blocking.
- Do not yield while holding SQLite locks; per‑file `Mutex` serializes ops that must be exclusive (e.g., `xSync`).

### Yield Points
- `xRead`: PM block S3 GET (if cold), Segment range GET (if cache miss).
- `xWrite`: typically none (local disk); may yield if you choose async fs.
- `xSync`: multipart PUTs, `.pm` PUTs, root CAS.

---

## 6. Compaction (Record‑Aware)

### Goals
- Maintain LSM health (reduce L0 fanout).
- Repack live pages into fresh segments to reduce read amplification and reclaim garbage.

### Triggers
- L0 file count > 8; level size ratio breach; segment live‑ratio < 0.4; read‑heat in a range.

### Algorithm (leveled)
1. Pick newest L0 file; union with overlapping L0/L1.
2. K‑way merge entries newest→oldest by `lsn`; emit winners only.
3. For each winner page: read its bytes (cache/S3), append to **new compacted segment**; write new pointer into output `.pm` files.
4. Upload new segments, then new PageMap files; CAS root; enqueue old artifacts for GC (after grace).

### QoS
- Bandwidth/I/O budgets; pause if read miss latency exceeds SLO; round‑robin ranges.

---

## 7. Caching & Prefetching

- **PageMap block cache (RAM):** ~256 MB default; stores decoded PM blocks and tiny top‑level indexes.
- **Segment cache (NVMe):** large (50–200 GB) of slices/segments; TinyLFU admission + segmented LRU.
- **RAM slice cache:** 64–256 MB for very hot pages.
- **Prefetch:** sequential stride detection; B‑tree‑aware child prefetch for internal nodes; coalesce adjacent range GETs.

---

## 8. Locking & Leases

- **Single writer:** short‑TTL fencing token (Raft/DynamoDB conditional write). Refresh every 3–5 s; TTL ~10 s.
- **Readers:** no lease; follow root pointer. On 404, refresh root and retry.

---

## 9. Crash Recovery

**Writer startup:**
- Acquire lease.
- Load root; replay **local WAL** entries with `lsn > root.lsn`.
- If sealed local segments referenced by WAL aren’t in S3 yet, upload now; otherwise discard those WAL entries.

**Readers:** continue at last known root; refresh lazily.

---

## 10. Truncation, File Size & Vacuum

- `filesize_pages` stored in root. `xTruncate(new_size)` updates root with new count (bumped `lsn`). Pages beyond end are logically deleted (no PM tombstones required).
- `VACUUM` benefits compaction; can trigger large rewrite which the compactor handles.

---

## 11. Observability & Metrics

Collect via OpenTelemetry (export Prometheus):
- **I/O:** S3 GET/PUT throughput, P99 latencies, retries, multipart concurrency.
- **Cache:** hit ratios (PM block, segment, RAM slice), prefetch accuracy.
- **LSM:** L0 count, level sizes, compaction bytes in/out, write amplification.
- **Segments:** live ratio, age histograms, GC lag.
- **Consistency:** root publish latency, reader staleness (ms since last root).
- **Errors:** CRC mismatches, CAS conflicts, lease churn.

---

## 12. Security & Encryption

- **Envelope encryption:** per‑segment DEK (AES‑GCM) wrapped by KMS. Store wrapped key in segment header metadata. PM files optionally encrypted, or left plaintext if values are non‑sensitive.
- **At rest:** S3 SSE‑KMS; NVMe cache either plaintext (trusted host) or LUKS/FS encryption.
- **In transit:** TLS; SDK verifies.

---

## 13. Configuration (Defaults)

- `page_size`: 4096 (or 8192 for scan‑heavy workloads).
- `segment_target`: 64 MiB; `multipart_part`: 8–16 MiB.
- `L0_file`: ~8 MiB; `LSM_T`: 10; `PM_block_size`: 4 KiB; `restart_every`: 16.
- **CRC:** CRC‑64/NVMe everywhere.
- **Caches:** NVMe 100 GB; PM block cache 256 MB; RAM slice 128 MB.
- **Lease:** TTL 10 s; refresh 3 s.

---

## 14. Interfaces

### 14.1 VFS controls (`xFileControl`)
- `LPS3_CTL_REFRESH_ROOT` — force reader root refresh.
- `LPS3_CTL_PUBLISH` — request immediate publish (writer only).
- `LPS3_CTL_METRICS` — fetch counters snapshot.

### 14.2 S3 layout
```
root
pagemap/0/*.pm
pagemap/1/*.pm
segments/<epoch>/<seq>.seg
snapshots/<ts>
```

---

## 15. Rust Module Sketch

```
crate lps_vfs
├── vfs/               # sqlite3_vfs FFI + façade → coroutine calls
├── coro/              # generator-rs bridge, await_in_coroutine, scope
├── s3io/              # aws-sdk-s3 wrapper, CRC64NVME on PUT/GET
├── segment/           # writer (local), reader, index/bloom, zstd
├── pagemap/
│   ├── memtable.rs    # sharded RH-hash + immu queues
│   ├── sstable.rs     # builder/reader, block cache, CRC64
│   └── lsm.rs         # levels, flush, lookup, compaction planner
├── root/              # serialize/deserialize, CAS publish
├── cache/             # NVMe segment cache, RAM slice cache, TinyLFU
├── wal/               # local recovery log (page ptr + crc64)
├── lease/             # Raft/DynamoDB fencing token
├── prefetch/          # B-tree aware heuristics
└── metrics/           # OTEL/Prom exporter
```

---

## 16. Failure Model & Tests

### 16.1 Failure Points
- Local: power loss around (A) append, (B) WAL record, (C) sealed‑before‑upload, (D) after seg upload/before PM, (E) after PM/before CAS, (F) after CAS/before WAL truncate.
- Remote: S3 part failure/timeouts; CAS ETag mismatch; checksum mismatch.
- Lease: expiry mid‑publish; split brain attempt.

### 16.2 Harness
- **Chaos points** identified above (A..F) with deterministic triggers.
- **Property tests:** replay after crash == single successful publish; root monotonic; page reads reflect last committed `lsn`.
- **Golden tests:** encode/decode `.seg`/`.pm` with CRC64 validation; random range GET slicing.

---

## 17. GC, Snapshots, Retention

- Maintain **segment refcounts** by scanning PM outputs (or incrementally during compaction). Keep a grace period (10–30 min) beyond `gc.low_watermark_lsn`.
- **Snapshots:** copy `root` to `snapshots/<ts>`; pin all referenced files until snapshot removed.
- Lifecycle rules in S3 can help purge old objects after refcount→0 and grace.

---

## 18. Operational Playbook

- **Cold start:** warm PM indexes (HEAD `.pm` files; prefetch top‑level index) and pin hot segments.
- **Hot path tuning:** increase PM block cache → fewer S3 GETs.
- **Compaction cadence:** steady background; avoid spikes by enforcing bandwidth budgets.
- **Alerting:** CAS failures, rising 404s on segment GETs, CRC mismatches, L0>threshold, GC lag > SLA.

---

## 19. Roadmap

**v0 (MVP):**
- Single DB, single writer, many readers. Local‑only WAL. Manual compaction trigger. Basic metrics.

**v1:**
- Automated compaction, QoS budgets, snapshots, GC. Reader prefetch heuristics.

**v2:**
- Parallel compactions across disjoint ranges. Tiered caching strategies. Incremental PM deltas.

**v3:**
- Multi‑region read replicas (S3 CRR). Optional streaming WAL for hot standby (still not authoritative for SQLite state).

---

## 20. Appendix — Coroutine Bridging Pattern (Pseudo‑Rust)

```rust
pub struct CoroutineScope { rt: tokio::runtime::Handle }
impl CoroutineScope {
  pub fn run_blocking<F,R>(&self, f: F) -> R { /* generator-rs resume loop */ }
  pub fn await_in_coroutine<T>(&self, fut: impl Future<Output=T> + Send + 'static) -> T { /* oneshot + yield */ }
}

// xRead skeleton
fn xread(&self, off:u64, buf:&mut [u8]) -> Result<()> {
  self.scope.run_blocking(|| {
    // locate → maybe S3 GET PM block (yield)
    // read segment slice → maybe S3 GET range (yield)
    // crc64 verify; memcpy
    Ok(())
  })
}

// xSync skeleton
fn xsync_publish(&self) -> Result<()> {
  self.scope.run_blocking(|| {
    // seal+fsync local → upload segments (yield on multipart)
    // build+upload L0 PM (yield) → CAS root (yield) → finalize local
    Ok(())
  })
}
```

---

## 21. Appendix — Field Reference (CRC‑64/NVMe)

- **Segment header**: `header_crc64`  
- **Record payload**: `payload_crc64` per page  
- **Index entries**: `payload_crc64`  
- **Segment footer**: `footer_crc64`, `seg_crc64`  
- **PM blocks**: optional `block_crc64`; **PM footer**: `footer_crc64`  
- **WAL entries (local)**: include `crc64`  
- **S3 uploads**: `x-amz-checksum-algorithm: CRC64NVME`

---

**End of Design v1**

