//! # MPMC Queue Example
//!
//! This example demonstrates the usage of BOP's high-performance MPMC
//! (Multi-Producer Multi-Consumer) queue wrapper.
//!
//! The moodycamel concurrent queue is one of the fastest lock-free queues
//! available, and this wrapper provides safe Rust access to it.

use bop_mpmc::mpmc::{ArcQueue, BlockingQueue, BoxQueue, MpmcResult, Queue};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> MpmcResult<()> {
    println!("🚀 BOP MPMC Queue Examples");
    println!("========================");

    basic_example()?;
    token_optimization_example()?;
    blocking_queue_example()?;
    bulk_operations_example()?;
    producer_consumer_example()?;
    generic_types_example()?;
    performance_benchmark()?;

    println!("✅ All MPMC queue examples completed successfully!");
    Ok(())
}

/// Basic non-blocking queue operations
fn basic_example() -> MpmcResult<()> {
    println!("\n📦 Basic Queue Operations");
    println!("-------------------------");

    let queue = Queue::new()?;

    // Enqueue some items
    assert!(queue.try_enqueue(10));
    assert!(queue.try_enqueue(20));
    assert!(queue.try_enqueue(30));

    println!("📊 Queue size (approx): {}", queue.size_approx());

    // Dequeue items
    while let Some(item) = queue.try_dequeue() {
        println!("📤 Dequeued: {}", item);
    }

    // Try to dequeue from empty queue
    assert!(queue.try_dequeue().is_none());
    println!("✅ Basic operations completed");

    Ok(())
}

/// Demonstrate token-based optimization for high-frequency operations
fn token_optimization_example() -> MpmcResult<()> {
    println!("\n🎯 Token Optimization Example");
    println!("-----------------------------");

    let queue = Queue::new()?;

    // Create tokens for better performance
    let producer_token = queue.create_producer_token()?;
    let consumer_token = queue.create_consumer_token()?;

    // Tokens provide better performance for repeated operations
    let items = [100, 200, 300, 400, 500];

    println!("🔥 Using producer token for high-frequency enqueues");
    for &item in &items {
        assert!(producer_token.enqueue(item));
        println!("📥 Enqueued: {}", item);
    }

    println!("🔥 Using consumer token for high-frequency dequeues");
    let mut dequeued = Vec::new();
    while let Some(item) = consumer_token.dequeue() {
        dequeued.push(item);
        println!("📤 Dequeued: {}", item);
    }

    assert_eq!(dequeued.len(), items.len());
    println!("✅ Token optimization completed");

    Ok(())
}

/// Blocking queue with timeout support
fn blocking_queue_example() -> MpmcResult<()> {
    println!("\n⏱️  Blocking Queue Example");
    println!("--------------------------");

    let queue = Arc::new(BlockingQueue::new()?);
    let queue_clone = queue.clone();

    // Producer thread that waits before producing
    let producer = thread::spawn(move || {
        println!("⏳ Producer waiting 100ms before producing...");
        thread::sleep(Duration::from_millis(100));

        println!("📥 Producer enqueuing item 777");
        queue_clone.enqueue(777);
    });

    // Consumer with timeout
    println!("⏳ Consumer waiting for item (timeout: 500ms)");
    let start = Instant::now();

    match queue.dequeue_wait(Duration::from_millis(500))? {
        Some(item) => {
            let elapsed = start.elapsed();
            println!("📤 Received item: {} after {:?}", item, elapsed);
            assert_eq!(item, 777);
        }
        None => {
            println!("❌ Timeout expired without receiving item");
        }
    }

    producer.join().unwrap();

    // Test timeout
    println!("⏳ Testing timeout (should timeout after 50ms)");
    let start = Instant::now();
    match queue.dequeue_wait(Duration::from_millis(50))? {
        Some(_) => println!("❌ Unexpectedly received item"),
        None => {
            let elapsed = start.elapsed();
            println!("✅ Correctly timed out after {:?}", elapsed);
        }
    }

    Ok(())
}

/// Bulk operations for efficiency
fn bulk_operations_example() -> MpmcResult<()> {
    println!("\n📦 Bulk Operations Example");
    println!("--------------------------");

    let queue = Queue::new()?;

    // Bulk enqueue
    let items_to_enqueue = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    println!("📥 Bulk enqueuing {} items", items_to_enqueue.len());

    assert!(queue.try_enqueue_bulk(&items_to_enqueue));
    println!("📊 Queue size after bulk enqueue: {}", queue.size_approx());

    // Bulk dequeue
    let mut buffer = vec![0u64; 15]; // Buffer larger than available items
    let dequeued_count = queue.try_dequeue_bulk(&mut buffer);

    println!("📤 Bulk dequeued {} items", dequeued_count);
    println!("🔍 Dequeued items: {:?}", &buffer[..dequeued_count]);

    assert_eq!(dequeued_count, items_to_enqueue.len());

    // Verify all items were dequeued correctly
    let mut dequeued_sorted = buffer[..dequeued_count].to_vec();
    dequeued_sorted.sort_unstable();
    assert_eq!(dequeued_sorted, items_to_enqueue);

    println!("✅ Bulk operations completed");
    Ok(())
}

/// Multi-producer multi-consumer scenario
fn producer_consumer_example() -> MpmcResult<()> {
    println!("\n👥 Multi-Producer Multi-Consumer Example");
    println!("----------------------------------------");

    let queue = Arc::new(Queue::new()?);
    let num_producers = 3;
    let num_consumers = 2;
    let items_per_producer = 20;

    let mut producers = Vec::new();
    let mut consumers = Vec::new();

    println!(
        "🏭 Spawning {} producers, {} items each",
        num_producers, items_per_producer
    );

    // Spawn producer threads
    for producer_id in 0..num_producers {
        let queue_clone = queue.clone();
        producers.push(thread::spawn(move || {
            let token = queue_clone
                .create_producer_token()
                .expect("Failed to create producer token");

            for i in 0..items_per_producer {
                let item = (producer_id * 1000 + i) as u64;

                // Keep trying until successful (in real code, you might add backoff)
                while !token.enqueue(item) {
                    thread::yield_now();
                }
            }

            println!("✅ Producer {} finished", producer_id);
            items_per_producer
        }));
    }

    println!("🏭 Spawning {} consumers", num_consumers);

    // Spawn consumer threads
    for consumer_id in 0..num_consumers {
        let queue_clone = queue.clone();
        consumers.push(thread::spawn(move || {
            let token = queue_clone
                .create_consumer_token()
                .expect("Failed to create consumer token");

            let mut collected = Vec::new();
            let start = Instant::now();

            // Consume for a reasonable amount of time
            while start.elapsed() < Duration::from_millis(2000) {
                if let Some(item) = token.dequeue() {
                    collected.push(item);
                } else {
                    thread::sleep(Duration::from_millis(1));
                }
            }

            println!(
                "✅ Consumer {} collected {} items",
                consumer_id,
                collected.len()
            );
            collected
        }));
    }

    // Wait for all producers to complete
    let mut total_produced = 0;
    for producer in producers {
        total_produced += producer.join().unwrap();
    }

    // Collect results from all consumers
    let mut total_consumed = 0;
    let mut all_items = Vec::new();
    for consumer in consumers {
        let items = consumer.join().unwrap();
        total_consumed += items.len();
        all_items.extend(items);
    }

    println!(
        "📊 Total produced: {}, Total consumed: {}",
        total_produced, total_consumed
    );
    println!("📊 Final queue size: {}", queue.size_approx());

    // Verify no duplicates in consumed items
    all_items.sort_unstable();
    let original_len = all_items.len();
    all_items.dedup();
    assert_eq!(all_items.len(), original_len, "Found duplicate items!");

    println!("✅ Producer-consumer example completed");
    Ok(())
}

/// Demonstrate generic Box and Arc queue support
fn generic_types_example() -> MpmcResult<()> {
    println!("\n📦 Generic Types Example (Box & Arc)");
    println!("-------------------------------------");

    // BoxQueue example with custom structs
    {
        println!("\n🎁 BoxQueue Example");

        #[derive(Debug, PartialEq)]
        struct Task {
            id: u32,
            description: String,
        }

        let queue: BoxQueue<Task> = BoxQueue::new()?;

        // Create and enqueue boxed tasks
        let tasks = vec![
            Task {
                id: 1,
                description: "Process data".to_string(),
            },
            Task {
                id: 2,
                description: "Send notification".to_string(),
            },
            Task {
                id: 3,
                description: "Update database".to_string(),
            },
        ];

        for task in tasks.iter() {
            let boxed_task = Box::new(Task {
                id: task.id,
                description: task.description.clone(),
            });
            assert!(queue.enqueue(boxed_task));
            println!("📥 Enqueued task: {:?}", task);
        }

        // Process all tasks
        let mut processed_tasks = Vec::new();
        while let Some(boxed_task) = queue.dequeue() {
            println!("📤 Processing task: {:?}", *boxed_task);
            processed_tasks.push(*boxed_task);
        }

        assert_eq!(processed_tasks.len(), tasks.len());
        println!("✅ Processed {} tasks with BoxQueue", processed_tasks.len());
    }

    // ArcQueue example with shared data
    {
        println!("\n🔗 ArcQueue Example");

        #[derive(Debug)]
        struct SharedResource {
            name: String,
            value: i32,
        }

        let queue: ArcQueue<SharedResource> = ArcQueue::new()?;

        // Create shared resource
        let resource = Arc::new(SharedResource {
            name: "Database Connection".to_string(),
            value: 42,
        });

        println!("🔧 Created shared resource: {:?}", resource);

        // Multiple references to the same resource
        for i in 0..3 {
            assert!(queue.enqueue_cloned(&resource));
            println!("📥 Enqueued reference {} to shared resource", i + 1);
        }

        println!(
            "📊 Original Arc strong count: {}",
            Arc::strong_count(&resource)
        );

        // Consume the references
        let mut consumed_count = 0;
        while let Some(arc_resource) = queue.dequeue() {
            consumed_count += 1;
            println!(
                "📤 Consumed reference {}: {:?}",
                consumed_count, arc_resource
            );
            println!("📊 Arc strong count: {}", Arc::strong_count(&arc_resource));

            // Verify it's the same resource
            assert!(Arc::ptr_eq(&resource, &arc_resource));
        }

        assert_eq!(consumed_count, 3);
        println!(
            "📊 Final Arc strong count: {}",
            Arc::strong_count(&resource)
        );
        println!("✅ Processed {} Arc references", consumed_count);
    }

    // Multi-threaded example with generic queues
    {
        println!("\n👥 Multi-threaded Generic Example");

        let box_queue = Arc::new(BoxQueue::<String>::new()?);
        let arc_queue = Arc::new(ArcQueue::<i32>::new()?);

        // Shared data for Arc queue
        let shared_counter = Arc::new(100);

        // Producer for BoxQueue
        let box_producer = {
            let queue = box_queue.clone();
            thread::spawn(move || {
                for i in 0..5 {
                    loop {
                        let message = Box::new(format!("Message #{}", i));
                        if queue.enqueue(message) {
                            break;
                        }
                        thread::yield_now();
                    }
                }
            })
        };

        // Producer for ArcQueue
        let arc_producer = {
            let queue = arc_queue.clone();
            let counter = shared_counter.clone();
            thread::spawn(move || {
                for _ in 0..5 {
                    while !queue.enqueue_cloned(&counter) {
                        thread::yield_now();
                    }
                }
            })
        };

        // Consumer for BoxQueue
        let box_consumer = {
            let queue = box_queue.clone();
            thread::spawn(move || {
                let mut messages = Vec::new();
                for _ in 0..5 {
                    while let Some(boxed_msg) = queue.dequeue() {
                        messages.push(*boxed_msg);
                        break;
                    }
                    thread::yield_now();
                }
                messages
            })
        };

        // Consumer for ArcQueue
        let arc_consumer = {
            let queue = arc_queue.clone();
            thread::spawn(move || {
                let mut values = Vec::new();
                for _ in 0..5 {
                    while let Some(arc_val) = queue.dequeue() {
                        values.push(*arc_val);
                        break;
                    }
                    thread::yield_now();
                }
                values
            })
        };

        // Wait for completion
        box_producer.join().unwrap();
        arc_producer.join().unwrap();

        let box_messages = box_consumer.join().unwrap();
        let arc_values = arc_consumer.join().unwrap();

        println!("📦 BoxQueue messages: {:?}", box_messages);
        println!("🔗 ArcQueue values: {:?}", arc_values);

        assert_eq!(box_messages.len(), 5);
        assert_eq!(arc_values.len(), 5);
        assert!(arc_values.iter().all(|&v| v == 100));

        println!("✅ Multi-threaded generic queues completed successfully");
    }

    Ok(())
}

/// Performance benchmark comparing different approaches
fn performance_benchmark() -> MpmcResult<()> {
    println!("\n⚡ Performance Benchmark");
    println!("------------------------");

    let iterations = 100_000;

    // Benchmark 1: Basic queue operations
    {
        let queue = Queue::new()?;
        let start = Instant::now();

        for i in 0..iterations {
            queue.try_enqueue(i);
        }

        for _ in 0..iterations {
            queue.try_dequeue();
        }

        let elapsed = start.elapsed();
        let ops_per_sec = (iterations as f64 * 2.0) / elapsed.as_secs_f64();
        println!("📈 Basic operations: {:.0} ops/sec", ops_per_sec);
    }

    // Benchmark 2: Token-optimized operations
    {
        let queue = Queue::new()?;
        let producer_token = queue.create_producer_token()?;
        let consumer_token = queue.create_consumer_token()?;

        let start = Instant::now();

        for i in 0..iterations {
            producer_token.enqueue(i);
        }

        for _ in 0..iterations {
            consumer_token.dequeue();
        }

        let elapsed = start.elapsed();
        let ops_per_sec = (iterations as f64 * 2.0) / elapsed.as_secs_f64();
        println!("📈 Token operations: {:.0} ops/sec", ops_per_sec);
    }

    // Benchmark 3: Bulk operations
    {
        let queue = Queue::new()?;
        let batch_size = 1000;
        let num_batches = iterations / batch_size;

        let start = Instant::now();

        for batch in 0..num_batches {
            let items: Vec<u64> = (0..batch_size)
                .map(|i| (batch * batch_size + i) as u64)
                .collect();
            queue.try_enqueue_bulk(&items);

            let mut buffer = vec![0u64; batch_size as usize];
            queue.try_dequeue_bulk(&mut buffer);
        }

        let elapsed = start.elapsed();
        let ops_per_sec = ((num_batches * batch_size) as f64 * 2.0) / elapsed.as_secs_f64();
        println!("📈 Bulk operations: {:.0} ops/sec", ops_per_sec);
    }

    println!("✅ Performance benchmark completed");
    Ok(())
}

/// Example showing queue usage patterns for different scenarios
#[allow(dead_code)]
fn usage_patterns_example() -> MpmcResult<()> {
    println!("\n🎨 Usage Patterns Example");
    println!("--------------------------");

    // Pattern 1: Work queue for task distribution
    work_queue_pattern()?;

    // Pattern 2: Event streaming
    event_streaming_pattern()?;

    // Pattern 3: Producer-consumer with backpressure
    backpressure_pattern()?;

    Ok(())
}

fn work_queue_pattern() -> MpmcResult<()> {
    println!("🔧 Work Queue Pattern");

    let work_queue = Arc::new(Queue::new()?);
    let num_workers = 4;

    // Add work items
    for task_id in 0..20 {
        work_queue.try_enqueue(task_id);
    }

    // Spawn worker threads
    let workers: Vec<_> = (0..num_workers)
        .map(|worker_id| {
            let queue_clone = work_queue.clone();
            thread::spawn(move || {
                let consumer_token = queue_clone
                    .create_consumer_token()
                    .expect("Failed to create consumer token");

                let mut tasks_completed = 0;
                while let Some(task_id) = consumer_token.dequeue() {
                    // Simulate work
                    thread::sleep(Duration::from_millis(10));
                    println!("👷 Worker {} completed task {}", worker_id, task_id);
                    tasks_completed += 1;
                }
                tasks_completed
            })
        })
        .collect();

    // Wait for workers
    let total_completed: usize = workers.into_iter().map(|w| w.join().unwrap()).sum();

    println!("✅ Work queue completed {} tasks", total_completed);
    Ok(())
}

fn event_streaming_pattern() -> MpmcResult<()> {
    println!("📡 Event Streaming Pattern");

    let event_queue = Arc::new(BlockingQueue::new()?);
    let producer_queue = event_queue.clone();

    // Event producer
    let producer = thread::spawn(move || {
        for event_id in 0..10 {
            producer_queue.enqueue(event_id);
            thread::sleep(Duration::from_millis(50));
        }
    });

    // Event consumers with different processing speeds
    let consumer = thread::spawn(move || {
        let consumer_token = event_queue
            .create_consumer_token()
            .expect("Failed to create consumer token");

        let mut events_processed = 0;
        for _ in 0..10 {
            if let Ok(Some(event_id)) = consumer_token.dequeue_wait(Duration::from_millis(200)) {
                println!("📨 Processed event {}", event_id);
                events_processed += 1;
            }
        }
        events_processed
    });

    producer.join().unwrap();
    let processed = consumer.join().unwrap();
    println!("✅ Event streaming processed {} events", processed);

    Ok(())
}

fn backpressure_pattern() -> MpmcResult<()> {
    println!("🌊 Backpressure Pattern");

    let queue = Arc::new(Queue::new()?);
    let producer_queue = queue.clone();

    // Fast producer with backpressure handling
    let producer = thread::spawn(move || {
        let producer_token = producer_queue
            .create_producer_token()
            .expect("Failed to create producer token");

        let mut produced = 0;
        let mut backpressure_count = 0;

        for i in 0..100 {
            if producer_token.enqueue(i) {
                produced += 1;
            } else {
                backpressure_count += 1;
                // Handle backpressure - could implement exponential backoff here
                thread::sleep(Duration::from_millis(1));
            }
        }

        println!(
            "📊 Producer: {} items, {} backpressure events",
            produced, backpressure_count
        );
        produced
    });

    // Slow consumer
    let consumer = thread::spawn(move || {
        let consumer_token = queue
            .create_consumer_token()
            .expect("Failed to create consumer token");

        let mut consumed = 0;
        let start = Instant::now();

        while start.elapsed() < Duration::from_millis(500) {
            if let Some(_item) = consumer_token.dequeue() {
                consumed += 1;
                // Simulate slow processing
                thread::sleep(Duration::from_millis(5));
            }
        }

        consumed
    });

    let produced = producer.join().unwrap();
    let consumed = consumer.join().unwrap();

    println!(
        "📊 Backpressure: produced {}, consumed {}",
        produced, consumed
    );
    println!("✅ Backpressure pattern completed");

    Ok(())
}

