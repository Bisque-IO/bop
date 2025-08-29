# uWS Documentation

This directory contains comprehensive documentation for the uWS library components, including HTTP client/server, WebSocket, and TCP functionality.

## Documentation Index

### TCP Components
- **[TCPAPP_README.md](TCPAPP_README.md)** - Complete guide to TCPApp and TCPClientApp functionality
- **[TCPConnection.md](TCPConnection.md)** - TCPConnection struct design and usage

### HTTP Client Components
- **[CLIENT_README.md](CLIENT_README.md)** - Overview of HTTP client functionality
- **[HTTP_CLIENT_ARCHITECTURE.md](HTTP_CLIENT_ARCHITECTURE.md)** - HTTP client architecture overview
- **[HTTP_CLIENT_DATA_ARCHITECTURE.md](HTTP_CLIENT_DATA_ARCHITECTURE.md)** - Data architecture for HTTP clients
- **[HTTP_CLIENT_STATE_MACHINE.md](HTTP_CLIENT_STATE_MACHINE.md)** - State machine implementation details
- **[HTTP_CLIENT_STREAMING.md](HTTP_CLIENT_STREAMING.md)** - Streaming request/response handling
- **[HTTP_CLIENT_STREAMING_REQUESTS.md](HTTP_CLIENT_STREAMING_REQUESTS.md)** - Advanced streaming request features
- **[HTTP_CLIENT_BACKPRESSURE.md](HTTP_CLIENT_BACKPRESSURE.md)** - Backpressure handling in HTTP clients
- **[HTTP_CLIENT_CALLBACK_API_REFACTORING.md](HTTP_CLIENT_CALLBACK_API_REFACTORING.md)** - Callback API improvements
- **[HTTP_CLIENT_CALLBACK_IMPROVEMENTS.md](HTTP_CLIENT_CALLBACK_IMPROVEMENTS.md)** - Additional callback enhancements
- **[HTTP_CLIENT_CALLBACK_RENAMING_AND_BUFFER_OPTIMIZATION.md](HTTP_CLIENT_CALLBACK_RENAMING_AND_BUFFER_OPTIMIZATION.md)** - Callback renaming and buffer optimizations
- **[HTTP_CLIENT_HEADER_PARSING_OPTIMIZATION.md](HTTP_CLIENT_HEADER_PARSING_OPTIMIZATION.md)** - Header parsing performance improvements
- **[HTTP_CLIENT_COMPRESSION_FLAGS_UPDATE.md](HTTP_CLIENT_COMPRESSION_FLAGS_UPDATE.md)** - Compression flag updates
- **[HTTPCLIENT_PIPELINING_SUPPORT.md](HTTPCLIENT_PIPELINING_SUPPORT.md)** - HTTP pipelining support
- **[HTTP_PIPELINING_SUPPORT.md](HTTP_PIPELINING_SUPPORT.md)** - General HTTP pipelining documentation

### HTTP Parser Components
- **[HTTP_PARSER_STANDARDIZATION_PLAN.md](HTTP_PARSER_STANDARDIZATION_PLAN.md)** - Plan for standardizing HTTP parsing
- **[HTTP_PARSER_STANDARDIZATION_COMPLETED.md](HTTP_PARSER_STANDARDIZATION_COMPLETED.md)** - Completed HTTP parser standardization
- **[HTTP_PARSER_STATE_MANAGEMENT_IMPROVEMENTS.md](HTTP_PARSER_STATE_MANAGEMENT_IMPROVEMENTS.md)** - State management improvements
- **[PICOHTPPARSER_MIGRATION.md](PICOHTPPARSER_MIGRATION.md)** - Migration from picohttpparser
- **[HTTP_CLIENT_PARSER_ISSUE.md](HTTP_CLIENT_PARSER_ISSUE.md)** - Parser issue resolution

### Context and Data Management
- **[HTTPCLIENT_CONTEXT_DATA_CLEANUP.md](HTTPCLIENT_CONTEXT_DATA_CLEANUP.md)** - Context data cleanup improvements
- **[HTTPCLIENT_CONTEXT_DATA_ADDITIONAL_CLEANUP.md](HTTPCLIENT_CONTEXT_DATA_ADDITIONAL_CLEANUP.md)** - Additional context data cleanup
- **[CLIENT_CONTEXT_OPTIMIZATION.md](CLIENT_CONTEXT_OPTIMIZATION.md)** - Context optimization strategies
- **[PROCESSING_STATE_REMOVAL.md](PROCESSING_STATE_REMOVAL.md)** - Removal of processing state
- **[STREAMING_STATE_IMPROVEMENTS.md](STREAMING_STATE_IMPROVEMENTS.md)** - Streaming state improvements

### Chunked Encoding
- **[CHUNKED_ENCODING_STATE_FIX.md](CHUNKED_ENCODING_STATE_FIX.md)** - Chunked encoding state fixes
- **[CHUNKED_STATE_PERSISTENCE_FIX.md](CHUNKED_STATE_PERSISTENCE_FIX.md)** - Chunked state persistence fixes

### Refactoring and Cleanup
- **[CLIENT_REFACTORING.md](CLIENT_REFACTORING.md)** - Client refactoring overview
- **[HTTP_CLIENT_REFACTORING_SUMMARY.md](HTTP_CLIENT_REFACTORING_SUMMARY.md)** - HTTP client refactoring summary
- **[HTTP_CLIENT_MERGE_SUMMARY.md](HTTP_CLIENT_MERGE_SUMMARY.md)** - Merge summary for HTTP client
- **[HTTP_CLIENT_SERVER_SEPARATION_FIXES.md](HTTP_CLIENT_SERVER_SEPARATION_FIXES.md)** - Server separation fixes
- **[API_CLEANUP.md](API_CLEANUP.md)** - API cleanup documentation
- **[METHOD_RENAMING_SUMMARY.md](METHOD_RENAMING_SUMMARY.md)** - Method renaming summary

## Quick Start

### TCP Server
```cpp
#include "TCPApp.h"
#include "TCPContext.h"

uWS::TCPApp server;
uWS::TCPBehavior<false> behavior;
behavior.onData = [](uWS::TCPConnection<false, void>* conn, std::string_view data) {
    conn->send("Echo: " + std::string(data));
};

auto* context = server.createTCPContext(behavior);
auto* listenSocket = context->listen(nullptr, 8080, 0);
if (listenSocket) {
    std::cout << "Server listening on port 8080" << std::endl;
}
```

### HTTP Client
```cpp
#include "ClientApp.h"

uWS::ClientApp client;
uint64_t contextId = client.createHttpClientContext();

client.onResponse(contextId, [](uWS::HttpClientResponse* response) {
    std::cout << "Response status: " << response->getStatus() << std::endl;
});

client.get(contextId, "http://example.com", [](uWS::HttpClientResponse* response) {
    // Handle response
});
client.run();
```

## Architecture Overview

The uWS library is organized into several key components:

1. **TCP Components** - Raw TCP server and client functionality
2. **HTTP Components** - HTTP server and client with full protocol support
3. **WebSocket Components** - WebSocket server and client with compression
4. **Context Management** - Connection context and data management
5. **Parser Components** - HTTP and WebSocket protocol parsing

Each component follows consistent design patterns:
- Builder pattern for configuration
- Move semantics for performance
- Backpressure handling for flow control
- Streaming support for large data
- Context-based connection management

## Contributing

When adding new documentation:
1. Place all markdown files in this `doc/` directory
2. Update this README.md to include new documentation
3. Follow the existing naming conventions
4. Include code examples where appropriate
5. Link related documentation together

## Related Documentation

- [Main uWS Documentation](https://github.com/uNetworking/uWebSockets)
- [uSockets Documentation](https://github.com/uNetworking/uSockets)
- [HTTP/1.1 Specification](https://tools.ietf.org/html/rfc7230)
- [WebSocket Protocol](https://tools.ietf.org/html/rfc6455)
