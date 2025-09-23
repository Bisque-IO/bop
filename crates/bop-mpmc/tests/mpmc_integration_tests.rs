// //! Integration tests for MPMC queue functionality
// //!
// //! These tests validate the interaction between MPMC queues and other BOP components,
// //! as well as more complex usage scenarios.

// use bop_mpmc::{BoxQueue, ArcQueue, BlockingBoxQueue, BlockingArcQueue};
// use std::sync::Arc;
// use std::thread;
// use std::time::Duration;

// /// Test MPMC queue integration with BOP allocator
// #[test]
// fn test_mpmc_with_bop_allocator() {
//     // This test ensures MPMC queues work with BOP's custom allocator
//     let queue = BoxQueue::<u64>::new().expect("Failed to create MPMC queue");

//     // Test basic operations
//     assert!(queue.enqueue(Box::new(12345)));
//     assert_eq!(*queue.dequeue().unwrap(), 12345);

//     // Test with large numbers of allocations
//     let items: Vec<Box<u64>> = (0..1000).map(|i| Box::new(i)).collect();
//     assert!(queue.enqueue_bulk(items));

//     let results = queue.dequeue_bulk(1000);
//     assert_eq!(results.len(), 1000);

//     // Verify all items are present (order may vary)
//     let mut values: Vec<u64> = results.iter().map(|b| **b).collect();
//     values.sort_unstable();
//     let expected: Vec<u64> = (0..1000).collect();
//     assert_eq!(values, expected);
// }

// /// Test MPMC queue under high contention
// #[test]
// fn test_high_contention_scenario() {
//     let queue = Arc::new(BoxQueue::<u64>::new().unwrap());
//     let num_producers = 8;
//     let num_consumers = 4;
//     let items_per_producer = 500;

//     let mut producers = Vec::new();
//     let mut consumers = Vec::new();

//     // Spawn producers
//     for producer_id in 0..num_producers {
//         let queue_clone = queue.clone();
//         producers.push(thread::spawn(move || {
//             let mut produced = 0;

//             for i in 0..items_per_producer {
//                 let item = (producer_id * 10000 + i) as u64;

//                 // Keep trying with occasional yielding
//                 let mut attempts = 0;
//                 while !queue_clone.enqueue(Box::new(item)) {
//                     attempts += 1;
//                     if attempts % 1000 == 0 {
//                         thread::yield_now();
//                     }
//                 }
//                 produced += 1;
//             }
//             produced
//         }));
//     }

//     // Spawn consumers
//     for _consumer_id in 0..num_consumers {
//         let queue_clone = queue.clone();
//         consumers.push(thread::spawn(move || {
//             let mut consumed = Vec::new();
//             let start = std::time::Instant::now();

//             // Consume for up to 5 seconds
//             while start.elapsed() < Duration::from_secs(5) && consumed.len() < items_per_producer * 2 {
//                 if let Some(item) = queue_clone.dequeue() {
//                     consumed.push(*item);
//                 } else {
//                     thread::yield_now();
//                 }
//             }
//             consumed
//         }));
//     }

//     // Wait for all producers
//     let mut total_produced = 0;
//     for producer in producers {
//         total_produced += producer.join().unwrap();
//     }

//     // Collect from all consumers
//     let mut all_consumed = Vec::new();
//     for consumer in consumers {
//         let items = consumer.join().unwrap();
//         all_consumed.extend(items);
//     }

//     assert_eq!(total_produced, num_producers * items_per_producer);
//     assert_eq!(all_consumed.len(), total_produced);

//     // Verify no duplicates
//     all_consumed.sort_unstable();
//     let original_len = all_consumed.len();
//     all_consumed.dedup();
//     assert_eq!(all_consumed.len(), original_len, "Found duplicate items in high contention test");
// }

// /// Test blocking queue timeout behavior
// #[test]
// fn test_blocking_queue_precise_timeouts() {
//     let queue = BlockingBoxQueue::<u64>::new().unwrap();

//     // Test short timeout
//     let start = std::time::Instant::now();
//     let result = queue.dequeue_wait(Duration::from_millis(100)).unwrap();
//     let elapsed = start.elapsed();

//     assert!(result.is_none()); // Should timeout
//     assert!(elapsed >= Duration::from_millis(95)); // Allow some tolerance
//     assert!(elapsed < Duration::from_millis(150)); // But not too much

//     // Test that items can be received within timeout
//     let queue = Arc::new(BlockingBoxQueue::<u64>::new().unwrap());
//     let queue_clone = queue.clone();

//     let producer = thread::spawn(move || {
//         thread::sleep(Duration::from_millis(50));
//         queue_clone.enqueue(Box::new(999));
//     });

//     let start = std::time::Instant::now();
//     let result = queue.dequeue_wait(Duration::from_millis(200)).unwrap();
//     let elapsed = start.elapsed();

//     assert_eq!(*result.unwrap(), 999);
//     assert!(elapsed >= Duration::from_millis(45));
//     assert!(elapsed < Duration::from_millis(100));

//     producer.join().unwrap();
// }

// /// Test bulk operations with large datasets (demonstrates zero-copy optimization)
// #[test]
// fn test_bulk_operations_large_dataset() {
//     let queue = BoxQueue::<u64>::new().unwrap();
//     let chunk_size = 10000;
//     let num_chunks = 5;

//     // Test large bulk enqueue - zero allocations during bulk operations
//     for chunk in 0..num_chunks {
//         let items: Vec<Box<u64>> = (0..chunk_size)
//             .map(|i| Box::new((chunk * chunk_size + i) as u64))
//             .collect();

//         assert!(queue.enqueue_bulk(items)); // Zero-copy operation
//     }

//     let expected_total = chunk_size * num_chunks;
//     let approx_size = queue.size_approx();

//     // Size should be approximately correct (may vary due to concurrent nature)
//     assert!(approx_size <= expected_total + 100); // Allow some variance

//     // Test large bulk dequeue - zero-copy allocation pattern
//     let mut all_dequeued = Vec::new();
//     let batch_size = 5000;

//     while all_dequeued.len() < expected_total {
//         let batch = queue.dequeue_bulk(batch_size); // Zero-copy dequeue

//         if batch.is_empty() {
//             break; // No more items
//         }

//         all_dequeued.extend(batch.into_iter().map(|boxed| *boxed));
//     }

//     assert_eq!(all_dequeued.len(), expected_total);

//     // Verify all expected items are present
//     all_dequeued.sort_unstable();
//     let expected: Vec<u64> = (0..(expected_total as u64)).collect();
//     assert_eq!(all_dequeued, expected);
// }

// /// Test memory efficiency - ensure no leaks in long-running scenarios
// #[test]
// fn test_memory_efficiency() {
//     let queue = Arc::new(BoxQueue::<u64>::new().unwrap());
//     let iterations = 1000; // Reduced for faster test

//     // Simulate long-running producer-consumer workload
//     for _ in 0..5 { // 5 cycles of produce-consume
//         let queue_clone = queue.clone();
//         let producer = thread::spawn(move || {
//             for i in 0..iterations {
//                 while !queue_clone.enqueue(Box::new(i)) {
//                     thread::yield_now();
//                 }
//             }
//         });

//         let queue_clone = queue.clone();
//         let consumer = thread::spawn(move || {
//             let mut count = 0;
//             while count < iterations {
//                 if queue_clone.dequeue().is_some() {
//                     count += 1;
//                 } else {
//                     thread::yield_now();
//                 }
//             }
//             count
//         });

//         producer.join().unwrap();
//         let consumed = consumer.join().unwrap();

//         assert_eq!(consumed, iterations);
//         assert_eq!(queue.size_approx(), 0);
//     }
// }

// /// Test error conditions and edge cases
// #[test]
// fn test_edge_cases() {
//     // Test empty bulk operations
//     let queue = BoxQueue::<i32>::new().unwrap();
//     assert!(queue.enqueue_bulk(Vec::new())); // Empty vec should succeed

//     let results = queue.dequeue_bulk(0);
//     assert_eq!(results.len(), 0);

//     // Test Arc queue empty bulk operations
//     let arc_queue = ArcQueue::<String>::new().unwrap();
//     assert!(arc_queue.enqueue_bulk_cloned(&[]));

//     let arc_results = arc_queue.dequeue_bulk(0);
//     assert_eq!(arc_results.len(), 0);
// }

// /// Performance test under realistic workload
// #[test]
// fn test_realistic_workload_performance() {
//     let queue = Arc::new(BoxQueue::<u64>::new().unwrap());
//     let num_producers = 4;
//     let num_consumers = 2;
//     let work_duration = Duration::from_millis(500); // Reduced for faster test

//     let mut producers = Vec::new();
//     let mut consumers = Vec::new();

//     // Producers with realistic work simulation
//     for _i in 0..num_producers {
//         let queue_clone = queue.clone();
//         producers.push(thread::spawn(move || {
//             let start = std::time::Instant::now();
//             let mut produced = 0;

//             while start.elapsed() < work_duration {
//                 let work_item = produced; // Simple work item

//                 if queue_clone.enqueue(Box::new(work_item)) {
//                     produced += 1;
//                 } else {
//                     // Simulate backoff on contention
//                     thread::sleep(Duration::from_micros(10));
//                 }

//                 // Simulate variable work generation rate
//                 if produced % 100 == 0 {
//                     thread::sleep(Duration::from_micros(50));
//                 }
//             }
//             produced
//         }));
//     }

//     // Consumers with realistic work processing
//     for _i in 0..num_consumers {
//         let queue_clone = queue.clone();
//         consumers.push(thread::spawn(move || {
//             let start = std::time::Instant::now();
//             let mut consumed = 0;

//             while start.elapsed() < work_duration + Duration::from_millis(200) {
//                 if let Some(_work_item) = queue_clone.dequeue() {
//                     // Simulate processing work
//                     thread::sleep(Duration::from_micros(50));
//                     consumed += 1;
//                 } else {
//                     thread::sleep(Duration::from_micros(25));
//                 }
//             }
//             consumed
//         }));
//     }

//     // Collect results
//     let mut total_produced = 0;
//     for producer in producers {
//         total_produced += producer.join().unwrap();
//     }

//     let mut total_consumed = 0;
//     for consumer in consumers {
//         total_consumed += consumer.join().unwrap();
//     }

//     println!("Workload test: produced {}, consumed {}", total_produced, total_consumed);
//     assert!(total_produced > 100); // Should produce a reasonable amount
//     assert!(total_consumed > 0); // Should consume some items

//     // In realistic scenarios, consumers might not consume everything
//     // due to processing time, which is expected
// }

// /// Test generic BoxQueue functionality
// #[test]
// fn test_box_queue_functionality() {
//     let queue: BoxQueue<String> = BoxQueue::new().expect("Failed to create BoxQueue");

//     // Test basic Box operations
//     let data = Box::new("Hello, BoxQueue!".to_string());
//     assert!(queue.enqueue(data));

//     if let Some(result) = queue.dequeue() {
//         assert_eq!(*result, "Hello, BoxQueue!");
//     } else {
//         panic!("Failed to dequeue from BoxQueue");
//     }

//     // Test multiple items
//     let items = vec!["First", "Second", "Third"];
//     for item in &items {
//         let boxed = Box::new(item.to_string());
//         assert!(queue.enqueue(boxed));
//     }

//     let mut dequeued = Vec::new();
//     while let Some(boxed) = queue.dequeue() {
//         dequeued.push(*boxed);
//     }

//     assert_eq!(dequeued.len(), items.len());
//     // Note: Order might not be preserved in concurrent queue
// }

// /// Test generic ArcQueue functionality
// #[test]
// fn test_arc_queue_functionality() {
//     let queue: ArcQueue<i32> = ArcQueue::new().expect("Failed to create ArcQueue");

//     // Test basic Arc operations
//     let data = Arc::new(42);
//     assert!(queue.enqueue_cloned(&data));

//     // Original Arc should still be valid
//     assert_eq!(*data, 42);

//     if let Some(result) = queue.dequeue() {
//         assert_eq!(*result, 42);
//     } else {
//         panic!("Failed to dequeue from ArcQueue");
//     }

//     // Test direct Arc enqueue
//     let direct_arc = Arc::new(100);
//     assert!(queue.enqueue(direct_arc.clone()));

//     if let Some(result) = queue.dequeue() {
//         assert_eq!(*result, 100);
//         // Both references should point to the same data
//         assert!(Arc::ptr_eq(&result, &direct_arc));
//     }
// }

// /// Test Box and Arc queues in multi-threaded scenario
// #[test]
// fn test_generic_queues_multithreaded() {
//     let box_queue = Arc::new(BoxQueue::<u64>::new().unwrap());
//     let arc_queue = Arc::new(ArcQueue::<String>::new().unwrap());

//     let num_items = 100;

//     // Test BoxQueue with multiple producers/consumers
//     let box_producers: Vec<_> = (0..2).map(|producer_id| {
//         let queue = box_queue.clone();
//         thread::spawn(move || {
//             for i in 0..num_items/2 {
//                 let value = (producer_id * 1000 + i) as u64;
//                 while !queue.enqueue(Box::new(value)) {
//                     thread::yield_now();
//                     // Re-create the boxed value if enqueue failed
//                     let boxed2 = Box::new(value);
//                     if queue.enqueue(boxed2) {
//                         break;
//                     }
//                 }
//             }
//         })
//     }).collect();

//     let box_consumer = {
//         let queue = box_queue.clone();
//         thread::spawn(move || {
//             let mut collected = Vec::new();
//             let start = std::time::Instant::now();

//             while collected.len() < num_items && start.elapsed() < Duration::from_secs(5) {
//                 if let Some(boxed) = queue.dequeue() {
//                     collected.push(*boxed);
//                 } else {
//                     thread::yield_now();
//                 }
//             }
//             collected
//         })
//     };

//     // Test ArcQueue with shared data
//     let shared_data = Arc::new("Shared Message".to_string());
//     let arc_producers: Vec<_> = (0..2).map(|_| {
//         let queue = arc_queue.clone();
//         let data = shared_data.clone();
//         thread::spawn(move || {
//             for _ in 0..num_items/2 {
//                 while !queue.enqueue_cloned(&data) {
//                     thread::yield_now();
//                 }
//             }
//         })
//     }).collect();

//     let arc_consumer = {
//         let queue = arc_queue.clone();
//         thread::spawn(move || {
//             let mut collected = Vec::new();
//             let start = std::time::Instant::now();

//             while collected.len() < num_items && start.elapsed() < Duration::from_secs(5) {
//                 if let Some(arc) = queue.dequeue() {
//                     collected.push(arc);
//                 } else {
//                     thread::yield_now();
//                 }
//             }
//             collected
//         })
//     };

//     // Wait for all threads
//     for producer in box_producers {
//         producer.join().unwrap();
//     }
//     for producer in arc_producers {
//         producer.join().unwrap();
//     }

//     let box_results = box_consumer.join().unwrap();
//     let arc_results = arc_consumer.join().unwrap();

//     assert_eq!(box_results.len(), num_items);
//     assert_eq!(arc_results.len(), num_items);

//     // All Arc items should point to the same string
//     for arc_item in &arc_results {
//         assert_eq!(**arc_item, "Shared Message");
//     }
// }

// #[cfg(feature = "stress-test")]
// mod stress_tests {
//     use super::*;

//     /// Very long-running stress test (only run when specifically requested)
//     #[test]
//     fn test_long_running_stability() {
//         let queue = Arc::new(MpmcQueue::new().unwrap());
//         let duration = Duration::from_secs(60); // 1 minute stress test

//         // This test is designed to run for an extended period
//         // to catch any potential stability issues

//         let _handles: Vec<_> = (0..8).map(|i| {
//             let queue_clone = queue.clone();
//             thread::spawn(move || {
//                 let token = if i % 2 == 0 {
//                     // Producer
//                     let token = queue_clone.create_producer_token().unwrap();
//                     let start = std::time::Instant::now();
//                     let mut ops = 0;

//                     while start.elapsed() < duration {
//                         if token.try_enqueue(ops) {
//                             ops += 1;
//                         }
//                         if ops % 10000 == 0 {
//                             thread::yield_now();
//                         }
//                     }
//                     ops
//                 } else {
//                     // Consumer
//                     let token = queue_clone.create_consumer_token().unwrap();
//                     let start = std::time::Instant::now();
//                     let mut ops = 0;

//                     while start.elapsed() < duration {
//                         if token.try_dequeue().is_some() {
//                             ops += 1;
//                         }
//                         if ops % 10000 == 0 {
//                             thread::yield_now();
//                         }
//                     }
//                     ops
//                 };
//                 token
//             })
//         }).collect();

//         // This test primarily checks that we don't crash or deadlock
//         // over extended periods
//     }
// }
