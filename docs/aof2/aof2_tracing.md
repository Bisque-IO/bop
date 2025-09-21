# AOF2 Tracing Quickstart

AOF2 emits structured tracing events around segment lifecycle, flush scheduling, and recovery. This makes it straightforward to stream telemetry into log sinks or metrics backends without touching internal state.

## Instrumentation Overview

The runtime publishes the following high-signal events:

- bop_rs::aof2::mod: segment allocation, sealing, archival queueing, and flush scheduling decisions (fields include segment_id, pending_remaining, requested, durable, and lag_ms).
- bop_rs::aof2::flush: retry attempts, synchronous fallback usage, and queue backpressure notifications (attempts, delay_ms, error).
- bop_rs::aof2::reader: tail follower lifecycle updates (event, segment_id).

Those events log at debug when routine and escalate to warn when disk or system issues push the retry loop or block the tail.

## Sample Subscriber

The examples/aof_tracing.rs program installs a tracing_subscriber pipeline that lifts AOF2 modules to debug verbosity, spawns a tail follower, and periodically exports the flush metrics gauge via structured log records:

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().compact())
        .with(EnvFilter::new("info,bop_rs::aof2=debug"))
        .init();

    let rt = Arc::new(Runtime::new()?);
    let aof = Arc::new(Aof::new(rt.clone(), cfg)?);

    rt.spawn(async move {
        while let Some(segment) = tail.next_sealed_segment(&mut follower).await {
            info!(segment = segment.id().as_u64(), "sealed segment ready");
        }
    });

    FlushMetricsExporter::new(aof.flush_metrics()).emit(|sample| {
        info!(metric = sample.name, value = sample.value, "flush metric sample");
    });

Run it locally to see sealing and flush activity in the console:

    cargo run --example aof_tracing

Set RUST_LOG to tighten or widen the feed:

    RUST_LOG=warn,bop_rs::aof2=trace cargo run --example aof_tracing

## Shipping Metrics

When integrating with a metrics registry, reuse the FlushMetricsExporter helper so counters stay consistent with the runtime:

    FlushMetricsExporter::new(aof.flush_metrics()).emit(|sample| {
        recorder.gauge(sample.name).set(sample.value as f64);
    });

Similarly, subscribing to Aof::tail_events() gives a cheap way to detect rollover lag:

    let mut rx = aof.tail_events();
    rx.borrow_and_update(); // initial state
    while rx.changed().await.is_ok() {
        let state = rx.borrow().clone();
        metrics::gauge!("aof_tail_pending_segments", state.pending.len() as f64);
    }

These hooks cover the observability contract for milestone OB2: operators can attach a subscriber, stream structured events, and convert the built-in metrics snapshot into their monitoring stack without additional plumbing.
