// //! Integration tests for bop-rs components
// //!
// //! These tests validate the interaction between different modules
// //! and components in the bop-rs library.

// use bop_rs::{allocator::*, usockets::*};
// use std::sync::{Arc, Mutex};
// use std::thread;
// use std::time::Duration;

// /// Test using BOP allocator with usockets components
// #[test] 
// fn test_allocator_with_usockets() {
//     // This test would only work if we set BOP allocator as global allocator
//     // For now, just test that both modules work independently
    
//     let _allocator = BopAllocator;
    
//     // Test that vectors work (using whatever allocator is active)
//     let mut vec = Vec::new();
//     vec.push(1);
//     vec.push(2);
//     vec.push(3);
//     assert_eq!(vec.len(), 3);

//     // Test usockets creation
//     if let Ok(app) = HttpApp::new() {
//         // Successfully created HTTP app
//         drop(app); // Explicit drop to test cleanup
//     }
// }

// /// Test memory allocation patterns with usockets
// #[cfg(feature = "alloc-stats")]
// #[test]
// fn test_memory_usage_with_stats() {
//     use std::alloc::{Layout, GlobalAlloc};
    
//     let stats_allocator = BopStatsAllocator::new();
    
//     unsafe {
//         // Simulate some allocations
//         let layout = Layout::from_size_align(1024, 8).unwrap();
//         let ptr1 = stats_allocator.alloc(layout);
//         assert!(!ptr1.is_null());
        
//         let stats = stats_allocator.stats();
//         assert_eq!(stats.allocations, 1);
//         assert!(stats.current_memory >= layout.size());
        
//         // Allocate more memory
//         let ptr2 = stats_allocator.alloc_zeroed(layout);
//         assert!(!ptr2.is_null());
        
//         let stats = stats_allocator.stats();
//         assert_eq!(stats.allocations, 2);
//         assert!(stats.current_memory >= layout.size() * 2);
        
//         // Deallocate
//         stats_allocator.dealloc(ptr1, layout);
//         stats_allocator.dealloc(ptr2, layout);
        
//         let stats = stats_allocator.stats();
//         assert_eq!(stats.deallocations, 2);
//         assert_eq!(stats.outstanding_allocations(), 0);
//     }
// }

// /// Test concurrent access to usockets with multiple threads
// #[test]
// fn test_concurrent_usockets() {
//     let success_count = Arc::new(Mutex::new(0));
//     let handles: Vec<_> = (0..5).map(|i| {
//         let success_count = success_count.clone();
//         thread::spawn(move || {
//             if let Ok(mut app) = HttpApp::new() {
//                 let route = format!("/thread{}", i);
//                 if app.get(&route, |mut res, _req| {
//                     let _ = res.end_str("OK", false);
//                 }).is_ok() {
//                     *success_count.lock().unwrap() += 1;
//                 }
//             }
//             thread::sleep(Duration::from_millis(10));
//         })
//     }).collect();

//     for handle in handles {
//         handle.join().unwrap();
//     }

//     let final_count = *success_count.lock().unwrap();
//     println!("Successfully created {} HTTP apps concurrently", final_count);
// }

// /// Test error propagation between modules
// #[test]
// fn test_error_handling_integration() {
//     // Test invalid SSL configuration
//     let invalid_ssl = SslOptions {
//         cert_file_name: "/definitely/does/not/exist.pem".to_string(),
//         key_file_name: "/also/missing.key".to_string(),
//         passphrase: "".to_string(),
//         ca_file_name: "".to_string(),
//         ssl_prefer_low_memory_usage: false,
//         dh_params_file_name: "".to_string(),
//     };

//     let result = HttpApp::new_ssl(invalid_ssl);
//     match result {
//         Ok(_) => {
//             // Unexpected success - maybe SSL is not properly configured
//             println!("SSL app created unexpectedly (SSL might be disabled)");
//         }
//         Err(e) => {
//             // Expected error
//             println!("Got expected SSL error: {}", e);
//             assert!(format!("{}", e).contains("SSL") || 
//                     format!("{}", e).contains("TLS") || 
//                     format!("{}", e).contains("certificate") ||
//                     format!("{}", e).contains("InitializationFailed"));
//         }
//     }
// }

// /// Test resource cleanup and memory management
// #[test]
// fn test_resource_cleanup() {
//     // Test that apps can be created and dropped without issues
//     for i in 0..10 {
//         if let Ok(mut app) = HttpApp::new() {
//             let route = format!("/cleanup{}", i);
//             let _ = app.get(&route, |mut res, _req| {
//                 let _ = res.end_str("cleanup test", false);
//             });
            
//             // Add WebSocket for more complex cleanup
//             let ws_behavior = WebSocketBehavior::default();
//             let _ = app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior);
            
//             // App will be dropped here, testing cleanup
//         }
//     }
    
//     // Test TCP server cleanup
//     for _i in 0..5 {
//         let tcp_behavior = TcpBehavior::default();
//         if let Ok(_server) = TcpServerApp::new(tcp_behavior) {
//             // Server will be dropped here
//         }
//     }
// }

// /// Test large data structures with custom allocator
// #[test]
// fn test_large_allocations() {
//     // Test that large allocations work correctly
//     let large_vec: Vec<u8> = vec![0; 1024 * 1024]; // 1MB
//     assert_eq!(large_vec.len(), 1024 * 1024);
    
//     let large_string = String::with_capacity(512 * 1024); // 512KB
//     assert!(large_string.capacity() >= 512 * 1024);
    
//     // Test with usockets if available
//     if let Ok(mut app) = HttpApp::new() {
//         // Create many routes to test memory usage
//         for i in 0..100 {
//             let route = format!("/large{}", i);
//             let _ = app.get(&route, |mut res, _req| {
//                 // Create large response
//                 let large_response = "x".repeat(1024);
//                 let _ = res.end_str(&large_response, false);
//             });
//         }
//     }
// }

// /// Test WebSocket and HTTP interaction
// #[test]
// fn test_websocket_http_integration() {
//     if let Ok(mut app) = HttpApp::new() {
//         // Add HTTP route
//         assert!(app.get("/", |mut res, _req| {
//             let _ = res.header("Content-Type", "text/html")
//                       .and_then(|mut r| r.end_str(
//                           r#"<html><body><script>
//                               var ws = new WebSocket('ws://localhost:8080/ws');
//                               ws.onmessage = function(e) { console.log(e.data); };
//                           </script></body></html>"#, false
//                       ));
//         }).is_ok());
        
//         // Add WebSocket endpoint  
//         let ws_behavior = WebSocketBehavior {
//             max_message_size: 4096,
//             idle_timeout: 60,
//             send_pings_automatically: true,
//             ..Default::default()
//         };
        
//         assert!(app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior).is_ok());
        
//         // Test publishing to WebSocket
//         let message = b"Hello WebSocket!";
//         let publish_result = app.publish("test-topic", message, WebSocketOpcode::Text);
//         // This might fail if no WebSocket connections exist, which is expected
//         match publish_result {
//             Ok(_) => println!("WebSocket publish succeeded"),
//             Err(e) => println!("WebSocket publish failed (expected): {}", e),
//         }
//     }
// }

// /// Test TCP client-server interaction setup
// #[test]
// fn test_tcp_client_server_setup() {
//     // Test TCP server setup
//     let server_behavior = TcpBehavior {
//         connection: Some(Box::new(|_conn| {
//             println!("Server: Client connected");
//         })),
//         message: Some(Box::new(|_conn, data| {
//             println!("Server: Received {} bytes", data.len());
//             // Echo back in a real scenario
//         })),
//         close: Some(Box::new(|_conn| {
//             println!("Server: Client disconnected");
//         })),
//     };
    
//     if let Ok(_server) = TcpServerApp::new(server_behavior) {
//         // Test TCP client setup
//         let client_behavior = TcpBehavior {
//             connection: Some(Box::new(|_conn| {
//                 println!("Client: Connected to server");
//             })),
//             message: Some(Box::new(|_conn, data| {
//                 println!("Client: Received {} bytes from server", data.len());
//             })),
//             close: Some(Box::new(|_conn| {
//                 println!("Client: Disconnected from server");
//             })),
//         };
        
//         if let Ok(_client) = TcpClientApp::new(client_behavior) {
//             println!("TCP client-server setup successful");
//         }
//     }
// }

// /// Test compatibility with standard library types
// #[test] 
// fn test_stdlib_compatibility() {
//     use std::collections::HashMap;
//     use std::sync::mpsc;
    
//     // Test that standard collections work
//     let mut map = HashMap::new();
//     map.insert("key1", "value1");
//     map.insert("key2", "value2");
//     assert_eq!(map.len(), 2);
    
//     // Test channels
//     let (tx, rx) = mpsc::channel();
//     tx.send("test message").unwrap();
//     let received = rx.recv().unwrap();
//     assert_eq!(received, "test message");
    
//     // Test with usockets if available
//     if let Ok(mut app) = HttpApp::new() {
//         // Store handler info in HashMap
//         let mut handlers = HashMap::new();
//         handlers.insert("/test", "Test handler");
        
//         assert!(app.get("/test", |mut res, _req| {
//             let _ = res.end_str("Standard library compatibility test", false);
//         }).is_ok());
        
//         assert_eq!(handlers.get("/test"), Some(&"Test handler"));
//     }
// }

// /// Stress test with many operations
// #[test]
// fn test_stress_operations() {
//     const ITERATIONS: usize = 50;
    
//     for i in 0..ITERATIONS {
//         // Create and destroy HTTP app
//         if let Ok(mut app) = HttpApp::new() {
//             let route = format!("/stress{}", i);
//             let _ = app.get(&route, move |mut res, _req| {
//                 let response = format!("Stress test iteration {}", i);
//                 let _ = res.end_str(&response, false);
//             });
            
//             // Add WebSocket
//             let ws_behavior = WebSocketBehavior::default();
//             let _ = app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior);
//         }
        
//         // Create and destroy TCP server
//         let tcp_behavior = TcpBehavior::default();
//         let _ = TcpServerApp::new(tcp_behavior);
        
//         // Some memory allocation stress
//         let vec: Vec<u32> = (0..1000).collect();
//         assert_eq!(vec.len(), 1000);
//     }
    
//     println!("Completed {} stress test iterations", ITERATIONS);
// }

// /// Test that Debug trait implementations work
// #[test]
// fn test_debug_implementations() {
//     let ssl_options = SslOptions::default();
//     let debug_str = format!("{:?}", ssl_options);
//     assert!(!debug_str.is_empty());
    
//     let ws_behavior = WebSocketBehavior::default();
//     let debug_str = format!("{:?}", ws_behavior);  
//     assert!(!debug_str.is_empty());
    
//     #[cfg(feature = "alloc-stats")]
//     {
//         let stats = AllocationStats {
//             allocations: 10,
//             deallocations: 5,
//             bytes_allocated: 1024,
//             bytes_deallocated: 512,
//             peak_memory: 1024,
//             current_memory: 512,
//         };
//         let debug_str = format!("{:?}", stats);
//         assert!(debug_str.contains("10"));
//         assert!(debug_str.contains("512"));
        
//         let display_str = format!("{}", stats);
//         assert!(display_str.contains("Allocations"));
//         assert!(display_str.contains("Outstanding"));
//     }
// }