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
## AOF2 Manifest Log Tests

### MAN1/MAN3 crash-mid-commit replay

1. Enable the manifest log path before running the test:
   - Set `AOF2_MANIFEST_LOG=1` (or toggle `aof2_manifest_log_enabled` in `AofManagerConfig`) so Tier 1 writes go to the append-only log.
   - Execute `cargo test manifest_crash_mid_commit -- --nocapture` to reproduce the crash/trim/replay sequence end to end.
2. Verify metrics once the test completes:
   - `aof_manifest_replay_chunk_lag_seconds` should remain below 5 seconds; the updated test snapshots this via `ManifestReplayMetrics`.
   - `aof_manifest_replay_journal_lag_bytes` must equal the trimmed tail (expect the single uncommitted record to report its byte count).
   - Run with `RUST_LOG=info` if you need to capture metric deltas in the console output for manual comparison.
3. Collect the generated artifacts for inspection:
   - Trimmed chunk: `<tmp>/000000000000000042/000000000000000000.mlog` (the test stream id is 42; replace `<tmp>` with your OS temp dir).
   - Journal snapshot before trim is available if you copy the same file prior to `set_len`; compare committed vs actual length for manual audits.
   - Record both metrics above alongside the artifact, plus the boolean indicating whether Tier 2 delete was queued.
4. Cross-check the behaviour against `docs/aof2_manifest_log.md` (crash semantics) and the replay section in `docs/aof2/aof2_store.md`.
5. If parity issues appear, disable the manifest log feature flag to fall back to the legacy JSON manifest while you investigate, then re-run the test to confirm metrics reset to zero.


### MAN2 log-only replay

1. Enable the manifest log (`AOF2_MANIFEST_LOG_ENABLED=1`) and turn on log-only mode (`AOF2_MANIFEST_LOG_ONLY=1`) before running validation.
2. Execute `cargo test manifest_log_only_replay_bootstrap -- --nocapture` to exercise Tier1 bootstrap from log snapshots, residency updates, and Tier2 promotions.
3. Confirm the test output reports trimmed journal bytes, and inspect the replay metrics/Grafana panels to ensure chunk lag stays <5s.
4. To revert, clear `AOF2_MANIFEST_LOG_ONLY`, rerun the test, and verify the warm manifest JSON is repopulated alongside the log snapshots.

### MAN3 Tier2 retry flow

1. Keep the manifest log enabled (`AOF2_MANIFEST_LOG_ENABLED=1`); no other flags are required because the test drives Tier1 in mixed JSON/log mode.
2. Run `cargo test manifest_tier2_retry_flow -- --nocapture` to simulate a flaky Tier2 upload/delete cycle. The harness injects a single failure for each operation and waits for Tier1 to hydrate from the manifest log before verifying recovery.
3. Verify the output shows at least two upload/delete attempts (the `FlakyTier2Client` prints the counts) and that `manifest_snapshot` no longer contains the original segment after eviction.
4. For manual inspection, point `ManifestInspector::inspect_stream` at the temp manifest directory printed by the test; you should see the newest compression entry plus replay snapshots for the surviving segment.
5. Cross-reference the retry behaviour with `docs/aof2/aof2_store.md` (Tier2 operations) and `docs/aof2_manifest_log.md` (failure semantics) if additional debugging is required.

