# SQLite S3 Large‑Page VFS — Log‑Structured Segments + PageMap (Design v1)

**Status:** Draft v1
**Author:** —
**Scope:** A production‑ready SQLite VFS that stores data in **append‑only segments on S3** with a **purpose‑built PageMap‑LSM**, **local‑only WAL / virtual WAL2**, **NVMe/RAM cache**, **CRC‑64/NVMe integrity**, and **stackful coroutine** execution using `generator-rs` to bridge synchronous VFS APIs to async I/O.

---

## 0. Goals & Non‑Goals

### Goals

* Support **very large** SQLite databases backed by S3 with high read throughput and scalable write ingestion.
* Keep SQLite semantics: **single writer, many readers**, WAL mode, ACID at **checkpoint** boundaries.
* Provide a **custom WAL2‑like** system built on **libsql virtual WAL**, integrated with our VFS.
* Append‑only **segments** (32–128 MiB) with immutable content suitable for CDN/S3.
* **Custom LSM PageMap** for page→location pointers (no RocksDB dependency).
* **CRC‑64/NVMe** everywhere (pages, tables, headers, footers, objects, multipart validation).
* **Async internals** using **stackful coroutines** (`generator-rs`); VFS API remains synchronous to SQLite.
* Cheap **snapshots/time‑travel** by pinning root; efficient **GC**.
* First‑class **observability** and **disaster‑recovery** story.

### Non‑Goals

* Multi‑writer concurrency (beyond a single writer lease).
* Transactional semantics across **remote readers** prior to publish.
* Full SQL layer changes — this is a pure VFS/storage swap.

---

## 1. Architecture Overview

```
            ┌──────────────────────────────── SQLite ────────────────────────────────┐
            │                                                                        │
            │  WAL2 (virtual wal) ──► Checkpoint ──► VFS.xWrite/xSync                │
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

* **Segments:** immutable append‑only blobs of `(page_no, payload)` with footer index + Bloom.
* **PageMap‑LSM:** key = `page_no (u64)` → value = `(segment_epoch, seq, offset, len, crc64, lsn)`.
* **Root:** lists PageMap files per level + metadata; updated atomically with **CAS** (If‑Match ETag).
* **Lease:** external fencing token for the single writer (Raft/DynamoDB).
* **Virtual WAL2:** implemented via **libsql virtual WAL** hooks so page updates enter our WAL2 pipeline rather than the default `-wal`.

---

## 2. Data Model & Formats

> All CRC fields are **CRC‑64/NVMe** (64‑bit) and all fixed integers are **little‑endian**.

### 2.1 Segment file (`segments/<epoch>/<seq>.seg`)

* **Target size:** 64 MiB (tunable 32–128 MiB). Multipart parts: 8–16 MiB.
* **Header (52B):** `magic=LPSE`, `version=1`, `page_size u32`, `created_lsn u64`, `n_records u64`, `reserved[2] u64`, `header_crc64 u64` (over bytes 0..43).
* **Record stream (repeated):**

  * `delta_page_no varint` (first record uses absolute page\_no),
  * `payload_len varint`,
  * `payload_crc64 u64`,
  * `flags varint` (bit0: compressed),
  * `uncompressed_len varint` (iff compressed),
  * `payload bytes` (raw or zstd‑compressed page).
* **Footer (72B):** `magic=LPFT u32`, `version u32`, `index_off u64`, `index_len u32`, `bloom_off u64`, `bloom_len u32`, `fence_min_page u64`, `fence_max_page u64`, `n_records u64`, `footer_crc64 u64`, `seg_crc64 u64` (whole file except this field).
* **Index entries (sorted by absolute page\_no):** `page_no u64, file_off u64, payload_len u32, payload_crc64 u64`.
* **Bloom filter:** fp≈1%, k=7 over page\_no; xxh3\_64(key+seed) to set bits.
* **Compression:** optional **zstd** per page; skip if gain <10%.
* **S3 I/O:** set `x-amz-checksum-algorithm: CRC64NVME`. Retain `ETag` and checksum metadata.

### 2.2 PageMap SSTable (`pagemap/<level>/<file_id>.pm`)

* **Block size:** 4 KiB aligned (configurable via `block_size_log2`).
* **Key coding:** prefix‑compressed keys with restarts every 16 entries.
* **Value encoding:** `epoch varint, seq varint, offset varint, len varint, crc64 u64, lsn varint, flags varint`.
* **Footer (48B):** `magic=LPPF u32`, `version u16`, `block_size_log2 u16`, `index_off u64`, `index_len u32`, `min_key u64`, `max_key u64`, `file_lsn_highwater u64`, `footer_crc64 u64`.
* **Top‑index entries:** `first_key u64, block_off u64, block_len u32`.
* **Optional per‑block Bloom** + `block_crc64 u64` trailer for fast reject.

### 2.3 Root object (`root`)

* **Encoding:** CBOR or Cap’n Proto.
* **Fields:** `{ schema:1, dbid:uuid, page_size:u32, lsn:u64, filesize_pages:u64, pagemap:{levels:[{level:u8,files:[str]}]}, epochs:{active:u64}, gc:{low_watermark_lsn:u64} }`.
* **Publish:** S3 PUT with `If‑Match` on previous ETag (linearizable flip if stored behind Raft/DDB KV). Readers refresh lazily.

### 2.4 WAL‑B roll (`vwal/<epoch>/<roll>.vwl`)

* **Header:** `magic=LPVW`, `version=1`, `page_size u32`, `start_lsn u64`.
* **Record:** `page_no varint, payload_len varint, crc64 u64, flags varint, bytes`.
* **Footer:** `end_lsn u64`, `footer_crc64 u64`.
* **Mirroring:** optional async upload to S3 with CRC64 validate.

---

## 3. PageMap‑LSM

### 3.1 Structures

* **Memtable:** sharded Robin‑Hood hash map (or lock‑free skiplist) keyed by `page_no`, value `PagePtr {epoch,seq,off,len,crc64,lsn}`. Target size: 16–64 MB.
* **Immu‑memtables:** pending flush queue.
* **Levels:** L0 many overlapping files (8 MiB each). L1+ non‑overlapping ranges with size ratio **T=10**.

### 3.2 Lookups

1. Probe **memtable**, then **immu** (RAM).
2. Probe **L0** (newest→oldest). Maintain tiny in‑RAM index of min/max + Bloom per file.
3. Binary search single file in **L1+** (non‑overlapping). Fetch 1 data block from S3 if cold.
4. Resolve `PagePtr` → segment slice.

### 3.3 Flush & Build

* Drain memtable sorted by key → build one L0 `.pm`: prefix‑compress, emit optional per‑block Bloom and `block_crc64`; compress blocks with zstd (optional). Upload.

### 3.4 Merge Rule

* **Last‑writer‑wins** by `lsn`. No tombstones; truncation handled by `filesize_pages` in root.

---

## 4. Write/Publish Semantics (Virtual WAL2)

We implement a WAL2‑like system on top of **libsql virtual WAL** hooks.

1. **Virtual frames:** our vWAL captures each page update: append to local **WAL‑B** (NVMe) with `crc64`, `page_no`, `lsn`, and (optionally) the local segment offset if we pre‑stage.

2. **Checkpoint:** when SQLite checkpoints WAL2:

* Read frames ≤ barrier LSN **K** from WAL‑B.
* Append those pages into the **open segment** and **upsert** memtable.
* Append to our **local recovery log** (idempotent replay).
* **Publish**: seal segments → fsync → multipart PUT; flush immu‑mems → L0 `.pm` PUT; **CAS** `root` at LSN **K**.

3. **Durability:** WAL‑B stays local until publish succeeds. On crash, replay WAL‑B rolls with `lsn > root.lsn` and re‑publish.

> If vWAL is disabled, we still support the classic flow where SQLite writes to a local `-wal` file and our VFS ingests at checkpoint; performance is lower but semantics unchanged.

---

## 5. Async Execution with Stackful Coroutines

* Each VFS call executes in a **stackful coroutine** (`generator-rs`).
* To wait on async I/O (S3 GET/PUT, CAS), we **spawn** the future on a Tokio runtime and the coroutine **yields** until a oneshot completes, resuming with the result.
* **Never yield** while holding SQLite locks or referencing SQLite‑owned buffers.

### 5.1 Yield Points

* **`xRead`**: PM block fetch (cold), segment range GET (cache miss).
* **`xWrite`**: normally local; may yield if using async fs for NVMe.
* **`xSync`**: multipart PUTs, `.pm` PUTs, root CAS.

### 5.2 Fairness

* Bound work between yields (e.g., one page / one block) to avoid starving other calls. Coalesce adjacent segment ranges and then yield on `join_all`.

---

## 6. Compaction (Record‑Aware)

### 6.1 Goals

* Keep LSM healthy (cap L0 overlaps, respect size ratios).
* **Repack live pages** into fresh segments to lower read amplification and reclaim garbage.

### 6.2 Triggers

* `L0_count > 8`, level size ratio breach `bytes(Lk) > T*bytes(Lk+1)`, segment **live\_ratio < 0.4**, or hot range showing high cold misses.

### 6.3 Algorithm

* Choose a victim L0 file; gather overlapping L0/L1.
* **K‑way merge** newest→oldest by `lsn`; emit winner for each key.
* For each winner page: read bytes (cache/S3), append to **new compacted segment**, and emit updated `PagePtr` into new `.pm` output.
* Upload new segments, then new `.pm` files; **CAS** root; enqueue old artifacts for **GC** after grace.

### 6.4 QoS

* Bandwidth/IO budgets; pause on read‑latency SLO breach; round‑robin key ranges. One compactor thread per DB initially; parallelize disjoint ranges later.

---

## 7. Caching & Prefetching

### 7.1 PageMap Cache (RAM)

* **Block cache** (\~256 MB) of decoded PM blocks; tiny top‑level indexes pinned.

### 7.2 Segment Cache

* **NVMe** blob/slice cache (50–200 GB) with TinyLFU admission and segmented LRU eviction.
* **RAM slice cache** (64–256 MB) for ultra‑hot pages.

### 7.3 Prefetch

* Detect sequential stride (∆page≈1) and prefetch next N pointers + segment ranges.
* **B‑tree aware:** on internal node read, prefetch expected children (bounded fan‑out). Coalesce adjacent range GETs per segment.

---

## 8. Locking & Leases

* **Single writer** via an external fencing token (Raft/DynamoDB conditional write). TTL \~10 s; refresh every 3–5 s.
* **VFS `xLock/xUnlock`** integrates with the lease; publishing requires an active lease. Readers do **not** take a lease and may refresh root periodically or on cache anomalies (404).

---

## 9. Crash Recovery

**Writer startup**

1. Acquire lease.
2. Load `root` (LSN **R**).
3. Replay **WAL‑B** (or classic local WAL) entries with `lsn > R` into local segments and memtable; validate `crc64`.
4. If sealed segments exist locally but were not uploaded, upload now; rebuild L0 `.pm` as needed; **CAS** a new root.

**Readers**

* Keep serving from last `root`; on missing object/404 or periodic timer, refresh root and retry.

---

## 10. Truncation, File Size & Vacuum

* `filesize_pages` is authoritative in `root`. `xTruncate(new_bytes)` updates `filesize_pages = ceil(new_bytes/page_size)` and bumps `lsn`; no PM tombstones are required — readers ignore pages `>= filesize_pages`.
* `VACUUM` triggers large rewrites; compactor handles repacking.

---

## 11. Observability & Metrics

* **I/O:** S3 GET/PUT throughput; P50/P95/P99 latencies; retries; multipart concurrency.
* **Cache:** PM block hit %, NVMe slice hit %, prefetch accuracy.
* **LSM:** L0 count, level sizes, compaction in/out bytes, write amp, pending compactions.
* **Segments:** live ratio, age histogram; GC backlog/lag.
* **Consistency:** root publish latency; reader staleness (ms since last root).
* **Errors:** CRC mismatches, CAS conflicts, lease churn, 404s.

---

## 12. Security & Encryption

* **Integrity:** CRC‑64/NVMe for page payloads and structural footers/headers; S3 object checksum set to CRC64NVME.
* **Encryption at rest:** per‑segment **DEK (AES‑GCM)** wrapped with KMS and stored in header metadata; S3 SSE‑KMS enabled.
* **In transit:** TLS; SDK verifies object checksums on GET/PUT.
* **Local cache:** plaintext or disk‑level encryption (LUKS) depending on trust model.

---

## 13. Configuration (Defaults)

* `page_size`: 4096 (or 8192 for scan‑heavy).
* `segment_target`: 64 MiB; `multipart_part`: 8–16 MiB.
* `L0_file`: \~8 MiB; `LSM_T`: 10; `PM_block_size`: 4 KiB; `restart_every`: 16.
* Cache: NVMe 100 GB; PM block cache 256 MB; RAM slice 128 MB.
* Lease: TTL 10 s; refresh 3 s.
* Compactor thresholds: `L0_count_max=8`, `live_ratio_min=0.4`.

---

## 14. Interfaces

### 14.1 VFS controls (`xFileControl`)

* `LPS3_CTL_REFRESH_ROOT` — force reader root refresh.
* `LPS3_CTL_PUBLISH` — request immediate publish (writer only).
* `LPS3_CTL_METRICS` — return counters snapshot.

### 14.2 Pragmas / Admin

* `PRAGMA lps3.snapshot('label','soft|strong');`
* `PRAGMA lps3.vwal=on|off;`
* `PRAGMA lps3.vwal_max_roll_mb=32;`
* `PRAGMA lps3.vwal_publish_interval_ms=500;`
* `PRAGMA lps3.force_publish();`

### 14.3 S3 Layout

```
root
pagemap/0/*.pm
pagemap/1/*.pm
segments/<epoch>/<seq>.seg
vwal/<epoch>/*.vwl   # optional mirror
snapshots/<id>/root
snapshots/<id>/seglist.bin
snapshots/<id>/meta.json
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
├── wal2/              # libsql virtual WAL integration, WAL-B rolls
├── root/              # serialize/deserialize, CAS publish
├── cache/             # NVMe segment cache, RAM slice cache, TinyLFU
├── wal/               # local recovery log (page ptr + crc64)
├── lease/             # Raft/DynamoDB fencing token
├── prefetch/          # B-tree aware heuristics
└── metrics/           # OTEL/Prom exporter
```

**Key structs**

```rust
pub struct PagePtr {
  pub epoch: u64,
  pub seq:   u64,
  pub off:   u64,
  pub len:   u32,
  pub crc64: u64,
  pub lsn:   u64,
}
```

---

## 16. Failure Model & Tests

### 16.1 Failure Points

* Local power loss around: (A) WAL‑B append, (B) local recovery WAL append, (C) sealed‑before‑upload, (D) after segment upload/before PM, (E) after PM/before root CAS, (F) after CAS/before WAL truncate.
* Remote: S3 part failure/timeouts; checksum mismatch; CAS ETag conflict; lease expiry.

### 16.2 Harness

* **Chaos points** (A..F) with deterministic triggers.
* **Property tests:** replay after crash == one successful publish; root monotonic; reads reflect last **committed** LSN.
* **Golden tests:** encode/decode `.seg`/`.pm` with CRC64; random range GET slicing; multipart checksum mismatch should fail.

### 16.3 WAL2 specifics

* Crash mid‑WAL‑B append → drop partial frames failing CRC64, replay rest.
* Crash after WAL‑B fsync but before publish → replay and publish idempotently at next boot.

---

## 17. GC, Snapshots, Retention

### 17.1 Snapshotting

* **Soft snapshot:** copy live `root` → `snapshots/<id>/root` (or record VersionId).
* **Strong snapshot:** barrier checkpoint → publish → copy new root.
* **Pinning:** via RefTable (segment refcounts) or S3‑only `seglist.bin` union.
* **Usage:** read‑only open via snapshot root; clone by copying root; export by iterating PageMap.

### 17.2 segset format

* Each `.pm` emits `.pm.segset`: header + sorted unique `(epoch u64, seq u64)` entries; zstd + CRC64. Snapshots may union segsets into `seglist.bin`.

### 17.3 GC policy

* Grace period 10–30 min after publish. Delete segments with refcount==0 (or absent from union) and age > grace. Keep PageMaps until no root/snapshot references them.

---

## 18. Operational Playbook

* **Cold start:** warm PM top‑indexes; optionally prefetch hot segments.
* **Hot path:** grow PM block cache to reduce S3 GETs.
* **Compaction:** keep steady; enforce budgets to avoid spikes.
* **Snapshots:** schedule nightly soft snapshots; strong snapshots before migrations.
* **Alerts:** CAS conflicts, CRC errors, L0>threshold, GC lag, rising 404s.

---

## 19. Roadmap

* **v0 (MVP):** single writer; local WAL or WAL‑B; manual compaction; basic metrics.
* **v1:** automated compaction; snapshots; GC; richer metrics & dashboards.
* **v2:** parallel compaction; smarter prefetch; PM delta files.
* **v3:** multi‑region read replicas (S3 CRR); optional streaming standby via mirrored WAL‑B.

---

## 20. Appendix — Coroutine Bridging Pattern (Pseudo‑Rust)

```rust
pub struct CoroutineScope { rt: tokio::runtime::Handle }

impl CoroutineScope {
  pub fn run_blocking<F,R>(&self, f: F) -> R { /* generator-rs resume loop */ }
  pub fn await_in_coroutine<T>(&self, fut: impl Future<Output=T> + Send + 'static) -> T { /* oneshot + yield */ }
}

fn xread(&self, off:u64, buf:&mut [u8]) -> Result<()> {
  self.scope.run_blocking(|| {
    // locate → maybe S3 GET PM block (yield)
    // read segment slice → maybe S3 GET range (yield)
    // crc64 verify; memcpy
    Ok(())
  })
}

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

* **Segment header:** `header_crc64`
* **Record payload:** `payload_crc64` per page
* **Index entries:** `payload_crc64`
* **Segment footer:** `footer_crc64`, `seg_crc64`
* **PM blocks:** optional `block_crc64`; **PM footer:** `footer_crc64`
* **WAL‑B entries (local):** include `crc64`
* **S3 uploads:** `x-amz-checksum-algorithm: CRC64NVME`

---

**End of Design v1**
