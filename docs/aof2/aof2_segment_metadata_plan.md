# Segment Metadata Consolidation Plan

## Summary
- Eliminate the standalone catalog snapshot and encode catalog/durability state directly into segment headers/footers.
- Introduce an atomic pointer to the latest sealed segment for fast recovery.
- Carry an external identifier (ext_id) through headers and records for upstream log correlation.
- Align the manifest log to consume the enriched metadata instead of maintaining a parallel snapshot.

## Implementation Tasks
1. **Header/Footer Enrichment**
   - Add an `ext_id` field to `SegmentHeader`, record headers, and sealing footers (default 0 when unused).
   - When sealing, persist `durable_bytes`, `record_count`, `last_record_timestamp`, `coordinator_watermark`, and `flush_failure_flag` in the footer.
2. **Current-Sealed Pointer**
   - Add a  pointer file holding {, , }.
   - Update the pointer atomically (TempFileGuard + rename) after each successful seal and fsync.
3. **Recovery Pipeline**
   - Update restart logic to read `current.sealed`, trust header/footer metadata, and rescan only the active tail segment.
   - Remove dependencies on `durability.json` and the old catalog snapshot.
4. **Manifest Alignment**
   - Include `ext_id` and coordinator watermark in manifest entries describing sealed segments.
   - During replay, prefer header/footer state for catalog reconstruction and use the manifest solely for Tier1/Tier2 residency.
5. **Testing**
   - Add unit tests covering footer layout and pointer updates.
   - Update crash-recovery integrations to exercise the new flow.

## Documentation Updates
- `docs/aof2/aof2_design_next.md`: describe header/footer enrichment, pointer workflow, and the revised recovery sequence.
- `docs/aof2/aof2_implementation_plan.md`: replace the RC1–RC3 snapshot tasks with the metadata/pointer/manifest work listed above.
- `docs/aof2/aof2_catalog_snapshots.md`: repurpose as the segment metadata & recovery plan summarised here.
- `docs/aof2/aof2_manifest_log.md`: note the new fields and how replay uses them.

## Open Questions
- Should `current.sealed` be a symlink (Unix-friendly) or a small binary blob (portable)?
- Do we want to persist additional counters (e.g. cumulative appended bytes) in the footer for observability?
- How do downstream systems consume `ext_id` (metrics, tracing, replication tooling) consistently?
