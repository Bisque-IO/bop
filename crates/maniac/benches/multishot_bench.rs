use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::OnceLock;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Wake};
use std::thread;

fn create_dummy_context() -> Context<'static> {
    static WAKER: OnceLock<std::task::Waker> = OnceLock::new();

    let waker = WAKER.get_or_init(|| {
        unsafe fn clone(_data: *const ()) -> RawWaker {
            raw_waker()
        }
        unsafe fn wake(_data: *const ()) {}
        unsafe fn wake_by_ref(_data: *const ()) {}
        unsafe fn drop(_data: *const ()) {}

        fn raw_waker() -> RawWaker {
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
            RawWaker::new(std::ptr::null(), &VTABLE)
        }

        unsafe { std::task::Waker::from_raw(raw_waker()) }
    });

    Context::from_waker(waker)
}

// Benchmark 1: Single-threaded send and immediate receive
fn bench_single_threaded_send_recv(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_threaded_send_recv");

    // Multishot benchmark
    group.bench_function("multishot", |b| {
        b.iter(|| {
            let (s, mut r) = maniac::sync::multishot::channel::<i32>();
            s.send(42);
            let mut fut = r.recv();
            let mut fut = std::pin::Pin::new(&mut fut);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    // Tokio oneshot benchmark
    group.bench_function("tokio_oneshot", |b| {
        b.iter(|| {
            let (s, r) = tokio::sync::oneshot::channel::<i32>();
            let _ = s.send(42);
            let mut fut = Box::pin(r);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    group.finish();
}

// Benchmark 2: Send from another thread
fn bench_cross_thread_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_thread_send");

    // Multishot benchmark
    group.bench_function("multishot", |b| {
        b.iter(|| {
            let (s, mut r) = maniac::sync::multishot::channel::<i32>();
            let mut fut = r.recv();
            let mut fut = std::pin::Pin::new(&mut fut);
            let mut cx = create_dummy_context();
            // Poll once to register waker
            let _ = fut.as_mut().poll(&mut cx);

            let handle = thread::spawn(move || {
                s.send(42);
            });
            handle.join().unwrap();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    // Tokio oneshot benchmark
    group.bench_function("tokio_oneshot", |b| {
        b.iter(|| {
            let (s, r) = tokio::sync::oneshot::channel::<i32>();
            let mut fut = Box::pin(r);
            let mut cx = create_dummy_context();
            // Poll once to register waker
            let _ = fut.as_mut().poll(&mut cx);

            let handle = thread::spawn(move || {
                let _ = s.send(42);
            });
            handle.join().unwrap();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    group.finish();
}

// Benchmark 3: Reuse (multishot's key feature)
fn bench_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("reuse");

    // Multishot: create once, reuse multiple times
    for &iterations in &[10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("multishot", iterations),
            &iterations,
            |b, &iterations| {
                b.iter(|| {
                    let (s, mut r) = maniac::sync::multishot::channel::<i32>();
                    let mut current_sender = Some(s);
                    for i in 0..iterations {
                        let sender = current_sender.take().unwrap();
                        sender.send(i);
                        let mut fut = r.recv();
                        let mut fut = std::pin::Pin::new(&mut fut);
                        let mut cx = create_dummy_context();
                        match fut.as_mut().poll(&mut cx) {
                            Poll::Ready(Ok(value)) => {
                                black_box(value);
                                // Get new sender for next iteration
                                if i + 1 < iterations {
                                    current_sender = Some(r.sender().expect("should get sender"));
                                }
                            }
                            _ => panic!("Expected ready value"),
                        }
                    }
                })
            },
        );

        // Tokio oneshot: create new channel each time
        group.bench_with_input(
            BenchmarkId::new("tokio_oneshot", iterations),
            &iterations,
            |b, &iterations| {
                b.iter(|| {
                    for i in 0..iterations {
                        let (s, r) = tokio::sync::oneshot::channel::<i32>();
                        let _ = s.send(i);
                        let mut fut = Box::pin(r);
                        let mut cx = create_dummy_context();
                        match fut.as_mut().poll(&mut cx) {
                            Poll::Ready(Ok(value)) => {
                                black_box(value);
                            }
                            _ => panic!("Expected ready value"),
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

// Benchmark 4: Drop sender without sending
fn bench_drop_without_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("drop_without_send");

    // Multishot benchmark
    group.bench_function("multishot", |b| {
        b.iter(|| {
            let (s, mut r) = maniac::sync::multishot::channel::<i32>();
            let mut fut = r.recv();
            let mut fut = std::pin::Pin::new(&mut fut);
            let mut cx = create_dummy_context();
            // Poll once to register waker
            let _ = fut.as_mut().poll(&mut cx);

            drop(s);
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Err(_)) => {
                    black_box(());
                }
                _ => panic!("Expected error"),
            }
        })
    });

    // Tokio oneshot benchmark
    group.bench_function("tokio_oneshot", |b| {
        b.iter(|| {
            let (s, r) = tokio::sync::oneshot::channel::<i32>();
            let mut fut = Box::pin(r);
            let mut cx = create_dummy_context();
            // Poll once to register waker
            let _ = fut.as_mut().poll(&mut cx);

            drop(s);
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Err(_)) => {
                    black_box(());
                }
                _ => panic!("Expected error"),
            }
        })
    });

    group.finish();
}

// Benchmark 5: Sender recycling performance
fn bench_sender_recycling(c: &mut Criterion) {
    let mut group = c.benchmark_group("sender_recycling");

    // Measure the cost of getting a new sender after receiving a value
    group.bench_function("multishot_recycle_sender", |b| {
        b.iter(|| {
            let (s, mut r) = maniac::sync::multishot::channel::<i32>();
            s.send(42);
            let mut fut = r.recv();
            let mut fut = std::pin::Pin::new(&mut fut);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                    // Now recycle sender
                    let new_sender = r.sender().expect("should get sender");
                    black_box(new_sender);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    // For comparison, measure the cost of creating a new tokio oneshot channel
    group.bench_function("tokio_oneshot_new_channel", |b| {
        b.iter(|| {
            let (s, r) = tokio::sync::oneshot::channel::<i32>();
            let _ = s.send(42);
            let mut fut = Box::pin(r);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(value)) => {
                    black_box(value);
                    // Create new channel for next message (no recycling)
                    let (new_s, _new_r) = tokio::sync::oneshot::channel::<i32>();
                    black_box(new_s);
                }
                _ => panic!("Expected ready value"),
            }
        })
    });

    // Benchmark repeated recycling
    for &iterations in &[10, 100] {
        group.bench_with_input(
            BenchmarkId::new("multishot_repeated_recycling", iterations),
            &iterations,
            |b, &iterations| {
                b.iter(|| {
                    let (s, mut r) = maniac::sync::multishot::channel::<i32>();
                    let mut current_sender = Some(s);
                    for i in 0..iterations {
                        let sender = current_sender.take().unwrap();
                        sender.send(i);
                        let mut fut = r.recv();
                        let mut fut = std::pin::Pin::new(&mut fut);
                        let mut cx = create_dummy_context();
                        match fut.as_mut().poll(&mut cx) {
                            Poll::Ready(Ok(value)) => {
                                black_box(value);
                                if i + 1 < iterations {
                                    current_sender = Some(r.sender().expect("should get sender"));
                                }
                            }
                            _ => panic!("Expected ready value"),
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

// Benchmark 6: Waker registration overhead
fn bench_waker_registration(c: &mut Criterion) {
    let mut group = c.benchmark_group("waker_registration");

    // Multishot: poll without value ready (register waker)
    group.bench_function("multishot_register_waker", |b| {
        b.iter(|| {
            let (_s, mut r) = maniac::sync::multishot::channel::<i32>();
            let mut fut = r.recv();
            let mut fut = std::pin::Pin::new(&mut fut);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Pending => {
                    black_box(());
                }
                _ => panic!("Expected pending"),
            }
        })
    });

    // Tokio oneshot: poll without value ready (register waker)
    group.bench_function("tokio_oneshot_register_waker", |b| {
        b.iter(|| {
            let (_s, r) = tokio::sync::oneshot::channel::<i32>();
            let mut fut = Box::pin(r);
            let mut cx = create_dummy_context();
            match fut.as_mut().poll(&mut cx) {
                Poll::Pending => {
                    black_box(());
                }
                _ => panic!("Expected pending"),
            }
        })
    });

    group.finish();
}

// Benchmark 7: Memory allocation patterns
fn bench_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation");

    // Measure creating many channels
    for &count in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("multishot_create", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    for _ in 0..count {
                        let (_s, _r) = maniac::sync::multishot::channel::<i32>();
                        black_box(());
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tokio_oneshot_create", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    for _ in 0..count {
                        let (_s, _r) = tokio::sync::oneshot::channel::<i32>();
                        black_box(());
                    }
                })
            },
        );
    }

    group.finish();
}

// Benchmark 8: Contention - multiple threads trying to send to different channels
fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");

    for &num_threads in &[1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("multishot_throughput", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let mut handles = Vec::new();
                    for _ in 0..num_threads {
                        handles.push(thread::spawn(|| {
                            let (s, mut r) = maniac::sync::multishot::channel::<i32>();
                            s.send(42);
                            let mut fut = r.recv();
                            let mut fut = std::pin::Pin::new(&mut fut);
                            let mut cx = create_dummy_context();
                            match fut.as_mut().poll(&mut cx) {
                                Poll::Ready(Ok(value)) => black_box(value),
                                _ => panic!("Expected ready value"),
                            }
                        }));
                    }
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tokio_oneshot_throughput", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let mut handles = Vec::new();
                    for _ in 0..num_threads {
                        handles.push(thread::spawn(|| {
                            let (s, r) = tokio::sync::oneshot::channel::<i32>();
                            let _ = s.send(42);
                            let mut fut = Box::pin(r);
                            let mut cx = create_dummy_context();
                            match fut.as_mut().poll(&mut cx) {
                                Poll::Ready(Ok(value)) => black_box(value),
                                _ => panic!("Expected ready value"),
                            }
                        }));
                    }
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(100);
    targets =
        bench_single_threaded_send_recv,
        bench_cross_thread_send,
        bench_reuse,
        bench_drop_without_send,
        bench_sender_recycling,
        bench_waker_registration,
        bench_allocation,
        bench_contention
);

criterion_main!(benches);
