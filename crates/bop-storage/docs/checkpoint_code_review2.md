# Checkpoint Code Review (Round 2)

## Blocking Findings

1. **Delta files emit an all-zero content hash**
   - Location: crates/bop-storage/src/checkpoint/delta.rs:234
   - The delta builder currently hardcodes the CRC64 footer to [0, 0, 0, 0]. Every staged delta therefore receives the same content hash, which means content-addressed file names collide (delta-…-0000000000000000.blob), the executor's deduplication and the manifest's recorded hash both become meaningless, and remote retries cannot detect previously uploaded data.
   - **Fix:** hash the actual page bytes (we already depend on crc64fast-nvme) and persist the real 256-bit hash in both the footer and the return value. Add a regression test that fails if the hash is zero for non-empty input.

2. **Manifest::resolve_page_location is incorrect and O(n) per read**
   - Location: crates/bop-storage/src/manifest/api.rs:515
   - Every page read walks the entire chunk_catalog, clones every record, and infers the owning chunk by cumulatively adding effective_page_count sorted by chunk_id. This is both wildly expensive (hot path becomes linear in the number of chunks) and semantically wrong because chunk IDs are not guaranteed to be contiguous or ordered by page range. The computed base_start_page can point at the wrong chunk and mis-layer deltas, and the code ignores residency so it happily returns evicted data.
   - **Fix:** add manifest indexes that map (db_id, page_no) to the owning chunk using the real metadata (store explicit start/end page ranges, or query by range) and rely on cursor lookups instead of materialising the whole table. Filter out chunks that are not remotely readable. The lookup must be logarithmic and exact or we risk corrupt reads.

3. **Newly dirty pages without a chunk assignment are dropped**
   - Location: crates/bop-storage/src/checkpoint/planner.rs:447
   - plan_libsql_with_pages only enqueues entries whose DirtyPage::chunk_id is Some(_). Any page with chunk_id == None—how we represent freshly allocated pages—never makes it into the plan, so checkpoints silently miss new data.
   - **Fix:** collect the None bucket and emit a CreateBase action (or consult the manifest for an assignment) before planning deltas. Add a unit test that feeds brand-new pages and asserts they appear in the plan.

4. **Local hot-cache uses ChunkId alone as the key**
   - Location: crates/bop-storage/src/local_store.rs:101
   - LocalStore stores metadata in HashMap<ChunkId, CachedChunk> and tracks LRU order with just the chunk ID. Chunk IDs are scoped per database, so tenant A can evict tenant B's files if they share the same identifier. The janitor will also delete another tenant's scratch file.
   - **Fix:** scope everything by (DbId, ChunkId) (and mirror that in the on-disk layout if we want per-tenant isolation). Update eviction, pinning, and tests accordingly.

5. **Leases are never released on upload/publish failure**
   - Location: crates/bop-storage/src/checkpoint/orchestrator.rs:314
   - We register a lease before staging and only release after publish_manifest_updates. Any error in the upload loop or during manifest publication exits early, so those leases remain held until the TTL expires, blocking truncation and retries. Scratch directories and staged blobs also linger.
   - **Fix:** wrap staging/upload/publish in a guard that releases leases and cleans scratch state on every exit path. Tests should inject an upload error and assert that leases are cleared.

## Additional Observations

- The hot-path implementation of resolve_page_location copies manifest records into heap-allocated vectors. Even after fixing correctness, eliminate the extra cloning.
- The CRC helper in CheckpointExecutor already produces a real hash—once the delta footer is fixed we can reuse that helper to keep hashing logic consistent between bases and deltas.

Let me know if you want deep dives or suggested patches for any of the above.
