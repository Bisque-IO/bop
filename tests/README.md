# uWebSockets Test Suite

This directory contains comprehensive tests for the uWebSockets HttpClient and TCP functionality.

## Overview

The test suite is designed to validate the functionality, reliability, and performance of the uWebSockets library components. It includes tests for:

- **HttpClient**: HTTP client functionality, request/response handling, timeout management, backpressure, and SSL/TLS support
- **TCP**: TCP connection management, data transmission, timeout handling, backpressure, and SSL/TLS support

## Test Structure

### Files

- `HttpClientTest.cpp` - Comprehensive HttpClient tests
- `TCPTest.cpp` - Comprehensive TCP tests  
- `TestRunner.cpp` - Main test runner with reporting
- `CMakeLists.txt` - Build configuration
- `README.md` - This file

### Test Categories

#### HttpClient Tests

1. **Basic Creation and Configuration**
   - Context creation and default settings
   - Configuration setters and getters
   - Behavior configuration

2. **Connection Management**
   - Connection establishment
   - Connection error handling
   - Connection lifecycle

3. **HTTP Protocol**
   - Request/response parsing
   - Header handling
   - Chunked encoding
   - Error handling

4. **Timeout Management**
   - Idle timeout handling
   - Long timeout handling
   - Timeout callback execution

5. **Backpressure Management**
   - Backpressure detection
   - Data dropping
   - Writable callbacks

6. **Statistics Tracking**
   - Bytes sent/received
   - Messages sent/received
   - Request/response counts

7. **SSL/TLS Support**
   - SSL context creation
   - SSL behavior configuration
   - Mutual TLS support

8. **State Management**
   - Connection state queries
   - Configuration validation
   - Error state handling

#### TCP Tests

1. **Basic Creation and Configuration**
   - Context creation and default settings
   - Configuration setters and getters
   - Behavior configuration

2. **Connection Management**
   - Connection establishment
   - Connection error handling
   - Connection lifecycle

3. **Data Transmission**
   - Send/receive operations
   - Data integrity
   - Performance metrics

4. **Timeout Management**
   - Idle timeout handling
   - Long timeout handling
   - Timeout callback execution

5. **Backpressure Management**
   - Backpressure detection
   - Data dropping
   - Writable callbacks

6. **Statistics Tracking**
   - Bytes sent/received
   - Messages sent/received

7. **SSL/TLS Support**
   - SSL context creation
   - SSL behavior configuration

8. **Server Functionality**
   - Server creation
   - Client connection handling
   - Server lifecycle

9. **Error Handling**
   - Connection errors
   - Transmission errors
   - Error callback execution

10. **Performance Testing**
    - Stress testing
    - Performance metrics
    - Resource usage

## Building the Tests

### Prerequisites

- CMake 3.16 or higher
- C++17 compatible compiler
- uSockets library
- WolfSSL library

### Build Commands

```bash
# Create build directory
mkdir build
cd build

# Configure with CMake
cmake ..

# Build all tests
make

# Or build specific tests
make http_client_test
make tcp_test
make test_runner
```

### Build Options

The CMake configuration supports several options:

- `CMAKE_BUILD_TYPE`: Debug, Release, RelWithDebInfo, MinSizeRel
- `UWS_HTTP_MAX_HEADERS_SIZE`: Maximum HTTP headers size (default: 4096)
- `UWS_HTTP_MAX_HEADERS_COUNT`: Maximum HTTP headers count (default: 100)

## Running the Tests

### Individual Test Executables

```bash
# Run HttpClient tests only
./http_client_test

# Run TCP tests only
./tcp_test

# Run all tests with reporting
./test_runner
```

### Test Runner Options

The test runner supports several command-line options:

```bash
# Run all tests
./test_runner

# Run only HttpClient tests
./test_runner --http-client-only

# Run only TCP tests
./test_runner --tcp-only

# Show help
./test_runner --help
```

### Using CMake Targets

```bash
# Run all tests
make run_tests

# Run only HttpClient tests
make run_http_client_tests

# Run only TCP tests
make run_tcp_tests
```

### Using CTest

```bash
# Run all tests
ctest

# Run specific test
ctest -R HttpClientTests
ctest -R TCPTests

# Run with verbose output
ctest -V

# Run with parallel execution
ctest -j4
```

## Test Output

### Success Output

```
uWebSockets Test Suite
======================
Testing HttpClient and TCP functionality

Running HttpClient Tests...
==========================================
✓ HttpClient Creation PASSED
✓ HttpClient Behavior Configuration PASSED
✓ Connection Callbacks PASSED
...

HttpClient Test Summary:
==========================================
✓ HttpClient Creation (15ms)
✓ HttpClient Behavior Configuration (8ms)
✓ Connection Callbacks (1250ms)
...

Results: 10 passed, 0 failed
==========================================

Running TCP Tests...
==========================================
✓ TCP Context Creation PASSED
✓ TCP Behavior Configuration PASSED
...

TCP Test Summary:
==========================================
✓ TCP Context Creation (12ms)
✓ TCP Behavior Configuration (7ms)
...

Results: 15 passed, 0 failed
==========================================

Overall Test Summary:
=====================
Total execution time: 2847ms
✓ All test suites passed!
```

### Failure Output

```
✗ HttpClient Creation FAILED: Assertion failed: context != nullptr
✗ Connection Callbacks FAILED: Test timeout

HttpClient Test Summary:
==========================================
✗ HttpClient Creation - Assertion failed: context != nullptr (5ms)
✓ HttpClient Behavior Configuration (8ms)
✗ Connection Callbacks - Test timeout (5000ms)
...

Results: 8 passed, 2 failed
==========================================

Overall Test Summary:
=====================
Total execution time: 2847ms
✗ Some test suites failed!
  - HttpClient tests failed
```

## Test Configuration

### Timeout Settings

- **Individual test timeout**: 5 seconds (configurable)
- **Test suite timeout**: 5 minutes (configurable)
- **Overall timeout**: 10 minutes (configurable)

### Environment Variables

- `UWS_HTTP_MAX_HEADERS_SIZE`: Maximum HTTP headers size
- `UWS_HTTP_MAX_HEADERS_COUNT`: Maximum HTTP headers count

### Test Harness Features

The `TestHarness` class provides:

- **Automatic cleanup**: Proper resource cleanup on test completion
- **Timeout handling**: Configurable test timeouts
- **Error reporting**: Detailed error messages and stack traces
- **Performance tracking**: Test execution time measurement

## Adding New Tests

### HttpClient Test Template

```cpp
void testNewHttpClientFeature() {
    TestHarness test("New HttpClient Feature");
    
    // Test setup
    auto* context = HttpClientContext<false>::create(test.getLoop());
    assert(context != nullptr);
    
    // Test implementation
    // ... test logic ...
    
    // Verify results
    assert(/* expected condition */);
    
    // Cleanup
    context->free();
    test.pass();
}
```

### TCP Test Template

```cpp
void testNewTCPFeature() {
    TestHarness test("New TCP Feature");
    
    // Test setup
    auto* context = TCPContext<false>::create(test.getLoop());
    assert(context != nullptr);
    
    // Test implementation
    // ... test logic ...
    
    // Verify results
    assert(/* expected condition */);
    
    // Cleanup
    context->free();
    test.pass();
}
```

### Adding to Test Runner

1. Add the test function declaration to the appropriate test file
2. Add the test function call to the `main()` function
3. Update this README with test description

## Troubleshooting

### Common Issues

1. **Build failures**: Ensure all dependencies are properly installed and linked
2. **Test timeouts**: Increase timeout values for slow systems
3. **Memory leaks**: Use valgrind or similar tools for memory analysis
4. **SSL errors**: Ensure WolfSSL is properly configured

### Debug Mode

Build with debug information:

```bash
cmake -DCMAKE_BUILD_TYPE=Debug ..
make
```

### Verbose Output

Enable verbose test output:

```bash
./test_runner --verbose
ctest -V
```

## Contributing

When adding new tests:

1. Follow the existing test patterns
2. Include proper error handling
3. Add comprehensive assertions
4. Update documentation
5. Ensure tests pass on multiple platforms

## License

This test suite is licensed under the Apache License, Version 2.0.
