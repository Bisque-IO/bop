// //! Example demonstrating uWebSockets wrappers
// //!
// //! This example shows how to use the uWebSockets Rust wrapper to create
// //! HTTP servers, WebSocket servers, and TCP servers with SSL support.

// use bop_rs::usockets::*;

// fn main() -> UwsResult<()> {
//     println!("uWebSockets Example");
//     println!("===================");

//     // HTTP server example
//     http_server_example()?;

//     // WebSocket server example
//     websocket_server_example()?;

//     // TCP server example
//     tcp_server_example()?;

//     // SSL example
//     ssl_example()?;

//     println!("✅ All examples completed successfully!");
//     Ok(())
// }

// fn http_server_example() -> UwsResult<()> {
//     println!("\n🌐 HTTP Server Example");
//     println!("----------------------");

//     let mut app = HttpApp::new()?;

//     // Add GET route
//     app.get("/", |mut res, req| {
//         let url = req.url().unwrap_or_default();
//         let method = req.method().unwrap_or_default();

//         let response_body = format!(
//             "Hello from uWebSockets!\nMethod: {}\nURL: {}\n",
//             method, url
//         );

//         res.header("Content-Type", "text/plain").unwrap()
//            .end_str(&response_body, false).unwrap();
//     })?;

//     // Add POST route with JSON response
//     app.post("/api/data", |mut res, req| {
//         let headers = req.headers().unwrap_or_default();

//         let json_response = format!(
//             r#"{{"status":"success","headers_count":{}}}"#,
//             headers.len()
//         );

//         res.header("Content-Type", "application/json").unwrap()
//            .header("Access-Control-Allow-Origin", "*").unwrap()
//            .end_str(&json_response, false).unwrap();
//     })?;

//     println!("✅ HTTP server configured");
//     println!("  - GET /");
//     println!("  - POST /api/data");

//     // In a real application, you would call app.listen() and app.run()
//     // app.listen(8080)?.run();

//     Ok(())
// }

// fn websocket_server_example() -> UwsResult<()> {
//     println!("\n🔌 WebSocket Server Example");
//     println!("----------------------------");

//     let mut app = HttpApp::new()?;

//     // Configure WebSocket behavior
//     let ws_behavior = WebSocketBehavior {
//         max_message_size: 1024 * 1024, // 1MB
//         idle_timeout: 120, // 2 minutes
//         max_backpressure: 1024 * 1024, // 1MB
//         send_pings_automatically: true,
//         ..Default::default()
//     };

//     // Add WebSocket route
//     app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior)?;

//     // Publish example
//     let message = b"Hello WebSocket clients!";
//     app.publish("general", message, WebSocketOpcode::Text)?;

//     println!("✅ WebSocket server configured");
//     println!("  - WebSocket endpoint: /ws");
//     println!("  - Max message size: 1MB");
//     println!("  - Idle timeout: 2 minutes");
//     println!("  - Auto ping enabled");

//     Ok(())
// }

// fn tcp_server_example() -> UwsResult<()> {
//     println!("\n🔗 TCP Server Example");
//     println!("----------------------");

//     let tcp_behavior = TcpBehavior {
//         connection: Some(Box::new(|conn| {
//             println!("New TCP connection established");
//         })),
//         message: Some(Box::new(|conn, data| {
//             println!("Received {} bytes", data.len());
//             // Echo the data back
//             let _ = conn.write(data);
//         })),
//         close: Some(Box::new(|_conn| {
//             println!("TCP connection closed");
//         })),
//     };

//     let mut server = TcpServerApp::new(tcp_behavior)?;

//     println!("✅ TCP server configured");
//     println!("  - Echo server behavior");
//     println!("  - Connection, message, and close handlers");

//     // In a real application, you would call listen() and run()
//     // server.listen(9090)?.run();

//     Ok(())
// }

// fn ssl_example() -> UwsResult<()> {
//     println!("\n🔒 SSL/TLS Example");
//     println!("------------------");

//     let ssl_options = SslOptions {
//         cert_file_name: "cert.pem".to_string(),
//         key_file_name: "key.pem".to_string(),
//         passphrase: "".to_string(),
//         ca_file_name: "ca.pem".to_string(),
//         ssl_prefer_low_memory_usage: true,
//         ..Default::default()
//     };

//     // HTTPS server
//     let https_result = HttpApp::new_ssl(ssl_options.clone());
//     match https_result {
//         Ok(_app) => {
//             println!("✅ HTTPS server creation succeeded");
//         },
//         Err(e) => {
//             println!("⚠️  HTTPS server creation failed: {}", e);
//             println!("   (This is expected without actual certificate files)");
//         }
//     }

//     // SSL TCP server
//     let tcp_behavior = TcpBehavior::default();
//     let ssl_tcp_result = TcpServerApp::new_ssl(tcp_behavior, ssl_options.clone());
//     match ssl_tcp_result {
//         Ok(_server) => {
//             println!("✅ SSL TCP server creation succeeded");
//         },
//         Err(e) => {
//             println!("⚠️  SSL TCP server creation failed: {}", e);
//             println!("   (This is expected without actual certificate files)");
//         }
//     }

//     // SSL TCP client
//     let tcp_behavior = TcpBehavior::default();
//     let ssl_client_result = TcpClientApp::new_ssl(tcp_behavior, ssl_options);
//     match ssl_client_result {
//         Ok(_client) => {
//             println!("✅ SSL TCP client creation succeeded");
//         },
//         Err(e) => {
//             println!("⚠️  SSL TCP client creation failed: {}", e);
//             println!("   (This is expected without actual certificate files)");
//         }
//     }

//     println!("SSL configuration:");
//     println!("  - Certificate: cert.pem");
//     println!("  - Private key: key.pem");
//     println!("  - CA certificate: ca.pem");
//     println!("  - Low memory usage: enabled");

//     Ok(())
// }

// /// Example of a complete HTTP server with multiple routes
// #[allow(dead_code)]
// fn complete_http_server_example() -> UwsResult<()> {
//     let mut app = HttpApp::new()?;

//     // Static file serving (placeholder)
//     app.get("/static/*", |mut res, req| {
//         let url = req.url().unwrap_or("/");
//         let static_content = format!("Static content for: {}", url);

//         res.header("Content-Type", "text/plain").unwrap()
//            .header("Cache-Control", "max-age=3600").unwrap()
//            .end_str(&static_content, false).unwrap();
//     })?;

//     // API routes with JSON
//     app.get("/api/status", |mut res, _req| {
//         let json = r#"{"status":"online","timestamp":"2024-01-01T00:00:00Z"}"#;
//         res.header("Content-Type", "application/json").unwrap()
//            .end_str(json, false).unwrap();
//     })?;

//     app.post("/api/users", |mut res, req| {
//         // In real app, you would parse the request body
//         let headers = req.headers().unwrap_or_default();
//         let content_type = headers.get("content-type").unwrap_or("unknown");

//         let response = format!(
//             r#"{{"message":"User creation endpoint","content_type":"{}"}}"#,
//             content_type
//         );

//         res.status("201 Created").unwrap()
//            .header("Content-Type", "application/json").unwrap()
//            .end_str(&response, false).unwrap();
//     })?;

//     // WebSocket chat room
//     let ws_behavior = WebSocketBehavior {
//         max_message_size: 1024,
//         idle_timeout: 300,
//         send_pings_automatically: true,
//         ..Default::default()
//     };

//     app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/chat", ws_behavior)?;

//     println!("Complete HTTP server configured with:");
//     println!("  - Static file serving: /static/*");
//     println!("  - Status API: GET /api/status");
//     println!("  - User API: POST /api/users");
//     println!("  - WebSocket chat: /chat");

//     // To run: app.listen(8080)?.run();
//     Ok(())
// }

// /// Example of event loop usage
// #[allow(dead_code)]
// fn event_loop_example() -> UwsResult<()> {
//     let app = HttpApp::new()?;
//     let event_loop = app.event_loop();

//     // Defer a task
//     event_loop.defer(|| {
//         println!("This will be executed in the next event loop iteration");
//     });

//     // Schedule periodic tasks (conceptual - would need timer implementation)
//     event_loop.defer(|| {
//         println!("Periodic task executed");
//     });

//     println!("Event loop configured with deferred tasks");
//     Ok(())
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_examples_compile() {
//         // These tests ensure the examples compile correctly
//         assert!(http_server_example().is_ok());
//         assert!(websocket_server_example().is_ok());
//         assert!(tcp_server_example().is_ok());
//         assert!(ssl_example().is_ok());
//     }

//     #[test]
//     fn test_ssl_options() {
//         let ssl_options = SslOptions {
//             cert_file_name: "test.pem".to_string(),
//             key_file_name: "test-key.pem".to_string(),
//             ssl_prefer_low_memory_usage: true,
//             ..Default::default()
//         };

//         assert_eq!(ssl_options.cert_file_name, "test.pem");
//         assert_eq!(ssl_options.key_file_name, "test-key.pem");
//         assert!(ssl_options.ssl_prefer_low_memory_usage);
//     }
// }
//
fn main() {}
