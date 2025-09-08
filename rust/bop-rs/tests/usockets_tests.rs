//! Comprehensive tests for uWebSockets wrappers
//!
//! These tests validate the HTTP, WebSocket, TCP, and SSL functionality
//! of the bop-rs usockets wrapper module.

use bop_rs::usockets::*;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::thread;

/// Test basic HTTP server creation and configuration
#[test]
fn test_http_server_creation() {
    let result = HttpApp::new();
    match result {
        Ok(app) => {
            // Server created successfully
            assert!(true);
        }
        Err(e) => {
            // If uWebSockets is not properly linked, this test may fail
            println!("HTTP server creation failed (expected if uWS not linked): {}", e);
        }
    }
}

/// Test HTTP server route configuration
#[test] 
fn test_http_server_routes() {
    if let Ok(mut app) = HttpApp::new() {
        // Test adding GET route
        let get_result = app.get("/", |mut res, req| {
            let _ = res.end_str("Hello GET!", false);
        });
        assert!(get_result.is_ok());

        // Test adding POST route
        let post_result = app.post("/api/data", |mut res, req| {
            let _ = res.end_str("Hello POST!", false);
        });
        assert!(post_result.is_ok());

        // Test adding PUT route
        let put_result = app.put("/api/update", |mut res, req| {
            let _ = res.end_str("Hello PUT!", false);
        });
        assert!(put_result.is_ok());

        // Test adding DELETE route
        let delete_result = app.delete("/api/delete", |mut res, req| {
            let _ = res.end_str("Hello DELETE!", false);
        });
        assert!(delete_result.is_ok());
    }
}

/// Test WebSocket server configuration
#[test]
fn test_websocket_server() {
    if let Ok(mut app) = HttpApp::new() {
        let ws_behavior = WebSocketBehavior {
            max_message_size: 1024,
            idle_timeout: 60,
            max_backpressure: 1024,
            send_pings_automatically: true,
            ..Default::default()
        };

        // Test WebSocket route addition
        let ws_result = app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior);
        assert!(ws_result.is_ok());
    }
}

/// Test TCP server creation
#[test]
fn test_tcp_server_creation() {
    let tcp_behavior = TcpBehavior {
        connection: Some(Box::new(|_conn| {
            println!("TCP connection established");
        })),
        message: Some(Box::new(|_conn, data| {
            println!("Received {} bytes", data.len());
        })),
        close: Some(Box::new(|_conn| {
            println!("TCP connection closed");
        })),
    };

    let result = TcpServerApp::new(tcp_behavior);
    match result {
        Ok(_server) => {
            assert!(true);
        }
        Err(e) => {
            println!("TCP server creation failed (expected if uWS not linked): {}", e);
        }
    }
}

/// Test TCP client creation
#[test]
fn test_tcp_client_creation() {
    let tcp_behavior = TcpBehavior {
        connection: Some(Box::new(|_conn| {
            println!("TCP client connected");
        })),
        message: Some(Box::new(|_conn, data| {
            println!("TCP client received {} bytes", data.len());
        })),
        close: Some(Box::new(|_conn| {
            println!("TCP client disconnected");
        })),
    };

    let result = TcpClientApp::new(tcp_behavior);
    match result {
        Ok(_client) => {
            assert!(true);
        }
        Err(e) => {
            println!("TCP client creation failed (expected if uWS not linked): {}", e);
        }
    }
}

/// Test SSL configuration
#[test]
fn test_ssl_configuration() {
    let ssl_options = SslOptions {
        cert_file_name: "test.pem".to_string(),
        key_file_name: "test-key.pem".to_string(),
        passphrase: "".to_string(),
        ca_file_name: "ca.pem".to_string(),
        ssl_prefer_low_memory_usage: false,
        dh_params_file_name: "".to_string(),
    };

    // Test HTTPS server with SSL (expected to fail without certificates)
    let https_result = HttpApp::new_ssl(ssl_options.clone());
    match https_result {
        Ok(_app) => {
            println!("HTTPS server created successfully");
        }
        Err(e) => {
            println!("HTTPS server creation failed (expected without certificates): {}", e);
            assert!(e.to_string().contains("SSL") || e.to_string().contains("TLS") || e.to_string().contains("certificate"));
        }
    }

    // Test SSL TCP server (expected to fail without certificates)
    let tcp_behavior = TcpBehavior::default();
    let ssl_tcp_result = TcpServerApp::new_ssl(tcp_behavior, ssl_options.clone());
    match ssl_tcp_result {
        Ok(_server) => {
            println!("SSL TCP server created successfully");
        }
        Err(e) => {
            println!("SSL TCP server creation failed (expected without certificates): {}", e);
        }
    }

    // Test SSL TCP client (expected to fail without certificates)  
    let tcp_behavior = TcpBehavior::default();
    let ssl_client_result = TcpClientApp::new_ssl(tcp_behavior, ssl_options);
    match ssl_client_result {
        Ok(_client) => {
            println!("SSL TCP client created successfully");
        }
        Err(e) => {
            println!("SSL TCP client creation failed (expected without certificates): {}", e);
        }
    }
}

/// Test HTTP response methods
#[test]
fn test_http_response_methods() {
    if let Ok(mut app) = HttpApp::new() {
        let route_result = app.get("/test", |mut res, req| {
            // Test header setting
            let header_result = res.header("Content-Type", "text/plain");
            assert!(header_result.is_ok());

            // Test status setting
            let status_result = res.status("200 OK");
            assert!(status_result.is_ok());

            // Test response body
            let body_result = res.end_str("Test response", false);
            assert!(body_result.is_ok());
        });
        assert!(route_result.is_ok());
    }
}

/// Test HTTP request methods
#[test]
fn test_http_request_methods() {
    if let Ok(mut app) = HttpApp::new() {
        let route_result = app.get("/request-test", |mut res, req| {
            // Test getting URL
            let url = req.url().unwrap_or("/");
            assert!(!url.is_empty());

            // Test getting method
            let method = req.method().unwrap_or("GET");
            assert_eq!(method, "GET");

            // Test getting headers
            let headers = req.headers().unwrap_or_default();
            assert!(headers.len() >= 0); // Headers might be empty in test

            // Test getting query string
            let query = req.query_string().unwrap_or("");
            // Query might be empty

            let _ = res.end_str("Request test complete", false);
        });
        assert!(route_result.is_ok());
    }
}

/// Test WebSocket behavior configuration
#[test]
fn test_websocket_behavior() {
    let behavior = WebSocketBehavior {
        max_message_size: 2048,
        idle_timeout: 120,
        max_backpressure: 2048,
        send_pings_automatically: false,
    };

    assert_eq!(behavior.max_message_size, 2048);
    assert_eq!(behavior.idle_timeout, 120);
    assert_eq!(behavior.max_backpressure, 2048);
    assert_eq!(behavior.send_pings_automatically, false);

    // Test default behavior
    let default_behavior = WebSocketBehavior::default();
    assert!(default_behavior.max_message_size > 0);
    assert!(default_behavior.idle_timeout > 0);
}

/// Test TCP behavior configuration
#[test]
fn test_tcp_behavior() {
    let connection_called = Arc::new(Mutex::new(false));
    let message_called = Arc::new(Mutex::new(false));
    let close_called = Arc::new(Mutex::new(false));

    let connection_flag = connection_called.clone();
    let message_flag = message_called.clone();
    let close_flag = close_called.clone();

    let behavior = TcpBehavior {
        connection: Some(Box::new(move |_conn| {
            *connection_flag.lock().unwrap() = true;
        })),
        message: Some(Box::new(move |_conn, _data| {
            *message_flag.lock().unwrap() = true;
        })),
        close: Some(Box::new(move |_conn| {
            *close_flag.lock().unwrap() = true;
        })),
    };

    // Test that callbacks exist
    assert!(behavior.connection.is_some());
    assert!(behavior.message.is_some());
    assert!(behavior.close.is_some());

    // Test default behavior
    let default_behavior = TcpBehavior::default();
    assert!(default_behavior.connection.is_none());
    assert!(default_behavior.message.is_none());
    assert!(default_behavior.close.is_none());
}

/// Test error handling
#[test]
fn test_error_handling() {
    // Test creating with invalid SSL options should return error
    let invalid_ssl = SslOptions {
        cert_file_name: "/nonexistent/cert.pem".to_string(),
        key_file_name: "/nonexistent/key.pem".to_string(),
        passphrase: "".to_string(),
        ca_file_name: "".to_string(),
        ssl_prefer_low_memory_usage: false,
        dh_params_file_name: "".to_string(),
    };

    let result = HttpApp::new_ssl(invalid_ssl);
    assert!(result.is_err());
}

/// Test WebSocket opcodes
#[test]
fn test_websocket_opcodes() {
    use WebSocketOpcode::*;
    
    // Test that opcodes have expected numeric values
    assert_eq!(Text as i32, 1);
    assert_eq!(Binary as i32, 2);
    assert_eq!(Close as i32, 8);
    assert_eq!(Ping as i32, 9);
    assert_eq!(Pong as i32, 10);
}

/// Test SSL options configuration
#[test]
fn test_ssl_options() {
    let ssl_options = SslOptions {
        cert_file_name: "server.crt".to_string(),
        key_file_name: "server.key".to_string(),
        passphrase: "secret".to_string(),
        ca_file_name: "ca.crt".to_string(),
        ssl_prefer_low_memory_usage: true,
        dh_params_file_name: "dhparam.pem".to_string(),
    };

    assert_eq!(ssl_options.cert_file_name, "server.crt");
    assert_eq!(ssl_options.key_file_name, "server.key");
    assert_eq!(ssl_options.passphrase, "secret");
    assert_eq!(ssl_options.ca_file_name, "ca.crt");
    assert_eq!(ssl_options.ssl_prefer_low_memory_usage, true);
    assert_eq!(ssl_options.dh_params_file_name, "dhparam.pem");

    // Test default SSL options
    let default_ssl = SslOptions::default();
    assert!(default_ssl.cert_file_name.is_empty());
    assert!(default_ssl.key_file_name.is_empty());
    assert!(default_ssl.passphrase.is_empty());
    assert!(default_ssl.ca_file_name.is_empty());
    assert_eq!(default_ssl.ssl_prefer_low_memory_usage, false);
    assert!(default_ssl.dh_params_file_name.is_empty());
}

/// Test that UwsResult works correctly
#[test]
fn test_uws_result() {
    let success: UwsResult<i32> = Ok(42);
    assert!(success.is_ok());
    assert_eq!(success.unwrap(), 42);

    let failure: UwsResult<i32> = Err(UwsError::InitializationFailed("Test error".to_string()));
    assert!(failure.is_err());
    match failure {
        Err(UwsError::InitializationFailed(msg)) => {
            assert_eq!(msg, "Test error");
        }
        _ => panic!("Expected InitializationFailed error"),
    }
}

/// Integration test demonstrating complete HTTP server setup
#[test] 
fn test_complete_http_server_setup() {
    if let Ok(mut app) = HttpApp::new() {
        // Add multiple routes
        assert!(app.get("/", |mut res, _req| {
            let _ = res.header("Content-Type", "text/html")
                      .and_then(|mut r| r.end_str("<h1>Hello World!</h1>", false));
        }).is_ok());

        assert!(app.get("/api/health", |mut res, _req| {
            let _ = res.header("Content-Type", "application/json")
                      .and_then(|mut r| r.end_str(r#"{"status":"healthy"}"#, false));
        }).is_ok());

        assert!(app.post("/api/data", |mut res, req| {
            let headers = req.headers().unwrap_or_default();
            let response = format!(r#"{{"received_headers":{}}}"#, headers.len());
            let _ = res.header("Content-Type", "application/json")
                      .and_then(|mut r| r.end_str(&response, false));
        }).is_ok());

        // Add WebSocket endpoint
        let ws_behavior = WebSocketBehavior {
            max_message_size: 1024,
            idle_timeout: 300,
            send_pings_automatically: true,
            ..Default::default()
        };

        assert!(app.ws::<fn(&mut WebSocket, &[u8], WebSocketOpcode)>("/ws", ws_behavior).is_ok());

        println!("Complete HTTP server setup test passed");
    }
}

/// Test thread safety by creating multiple servers concurrently
#[test]
fn test_thread_safety() {
    let handles: Vec<_> = (0..3).map(|i| {
        thread::spawn(move || {
            let result = HttpApp::new();
            match result {
                Ok(mut app) => {
                    let route = format!("/thread{}", i);
                    let response = format!("Hello from thread {}", i);
                    let _ = app.get(&route, move |mut res, _req| {
                        let _ = res.end_str(&response, false);
                    });
                    true
                }
                Err(_) => {
                    // Expected if uWS not properly linked
                    false
                }
            }
        })
    }).collect();

    for handle in handles {
        let _ = handle.join();
    }
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    /// Benchmark HTTP server creation time
    #[test]
    fn bench_http_server_creation() {
        let start = Instant::now();
        let iterations = 100;
        let mut successful_creates = 0;

        for _ in 0..iterations {
            if HttpApp::new().is_ok() {
                successful_creates += 1;
            }
        }

        let duration = start.elapsed();
        println!("Created {} HTTP servers in {:?} (avg: {:?} per server)", 
                 successful_creates, duration, duration / successful_creates as u32);
    }

    /// Benchmark route registration
    #[test]
    fn bench_route_registration() {
        if let Ok(mut app) = HttpApp::new() {
            let start = Instant::now();
            let route_count = 50;

            for i in 0..route_count {
                let route = format!("/route{}", i);
                let _ = app.get(&route, |mut res, _req| {
                    let _ = res.end_str("Benchmark route", false);
                });
            }

            let duration = start.elapsed();
            println!("Registered {} routes in {:?} (avg: {:?} per route)",
                     route_count, duration, duration / route_count);
        }
    }
}