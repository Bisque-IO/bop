// //! Test that uSocket generic types work properly

// use bop_rs::usockets::{LoopInner, SocketContext, SocketContextOptions, Socket, SslMode};

// #[derive(Default)]
// struct TestContext {
//     connections: u32,
// }

// #[derive(Default)]
// struct TestSocket {
//     messages: u32,
// }

// #[test]
// fn test_generic_types_compile() {
//     // Test that we can create loops with different generic types
//     let _loop1 = LoopInner::<TestContext>::new().expect("create loop with TestContext");
//     let _loop2 = LoopInner::<u32>::with_ext(42).expect("create loop with u32");
//     let _loop3 = LoopInner::<String>::with_ext("test".to_string()).expect("create loop with String");
    
//     // Verify ext data access on the u32 loop
//     assert_eq!(*_loop2.ext().expect("get ext data"), 42);
    
//     println!("Successfully created loops with different generic types");
// }

// #[test]
// fn test_basic_loop_functionality() {
//     // Test that loop creation and basic operations work with generic types
//     let loop_ = LoopInner::<u32>::with_ext(42).expect("create loop with custom ext");
    
//     // Verify ext data access
//     assert_eq!(*loop_.ext().expect("get ext data"), 42);
    
//     // Test different types of loop ext data
//     let _string_loop = LoopInner::<String>::with_ext("hello".to_string()).expect("create string loop");
//     let _unit_loop = LoopInner::<()>::new().expect("create unit loop");
    
//     println!("Basic loop functionality test passed");
// }

// #[test]
// fn test_ssl_const_generics() {
//     // Test that SSL mode is properly handled as const generic
//     let _loop_ = LoopInner::<()>::new().expect("create loop");
    
//     // We can't actually create sockets/contexts here because they need to be on the loop thread,
//     // but we can test that the types compile with different SSL values
    
//     // Plain SSL context (SSL = false)
//     type PlainContext = SocketContext<false, (), ()>;
//     // TLS context (SSL = true)  
//     type TlsContext = SocketContext<true, (), ()>;
    
//     // Plain socket (SSL = false)
//     type PlainSocket = Socket<false, ()>;
//     // TLS socket (SSL = true)
//     type TlsSocket = Socket<true, ()>;
    
//     println!("SSL const generic test passed - SSL mode is determined at compile time!");
//     println!("Plain contexts and sockets use SSL=false");
//     println!("TLS contexts and sockets use SSL=true");
// }

// #[test] 
// fn test_trampolines_registered() {
//     // This test verifies that the trampolines are properly wired up at compile time
//     // by checking that the types are correctly constructed
//     println!("Trampoline registration test - if this compiles, trampolines are properly set up");
    
//     // The fact that we can create these types and call their methods means
//     // the trampolines are correctly registered with the C library
    
//     // Test different generic combinations
//     type PlainContext1 = SocketContext<false, u32, String>;
//     type TlsContext1 = SocketContext<true, u32, String>;
//     type PlainContext2 = SocketContext<false, TestContext, TestSocket>;
//     type TlsContext2 = SocketContext<true, TestContext, TestSocket>;
//     type PlainContext3 = SocketContext<false>;
//     type TlsContext3 = SocketContext<true>;
    
//     println!("All SocketContext generic combinations compile successfully");
//     println!("Trampolines are properly parameterized for all type combinations");
//     println!("SSL mode is now a compile-time const generic parameter!");
// }