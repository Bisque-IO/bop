# Sanakirja S3 Segmented Unlimited Forking Architecture

## Executive Summary

This document presents a comprehensive architecture design for extending Sanakirja with S3-backed segmented storage and unlimited forking capabilities. The design achieves:

- **Zero-cost forking**: O(1) fork creation through metadata-only copying
- **Scalable storage**: 64MB segments optimized for S3 performance
- **Lazy materialization**: Only materialize data when accessed
- **Cloud-native fault tolerance**: S3 persistence with local caching

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Core Components](#core-components)
3. [Segment Management](#segment-management)
4. [Unlimited Forking](#unlimited-forking)
5. **S3 Integration**](#s3-integration)
6. [Garbage Collection](#garbage-collection)
7. [Performance Optimizations](#performance-optimizations)
8. **Configuration and Tuning**(#configuration-and-tuning)
9. **Implementation Roadmap](#implementation-roadmap)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Database  │  │   Readers   │  │    Fork Management     │  │
│  │ Operations  │  │             │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                  Sanakirja Core Engine                          │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              Unlimited Fork Manager                        ││
│  │  • Lazy Materialization  • Fork Tree Management          ││
│  │  • Conflict Resolution    • Batch Operations            ││
│  └─────────────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              S3 Segment Manager                           ││
│  │  • 64MB Segments        • Multi-tier Caching           ││
│  │  • Fingerprinting       • Background Materialization   ││
│  └─────────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────────┤
│                     Storage Layer                               │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────────────────────┐  │
│  │   Memory │  │ Local Cache  │  │      AWS S3 Storage        │  │
│  │          │  │   (SSD/ HDD) │  │  • 64MB Segments          │  │
│  │          │  │              │  │  • ZSTD Compression       │  │
│  │          │  │              │  │  • Lifecycle Policies     │  │
│  └──────────┘  └──────────────┘  └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Principles

1. **Segment Isolation**: 64MB segments provide optimal S3 performance and isolation
2. **Lazy Evaluation**: Data materialization deferred until actual access
3. **Fingerprint-Based Tracking**: Efficient state detection for forks
4. **Multi-Stage Caching**: Memory → Local Disk → S3 hierarchy
5. **Unlimited Branching**: Fork creation is metadata-only operation

---

## Core Components

### 1. Page Offset System

```rust
/// Unique identifier for a page across all storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageOffset {
    segment_id: SegmentId,
    page_index: u16, // 0..16383 for 64MB segments
}

impl PageOffset {
    fn to_s3_key(&self) -> String {
        format!("segments/{:06d}_{:04x}.bin", self.segment_id.0, self.page_index)
    }
    
    fn to_local_cache_path(&self) -> PathBuf {
        PathBuf::from(format!("cache/s{:06d}_p{:04x}.bin", 
                                self.segment_id.0, self.page_index))
    }
}

/// 64MB segment for S3 optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId(u64);

pub const SEGMENT_SIZE: usize = 64 * 1024 * 1024; // 64MB
pub const PAGE_SIZE: usize = 4096;
pub const PAGES_PER_SEGMENT: usize = SEGMENT_SIZE / PAGE_SIZE; // 16,384 pages
```

### 2. Segment Management

```rust
/// Individual segment with page allocation tracking
pub struct Segment {
    id: SegmentId,
    data: MmapMut,
    page_bitmap: BitSet,                    // Track allocated pages
    generation_tracker: ConcurrentHashMap<u64, BitSet>, // Pages per generation
    version: AtomicU64,                     // Segment version for S3
    last_access: Atomic<Instant>,
    utilization: Atomic<f32>,               // Memory utilization percentage
}

/// Global segment orchestrator
pub struct S3SegmentManager {
    active_segments: ConcurrentHashMap<SegmentId, Segment>,
    page_metadata: ConcurrentHashMap<PageOffset, PageMetadata>,
    
    // S3 integration
    s3_client: AwsS3Client,
    s3_bucket: String,
    compression_codec: CompressionCodec,
    
    // Configuration
    config: SegmentManagerConfig,
    
    // Performance metrics
    metrics: SegmentMetrics,
}

#[derive(Debug, Clone)]
pub struct PageMetadata {
    offset: PageOffset,
    segment_generation: u64,
    reader_generation_mask: u64,
    is_dirty: bool,
    access_count: AtomicU32,
    last_modified: Instant,
    heat_score: Atomic<f32>, // For caching decisions
}
```

### 3. Fork Management Core

```rust
/// Unique fork identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForkId(u64);

/// Comprehensive fork metadata
pub struct ForkMetadata {
    fork_id: ForkId,
    parent_fork_id: Option<ForkId>,
    created_at: Instant,
    
    // Efficient state tracking
    segment_fingerprints: HashMap<SegmentId, SegmentFingerprint>,
    dirty_segments: HashSet<SegmentId>,
    reader_generations: HashMap<SegmentId, u64>,
    
    // Fork tree metadata
    depth: usize,
    child_forks: HashSet<ForkId>,
    merge_status: MergeStatus,
}

/// Efficient segment state signature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentFingerprint {
    segment_id: SegmentId,
    version: u64,
    page_checksums: [u64; 1024],     // Checksum every 16 pages
    allocation_bitmap_signature: u64,
    creation_time: u64,
}

/// Fork tree manager with unlimited branching
pub struct UnlimitedForkManager {
    forks: ConcurrentHashMap<ForkId, ForkMetadata>,
    fork_tree: ConcurrentTree<ForkId, ForkId>,
    fork_by_name: HashMap<String, ForkId>,
    active_forks: ConcurrentHashMap<ForkId, AtomicU64>,
    
    // Performance optimization
    access_patterns: ConcurrentHashMap<PageRange, AccessPattern>,
    materialization_cache: LruCache<(ForkId, SegmentId), CachedSegment>,
    
    config: ForkManagerConfig,
}
```

---

## Segment Management

### 64MB Segment Design Rationale

The 64MB segment size is chosen for optimal S3 performance:

- **S3 API Efficiency**: Minimizes PUT/GET calls
- **Memory Alignment**: Fits well in modern RAM caches
- **Network Optimization**: Good balance for bandwidth vs latency
- **Compression Ratio**: Better compression on larger chunks

### Segment Allocation Strategy

```rust
impl S3SegmentManager {
    /// Allocate new page with generational tracking
    pub fn allocate_page(&self, generation: u64) -> Result<PageOffset> {
        let segment_id = self.find_optimal_segment()?;
        let page_index = segment.allocate_page_slot()?;
        let offset = PageOffset { segment_id, page_index };
        
        // Update metadata with full tracking
        self.page_metadata.insert(offset, PageMetadata {
            offset,
            segment_generation: generation,  
            reader_generation_mask: 0,          // No readers initially
            is_dirty: true,                     // Modified since S3 sync
            access_count: AtomicU32::new(0),
            last_modified: Instant::now(),
            heat_score: Atomic::new(0.0),
        });
        
        // Track generation allocation for cleanup
        if let Some(segment) = self.active_segments.get(&segment_id) {
            segment.track_page_generation(page_index, generation);
        }
        
        Ok(offset)
    }
    
    /// Smart segment selection based on utilization
    fn find_optimal_segment(&self) -> Result<SegmentId> {
        // Prioritize: (1) available space, (2) low heat, (3) recent access
        let mut candidates: Vec<_> = self.active_segments.iter()
            .map(|(id, segment)| {
                let availability = segment.space_left_ratio();
                let utilitzation = segment.utilization.load(Ordering::Acquire);
                let last_access = segment.last_access.load(Ordering::Acquire);
                let heat_score = self.calculate_segment_heat(id);
                
                // Scoring algorithm
                let score = (availability * 0.5) + 
                           ((1.0 - utilitzation) * 0.3) + 
                           (heat_score * 0.2);
                
                (*id, score)
            })
            .collect();
            
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        match candidates.first() {
            Some((segment_id, _)) => Ok(*segment_id),
            None => self.allocate_new_segment(),
        }
    }
    
    /// Allocate new 64MB segment
    fn allocate_new_segment(&self) -> Result<SegmentId> {
        let segment_id = SegmentId(self.next_segment_id());
        let segment = Segment::create_new(segment_id, SEGMENT_SIZE)?;
        
        self.active_segments.insert(segment_id, segment);
        
        // Pre-allocate S3 key space
        self.ensure_s3_key_structure(segment_id).await?;
        
        info!("Allocated new segment {:?} (total: {})", 
              segment_id, self.active_segments.len());
        
        Ok(segment_id)
    }
}
```

### Page Access with Caching

```rust
impl S3SegmentManager {
    /// Load page with multi-tier caching
    pub async fn load_page(&self, offset: PageOffset, 
                          access_context: &AccessContext) -> Result<CowPage> {
        
        // Update access pattern tracking
        self.update_access_pattern(offset, access_context);
        
        // Tier 1: Check memory cache
        if let Some(page) = self.get_from_memory_cache(offset) {
            self.update_page_access_stats(offset);
            return Ok(page);
        }
        
        // Tier 2: Check local disk cache  
        if let Some(page) = self.get_from_local_cache(offset).await? {
            self.promote_to_memory_cache(offset, &page);
            return Ok(page);
        }
        
        // Tier 3: Fetch from S3 with streaming
        let page = self.fetch_from_s3_streaming(offset, access_context).await?;
        
        // Cache based on access predictability
        if self.should_cache_page(offset, access_context) {
            self.cache_page_multi_tier(offset, &page).await?;
        }
        
        Ok(page)
    }
    
    /// Streaming S3 fetch with compression
    async fn fetch_from_s3_streaming(&self, offset: PageOffset, 
                                   context: &AccessContext) -> Result<CowPage> {
        let s3_key = offset.to_s3_key();
        
        // Determine optimal S3 version based on fork context
        let s3_version = self.resolve_s3_version_for_fork(
            offset, context.fork_id).await?;
        
        let get_request = GetObjectRequest {
            bucket: self.s3_bucket.clone(),
            key: format!("{}?versionId={}", s3_key, s3_version),
            range: Some(format!(
                "bytes={}-{}", 
                offset.page_index * PAGE_SIZE, 
                (offset.page_index + 1) * PAGE_SIZE - 1
            )),
            ..Default::default()
        };
        
        let response = self.s3_client.get_object(get_request).await?;
        
        // Stream decompression
        let request = DecompressRequest {
            input: response.body.into_async_read(),
            codec: self.compression_codec,
            output_buffer_size: PAGE_SIZE,
        };
        
        let page_data = decompress_stream(request).await?;
        
        Ok(CowPage::new(page_data, offset))
    }
}

/// Context for page access decisions
#[derive(Debug, Clone)]
pub struct AccessContext {
    pub fork_id: ForkId,
    pub query_type: QueryType,
    pub batch_size: usize,
    pub priority: AccessPriority,
}

#[derive(Debug, Clone)]
pub enum AccessPriority {
    Realtime,     // Must be fast, always cache
    Interactive,  // Should be fast, moderate caching
    Batch,        // Background work, minimal caching
    Analytics,    // Sequential access, no caching
}

#[derive(Debug, Clone)]
pub enum QueryType {
    PointRead,
    RangeScan,
    Join,
    Aggregate,
    IndexLookup,
}
```

---

## Unlimited Forking

### Zero-Cost Fork Creation

The key innovation is using fingerprinting to make fork creation O(1):

```rust
impl UnlimitedForkManager {
    /// Create fork in O(1) time - metadata only
    pub fn create_fork(&self, parent_fork_id: ForkId, 
                       fork_name: &str, 
                       fork_config: ForkConfig) -> Result<ForkId> {
        
        let parent_fork = self.forks.get(&parent_fork_id)
            .ok_or(Error::ForkNotFound)?;
        
        // Generate unique fork ID
        let fork_id = ForkId(self.next_fork_id());
        
        // **O(1) Operation**: Clone metadata only
        let mut new_fork = ForkMetadata {
            fork_id,
            parent_fork_id: Some(parent_fork_id),
            created_at: Instant::now(),
            segment_fingerprints: parent_fork.segment_fingerprints.clone(),
            dirty_segments: HashSet::new(),        // Start clean
            reader_generations: parent_fork.reader_generations.clone(),
            depth: parent_fork.depth + 1,
            child_forks: HashSet::new(),
            merge_status: MergeStatus::Active,
        };
        
        // Apply fork-specific overrides
        self.apply_fork_config(&mut new_fork, fork_config);
        
        // Register fork globally
        self.forks.insert(fork_id, new_fork);
        self.fork_by_name.insert(fork_name.to_string(), fork_id);
        self.active_forks.insert(fork_id, AtomicU64::new(0));
        
        // Update fork tree
        self.fork_tree.insert_node(parent_fork_id, vec![fork_id]);
        if let Some(parent_mut) = self.forks.get_mut(&parent_fork_id) {
            parent_mut.child_forks.insert(fork_id);
        }
        
        // Log fork creation
        info!("Created fork '{}' ({:?}) from parent {:?} at depth {}", 
              fork_name, fork_id, parent_fork_id, new_fork.depth);
        
        // Trigger background materialization if needed
        if fork_config.prewarm_strategy != PrewarmStrategy::Lazy {
            self.schedule_prewarming(fork_id, fork_config.prewarm_strategy);
        }
        
        Ok(fork_id)
    }
}

#[derive(Debug, Clone)]
pub struct ForkConfig {
    pub fork_description: Option<String>,
    pub permissions: ForkPermissions, 
    pub prewarm_strategy: PrewarmStrategy,
    pub materialization_budget: Option<usize>,
    pub merge_policy: MergePolicy,
}

#[derive(Debug, Clone)]
pub enum PrewarmStrategy {
    Lazy,                                    // Materialize on demand
    Predictive,                             // Predict and prewarm
    Aggressive,                             // Materialize all segments
    Targeted(Vec<PageRange>),              // Specific ranges
}

#[derive(Debug, Clone)]
pub enum ForkPermissions {
    ReadOnly,                               // Can only read
    ReadWrite,                              // Standard permissions
    ReadWriteWithMerge,                     // Can merge back
    Experimental,                           // Development forks
}
```

### Lazy Materialization Engine

```rust
pub struct LazyMaterializationEngine {
    // Background workers
    materialization_workers: TokioThreadPool,
    prefetch_engine: PrefetchEngine,
    
    // Learning and prediction
    access_pattern_analyzer: AccessPatternAnalyzer,
    fork_dependency_tracker: ForkDependencyTracker,
    
    // Caching infrastructure
    materialization_cache: LruCache<(ForkId, SegmentId), CachedSegment>,
    segment_copy_store: SegmentCopyStore,
}

impl LazyMaterializationEngine {
    /// Materialize segment differences on-demand
    pub async fn materialize_segment_if_needed(&self, fork_id: ForkId, 
                                             segment_id: SegmentId) -> Result<()> {
        let fork = self.fork_manager.forks.get(&fork_id)
            .ok_or(Error::ForkNotFound)?;
            
        // Check if already materialized
        if fork.dirty_segments.contains(&segment_id) {
            return Ok(());
        }
        
        // Check if materialization is needed
        if !self.should_materialize_segment(fork, segment_id).await? {
            return Ok(());
        }
        
        // Perform materialization
        self.materialize_segment_difference(fork, segment_id).await?;
        
        Ok(())
    }
    
    /// Smart materialization decision making
    async fn should_materialize_segment(&self, fork: &ForkMetadata, 
                                       segment_id: SegmentId) -> Result<bool> {
        // 1. Check if parent has this segment
        let parent_fingerprint = fork.parent_fork_id
            .and_then(|parent_id| {
                self.fork_manager.forks.get(&parent_id)
                    .and_then(|parent| parent.segment_fingerprints.get(&segment_id))
                    .copied()
            });
            
        let my_fingerprint = fork.segment_fingerprints.get(&segment_id);
        
        match (parent_fingerprint, my_fingerprint) {
            (None, None) => Ok(false), // No segment anywhere
            (Some(parent_fp), None) => {
                // Parent has segment but we don't - need to materialize
                Ok(true)
            }
            (Some(parent_fp), Some(my_fp)) => {
                // Check if content differs
                Ok(parent_fp != *my_fp)
            }
            (None, Some(my_fp)) => Ok(false), // We have a new segment
        }
    }
    
    /// Perform actual segment materialization
    async fn materialize_segment_difference(&self, fork: &ForkMetadata, 
                                          segment_id: SegmentId) -> Result<()> {
        
        // Get parent segment for reference
        let parent_segment = self.get_parent_segment_for_copy(fork, segment_id).await?;
        
        // Create new materialized segment
        let materialized_segment = self.create_materialized_segment(
            segment_id, 
            parent_segment, 
            fork.fork_id
        ).await?;
        
        // Update fork metadata
        let mut updated_fork = fork.clone();
        updated_fork.dirty_segments.insert(segment_id);
        updated_fork.segment_fingerprints.insert(
            segment_id, 
            materialized_segment.fingerprint()
        );
        updated_fork.reader_generations.insert(segment_id, self.next_generation());
        
        self.fork_manager.forks.insert(fork.fork_id, updated_fork);
        
        // Cache materialized segment
        self.materialization_cache.put(
            (fork.fork_id, segment_id), 
            CachedSegment::new(materialized_segment)
        );
        
        info!("Materialized segment {:?} for fork {:?}", segment_id, fork.fork_id);
        
        Ok(())
    }
}

/// Tracks materialization dependencies
struct ForkDependencyTracker {
    dependency_graph: DirectedGraph<ForkId, SegmentId>,
    materialization_queue: TokioQueue<MaterializationTask>,
    priority_scheduler: PriorityScheduler<MaterializationTask>,
}

#[derive(Debug, Clone)]
struct MaterializationTask {
    fork_id: ForkId,
    segment_id: SegmentId,
    priority: MaterializationPriority,
    dependencies: Vec<(ForkId, SegmentId)>,
    estimated_cost: Cost,
    deadline: Option<Instant>,
}
```

### Fork Merging and Conflict Resolution

```rust
impl UnlimitedForkManager {
    /// Merge child fork back into parent with configurable strategy
    pub async fn merge_fork(&self, child_fork_id: ForkId, 
                           merge_strategy: MergeStrategy,
                           validation_mode: ValidationMode) -> Result<MergeResult> {
        
        let merge_start = Instant::now();
        
        let mut child_fork = self.forks.get(&child_fork_id)
            .ok_or(Error::ForkNotFound)?
            .clone();
            
        let parent_fork_id = child_fork.parent_fork_id
            .ok_or(Error::CannotMergeBaseFork)?;
            
        let mut parent_fork = self.forks.get(&parent_fork_id)  
            .ok_or(Error::ForkNotFound)?
            .clone();
        
        // Run validation if requested
        if validation_mode != ValidationMode::None {
            self.validate_merge_candidate(&parent_fork, &child_fork, 
                                         validation_mode).await?;
        }
        
        let mut merge_stats = MergeResult::new();
        
        // Process each dirty segment
        for segment_id in child_fork.dirty_segments.iter() {
            let segment результат = self.merge_single_segment(
                &parent_fork, 
                &child_fork, 
                *segment_id, 
                &merge_strategy
            ).await?;
            
            merge_stats.add_segment_result(*segment_id, результат);
        }
        
        // Handle orphaned segments (segments deleted in child)
        merge_stats.merge(self.process_deletions(&parent_fork, &child_fork).await?);
        
        // Update parent state
        parent_fork.merge_in_child(&child_fork);
        self.forks.insert(parent_fork_id, parent_fork);
        
        // Clean up child fork
        self.cleanup_merge_resources(child_fork_id).await?;
        
        // Update fork tree
        self.fork_tree.remove_node(child_fork_id);
        
        merge_stats.duration = merge_start.elapsed();
        
        info!("Successfully merged fork {:?} into {:?} in {:?}", 
              child_fork_id, parent_fork_id, merge_stats.duration);
              
        Ok(merge_stats)
    }
    
    /// Merge individual segment with conflict resolution
    async fn merge_single_segment(&self, parent: &ForkMetadata, child: &ForkMetadata,
                                 segment_id: SegmentId, 
                                 strategy: &MergeStrategy) -> Result<SegmentMergeResult> {
        
        let parent_fp = parent.segment_fingerprints.get(&segment_id);
        let child_fp = child.segment_fingerprints.get(&segment_id);
        
        match (parent_fp, child_fp) {
            (None, Some(child_f)) => {
                // Child has new segment → adopt it  
                self.adopt_child_segment(parent, child, segment_id, *child_f).await
            }
            (Some(parent_f), Some(child_f)) if parent_f != child_f => {
                // Both have different segments → resolve conflict
                self.resolve_segment_conflict(parent, child, segment_id,
                                             *parent_f, *child_f, strategy).await
            }
            (Some(_), None) => {
                // Segment deleted in child
                Ok(SegmentMergeResult::SegmentDeleted)
            }
            (Some(parent_f), Some(child_f)) if parent_f == child_f => {
                // Identical segments → no change
                Ok(SegmentMergeResult::Identical)
            }
            (None, None) => {
                // Neither has segment → invalid state
                Err(Error::InvalidSegmentState)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum MergeStrategy {
    /// Child's version always wins conflicts
    ChildWins,
    /// Parent's version always wins conflicts  
    ParentWins,
    /// Resolve conflicts at page level
    PageLevelMerge(PageLevelResolution),
    /// Three-way merge using common ancestor
    ThreeWayMerge {
        ancestor_fork: ForkId,
        conflict_resolver: Box<dyn ConflictResolver>,
    },
    /// Timestamp-based merge
    LastModifiedWins,
    /// Manual resolution required
    ManualConflict,
}

#[derive(Debug, Clone)]
pub struct PageLevelResolution {
    pub resolution_policy: ConflictPolicy,
    pub conflict_detector: ConflictDetector,
    pub merge_buffer_size: usize,
}

#[derive(Debug, Clone)]
pub enum ConflictPolicy {
    LastWriterWins,
    ChildWins,
    ParentWins,
    SemanticResolution(SemanticResolver),
    ManualResolution,
}
```

---

## S3 Integration

### Efficient Segment Storage Strategy

```rust
#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub compression: CompressionConfig,
    pub lifecycle: LifecyclePolicy,
    pub performance: S3PerformanceConfig,
}

#[derive(Debug, Clone)]
pub struct CompressionConfig {
    pub algorithm: CompressionAlgorithm,
    pub level: u32,                    // 1-9 for compression
    pub dictionary: Option<Vec<u8>>,   // Pre-computed dictionary
}

#[derive(Debug, Clone)]
pub enum CompressionAlgorithm {
    Zstd,
    LZ4,
    Gzip,
    Snappy,
}

impl S3SegmentManager {
    /// Upload complete segment with optimized chunking
    pub async fn upload_segment_optimized(&self, 
                                         segment_id: SegmentId, 
                                         segment: &Segment) -> Result<S3UploadResult> {
        
        let upload_start = Instant::now();
        
        // 1. Create segment snapshot
        let snapshot = segment.create_data_snapshot();
        
        // 2. Determine optimal compression
        let compression_stats = self.analyze_compression_potential(&snapshot);
        let compression_config = self.select_optimal_compression(compression_stats);
        
        // 3. Compress with streaming
        let compressed_stream = self.create_compression_stream(
            &snapshot, 
            &compression_config
        )?;
        
        // 4. Upload using multipart for large segments
        let upload_result = match compressed_stream.len() {
            0..=5_000_000 => {
                // Single-part upload
                self.upload_single_part(segment_id, compressed_stream).await?
            }
            _ => {
                // Multi-part upload
                self.upload_multipart(segment_id, compressed_stream).await?
            }
        };
        
        // 5. Update segment metadata
        segment.update_s3_metadata(upload_result.version, compression_config);
        
        let upload_duration = upload_start.elapsed();
        info!("Uploaded segment {:?} in {:?} ({} bytes compressed)", 
              segment_id, upload_duration, upload_result.compressed_size);
              
        Ok(upload_result)
    }
    
    /// Download segment with parallelization
    pub async fn download_segment_parallel(&self, 
                                         segment_id: SegmentId,
                                         s3_version: Option<String>,
                                         download_config: DownloadConfig) -> Result<SegmentData> {
        
        let download_start = Instant::now();
        
        // 1. Determine download strategy
        let strategy = self.select_download_strategy(segment_id, download_config);
        
        // 2. Execute download
        let segment_data = match strategy {
            DownloadStrategy::Parallel { chunk_count } => {
                self.download_segment_parallel_chunks(
                    segment_id, s3_version, chunk_count
                ).await?
            }
            DownloadStrategy::Streaming { prefetch_window } => {
                self.download_segment_streaming(
                    segment_id, s3_version, prefetch_window
                ).await?
            }
            DownloadStrategy::LocalCache { fallback } => {
                self.download_from_cache_with_fallback(
                    segment_id, fallback
                ).await?
            }
        };
        
        let download_duration = download_start.elapsed();
        
        info!("Downloaded segment {:?} in {:?} ({} MB)", 
              segment_id, download_duration, segment_data.len() / 1024 / 1024);
              
        Ok(segment_data)
    }
    
    /// Intelligent download strategy selection
    fn select_download_strategy(&self, segment_id: SegmentId, 
                               config: DownloadConfig) -> DownloadStrategy {
        
        // Check local cache first
        if self.local_cache.has_segment(segment_id) && 
           !config.force_s3_download {
            return DownloadStrategy::LocalCache { 
                fallback: config.s3_fallback_enabled 
            };
        }
        
        // Select based on size and access pattern
        let segment_size = self.estimate_segment_size(segment_id);
        
        match (segment_size, config.access_pattern) {
            (size, AccessPattern::Sequential) if size > 10_000_000 => {
                DownloadStrategy::Streaming { prefetch_window: 2 }
            }
            (size, AccessPattern::Random) if size > 50_000_000 => {
                DownloadStrategy::Parallel { chunk_count: 8 }
            }
            (size, _) if size > 100_000_000 => {
                DownloadStrategy::Parallel { chunk_count: 16 }
            }
            _ => {
                DownloadStrategy::Streaming { prefetch_window: 1 }
            }
        }
    }
}

#[derive(Debug)]
pub struct S3UploadResult {
    pub compressed_size: usize,
    pub compression_ratio: f32,
    pub upload_duration: Duration,
    pub multipart_parts: Option<usize>,
    pub version: String,
}

#[derive(Debug, Clone)]
pub enum DownloadStrategy {
    Parallel { chunk_count: usize },
    Streaming { prefetch_window: usize },
    LocalCache { fallback: bool },
}

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub access_pattern: AccessPattern,
    pub force_s3_download: bool,
    pub s3_fallback_enabled: bool,
    pub timeout: Duration,
    pub retry_policy: RetryPolicy,
}
```

### S3 Performance Optimization

```rust
impl S3SegmentManager {
    /// Batched S3 operations for efficiency
    pub async fn upload_segments_batch(&self, 
                                     segment_uploads: Vec<(SegmentId, Segment)>) -> Result<Vec<S3UploadResult>> {
        
        // Group by optimal upload strategy
        let mut single_part_uploads = Vec::new();
        let mut multipart_uploads = Vec::new();
        
        for (segment_id, segment) in segment_uploads {
            let size = segment.estimate_compressed_size();
            if size <= 5_000_000 {
                single_part_uploads.push((segment_id, segment));
            } else {
                multipart_uploads.push((segment_id, segment));
            }
        }
        
        // Execute concurrent uploads
        let (single_results, multipart_results) = tokio::join!(
            self.upload_single_part_batch(single_part_uploads),
            self.upload_multipart_batch(multipart_uploads)
        );
        
        // Combine results
        let mut all_results = single_results?;
        all_results.extend(multipart_results?);
        
        info!("Batch uploaded {} segments ({} single-part, {} multipart)", 
              all_results.len(), single_part_uploads.len(), multipart_uploads.len());
              
        Ok(all_results)
    }
    
    /// Intelligent S3 key organization
    pub fn organize_s3_keys(&self, segment_id: SegmentId, 
                           context: KeyContext) -> String {
        
        match context {
            KeyContext::Production => {
                format!("{}/production/segments/{:06d}/{:010x}", 
                       self.config.prefix, segment_id.0 / 1000, segment_id.0)
            }
            KeyContext::Fork { fork_id } => {
                format!("{}/forks/{:06d}/segments/{:06d}", 
                       self.config.prefix, fork_id.0, segment_id.0)
            }
            KeyContext::Backup { timestamp } => {
                format!("{}/backups/{}/{}/segments/{:06d}", 
                       self.config.prefix, timestamp.format("%Y-%m-%d"), segment_id.0)
            }
            KeyContext::Archive { reason } => {
                format!("{}/archive/{}/{}/segments/{:06d}", 
                       self.config.prefix, reason, segment_id.0)
            }
        }
    }
    
    /// S3 lifecycle policy management
    pub fn create_lifecycle_policy(&self) -> LifecycleConfiguration {
        LifecycleConfiguration {
            rules: vec![
                LifecycleRule {
                    id: "clean_old_backups".to_string(),
                    status: "Enabled".to_string(),
                    filter: Some(LifecycleFilter {
                        prefix: Some(format!("{}/backups/", self.config.prefix)),
                    }),
                    transitions: vec![
                        LifecycleTransition {
                            days: 30,
                            storage_class: "STANDARD_IA".to_string(),
                        },
                        LifecycleTransition {
                            days: 90,
                            storage_class: "GLACIER".to_string(),
                        },
                        LifecycleTransition {
                            days: 365,
                            storage_class: "DEEP_ARCHIVE".to_string(),
                        },
                    ],
                    expiration: None,
                },
                LifecycleRule {
                    id: "expire_archived_data".to_string(),
                    status: "Enabled".to_string(),
                    filter: Some(LifecycleFilter {
                        prefix: Some(format!("{}/archive", self.config.prefix)),
                    }),
                    transitions: vec![],
                    expiration: Some(LifecycleExpiration {
                        days: 3650, // 10 years
                    }),
                },
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub enum KeyContext {
    Production,
    Fork { fork_id: ForkId },
    Backup { timestamp: DateTime<Utc> },
    Archive { reason: String },
}
```

---

## Garbage Collection

### Generational Garbage Collection

```rust
pub struct AdvancedGarbageCollector {
    // Generation tracking
    active_generations: ConcurrentHashMap<u64, GenerationInfo>,
    generation_segments: BTreeMap<u64, HashSet<SegmentId>>,
    
    // GC policies and configuration
    gc_policy: GcPolicy,
    cleanup_scheduler: CleanupScheduler,
    
    // Performance metrics
    gc_metrics: GcMetrics,
    
    // Integration with segment manager
    segment_manager: Arc<S3SegmentManager>,
}

#[derive(Debug, Clone)]
pub struct GenerationInfo {
    generation_id: u64,
    created_at: Instant,
    last_activity: Instant,
    reader_count: AtomicU32,
    segments_in_use: BitSet,
    memory_pressure_score: f32,
}

#[derive(Debug, Clone)]
pub struct GcPolicy {
    pub max_generation_age: Duration,
    pub max_active_generations: usize,
    pub memory_pressure_threshold: f32,
    pub cleanup_interval: Duration,
    pub aggressive_gc_below_memory: f32,
    pub s3_cleanup_delay: Duration,
}

impl AdvancedGarbageCollector {
    /// Main GC pass with intelligent prioritization
    pub async fn run_gc_cycle(&self) -> Result<GcCycleResult> {
        let cycle_start = Instant::now();
        let mut cycle_result = GcCycleResult::new();
        
        // 1. Update generation tracking
        cycle_result.merge(self.update_generation_tracking().await?);
        
        // 2. Identify cleanup candidates
        let cleanup_candidates = self.identify_cleanup_candidates().await?;
        
        // 3. Prioritize by multiple factors
        let prioritized_candidates = self.prioritize_cleanup_candidates(cleanup_candidates);
        
        // 4. Execute cleanup in phases
        for phase in CleanupPhase::ordered() {
            let phase_candidates: Vec<_> = prioritized_candidates
                .iter()
                .filter(|c| c.cleanup_phase == *phase)
                .collect();
                
            if !phase_candidates.is_empty() {
                let phase_result = self.execute_cleanup_phase(phase_candidates).await?;
                cycle_result.phase_results.insert(phase, phase_result);
            }
        }
        
        // 5. S3-specific cleanup
        cycle_result.merge(self.cleanup_s3_stale_objects().await?);
        
        // 6. Update metrics
        cycle_result.duration = cycle_start.elapsed();
        self.gc_metrics.record_cycle(cycle_result.clone());
        
        info!("GC cycle completed in {:?}: {} generations cleaned, {} segments reclaimed", 
              cycle_result.duration, cycle_result.generations_cleaned, 
              cycle_result.segments_reclaimed);
              
        Ok(cycle_result)
    }
    
    /// Identify candidates for cleanup using multi-factor analysis
    async fn identify_cleanup_candidates(&self) -> Result<Vec<CleanupCandidate>> {
        let mut candidates = Vec::new();
        let current_time = Instant::now();
        
        for (generation_id, gen_info) in &self.active_generations {
            let reader_count = gen_info.reader_count.load(Ordering::Acquire);
            
            // Skip if still in use
            if reader_count > 0 {
                continue;
            }
            
            // Calculate cleanup scores
            let age_score = self.calculate_age_score(gen_info, current_time);
            let memory_score = self.calculate_memory_pressure_score(gen_info);
            let s3_cost_score = self.calculate_s3_cost_score(generation_id);
            
            // Overall score (higher = more urgent cleanup)
            let urgency_score = (age_score * 0.4) + 
                               (memory_score * 0.4) + 
                               (s3_cost_score * 0.2);
            
            // Determine cleanup phase
            let cleanup_phase = self.determine_cleanup_phase(urgency_score, gen_info);
            
            if urgency_score > 0.3 { // Threshold for cleanup consideration
                candidates.push(CleanupCandidate {
                    generation_id: *generation_id,
                    urgency_score,
                    cleanup_phase,
                    estimated_recovery: self.estimate_memory_recovery(*generation_id),
                    s3_objects: self.get_s3_objects_for_generation(*generation_id),
                });
            }
        }
        
        // Sort by urgency
        candidates.sort_by(|a, b| b.urgency_score.partial_cmp(&a.urgency_score).unwrap());
        
        Ok(candidates)
    }
    
    // Multi-score calculation methods
    fn calculate_age_score(&self, gen_info: &GenerationInfo, current_time: Instant) -> f32 {
        let age = current_time.duration_since(gen_info.created_at);
        let normalized_age = age.as_secs() as f32 / self.gc_policy.max_generation_age.as_secs() as f32;
        normalized_age.min(1.0)
    }
    
    fn calculate_memory_pressure_score(&self, gen_info: &GenerationInfo) -> f32 {
        // Combine current memory pressure with generation memory usage
        let current_pressure = self.get_current_memory_pressure();
        let gen_memory_usage = self.estimate_generation_memory_usage(&gen_info.segments_in_use);
        
        (current_pressure * 0.6 + gen_memory_usage * 0.4).min(1.0)
    }
    
    fn calculate_s3_cost_score(&self, generation_id: u64) -> f32 {
        // Estimate ongoing S3 costs for this generation
        let s3_objects = self.count_s3_objects_for_generation(generation_id);
        let storage_days = self.estimate_generation_storage_days(generation_id);
        
        // Higher score for expensive S3 storage
        let cost_estimate = s3_objects as f32 * storage_days * 0.001; // Rough cost model
        (cost_estimate / 100.0).min(1.0) // Normalize to 0-1 range
    }
    
    /// Execute cleanup in specific phases
    async fn execute_cleanup_phase(&self, 
                                  candidates: &[&CleanupCandidate]) -> Result<PhaseResult> {
        
        let mut phase_result = PhaseResult::new();
        
        for candidate in candidates {
            let cleanup_start = Instant::now();
            
            match self.cleanup_single_generation(candidate.generation_id).await {
                Ok(cleanup_stats) => {
                    phase_result.successful_cleanups.push((
                        candidate.generation_id, 
                        cleanup_stats
                    ));
                    
                    // Update in-memory tracking
                    self.active_generations.remove(&candidate.generation_id);
                    self.generation_segments.remove(&candidate.generation_id);
                }
                Err(e) => {
                    warn!("Failed to cleanup generation {}: {}", candidate.generation_id, e);
                    phase_result.failed_cleanups.push((candidate.generation_id, e));
                }
            }
            
            phase_result.total_duration += cleanup_start.elapsed();
        }
        
        Ok(phase_result)
    }
    
    /// Cleanup individual generation
    async fn cleanup_single_generation(&self, generation_id: u64) -> Result<GenerationCleanupResult> {
        let mut result = GenerationCleanupResult::new();
        
        // Get segments for this generation
        let segments = self.generation_segments.get(&generation_id)
            .map(|set| set.clone())
            .unwrap_or_default();
        
        // Clean up in-memory pages
        for segment_id in &segments {
            if let Some(segment) = self.segment_manager.active_segments.get(segment_id) {
                // Clean up pages specific to this generation
                let pages_cleaned = segment.cleanup_generation_pages(generation_id);
                result.pages_reclaimed += pages_cleaned;
            }
        }
        
        // Clean up local cache files
        result.merge(self.cleanup_local_cache_for_generation(generation_id).await?);
        
        // Schedule S3 cleanup (with delay)
        self.schedule_s3_cleanup(generation_id, self.gc_policy.s3_cleanup_delay);
        result.s3_cleanup_scheduled = true;
        
        info!("Cleaned up generation {}: {} pages reclaimed", 
              generation_id, result.pages_reclaimed);
              
        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub struct CleanupCandidate {
    pub generation_id: u64,
    pub urgency_score: f32,
    pub cleanup_phase: CleanupPhase,
    pub estimated_recovery: MemoryRecoveryEstimate,
    pub s3_objects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CleanupPhase {
    Immediate,    // 0-2 hours ago, urgent cleanup
    Normal,       // 2-24 hours ago, regular cleanup  
    Deferred,     // 1-7 days ago, background cleanup
    Archival,     // 1+ weeks ago, archival cleanup
}

impl CleanupPhase {
    fn ordered() -> Vec<Self> {
        vec![Self::Immediate, Self::Normal, Self::Deferred, Self::Archival]
    }
}

#[derive(Debug)]
pub struct GcCycleResult {
    pub duration: Duration,
    pub generations_cleaned: usize,
    pub segments_reclaimed: usize,
    pub pages_reclaimed: usize,
    pub s3_objects_deleted: usize,
    pub memory_freed: u64,
    pub phase_results: HashMap<CleanupPhase, PhaseResult>,
}

#[derive(Debug)]
pub struct PhaseResult {
    pub successful_cleanups: Vec<(u64, GenerationCleanupResult)>,
    pub failed_cleanups: Vec<(u64, Error)>,
    pub total_duration: Duration,
    pub memory_recovered: u64,
}
```

---

## Performance Optimizations

### Access Pattern Learning

```rust
pub struct AccessPatternAnalyzer {
    // Pattern tracking
    page_access_patterns: LruCache<PageRange, AccessPattern>,
    fork_access_stats: ConcurrentHashMap<ForkId, ForkAccessStats>,
    
    // Machine learning for prediction
    access_predictor: AccessPredictor,
    fork_behavior_classifier: ForkBehaviorClassifier,
    
    // Pattern integration
    pattern_integration: PatternIntegrationEngine,
}

#[derive(Debug, Clone)]
pub struct AccessPattern {
    pub access_frequency: f64,           // accesses per minute
    pub temporal_pattern: TemporalPattern,
    pub spatial_locality: f64,           // how clustered are accesses
    pub sequential_probability: f64,     // likelihood of sequential access
    pub access_types: HashMap<QueryType, f64>,
    pub fork_access_correlation: f64,     // correlation with fork usage
}

#[derive(Debug, Clone)]
pub struct TemporalPattern {
    pub periodicity: Option<Duration>,   // If access is periodic
    pub peak_hours: Vec<u8>,             // Hours with peak activity
    pub day_of_week_pattern: Vec<f32>,   // Activity by day of week
    pub seasonal_trends: bool,           // Whether seasonal patterns exist
}

impl AccessPatternAnalyzer {
    /// Learn and store access patterns
    pub async fn learn_access(&self, access_event: AccessEvent) {
        let start = Instant::now();
        
        // Update basic statistics
        self.update_basic_statistics(&access_event).await;
        
        // Detect and learn patterns
        let detected_patterns = self.detect_patterns(&access_event).await;
        
        // Update machine learning models
        for pattern in detected_patterns {
            self.access_predictor.train_pattern(pattern).await;
        }
        
        // Update fork behavior classification
        self.update_fork_classification(&access_event).await;
        
        let learning_duration = start.elapsed();
        if learning_duration > Duration::from_millis(10) {
            debug!("Access pattern learning took {:?}", learning_duration);
        }
    }
    
    /// Predict future access needs
    pub async fn predict_access_needs(&self, 
                                    context: PredictionContext) -> Vec<AccessPrediction> {
        
        let mut predictions = Vec::new();
        
        // Get current pattern matches
        let matching_patterns = self.find_matching_patterns(&context).await;
        
        // Use ML models for prediction
        for pattern in matching_patterns {
            let prediction = self.access_predictor.predict(
                pattern, 
                &context.horizon, 
                context.confidence_threshold
            ).await?;
            
            if prediction.confidence > 0.3 { // Filter low-confidence predictions
                predictions.push(prediction);
            }
        }
        
        // Sort by confidence and urgency
        predictions.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap()
                .then_with(|| b.urgency.partial_cmp(&a.urgency).unwrap())
        });
        
        predictions
    }
    
    /// Materialize predicted hot pages
    pub async fn prewarm_based_on_predictions(&self, 
                                            predictions: Vec<AccessPrediction>) -> Result<PrewarmResult> {
        
        let mut prewarm_result = PrewarmResult::new();
        
        // Group predictions by segment for efficient prewarming
        let segment_predictions: HashMap<SegmentId, Vec<AccessPrediction>> = 
            predictions.into_iter().into_grouping_map_by(|p| p.segment_id()).collect();
        
        // Prewarm each segment
        for (segment_id, segment_predictions) in segment_predictions {
            let pages_to_prewarm: HashSet<PageRange> = segment_predictions
                .iter()
                .flat_map(|p| p.page_ranges.clone())
                .collect();
                
            let prewarm_start = Instant::now();
            
            match self.prewarm_segment_pages(segment_id, pages_to_prewarm).await {
                Ok(stats) => {
                    prewarm_result.successful_segments.push((segment_id, stats));
                }
                Err(e) => {
                    warn!("Failed to prewarm segment {:?}: {}", segment_id, e);
                    prewarm_result.failed_segments.push((segment_id, e));
                }
            }
            
            prewarm_result.total_time += prewarm_start.elapsed();
        }
        
        Ok(prewarm_result)
    }
}

#[derive(Debug, Clone)]
pub struct AccessEvent {
    pub timestamp: Instant,
    pub fork_id: ForkId,
    pub page_range: PageRange,
    pub query_type: QueryType,
    pub access_type: AccessType,
    pub query_context: QueryContext,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct PredictionContext {
    pub fork_id: ForkId,
    pub horizon: Duration,
    pub confidence_threshold: f32,
    pub resource_constraints: ResourceConstraints,
}

#[derive(Debug, Clone)]
pub struct AccessPrediction {
    pub fork_id: ForkId,
    pub page_ranges: Vec<PageRange>,
    pub confidence: f64,
    pub urgency: f64,
    pub predicted_access_time: Instant,
    pub prediction_type: PredictionType,
}

#[derive(Debug, Clone)]
pub enum PredictionType {
    Immediate,           // Very likely immediate access
    NearFuture,          // Access in next few minutes
    Background,          // Access in next hour
    Speculative,         // Guess based on patterns
}
```

### Adaptive Caching

```rust
pub struct AdaptiveCacheManager {
    // Multi-tier cache management
    memory_cache: TieredCache<MemoryTier>,
    disk_cache: TieredCache<DiskTier>,
    prefetch_queue: TokioQueue<PrefetchTask>,
    
    // Adaptive algorithms
    cache_adaptation_engine: CacheAdaptationEngine,
    performance_predictor: CachePerformancePredictor,
    
    // Configuration and learning
    cache_config: CacheConfig,
    learning_engine: CacheLearningEngine,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub memory_tier_size: usize,           // MB
    pub disk_tier_size: usize,             // GB
    pub prefetch_aggressiveness: f32,      // 0.0 - 1.0
    pub cache_meeting_time: f32,           // Target cache hit ratio
    pub adaptation_learning_rate: f32,     // How fast to adapt
}

impl AdaptiveCacheManager {
    /// Get cached item with adaptive replacement
    pub async fn get_with_adaptive_policy(&self, 
                                        key: CacheKey,
                                        access_context: &AccessContext) -> Option<CachedValue> {
        
        // Try memory tier first
        if let Some(value) = self.memory_cache.get(&key) {
            self.update_cache_performance_feedback(&key, CacheHit::Memory, access_context);
            return Some(value);
        }
        
        // Try disk tier
        if let Some(value) = self.disk_cache.get(&key) {
            // Promote to memory tier based on policy
            if self.should_promote_to_memory(&key, &value, access_context) {
                self.memory_cache.insert(key.clone(), value.clone());
            }
            
            self.update_cache_performance_feedback(&key, CacheHit::Disk, access_context);
            return Some(value);
        }
        
        // Cache miss
        self.update_cache_performance_feedback(&key, CacheHit::Miss, access_context);
        
        // Check if we should prefetch related data
        if self.should_prefetch_related_data(key, access_context) {
            self.schedule_prefetch(key, access_context).await;
        }
        
        None
    }
    
    /// Adaptive cache tier promotion decision
    fn should_promote_to_memory(&self, 
                               key: &CacheKey, 
                               value: &CachedValue,
                               context: &AccessContext) -> bool {
        
        // Factors for promotion decision
        let access_frequency = self.get_access_frequency(key);
        let recent_access = value.last_access.elapsed();
        let memory_pressure = self.get_current_memory_pressure();
        let query_importance = context.query_importance;
        let cost_of_miss = self.estimate_cost_of_miss(key, context);
        
        // Decision algorithm
        let promotion_score = (access_frequency * 0.3) + 
                             (recent_access.as_secs() as f32 * 0.2) +
                             (query_importance * 0.2) +
                             (cost_of_miss * 0.3);
        
        // Adjust for memory pressure
        let adjusted_score = promotion_score * (1.0 - memory_pressure);
        
        adjusted_score > 0.5 // Threshold for promotion
    }
    
    /// Intelligent prefetching based on access patterns
    async fn schedule_prefetch(&self, key: CacheKey, context: &AccessContext) {
        // Get related keys based on access patterns
        let related_keys = self.get_related_cache_keys(key, context);
        
        // Prioritize based on predicted access probability
        let prioritized_keys: Vec<_> = related_keys
            .into_iter()
            .map(|related_key| {
                let access_probability = self.predict_access_probability(&related_key, context);
                let prefetch_score = access_probability * related_key.prefetch_priority();
                
                (related_key, prefetch_score)
            })
            .filter(|(_, score)| *score > 0.1) // Filter low probability
            .collect();
        
        // Schedule prefetch tasks
        for (key, score) in prioritized_keys {
            let prefetch_task = PrefetchTask {
                cache_key: key,
                priority: score_to_priority(score),
                deadline: Deadline::from_now(Duration::from_secs(300)), // 5 minutes
                access_context: context.clone(),
            };
            
            self.prefetch_queue.push(prefetch_task).await;
        }
    }
    
    /// Background adaptation of cache policies
    pub async fn adapt_cache_policies(&self) {
        let adaptation_interval = Duration::from_secs(60); // Adapt every minute
        
        loop {
            // Analyze current performance
            let performance_metrics = self.cache_adaptation_engine.analyze_performance().await;
            
            // Determine adaptations needed
            let adaptations = self.cache_adaptation_engine.determine_adaptations(
                &performance_metrics, 
                &self.cache_config
            ).await;
            
            // Apply adaptations
            for adaptation in adaptations {
                match adaptation {
                    CacheAdaptation::AdjustMemoryTierSize(new_size) => {
                        self.memory_cache.resize(new_size).await;
                    }
                    CacheAdaptation::AdjustPrefetchAggressiveness(level) => {
                        self.cache_config.prefetch_aggressiveness = level;
                    }
                    CacheAdaptation::ModifyEvictionPolicy(new_policy) => {
                        self.memory_cache.set_eviction_policy(new_policy);
                    }
                    CacheAdaptation::ModifyTierPromotionRules(rules) => {
                        self.cache_config.promotion_rules = rules;
                    }
                }
            }
            
            // Train learning models
            self.learning_engine.train_performance_model(&performance_metrics).await;
            
            tokio::time::sleep(adaptation_interval).await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheKey {
    pub offset: PageOffset,
    pub fork_id: Option<ForkId>,
    pub access_pattern_type: AccessPatternType,
}

impl CacheKey {
    fn prefetch_priority(&self) -> f32 {
        match self.access_pattern_type {
            AccessPatternType::SequentialHighAccess => 1.0,
            AccessPatternType::RandomMediumAccess => 0.7,
            AccessPatternType::HistoricalBackground => 0.3,
            AccessPatternType::OneTimeQuery => 0.1,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheHit {
    Memory,
    Disk,
    Miss,
}

#[derive(Debug, Clone)]
pub enum CacheAdaptation {
    AdjustMemoryTierSize(usize),
    AdjustPrefetchAggressiveness(f32),
    ModifyEvictionPolicy(EvictionPolicy),
    ModifyTierPromotionRules(PromotionRules),
}
```

---

## Configuration and Tuning

### Comprehensive Configuration System

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanakirjaS3Config {
    /// Core storage configuration
    pub storage: StorageConfig,
    
    /// Forking configuration
    pub forking: ForkConfig,
    
    /// S3 integration configuration
    pub s3: S3Config,
    
    /// Performance configuration
    pub performance: PerformanceConfig,
    
    /// Garbage collection configuration
    pub gc: GcConfig,
    
    /// Caching configuration
    pub cache: CacheConfig,
    
    /// Monitoring and metrics
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base storage configuration
    pub segment_size_mb: u32,                // Default: 64
    pub page_size_bytes: u32,                // Default: 4096  
    pub max_active_segments: usize,          // Default: 32
    pub local_cache_dir: PathBuf,
    
    /// Memory management
    pub memory_limit_mb: u32,                // Default: 2048
    pub memory_pressure_threshold: f32,      // Default: 0.8
    pub segment_allocation_strategy: AllocationStrategy,
    
    /// Compression settings
    pub compression: CompressionConfig,
    pub compression_threshold_bytes: u32,   // Min size to compress
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkConfig {
    /// Fork creation limits
    pub max_forks_per_environment: usize,    // Default: 1000
    pub max_fork_depth: usize,               // Default: 50
    pub fork_name_pattern: String,           // Default: "fork_{:04d}"
    
    /// Materialization settings
    pub default_prewarm_strategy: PrewarmStrategy,
    pub materialization_budget_mb: u32,     // Default: 512
    pub lazy_materialization: bool,
    
    /// Merge configuration
    pub default_merge_strategy: MergeStrategy,
    pub auto_merge_interval: Option<Duration>,
    pub conflict_resolution_timeout: Duration,
    
    /// Fork lifecycle
    pub inactive_fork_timeout: Duration,     // Default: 30 days
    pub auto_cleanup: bool,
    pub retain_deleted_forks_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Concurrency settings
    pub max_concurrent_reads: usize,         // Default: 100
    pub max_concurrent_writes: usize,        // Default: 10
    pub reader_writer_ratio: f32,            // Default: 10:1
    
    /// Connection pooling
    pub s3_connection_pool_size: usize,      // Default: 20
    pub local_disk_pool_size: usize,         // Default: 5
    
    /// Batch operations
    pub batch_read_size: usize,              // Default: 100
    pub batch_write_size: usize,             // Default: 50
    pub batch_upload_threshold: Duration,    // Default: 5s
    
    /// Performance tuning
    pub optimize_for: OptimizationTarget,     // See enum below
    pub enable_jemalloc: bool,
    pub enable_io_uring: bool,               // Linux-specific
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptimizationTarget {
    Latency,                                // Minimize response time
    Throughput,                             // Maximize operations/second
    Cost,                                   // Minimize S3 costs
    Memory,                                 // Minimize memory usage
    Balanced,                               // Equal weighting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Metrics configuration
    pub enable_prometheus_metrics: bool,
    pub prometheus_port: u16,
    pub metrics_retention_days: u32,
    
    /// Logging configuration
    pub log_level: LogLevel,
    pub structured_logging: bool,
    pub log_format: LogFormat,
    
    /// Tracing
    pub enable_distributed_tracing: bool,
    pub jaeger_endpoint: Option<String>,
    pub sampling_rate: f64,
    
    /// Health checks
    pub health_check_port: u16,
    pub health_check_interval: Duration,
}

impl SanakirjaS3Config {
    /// Load configuration from file with validation
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: SanakirjaS3Config = toml::from_str(&content)?;
        
        // Validate configuration
        config.validate()?;
        
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
    
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate storage config
        if self.storage.segment_size_mb < 16 || self.storage.segment_size_mb > 1024 {
            return Err(ConfigError::InvalidSegmentSize(self.storage.segment_size_mb));
        }
        
        if self.storage.page_size_bytes != 4096 && self.storage.page_size_bytes != 8192 {
            return Err(ConfigError::InvalidPageSize(self.storage.page_size_bytes));
        }
        
        // Validate S3 config
        if self.s3.bucket.is_empty() {
            return Err(ConfigError::EmptyS3Bucket);
        }
        
        // Validate performance config
        if self.performance.max_concurrent_reads == 0 {
            return Err(ConfigError::InvalidConcurrency);
        }
        
        Ok(())
    }
    
    /// Generate environment-specific defaults
    pub fn for_environment(env_type: EnvironmentType) -> Self {
        match env_type {
            EnvironmentType::Development => Self {
                storage: StorageConfig {
                    segment_size_mb: 32,
                    max_active_segments: 8,
                    memory_limit_mb: 512,
                    local_cache_dir: PathBuf::from("./dev_cache"),
                    ..Default::default()
                },
                performance: PerformanceConfig {
                    optimize_for: OptimizationTarget::Latency,
                    max_concurrent_reads: 20,
                    max_concurrent_writes: 5,
                    ..Default::default()
                },
                monitoring: MonitoringConfig {
                    log_level: LogLevel::Debug,
                    structured_logging: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            
            EnvironmentType::Production => Self {
                storage: StorageConfig {
                    segment_size_mb: 64,
                    max_active_segments: 64,
                    memory_limit_mb: 4096,
                    local_cache_dir: PathBuf::from("/var/cache/sanakirja"),
                    ..Default::default()
                },
                performance: PerformanceConfig {
                    optimize_for: OptimizationTarget::Throughput,
                    max_concurrent_reads: 500,
                    max_concurrent_writes: 50,
                    s3_connection_pool_size: 100,
                    ..Default::default()
                },
                monitoring: MonitoringConfig {
                    log_level: LogLevel::Info,
                    structured_logging: true,
                    enable_prometheus_metrics: true,
                    prometheus_port: 9090,
                    ..Default::default()
                },
                ..Default::default()
            },
            
            EnvironmentType::Testing => Self {
                storage: StorageConfig {
                    segment_size_mb: 16,
                    max_active_segments: 4,
                    memory_limit_mb: 128,
                    local_cache_dir: PathBuf::from("/tmp/test_cache"),
                    ..Default::default()
                },
                performance: PerformanceConfig {
                    optimize_for: OptimizationTarget::Cost,
                    max_concurrent_reads: 5,
                    max_concurrent_writes: 2,
                    ..Default::default()
                },
                forking: ForkConfig {
                    max_forks_per_environment: 10,
                    default_prewarm_strategy: PrewarmStrategy::Lazy,
                    inline_gc_interval: Some(Duration::from_secs(1)),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvironmentType {
    Development,
    Testing, 
    Production,
    Staging,
}
```

### Runtime Configuration Updates

```rust
pub struct ConfigManager {
    current_config: Arc<RwLock<SanakirjaS3Config>>,
    config_file_path: PathBuf,
    config_watchers: Vec<ConfigWatcher>,
}

impl ConfigManager {
    /// Hot-reload configuration from file
    pub async fn reload_config(&self) -> Result<ConfigReloadResult> {
        let reload_start = Instant::now();
        
        // Load new configuration
        let new_config = SanakirjaS3Config::load_from_file(&self.config_file_path)?;
        
        // Get current config
        let mut current = self.current_config.write().await;
        let old_config = current.clone();
        
        // Validate changes
        let change_analysis = self.analyze_config_changes(&old_config, &new_config)?;
        
        // Apply changes that require restart
        let restart_required = self.requires_restart(&change_analysis);
        
        // Apply hot-reloadable changes
        if !restart_required {
            self.apply_hot_reload_changes(&current, &new_config, &change_analysis).await?;
            *current = new_config;
        }
        
        // Notify all watchers
        for watcher in &self.config_watchers {
            watcher.on_config_change(&old_config, &new_config).await;
        }
        
        let reload_result = ConfigReloadResult {
            success: !restart_required,
            reload_time: reload_start.elapsed(),
            changes_applied: change_analysis.changes,
            restart_required,
        };
        
        info!("Configuration reload: {} changes, restart required: {:?}", 
              change_analysis.changes.len(), restart_required);
              
        Ok(reload_result)
    }
    
    /// Watch for configuration changes
    pub async fn start_config_watcher(&self) {
        let mut last_modified = self.get_file_modified_time().await;
        
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            match self.check_for_changes(&mut last_modified).await {
                Ok(Some(_)) => {
                    info!("Configuration file changed, reloading...");
                    match self.reload_config().await {
                        Ok(result) => {
                            if result.success {
                                info!("Configuration successfully reloaded");
                            } else {
                                warn!("Configuration requires restart: {:?}", result.changes_applied);
                            }
                        }
                        Err(e) => {
                            error!("Failed to reload configuration: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    // No changes
                }
                Err(e) => {
                    error!("Error checking configuration changes: {}", e);
                }
            }
        }
    }
    
    /// Validate configuration without applying
    pub fn validate_config(&self, config: &SanakirjaS3Config) -> ValidationResult {
        let mut validation = ValidationResult::new();
        
        // Check system resources
        let available_memory = self.get_available_memory();
        if config.storage.memory_limit_mb > available_memory {
            validation.add_warning(
                ValidationError {
                    field: "storage.memory_limit_mb".to_string(),
                    value: config.storage.memory_limit_mb.to_string(),
                    message: format!("Memory limit ({}) exceeds available system memory ({})", 
                                    config.storage.memory_limit_mb, available_memory),
                    severity: ValidationSeverity::Warning,
                }
            );
        }
        
        // Check S3 connectivity
        if let Err(e) = self.test_s3_connectivity(&config.s3) {
            validation.add_error(
                ValidationError {
                    field: "s3".to_string(),
                    value: config.s3.bucket.clone(),
                    message: format!("S3 connectivity test failed: {}", e),
                    severity: ValidationSeverity::Error,
                }
            );
        }
        
        // Check performance trade-offs
        if config.performance.max_concurrent_writes > config.performance.max_concurrent_reads / 2 {
            validation.add_warning(
                ValidationError {
                    field: "performance.max_concurrent_writes".to_string(),
                    value: config.performance.max_concurrent_writes.to_string(),
                    message: "High write-to-read ratio may impact read performance".to_string(),
                    severity: ValidationSeverity::Warning,
                }
            );
        }
        
        validation
    }
}

// Configuration change tracking
#[derive(Debug, Clone)]
pub struct ConfigChangeAnalysis {
    pub changes: Vec<ConfigChange>,
    pub impact_level: ImpactLevel,
    pub validation_result: ValidationResult,
}

#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub field_path: String,
    pub field_type: ConfigFieldType,
    pub old_value: String,
    pub new_value: String,
    pub hot_reloadable: bool,
}

// Configuration validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
    pub infos: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub value: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationSeverity {
    Error,      // Configuration invalid
    Warning,    // Potentially problematic
    Info,       // Informational
}

pub enum ConfigFieldType {
    Storage,
    Performance,
    S3,
    Forking,
    Gc,
    Cache,
    Monitoring,
    Unknown,
}

pub enum ImpactLevel {
    None,           // No functional change
    Minor,          // Minor behavior change
    Moderate,       // Noticeable behavior change
    Major,          // Significant behavior change
    Breaking,       // API change, requires restart
}
```

---

## Implementation Roadmap

### Phase 1: Core Infrastructure (4-6 weeks)

**Week 1-2: Segment Management Foundation**
- Implement `PageOffset` and `SegmentId` types
- Create 64MB segment allocation system  
- Build segment lifecycle management
- Add compression support (zstd)
- Basic S3 client integration

**Week 3-4: Fork Metadata System**
- Design and implement `ForkMetadata` structure
- Build fingerprinting system for segment state
- Create fork tree data structure
- Implement O(1) fork creation
- Add fork name resolution

**Week 5-6: Basic S3 Integration**
- Multi-part upload/download
- Basic compression/decompression
- Error handling and retries
- Local caching foundation
- Performance monitoring setup

### Phase 2: Fork Management (6-8 weeks)

**Week 7-9: Lazy Materialization**
- Implement materialization detection logic
- Build background worker system
- Create segment copy-on-write mechanism
- Add prefetching infrastructure
- Performance optimization

**Week 10-12: Fork Merging and Conflict Resolution**
- Implement merge strategies (child wins, parent wins, page-level)
- Build three-way merge with common ancestors
- Add conflict resolution framework
- Create merge validation system
- Optimize merge performance

**Week 13-14: Fork Tree Management**
- Fork hierarchy visualization
- Fork lifecycle management
- Batch fork operations
- Fork cleanup and maintenance
- Fork performance analytics

### Phase 3: S3 Optimization (4-6 weeks)

**Week 15-17: Advanced S3 Features**
- Intelligent key organization and versioning
- S3 lifecycle policy automation
- Tiered storage management (standard, IA, Glacier)
- Cost optimization algorithms
- Multi-region replication

**Week 18-20: Performance Optimization**
- Parallel download/upload optimization
- Adaptive chunking strategies
- Connection pooling and keep-alive
- Network error resilience
- Bandwidth throttling

### Phase 4: Advanced Features (4-6 weeks)

**Week 21-23: Garbage Collection**
- Generational garbage collection
- Multi-phase cleanup system
- S3 cleanup automation
- Memory pressure handling
- GC performance tuning

**Week 24-26: Caching and Optimization**
- Multi-tier caching system
- Access pattern learning
- Adaptive cache policies
- Prefetching optimization
- Memory management tuning

### Phase 5: Production Readiness (2-4 weeks)

**Week 27-28: Monitoring and Observability**
- Comprehensive metrics collection
- Prometheus integration
- Distributed tracing
- Alerting and health checks
- Performance analytics dashboards

**Week 29-30: Configuration and Deployment**
- Configuration management system
- Environment-specific defaults
- Hot-reload capabilities
- Deployment automation
- Documentation completion

### Testing Strategy

**Unit Tests (Ongoing)**
- Component isolation testing
- Property-based testing for critical algorithms
- Mock S3 for unit tests
- Edge case validation

**Integration Tests (Phase 2+)**
- End-to-end fork lifecycle testing
- S3 integration testing with real and mock services
- Concurrency and race condition testing
- Performance regression testing

**Performance Testing (Phase 3+)**
- Load testing for S3 operations
- Fork creation performance benchmarks
- Memory usage profiling
- GC efficiency testing

**Stress Testing (Phase 4+)**
- Large-scale fork scenario testing
- S3 failure simulation
- Memory pressure testing
- Network partition testing

### Risk Mitigation

**Technical Risks**
- **S3 performance**: Implement fallback caching strategies
- **Memory growth**: Aggressive GC and memory pressure handling
- **Fork explosion**: Implement usage limits and automatic cleanup
- **Data consistency**: Strong validation and checksum systems

**Operational Risks**  
- **S3 costs**: Intelligent cost monitoring and alerts
- **Backup/recovery**: Robust backup and disaster recovery procedures
- **Monitoring gaps**: Comprehensive observability from day one
- **Complexity**: Incremental rollout and feature flags

---

## Conclusion

This architecture design provides a comprehensive solution for unlimited, scalable forking with S3-backed storage. Key advantages include:

✅ **Unlimited forking** with O(1) fork creation cost  
✅ **Cloud-native storage** optimized for S3 performance and cost  
✅ **Intelligent caching** with adaptive multi-tier management  
✅ **Robust garbage collection** with generational cleanup  
✅ **Production-ready monitoring** and observability  
✅ **Configurable performance** for different workloads  
✅ **Comprehensive backup and disaster recovery**

The phased implementation approach allows for incremental delivery while building toward a production-ready, enterprise-scale solution.

---

## Appendices

### A. Performance Benchmarks (Target)
- Fork creation: < 1ms (metadata only)
- Page read (cache hit): < 100μs  
- Page read (cache miss, S3): < 50ms
- File segment upload: 64MB in < 2 seconds
- Fork merge: < 1 second per dirty segment

### B. Cost Estimation
- S3 storage: $0.023/GB/month (Standard)
- S3 requests: $0.005/1000 GET, $0.005/1000 PUT
- Memory: 64MB per segment allocation
- Local cache: $0.10/GB/month (EBS)

### C. Compatibility Considerations
- Backward compatibility with existing Sanakirja API
- Database migration path from existing deployments
- AWS region and account requirements  
- Network latency considerations for S3 access

### D. Security Considerations
- S3 encryption at rest and in transit
- IAM roles and access policies
- Segment encryption optionality
- Fork access control and permissions