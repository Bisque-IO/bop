//! MPSC Channel Benchmark
//!
//! Benchmarks comparing different async MPSC channel implementations:
//! - maniac::sync::mpsc::simple (tachyonix-based)
//! - crossfire
//! - tokio::sync::mpsc
//! - kanal
//! - flume
//!
//! Runs on both tokio and maniac runtimes for comparison.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MESSAGES: u64 = 1_000_000;
const CAPACITY: usize = 4096;
const WARMUP_MESSAGES: u64 = 10_000;

#[derive(Clone, Copy, Debug)]
struct Payload {
    value: u64,
    // _padding: [u8; 56], // 64 bytes total
}

impl Payload {
    fn new(value: u64) -> Self {
        Self {
            value,
            // _padding: [0u8; 56],
        }
    }
}

struct BenchResult {
    name: &'static str,
    duration: Duration,
    messages: u64,
}

impl BenchResult {
    fn print(&self) {
        let throughput = self.messages as f64 / self.duration.as_secs_f64();
        let latency_ns = self.duration.as_nanos() as f64 / self.messages as f64;
        println!(
            "{:30} {:>12.0} msg/s  {:>8.1} ns/msg  {:>8.2} ms total",
            self.name,
            throughput,
            latency_ns,
            self.duration.as_secs_f64() * 1000.0
        );
    }
}

const NUM_PRODUCERS: usize = 2;
const MESSAGES_PER_PRODUCER: u64 = MESSAGES / NUM_PRODUCERS as u64;

// ============================================================================
// Tokio Runtime Benchmarks
// ============================================================================

mod tokio_benches {
    use super::*;

    pub async fn bench_maniac_spsc_dynamic() -> BenchResult {
        use maniac::sync::signal::{AsyncSignalGate, AsyncSignalWaker, Signal};
        use maniac::sync::spsc::dynamic::{DynSpscConfig, new};

        // Create signal infrastructure
        let waker = Arc::new(AsyncSignalWaker::new());
        let signal = Signal::with_index(0);
        let gate = AsyncSignalGate::new(0, signal, waker);

        // Configure: 2^10 = 1024 items/segment, 2^2 = 4 segments
        let config = DynSpscConfig::new(10, 2);
        let (mut tx, mut rx) = new::<Payload>(config, gate);

        // Warmup
        for i in 0..WARMUP_MESSAGES {
            tx.send(Payload::new(i)).await.unwrap();
            rx.recv().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "maniac spsc dynamic checksum mismatch");

        BenchResult {
            name: "[tokio] maniac-spsc-dynamic",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_maniac_spsc() -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<Payload>(CAPACITY);

        // Warmup
        for i in 0..WARMUP_MESSAGES {
            tx.send(Payload::new(i)).await.unwrap();
            rx.recv().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "maniac checksum mismatch");

        BenchResult {
            name: "[tokio] maniac",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_crossfire_spsc() -> BenchResult {
        use crossfire::mpsc;

        let (tx, rx) = mpsc::bounded_async::<Payload>(CAPACITY);

        for i in 0..WARMUP_MESSAGES {
            tx.send(Payload::new(i)).await.unwrap();
            rx.recv().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "crossfire checksum mismatch");

        BenchResult {
            name: "[tokio] crossfire",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_tokio_spsc() -> BenchResult {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Payload>(CAPACITY);

        for i in 0..WARMUP_MESSAGES {
            tx.send(Payload::new(i)).await.unwrap();
            rx.recv().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "tokio checksum mismatch");

        BenchResult {
            name: "[tokio] tokio",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_kanal_spsc() -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<Payload>(CAPACITY);

        for i in 0..WARMUP_MESSAGES {
            tx.send(Payload::new(i)).await.unwrap();
            rx.recv().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "kanal checksum mismatch");

        BenchResult {
            name: "[tokio] kanal",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_flume_spsc() -> BenchResult {
        let (tx, rx) = flume::bounded::<Payload>(CAPACITY);

        for i in 0..WARMUP_MESSAGES {
            tx.send_async(Payload::new(i)).await.unwrap();
            rx.recv_async().await.unwrap();
        }

        let start = Instant::now();

        let sender = tokio::spawn(async move {
            for i in 0..MESSAGES {
                tx.send_async(Payload::new(i)).await.unwrap();
            }
        });

        let receiver = tokio::spawn(async move {
            let mut sum = 0u64;
            for _ in 0..MESSAGES {
                sum += rx.recv_async().await.unwrap().value;
            }
            sum
        });

        sender.await.unwrap();
        let sum = receiver.await.unwrap();
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "flume checksum mismatch");

        BenchResult {
            name: "[tokio] flume",
            duration,
            messages: MESSAGES,
        }
    }

    // MPSC benchmarks
    pub async fn bench_maniac_mpsc() -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut senders = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            senders.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send(Payload::new(base + i)).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut count = 0u64;
            for _ in 0..MESSAGES {
                rx.recv().await.unwrap();
                count += 1;
            }
            count
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let count = receiver.await.unwrap();
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "maniac mpsc count mismatch");

        BenchResult {
            name: "[tokio] maniac MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_crossfire_mpsc() -> BenchResult {
        use crossfire::mpsc;

        let (tx, rx) = mpsc::bounded_async::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut senders = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            senders.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send(Payload::new(base + i)).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut count = 0u64;
            for _ in 0..MESSAGES {
                rx.recv().await.unwrap();
                count += 1;
            }
            count
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let count = receiver.await.unwrap();
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "crossfire mpsc count mismatch");

        BenchResult {
            name: "[tokio] crossfire MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_tokio_mpsc() -> BenchResult {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut senders = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            senders.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send(Payload::new(base + i)).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut count = 0u64;
            for _ in 0..MESSAGES {
                rx.recv().await.unwrap();
                count += 1;
            }
            count
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let count = receiver.await.unwrap();
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "tokio mpsc count mismatch");

        BenchResult {
            name: "[tokio] tokio MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_kanal_mpsc() -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut senders = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            senders.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send(Payload::new(base + i)).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut count = 0u64;
            for _ in 0..MESSAGES {
                rx.recv().await.unwrap();
                count += 1;
            }
            count
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let count = receiver.await.unwrap();
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "kanal mpsc count mismatch");

        BenchResult {
            name: "[tokio] kanal MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub async fn bench_flume_mpsc() -> BenchResult {
        let (tx, rx) = flume::bounded::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut senders = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            senders.push(tokio::spawn(async move {
                for i in 0..MESSAGES_PER_PRODUCER {
                    tx.send_async(Payload::new(base + i)).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut count = 0u64;
            for _ in 0..MESSAGES {
                rx.recv_async().await.unwrap();
                count += 1;
            }
            count
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let count = receiver.await.unwrap();
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "flume mpsc count mismatch");

        BenchResult {
            name: "[tokio] flume MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    // Contention benchmarks
    pub async fn bench_contention_maniac() -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<u64>(64);
        let counter = Arc::new(AtomicU64::new(0));

        let start = Instant::now();

        let mut senders = Vec::new();
        for _ in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let counter = counter.clone();
            senders.push(tokio::spawn(async move {
                loop {
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    if n >= MESSAGES {
                        break;
                    }
                    tx.send(n).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut received = 0u64;
            while rx.recv().await.is_ok() {
                received += 1;
            }
            received
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let received = receiver.await.unwrap();
        let duration = start.elapsed();

        BenchResult {
            name: "[tokio] maniac contention",
            duration,
            messages: received,
        }
    }

    pub async fn bench_contention_kanal() -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<u64>(64);
        let counter = Arc::new(AtomicU64::new(0));

        let start = Instant::now();

        let mut senders = Vec::new();
        for _ in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let counter = counter.clone();
            senders.push(tokio::spawn(async move {
                loop {
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    if n >= MESSAGES {
                        break;
                    }
                    tx.send(n).await.unwrap();
                }
            }));
        }
        drop(tx);

        let receiver = tokio::spawn(async move {
            let mut received = 0u64;
            while rx.recv().await.is_ok() {
                received += 1;
            }
            received
        });

        for sender in senders {
            sender.await.unwrap();
        }
        let received = receiver.await.unwrap();
        let duration = start.elapsed();

        BenchResult {
            name: "[tokio] kanal contention",
            duration,
            messages: received,
        }
    }
}

// ============================================================================
// Maniac Runtime Benchmarks
// ============================================================================

mod maniac_benches {
    use super::*;
    use maniac::runtime::Runtime;

    pub fn bench_maniac_spsc_dynamic(rt: &Runtime) -> BenchResult {
        use maniac::sync::signal::{AsyncSignalGate, AsyncSignalWaker, Signal};
        use maniac::sync::spsc::new_async_with_config;

        // Create signal infrastructure
        let waker = Arc::new(AsyncSignalWaker::new());
        let signal = Signal::with_index(0);
        let gate = AsyncSignalGate::new(0, signal, waker);

        // Configure: 2^10 = 1024 items/segment, 2^2 = 4 segments
        let (mut tx, mut rx) = new_async_with_config::<Payload, 10, 2>(gate, 16);
        // tx.send(Payload::new(1)).await.unwrap();
        // rx.recv().await.unwrap();

        // Warmup - run sequentially since we can't spawn yet
        // maniac::future::block_on(async {
        //     for i in 0..WARMUP_MESSAGES {
        //         tx.send(Payload::new(i)).await.unwrap();
        //         rx.recv().await.unwrap();
        //     }
        // });

        let start = Instant::now();

        let sender_handle = rt
            .spawn(async move {
                for i in 0..MESSAGES {
                    tx.send(Payload::new(i)).await.unwrap();
                }
            })
            .unwrap();

        let receiver_handle = rt
            .spawn(async move {
                let mut sum = 0u64;
                for _ in 0..MESSAGES {
                    sum += rx.recv().await.unwrap().value;
                }
                sum
            })
            .unwrap();

        maniac::future::block_on(sender_handle);
        let sum = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "maniac spsc dynamic checksum mismatch");

        BenchResult {
            name: "[maniac] maniac-spsc-dynamic",
            duration,
            messages: MESSAGES,
        }
    }

    // pub fn bench_maniac_spsc_dynamic(rt: &Runtime) -> BenchResult {
    //     use maniac::sync::signal::{AsyncSignalGate, AsyncSignalWaker, Signal};
    //     use maniac::sync::spsc::dynamic::{DynSpscConfig, new};

    //     // Create signal infrastructure
    //     let waker = Arc::new(AsyncSignalWaker::new());
    //     let signal = Signal::with_index(0);
    //     let gate = AsyncSignalGate::new(0, signal, waker);

    //     // Configure: 2^10 = 1024 items/segment, 2^2 = 4 segments
    //     let config = DynSpscConfig::new(7, 1);
    //     let (mut tx, mut rx) = new::<Payload>(config, gate);

    //     // Warmup - run sequentially since we can't spawn yet
    //     // maniac::future::block_on(async {
    //     //     for i in 0..WARMUP_MESSAGES {
    //     //         tx.send(Payload::new(i)).await.unwrap();
    //     //         rx.recv().await.unwrap();
    //     //     }
    //     // });

    //     let start = Instant::now();

    //     let sender_handle = rt
    //         .spawn(async move {
    //             for i in 0..MESSAGES {
    //                 tx.send(Payload::new(i)).await.unwrap();
    //             }
    //         })
    //         .unwrap();

    //     let receiver_handle = rt
    //         .spawn(async move {
    //             let mut sum = 0u64;
    //             for _ in 0..MESSAGES {
    //                 sum += rx.recv().await.unwrap().value;
    //             }
    //             sum
    //         })
    //         .unwrap();

    //     maniac::future::block_on(sender_handle);
    //     let sum = maniac::future::block_on(receiver_handle);
    //     let duration = start.elapsed();

    //     let expected = (0..MESSAGES).sum::<u64>();
    //     assert_eq!(sum, expected, "maniac spsc dynamic checksum mismatch");

    //     BenchResult {
    //         name: "[maniac] maniac-spsc-dynamic",
    //         duration,
    //         messages: MESSAGES,
    //     }
    // }

    pub fn bench_maniac_spsc(rt: &Runtime) -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<Payload>(CAPACITY);

        // Warmup - run sequentially since we can't spawn yet
        maniac::future::block_on(async {
            for i in 0..WARMUP_MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
                rx.recv().await.unwrap();
            }
        });

        let start = Instant::now();

        let sender_handle = rt
            .spawn(async move {
                for i in 0..MESSAGES {
                    tx.send(Payload::new(i)).await.unwrap();
                }
            })
            .unwrap();

        let receiver_handle = rt
            .spawn(async move {
                let mut sum = 0u64;
                for _ in 0..MESSAGES {
                    sum += rx.recv().await.unwrap().value;
                }
                sum
            })
            .unwrap();

        maniac::future::block_on(sender_handle);
        let sum = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "maniac checksum mismatch");

        BenchResult {
            name: "[maniac] maniac",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_crossfire_spsc(rt: &Runtime) -> BenchResult {
        use crossfire::mpsc;

        let (tx, rx) = mpsc::bounded_async::<Payload>(CAPACITY);

        maniac::future::block_on(async {
            for i in 0..WARMUP_MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
                rx.recv().await.unwrap();
            }
        });

        let start = Instant::now();

        let sender_handle = rt
            .spawn(async move {
                for i in 0..MESSAGES {
                    tx.send(Payload::new(i)).await.unwrap();
                }
            })
            .unwrap();

        let receiver_handle = rt
            .spawn(async move {
                let mut sum = 0u64;
                for _ in 0..MESSAGES {
                    sum += rx.recv().await.unwrap().value;
                }
                sum
            })
            .unwrap();

        maniac::future::block_on(sender_handle);
        let sum = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "crossfire checksum mismatch");

        BenchResult {
            name: "[maniac] crossfire",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_kanal_spsc(rt: &Runtime) -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<Payload>(CAPACITY);

        maniac::future::block_on(async {
            for i in 0..WARMUP_MESSAGES {
                tx.send(Payload::new(i)).await.unwrap();
                rx.recv().await.unwrap();
            }
        });

        let start = Instant::now();

        let sender_handle = rt
            .spawn(async move {
                for i in 0..MESSAGES {
                    tx.send(Payload::new(i)).await.unwrap();
                }
            })
            .unwrap();

        let receiver_handle = rt
            .spawn(async move {
                let mut sum = 0u64;
                for _ in 0..MESSAGES {
                    sum += rx.recv().await.unwrap().value;
                }
                sum
            })
            .unwrap();

        maniac::future::block_on(sender_handle);
        let sum = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "kanal checksum mismatch");

        BenchResult {
            name: "[maniac] kanal",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_flume_spsc(rt: &Runtime) -> BenchResult {
        let (tx, rx) = flume::bounded::<Payload>(CAPACITY);

        maniac::future::block_on(async {
            for i in 0..WARMUP_MESSAGES {
                tx.send_async(Payload::new(i)).await.unwrap();
                rx.recv_async().await.unwrap();
            }
        });

        let start = Instant::now();

        let sender_handle = rt
            .spawn(async move {
                for i in 0..MESSAGES {
                    tx.send_async(Payload::new(i)).await.unwrap();
                }
            })
            .unwrap();

        let receiver_handle = rt
            .spawn(async move {
                let mut sum = 0u64;
                for _ in 0..MESSAGES {
                    sum += rx.recv_async().await.unwrap().value;
                }
                sum
            })
            .unwrap();

        maniac::future::block_on(sender_handle);
        let sum = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        let expected = (0..MESSAGES).sum::<u64>();
        assert_eq!(sum, expected, "flume checksum mismatch");

        BenchResult {
            name: "[maniac] flume",
            duration,
            messages: MESSAGES,
        }
    }

    // MPSC benchmarks
    pub fn bench_maniac_mpsc(rt: &Runtime) -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            sender_handles.push(
                rt.spawn(async move {
                    for i in 0..MESSAGES_PER_PRODUCER {
                        tx.send(Payload::new(base + i)).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut count = 0u64;
                for _ in 0..MESSAGES {
                    rx.recv().await.unwrap();
                    count += 1;
                }
                count
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let count = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "maniac mpsc count mismatch");

        BenchResult {
            name: "[maniac] maniac MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_crossfire_mpsc(rt: &Runtime) -> BenchResult {
        use crossfire::mpsc;

        let (tx, rx) = mpsc::bounded_async::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            sender_handles.push(
                rt.spawn(async move {
                    for i in 0..MESSAGES_PER_PRODUCER {
                        tx.send(Payload::new(base + i)).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut count = 0u64;
                for _ in 0..MESSAGES {
                    while !rx.recv().await.is_ok() {
                        // tokio::task::yield_now().await;
                    }
                    count += 1;
                }
                count
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let count = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "crossfire mpsc count mismatch");

        BenchResult {
            name: "[maniac] crossfire MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_kanal_mpsc(rt: &Runtime) -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            sender_handles.push(
                rt.spawn(async move {
                    for i in 0..MESSAGES_PER_PRODUCER {
                        tx.send(Payload::new(base + i)).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut count = 0u64;
                for _ in 0..MESSAGES {
                    rx.recv().await.unwrap();
                    count += 1;
                }
                count
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let count = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "kanal mpsc count mismatch");

        BenchResult {
            name: "[maniac] kanal MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_flume_mpsc(rt: &Runtime) -> BenchResult {
        let (tx, rx) = flume::bounded::<Payload>(CAPACITY);

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            sender_handles.push(
                rt.spawn(async move {
                    for i in 0..MESSAGES_PER_PRODUCER {
                        tx.send_async(Payload::new(base + i)).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut count = 0u64;
                for _ in 0..MESSAGES {
                    rx.recv_async().await.unwrap();
                    count += 1;
                }
                count
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let count = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "flume mpsc count mismatch");

        BenchResult {
            name: "[maniac] flume MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    pub fn bench_contention_crossfire_mpsc(rt: &Runtime) -> BenchResult {
        use crossfire::mpsc;

        let (tx, rx) = mpsc::bounded_async::<Payload>(64);

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for p in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let base = p as u64 * MESSAGES_PER_PRODUCER;
            sender_handles.push(
                rt.spawn(async move {
                    for i in 0..MESSAGES_PER_PRODUCER {
                        tx.send(Payload::new(base + i)).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut count = 0u64;
                for _ in 0..MESSAGES {
                    rx.recv().await.unwrap();
                    count += 1;
                }
                count
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let count = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        assert_eq!(count, MESSAGES, "crossfire mpsc count mismatch");

        BenchResult {
            name: "[maniac] crossfire MPSC",
            duration,
            messages: MESSAGES,
        }
    }

    // Contention benchmarks
    pub fn bench_contention_maniac(rt: &Runtime) -> BenchResult {
        use maniac::sync::mpsc::simple::channel;

        let (tx, mut rx) = channel::<u64>(64);
        let counter = Arc::new(AtomicU64::new(0));

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for _ in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let counter = counter.clone();
            sender_handles.push(
                rt.spawn(async move {
                    loop {
                        let n = counter.fetch_add(1, Ordering::Relaxed);
                        if n >= MESSAGES {
                            break;
                        }
                        tx.send(n).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut received = 0u64;
                while rx.recv().await.is_ok() {
                    received += 1;
                }
                received
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let received = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        BenchResult {
            name: "[maniac] maniac contention",
            duration,
            messages: received,
        }
    }

    pub fn bench_contention_kanal(rt: &Runtime) -> BenchResult {
        let (tx, rx) = kanal::bounded_async::<u64>(64);
        let counter = Arc::new(AtomicU64::new(0));

        let start = Instant::now();

        let mut sender_handles = Vec::new();
        for _ in 0..NUM_PRODUCERS {
            let tx = tx.clone();
            let counter = counter.clone();
            sender_handles.push(
                rt.spawn(async move {
                    loop {
                        let n = counter.fetch_add(1, Ordering::Relaxed);
                        if n >= MESSAGES {
                            break;
                        }
                        tx.send(n).await.unwrap();
                    }
                })
                .unwrap(),
            );
        }
        drop(tx);

        let receiver_handle = rt
            .spawn(async move {
                let mut received = 0u64;
                while rx.recv().await.is_ok() {
                    received += 1;
                }
                received
            })
            .unwrap();

        for h in sender_handles {
            maniac::future::block_on(h);
        }
        let received = maniac::future::block_on(receiver_handle);
        let duration = start.elapsed();

        BenchResult {
            name: "[maniac] kanal contention",
            duration,
            messages: received,
        }
    }
}

fn main() {
    println!("MPSC Channel Benchmark");
    println!("======================");
    println!("Messages: {}", MESSAGES);
    println!("Capacity: {}", CAPACITY);
    println!("Payload size: {} bytes", std::mem::size_of::<Payload>());
    println!();

    const ITERATIONS: usize = 6;
    const TOKIO: bool = true;
    const MANIAC: bool = true;

    if TOKIO {
        // ========================================================================
        // Tokio Runtime Benchmarks
        // ========================================================================
        println!("=== TOKIO RUNTIME (4 worker threads) ===");
        println!();

        // let maniac_rt = maniac::runtime::new_multi_threaded(1, 4096).unwrap();

        let tokio_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();

        // let tokio_rt = tokio::runtime::Builder::new_current_thread().build().unwrap();

        println!("SPSC (Single Producer, Single Consumer)");
        println!("---------------------------------------");

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_maniac_spsc_dynamic())
                .print();
            // maniac_benches::bench_maniac_spsc_dynamic(&maniac_rt).print();
        }
        println!();

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_maniac_spsc())
                .print();
            // maniac_benches::bench_maniac_spsc(&maniac_rt).print();
        }
        println!();

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_crossfire_spsc())
                .print();
            // maniac_benches::bench_crossfire_spsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     tokio_rt.block_on(tokio_benches::bench_tokio_spsc()).print();
        // }
        // println!();

        for _ in 0..ITERATIONS {
            tokio_rt.block_on(tokio_benches::bench_kanal_spsc()).print();
            // maniac_benches::bench_kanal_spsc(&maniac_rt).print();
        }
        println!();

        for _ in 0..ITERATIONS {
            tokio_rt.block_on(tokio_benches::bench_flume_spsc()).print();
            // maniac_benches::bench_flume_spsc(&maniac_rt).print();
        }
        println!();

        println!("MPSC ({} Producers, Single Consumer)", NUM_PRODUCERS);
        println!("---------------------------------------");

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_maniac_mpsc())
                .print();
            // maniac_benches::bench_maniac_mpsc(&maniac_rt).print();
        }
        println!();

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_crossfire_mpsc())
                .print();
            // maniac_benches::bench_crossfire_mpsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     tokio_rt.block_on(tokio_benches::bench_tokio_mpsc()).print();
        // }
        // println!();

        for _ in 0..ITERATIONS {
            tokio_rt.block_on(tokio_benches::bench_kanal_mpsc()).print();
            // maniac_benches::bench_kanal_mpsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     tokio_rt.block_on(tokio_benches::bench_flume_mpsc()).print();
        //     maniac_benches::bench_flume_mpsc(&maniac_rt).print();
        // }
        // println!();

        println!(
            "Contention Benchmark ({} Producers, small buffer)",
            NUM_PRODUCERS
        );
        println!("--------------------------------------------------");

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_contention_maniac())
                .print();
            // maniac_benches::bench_contention_maniac(&maniac_rt).print();
        }
        println!();

        for _ in 0..ITERATIONS {
            tokio_rt
                .block_on(tokio_benches::bench_contention_kanal())
                .print();
            // maniac_benches::bench_contention_kanal(&maniac_rt).print();
        }
        println!();

        drop(tokio_rt);
    }

    if MANIAC {
        // ========================================================================
        // Maniac Runtime Benchmarks
        // ========================================================================
        // println!();
        // println!("=== MANIAC RUNTIME (4 worker threads) ===");
        // println!();

        // let maniac_rt = maniac::runtime::new_multi_threaded(8, 4096 * 64 * 8).unwrap();
        let maniac_rt = maniac::runtime::new_multi_threaded(4, 4096 * 32 * 2).unwrap();

        println!("SPSC (Single Producer, Single Consumer)");
        println!("---------------------------------------");

        for _ in 0..ITERATIONS {
            maniac_benches::bench_maniac_spsc_dynamic(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_maniac_spsc(&maniac_rt).print();
        // }
        // println!();

        for _ in 0..ITERATIONS {
            maniac_benches::bench_crossfire_spsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_kanal_spsc(&maniac_rt).print();
        // }
        // println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_flume_spsc(&maniac_rt).print();
        // }
        // println!();

        println!("MPSC ({} Producers, Single Consumer)", NUM_PRODUCERS);
        println!("---------------------------------------");

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_maniac_mpsc(&maniac_rt).print();
        // }
        // println!();

        for _ in 0..ITERATIONS {
            maniac_benches::bench_crossfire_mpsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_kanal_mpsc(&maniac_rt).print();
        // }
        // println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_flume_mpsc(&maniac_rt).print();
        // }
        // println!();

        println!(
            "Contention Benchmark ({} Producers, small buffer)",
            NUM_PRODUCERS
        );
        println!("--------------------------------------------------");

        for _ in 0..ITERATIONS {
            maniac_benches::bench_contention_crossfire_mpsc(&maniac_rt).print();
        }
        println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_contention_maniac(&maniac_rt).print();
        // }
        // println!();

        // for _ in 0..ITERATIONS {
        //     maniac_benches::bench_contention_kanal(&maniac_rt).print();
        // }

        // Keep a reference to the service to collect stats after workers drop
        let service = std::sync::Arc::clone(maniac_rt.service());
        drop(maniac_rt); // This drops workers, which copy their stats to the service

        // Now collect stats after workers have exited
        let stats = service.aggregate_stats();
        println!("\nManiac Runtime Stats:");
        println!("====================");
        println!("{}", stats);
    }
}
