
## High Level Architecture
```
+-------------------------------------------------------------------+
|                          [ client ]                               |
|                (Java, Go, Rust, Python, JS, etc)                  |
|                                                                   |
|  - Embed libbop or libbopd                                        |
|  - OR connect to bopd via TCP/UNIX socket                         |
+-------------------------------------------------------------------+
|                      [ bopd | libbopd ]                           |
|                   (linux, windows, macos)                         |
|                                                                   |
|       Core server written in Odin, embeds libbop                  |
|                                                                   |
|   - Can run standalone or embedded                                |
|   - Optional runtimes: Java GraalVM Isolates, LuaJIT, Wasmtime    |
|   - Hosts Raft + FSMs + Services                                  |
+-------------------------------------------------------------------+
|                          [ libbop ]                               |
|                   (linux, windows, macos)                         |
|                                                                   |
|             Low-level hardened C library core                     |
|      collection of open-source libs written in C/C++/Rust         |
|                  zero to minimal wrapping                         |
|                                                                   |
|   - C ABI  (universal embedding)                                  |
|   - Raft (NuRaft)                                                 |
|   - snmalloc (high-quality malloc)                                |
|   - Async Networking (uSockets/uWebSockets/ASIO)                  |
|   - Async File I/O (io_uring, IOCP, thread pool)                  |
|   - Storage (libmdbx, SQLite)                                     |
|   - HTTP Server / Client                                          |
|   - WebSocket Server / Client                                     |
|   - TLS (WolfSSL)                                                 |
+-------------------------------------------------------------------+
```

This layout reflects:

libbop as the foundational runtime layer

bopd as the orchestrating server daemon and embeddable runtime host

Client bindings and integrations at the top, with multiple language targets