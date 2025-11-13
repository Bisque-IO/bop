// // Integration tests for production socket wrapper with uSockets
// // This demonstrates real-world usage patterns with proper backpressure handling

// use bop_rs::production_socket::{ProductionSocket, ProductionSocketConfig, SocketState, BackpressureStrategy, RateLimitConfig, RateLimitAlgorithm};
// use bop_rs::usockets::{LoopInner, SocketContext, SslMode, SocketContextOptions, Socket};
// use std::sync::mpsc;
// use std::sync::atomic::{AtomicUsize, Ordering};
// use std::sync::Arc;
// use std::thread;
// use std::time::Duration;

// // Helper to find free TCP port
// fn free_tcp_port() -> u16 {
//     use std::net::{TcpListener, SocketAddr};
//     let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to random port");
//     let addr: SocketAddr = listener.local_addr().expect("Failed to get local address");
//     addr.port()
// }

// #[test]
// fn test_production_socket_basic_functionality() {
//     let config = ProductionSocketConfig {
//         max_buffer_size: 10240,
//         high_watermark: 8192,
//         low_watermark: 2048,
//         write_chunk_size: 1024,
//         ..Default::default()
//     };

//     let data_received = Arc::new(AtomicUsize::new(0));
//     let drain_count = Arc::new(AtomicUsize::new(0));
//     let backpressure_count = Arc::new(AtomicUsize::new(0));

//     let data_counter = data_received.clone();
//     let drain_counter = drain_count.clone();
//     let backpressure_counter = backpressure_count.clone();

//     let socket = ProductionSocket::new(config)
//         .on_data(move |data| {
//             data_counter.fetch_add(data.len(), Ordering::Relaxed);
//         })
//         .on_drain(move || {
//             drain_counter.fetch_add(1, Ordering::Relaxed);
//         })
//         .on_backpressure(move |active| {
//             if active {
//                 backpressure_counter.fetch_add(1, Ordering::Relaxed);
//             }
//         });

//     // Test basic state
//     assert_eq!(socket.state(), SocketState::Connecting);
//     assert_eq!(socket.buffer_utilization(), 0.0);

//     // Test writing small amounts
//     assert!(socket.write(b"hello").is_ok());
//     assert!(socket.write(b"world").is_ok());
    
//     // Buffer should have data now
//     assert!(socket.buffer_utilization() > 0.0);

//     // Test flushing
//     let flushed = socket.flush().unwrap();
//     assert!(flushed > 0);

//     // Test statistics
//     let stats = socket.stats();
//     assert!(stats.bytes_sent.load(Ordering::Relaxed) > 0);

//     println!("✅ Production socket basic functionality test passed");
// }

// #[test]
// fn test_backpressure_mechanism() {
//     let config = ProductionSocketConfig {
//         max_buffer_size: 1000,
//         high_watermark: 800,
//         low_watermark: 300,
//         write_chunk_size: 100,
//         ..Default::default()
//     };

//     let backpressure_events = Arc::new(AtomicUsize::new(0));
//     let backpressure_counter = backpressure_events.clone();

//     let socket = ProductionSocket::new(config)
//         .on_backpressure(move |active| {
//             if active {
//                 backpressure_counter.fetch_add(1, Ordering::Relaxed);
//             }
//         });

//     // Fill buffer gradually
//     for i in 0..10 {
//         let result = socket.write(&vec![0u8; 100]);
//         assert!(result.is_ok());
        
//         println!("Write {}: Buffer utilization: {:.1}%, Backpressure: {}", 
//                  i + 1, 
//                  socket.buffer_utilization() * 100.0,
//                  socket.has_backpressure());
        
//         if socket.has_backpressure() {
//             break;
//         }
//     }

//     // Should have triggered backpressure
//     assert!(socket.has_backpressure());
//     assert!(socket.is_reading_paused());
//     assert!(backpressure_events.load(Ordering::Relaxed) > 0);

//     // Drain some data
//     let drained = socket.drain_write_buffer().unwrap();
//     println!("Drained {} bytes, utilization now: {:.1}%", 
//              drained, socket.buffer_utilization() * 100.0);

//     // Continue draining until below low watermark
//     while socket.buffer_utilization() > 0.3 {
//         socket.drain_write_buffer().unwrap();
//     }

//     // Should resume normal operation
//     assert!(!socket.is_reading_paused());

//     println!("✅ Backpressure mechanism test passed");
// }

// #[test]
// fn test_different_backpressure_strategies() {
//     // Test DropOldest strategy
//     {
//         let config = ProductionSocketConfig {
//             max_buffer_size: 500,
//             high_watermark: 400,
//             low_watermark: 200,
//             ..Default::default()
//         };

//         let mut socket = ProductionSocket::new(config);
//         socket.backpressure_strategy = BackpressureStrategy::DropOldest;

//         // Fill buffer to capacity
//         socket.write(&vec![1u8; 400]).unwrap();
        
//         // This should drop old data and succeed
//         let result = socket.write(&vec![2u8; 200]);
//         assert!(result.is_ok());
        
//         println!("✅ DropOldest strategy test passed");
//     }

//     // Test DropNewest strategy  
//     {
//         let config = ProductionSocketConfig {
//             max_buffer_size: 500,
//             high_watermark: 400,
//             low_watermark: 200,
//             ..Default::default()
//         };

//         let mut socket = ProductionSocket::new(config);
//         socket.backpressure_strategy = BackpressureStrategy::DropNewest;

//         // Fill buffer to capacity
//         socket.write(&vec![1u8; 400]).unwrap();
        
//         // This should drop new data and return false
//         let result = socket.write(&vec![2u8; 200]);
//         assert!(result.is_ok());
//         assert_eq!(result.unwrap(), false); // Indicates data was dropped
        
//         println!("✅ DropNewest strategy test passed");
//     }

//     // Test CloseConnection strategy
//     {
//         let config = ProductionSocketConfig {
//             max_buffer_size: 500,
//             high_watermark: 400,
//             low_watermark: 200,
//             ..Default::default()
//         };

//         let mut socket = ProductionSocket::new(config);
//         socket.backpressure_strategy = BackpressureStrategy::CloseConnection;

//         // Fill buffer to capacity
//         socket.write(&vec![1u8; 400]).unwrap();
        
//         // This should close connection and return error
//         let result = socket.write(&vec![2u8; 200]);
//         assert!(result.is_err());
        
//         match socket.state() {
//             SocketState::Error(_) => println!("✅ CloseConnection strategy test passed"),
//             _ => panic!("Expected error state"),
//         }
//     }
// }

// #[test] 
// fn test_corking_mechanism() {
//     let config = ProductionSocketConfig {
//         enable_corking: true,
//         cork_timeout: 50, // 50ms
//         write_chunk_size: 1000,
//         ..Default::default()
//     };

//     let socket = ProductionSocket::new(config);

//     // Write small chunks that should be corked
//     socket.write(b"small1").unwrap();
//     socket.write(b"small2").unwrap();
//     socket.write(b"small3").unwrap();

//     // Cork events should be tracked
//     let cork_events = socket.stats().cork_events.load(Ordering::Relaxed);
//     assert!(cork_events > 0);

//     println!("Cork events: {}", cork_events);

//     // Wait for cork timeout and tick
//     thread::sleep(Duration::from_millis(60));
//     socket.tick().unwrap();

//     // Should have uncorked and drained
//     let bytes_sent = socket.stats().bytes_sent.load(Ordering::Relaxed);
//     assert!(bytes_sent > 0);

//     println!("✅ Corking mechanism test passed");
// }

// #[test]
// fn test_production_socket_with_usockets_integration() {
//     let loop_ = LoopInner::new().expect("create loop");
//     let port = free_tcp_port();
//     let (tx_events, rx_events) = mpsc::channel::<String>();

//     // Production socket config optimized for testing
//     let config = ProductionSocketConfig {
//         max_buffer_size: 8192,
//         high_watermark: 6144,
//         low_watermark: 2048,
//         write_chunk_size: 512,
//         enable_corking: true,
//         cork_timeout: 20,
//         ..Default::default()
//     };

//     let events_received = Arc::new(AtomicUsize::new(0));
//     let bytes_processed = Arc::new(AtomicUsize::new(0));
//     let backpressure_triggered = Arc::new(AtomicUsize::new(0));

//     let event_counter = events_received.clone();
//     let byte_counter = bytes_processed.clone();
//     let bp_counter = backpressure_triggered.clone();

//     // Create production socket with callbacks
//     let prod_socket = Arc::new(
//         ProductionSocket::new(config)
//             .on_data(move |data| {
//                 event_counter.fetch_add(1, Ordering::Relaxed);
//                 byte_counter.fetch_add(data.len(), Ordering::Relaxed);
//             })
//             .on_backpressure(move |active| {
//                 if active {
//                     bp_counter.fetch_add(1, Ordering::Relaxed);
//                 }
//             })
//     );

//     // Create server context
//     let mut server_ctx = SocketContext::new(&loop_, SslMode::Plain, SocketContextOptions::default())
//         .expect("create server context");

//     let tx_server = tx_events.clone();
//     let prod_socket_server = prod_socket.clone();
//     server_ctx.on_open(move |_sock, is_client, _ip| {
//         if !is_client {
//             let _ = tx_server.send("server_connection".to_string());
//         }
//     });

//     server_ctx.on_data(move |sock, data| {
//         // Simulate data processing through production socket
//         let _ = prod_socket_server.write(data);
        
//         // Echo back through production socket's drain mechanism
//         prod_socket_server.flush().ok();
        
//         let response = b"production_echo: ";
//         let _ = sock.write(response, true);
//         let _ = sock.write(data, false);
//     });

//     let listen_result = server_ctx.listen("127.0.0.1", port as i32, 0);
//     assert!(listen_result.is_some(), "Server should listen");

//     // Create client context
//     let mut client_ctx = SocketContext::new(&loop_, SslMode::Plain, SocketContextOptions::default())
//         .expect("create client context");

//     let tx_client = tx_events.clone();
//     let prod_socket_client = prod_socket.clone();
//     client_ctx.on_open(move |sock, is_client, _ip| {
//         if is_client {
//             let _ = tx_client.send("client_connected".to_string());
            
//             // Send multiple messages to test production socket buffering
//             for i in 0..5 {
//                 let msg = format!("test_message_{}", i);
//                 let _ = sock.write(msg.as_bytes(), false);
//             }
//         }
//     });

//     client_ctx.on_data(move |_sock, data| {
//         // Process received data through production socket
//         let _ = prod_socket_client.write(data);
//     });

//     let connect_result = client_ctx.connect("127.0.0.1", port as i32, None, 0);
//     assert!(connect_result.is_some(), "Client should connect");

//     // Run event loop with production socket maintenance
//     let mut iterations = 0;
//     let mut events = Vec::new();

//     while iterations < 50 {
//         loop_.integrate();
        
//         // Maintain production socket
//         prod_socket.tick().unwrap();
        
//         // Collect events
//         while let Ok(event) = rx_events.try_recv() {
//             events.push(event);
//         }
        
//         thread::sleep(Duration::from_millis(20));
//         iterations += 1;
        
//         // Break if we have activity
//         if events.len() >= 2 {
//             break;
//         }
//     }

//     println!("\n=== PRODUCTION SOCKET INTEGRATION TEST RESULTS ===");
//     println!("Events: {:?}", events);
//     println!("Production socket stats:");
//     println!("  - Events received: {}", events_received.load(Ordering::Relaxed));
//     println!("  - Bytes processed: {}", bytes_processed.load(Ordering::Relaxed));
//     println!("  - Backpressure events: {}", backpressure_triggered.load(Ordering::Relaxed));
//     println!("  - Buffer utilization: {:.1}%", prod_socket.buffer_utilization() * 100.0);
//     println!("  - Bytes sent: {}", prod_socket.stats().bytes_sent.load(Ordering::Relaxed));
//     println!("  - Cork events: {}", prod_socket.stats().cork_events.load(Ordering::Relaxed));
//     println!("  - Drain events: {}", prod_socket.stats().drain_events.load(Ordering::Relaxed));

//     // Verify basic integration worked
//     assert!(iterations > 0, "Should have run integration cycles");
    
//     println!("✅ Production socket integration test completed");
// }

// #[test]
// fn test_high_throughput_scenario() {
//     let config = ProductionSocketConfig {
//         max_buffer_size: 1024 * 1024,  // 1MB
//         high_watermark: 768 * 1024,    // 768KB
//         low_watermark: 256 * 1024,     // 256KB
//         write_chunk_size: 64 * 1024,   // 64KB chunks
//         enable_corking: true,
//         cork_timeout: 5,
//         ..Default::default()
//     };

//     let throughput_stats = Arc::new(AtomicUsize::new(0));
//     let backpressure_stats = Arc::new(AtomicUsize::new(0));

//     let throughput_counter = throughput_stats.clone();
//     let bp_counter = backpressure_stats.clone();

//     let socket = ProductionSocket::new(config)
//         .on_data(move |data| {
//             throughput_counter.fetch_add(data.len(), Ordering::Relaxed);
//         })
//         .on_backpressure(move |active| {
//             if active {
//                 bp_counter.fetch_add(1, Ordering::Relaxed);
//             }
//         });

//     println!("Starting high throughput test...");

//     // Simulate high-throughput scenario
//     let mut total_written = 0;
//     let chunk_size = 8192; // 8KB chunks

//     for round in 0..100 {
//         // Write data
//         let data = vec![round as u8; chunk_size];
//         match socket.write(&data) {
//             Ok(true) => total_written += data.len(),
//             Ok(false) => {
//                 println!("Data dropped at round {}", round);
//             }
//             Err(e) => {
//                 println!("Error at round {}: {}", round, e);
//                 break;
//             }
//         }

//         // Periodic maintenance and draining
//         if round % 10 == 0 {
//             socket.tick().unwrap();
//             let drained = socket.drain_write_buffer().unwrap();
            
//             println!("Round {}: Written {}KB, Buffer {:.1}%, Drained {}B, BP: {}", 
//                      round, 
//                      total_written / 1024,
//                      socket.buffer_utilization() * 100.0,
//                      drained,
//                      socket.has_backpressure());
//         }

//         // Small delay to simulate real conditions
//         if round % 20 == 0 {
//             thread::sleep(Duration::from_millis(1));
//         }
//     }

//     // Final drain
//     while socket.buffer_utilization() > 0.0 {
//         socket.drain_write_buffer().unwrap();
//     }

//     println!("\n=== HIGH THROUGHPUT TEST RESULTS ===");
//     println!("Total data written: {}KB", total_written / 1024);
//     println!("Total data sent: {}KB", socket.stats().bytes_sent.load(Ordering::Relaxed) / 1024);
//     println!("Backpressure events: {}", backpressure_stats.load(Ordering::Relaxed));
//     println!("Cork events: {}", socket.stats().cork_events.load(Ordering::Relaxed));
//     println!("Drain events: {}", socket.stats().drain_events.load(Ordering::Relaxed));
//     println!("Final buffer utilization: {:.2}%", socket.buffer_utilization() * 100.0);

//     assert!(total_written > 0);
//     assert_eq!(socket.buffer_utilization(), 0.0, "Buffer should be empty after final drain");

//     println!("✅ High throughput test passed");
// }

// #[test]
// fn test_rate_limiting_functionality() {
//     let rate_config = RateLimitConfig {
//         max_bytes_per_second: 10000,     // 10KB/s
//         max_operations_per_second: 100,  // 100 ops/s
//         burst_capacity: 15000,           // 15KB burst
//         algorithm: RateLimitAlgorithm::TokenBucket,
//         enabled: true,
//         ..Default::default()
//     };

//     let config = ProductionSocketConfig {
//         rate_limit: rate_config,
//         max_buffer_size: 1024 * 1024,
//         ..Default::default()
//     };

//     let rate_limit_events = Arc::new(AtomicUsize::new(0));
//     let bytes_rejected = Arc::new(AtomicUsize::new(0));

//     let rl_counter = rate_limit_events.clone();
//     let br_counter = bytes_rejected.clone();

//     let socket = ProductionSocket::new(config)
//         .on_rate_limit(move |bytes_requested, available_tokens| {
//             rl_counter.fetch_add(1, Ordering::Relaxed);
//             br_counter.fetch_add(bytes_requested, Ordering::Relaxed);
//             println!("Rate limited: {} bytes requested, {} tokens available", 
//                      bytes_requested, available_tokens);
//         });

//     println!("Starting rate limiting test...");

//     // First, use up the burst capacity
//     let mut successful_writes = 0;
//     let mut total_attempts = 0;

//     // Try to write 1KB chunks rapidly
//     for i in 0..30 {
//         total_attempts += 1;
//         let data = vec![i as u8; 1000]; // 1KB chunks
        
//         match socket.write(&data) {
//             Ok(true) => {
//                 successful_writes += 1;
//                 println!("Write {}: SUCCESS - Efficiency: {:.1}%", 
//                          i + 1, socket.rate_limit_efficiency() * 100.0);
//             },
//             Ok(false) => {
//                 println!("Write {}: RATE LIMITED - Efficiency: {:.1}%", 
//                          i + 1, socket.rate_limit_efficiency() * 100.0);
//             },
//             Err(e) => {
//                 println!("Write {}: ERROR - {}", i + 1, e);
//                 break;
//             }
//         }

//         // Show rate limiter status every 5 writes
//         if (i + 1) % 5 == 0 {
//             if let Some((available, max, ops_used, max_ops)) = socket.rate_limit_status() {
//                 println!("  Status: {} / {} tokens, {} / {} ops, Limited: {}", 
//                          available, max, ops_used, max_ops, socket.is_rate_limited());
//             }
//         }

//         // Small delay between writes
//         thread::sleep(Duration::from_millis(10));
//     }

//     println!("\n=== RATE LIMITING TEST RESULTS ===");
//     println!("Total write attempts: {}", total_attempts);
//     println!("Successful writes: {}", successful_writes);
//     println!("Rate limit events: {}", rate_limit_events.load(Ordering::Relaxed));
//     println!("Bytes rejected: {}", bytes_rejected.load(Ordering::Relaxed));
//     println!("Final efficiency: {:.1}%", socket.rate_limit_efficiency() * 100.0);
    
//     if let Some((available, max, ops_used, max_ops)) = socket.rate_limit_status() {
//         println!("Final status: {} / {} tokens, {} / {} ops", 
//                  available, max, ops_used, max_ops);
//     }

//     // Should have some rate limiting events
//     assert!(rate_limit_events.load(Ordering::Relaxed) > 0, "Should have triggered rate limiting");
//     assert!(successful_writes > 0, "Should have some successful writes");
//     assert!(successful_writes < total_attempts, "Should have some rate limited writes");

//     println!("✅ Rate limiting functionality test passed");
// }

// #[test]
// fn test_rate_limiting_with_time_recovery() {
//     let rate_config = RateLimitConfig {
//         max_bytes_per_second: 5000,      // 5KB/s
//         max_operations_per_second: 50,   // 50 ops/s 
//         burst_capacity: 2000,            // 2KB burst
//         algorithm: RateLimitAlgorithm::TokenBucket,
//         enabled: true,
//         ..Default::default()
//     };

//     let config = ProductionSocketConfig {
//         rate_limit: rate_config,
//         ..Default::default()
//     };

//     let socket = ProductionSocket::new(config);

//     println!("Testing rate limiting recovery over time...");

//     // Phase 1: Exhaust burst capacity
//     println!("Phase 1: Exhausting burst capacity");
//     let mut write_count = 0;
//     for i in 0..5 {
//         let result = socket.write(&vec![0u8; 500]);
//         if result.unwrap_or(false) {
//             write_count += 1;
//             println!("  Burst write {}: SUCCESS", i + 1);
//         } else {
//             println!("  Burst write {}: RATE LIMITED", i + 1);
//             break;
//         }
//     }

//     let (tokens_after_burst, _, _, _) = socket.rate_limit_status().unwrap();
//     println!("  Tokens after burst: {}", tokens_after_burst);

//     // Phase 2: Wait for token recovery
//     println!("Phase 2: Waiting for token recovery (2 seconds)");
//     thread::sleep(Duration::from_secs(2));

//     let (tokens_after_wait, max_tokens, _, _) = socket.rate_limit_status().unwrap();
//     println!("  Tokens after wait: {} / {}", tokens_after_wait, max_tokens);

//     // Should have recovered some tokens
//     assert!(tokens_after_wait > tokens_after_burst, 
//             "Tokens should have recovered: {} -> {}", tokens_after_burst, tokens_after_wait);

//     // Phase 3: Verify we can write again
//     println!("Phase 3: Verifying recovery");
//     let recovery_result = socket.write(&vec![0u8; 1000]);
//     assert!(recovery_result.unwrap_or(false), "Should be able to write after recovery");

//     println!("✅ Rate limiting recovery test passed");
// }

// #[test]
// fn test_operations_rate_limiting() {
//     let rate_config = RateLimitConfig {
//         max_bytes_per_second: 100000,    // High byte limit
//         max_operations_per_second: 10,   // Low ops limit
//         burst_capacity: 200000,          // High burst
//         algorithm: RateLimitAlgorithm::TokenBucket,
//         enabled: true,
//         ..Default::default()
//     };

//     let config = ProductionSocketConfig {
//         rate_limit: rate_config,
//         ..Default::default()
//     };

//     let socket = ProductionSocket::new(config);

//     println!("Testing operations-per-second rate limiting...");

//     let mut successful_ops = 0;
//     let mut rate_limited_ops = 0;

//     // Try 20 small operations rapidly (should hit ops limit before bytes limit)
//     for i in 0..20 {
//         let result = socket.write(&vec![0u8; 10]); // Small 10-byte writes
        
//         match result {
//             Ok(true) => {
//                 successful_ops += 1;
//                 println!("Op {}: SUCCESS", i + 1);
//             },
//             Ok(false) => {
//                 rate_limited_ops += 1;
//                 println!("Op {}: RATE LIMITED", i + 1);
//             },
//             Err(e) => {
//                 println!("Op {}: ERROR - {}", i + 1, e);
//                 break;
//             }
//         }

//         if let Some((_, _, ops_used, max_ops)) = socket.rate_limit_status() {
//             if ops_used >= max_ops {
//                 println!("  Hit operations limit: {} / {}", ops_used, max_ops);
//                 break;
//             }
//         }
//     }

//     println!("\n=== OPERATIONS RATE LIMITING TEST RESULTS ===");
//     println!("Successful operations: {}", successful_ops);
//     println!("Rate limited operations: {}", rate_limited_ops);

//     if let Some((tokens, max_tokens, ops_used, max_ops)) = socket.rate_limit_status() {
//         println!("Final status: {} / {} tokens, {} / {} ops", 
//                  tokens, max_tokens, ops_used, max_ops);
//     }

//     // Should have hit operations limit while still having plenty of tokens
//     assert!(successful_ops > 0, "Should have some successful operations");
//     assert!(rate_limited_ops > 0, "Should have some rate limited operations");
//     assert!(successful_ops <= 10, "Should not exceed operations limit");

//     println!("✅ Operations rate limiting test passed");
// }

// #[test]
// fn test_rate_limiting_disabled() {
//     let rate_config = RateLimitConfig {
//         enabled: false,
//         ..Default::default()
//     };

//     let config = ProductionSocketConfig {
//         rate_limit: rate_config,
//         ..Default::default()
//     };

//     let socket = ProductionSocket::new(config);

//     println!("Testing disabled rate limiting...");

//     // Should be able to write without any rate limiting
//     let mut total_written = 0;
//     for i in 0..50 {
//         let result = socket.write(&vec![0u8; 1000]);
//         assert!(result.unwrap(), "All writes should succeed when rate limiting is disabled");
//         total_written += 1000;
//     }

//     println!("Total written without rate limiting: {} bytes", total_written);

//     // Should show no rate limiting
//     assert!(!socket.is_rate_limited(), "Should not be rate limited when disabled");
//     assert_eq!(socket.rate_limit_efficiency(), 1.0, "Efficiency should be 100% when disabled");
//     assert!(socket.rate_limit_status().is_none(), "Should have no rate limit status when disabled");

//     println!("✅ Disabled rate limiting test passed");
// }