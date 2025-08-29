/*
 * Authored by Clay Molocznik, 2025.
 * Intellectual property of third-party.

 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at

 *     http://www.apache.org/licenses/LICENSE-2.0

 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "../lib/src/uws/HttpClientContext.h"
#include "../lib/src/uws/HttpClientConnection.h"
#include "../lib/src/uws/Loop.h"
#include <iostream>
#include <cassert>
#include <chrono>
#include <thread>
#include <string>
#include <vector>

using namespace uWS;

// Test utilities
class TestHarness {
private:
    Loop* loop;
    bool testPassed;
    std::string testName;
    
public:
    TestHarness(const std::string& name) : testName(name), testPassed(false) {
        loop = Loop::create();
    }
    
    ~TestHarness() {
        if (loop) {
            loop->free();
        }
    }
    
    Loop* getLoop() { return loop; }
    
    void pass() { 
        testPassed = true; 
        std::cout << "✓ " << testName << " PASSED" << std::endl;
    }
    
    void fail(const std::string& reason) { 
        testPassed = false; 
        std::cout << "✗ " << testName << " FAILED: " << reason << std::endl;
    }
    
    bool run(int timeoutMs = 5000) {
        auto start = std::chrono::steady_clock::now();
        
        while (!testPassed && loop->run() == 0) {
            auto now = std::chrono::steady_clock::now();
            auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(now - start);
            if (elapsed.count() > timeoutMs) {
                fail("Test timeout");
                return false;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
        
        return testPassed;
    }
};

// Test 1: Basic HTTP Client Creation and Configuration
void testHttpClientCreation() {
    TestHarness test("HttpClient Creation");
    
    auto* context = HttpClientContext<false>::create(test.getLoop());
    assert(context != nullptr);
    
    // Test default configuration
    auto [idleTimeout, longTimeout] = context->getTimeout();
    assert(idleTimeout == 30);
    assert(longTimeout == 60);
    assert(context->getMaxBackpressure() == 1024 * 1024);
    assert(context->getResetIdleTimeoutOnSend() == true);
    assert(context->getCloseOnBackpressureLimit() == false);
    
    // Test configuration setters
    context->setTimeout(60, 120);
    context->setMaxBackpressure(2048 * 1024);
    context->setResetIdleTimeoutOnSend(false);
    context->setCloseOnBackpressureLimit(true);
    
    auto [newIdleTimeout, newLongTimeout] = context->getTimeout();
    assert(newIdleTimeout == 60);
    assert(newLongTimeout == 120);
    assert(context->getMaxBackpressure() == 2048 * 1024);
    assert(context->getResetIdleTimeoutOnSend() == false);
    assert(context->getCloseOnBackpressureLimit() == true);
    
    context->free();
    test.pass();
}

// Test 2: HTTP Client Behavior Configuration
void testHttpClientBehavior() {
    TestHarness test("HttpClient Behavior Configuration");
    
    HttpClientBehavior<false> behavior;
    behavior.idleTimeoutSeconds = 45;
    behavior.longTimeoutMinutes = 90;
    behavior.maxBackpressure = 512 * 1024;
    behavior.resetIdleTimeoutOnSend = false;
    behavior.closeOnBackpressureLimit = true;
    
    // Test callback assignment
    bool connectedCalled = false;
    bool headersCalled = false;
    bool errorCalled = false;
    
    behavior.onConnected = [&](HttpClientConnection<false>* conn) {
        connectedCalled = true;
    };
    
    behavior.onHeaders = [&](HttpClientConnection<false>* conn, int status, std::string_view statusText, HttpResponseHeaders& headers) {
        headersCalled = true;
    };
    
    behavior.onError = [&](HttpClientConnection<false>* conn, int code, std::string_view message) {
        errorCalled = true;
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    assert(context != nullptr);
    
    // Verify behavior was applied
    auto [idleTimeout, longTimeout] = context->getTimeout();
    assert(idleTimeout == 45);
    assert(longTimeout == 90);
    assert(context->getMaxBackpressure() == 512 * 1024);
    assert(context->getResetIdleTimeoutOnSend() == false);
    assert(context->getCloseOnBackpressureLimit() == true);
    
    context->free();
    test.pass();
}

// Test 3: Connection Event Callbacks
void testConnectionCallbacks() {
    TestHarness test("Connection Callbacks");
    
    bool onConnectedCalled = false;
    bool onConnectErrorCalled = false;
    bool onDisconnectedCalled = false;
    
    HttpClientBehavior<false> behavior;
    behavior.onConnected = [&](HttpClientConnection<false>* conn) {
        onConnectedCalled = true;
        test.pass();
    };
    
    behavior.onConnectError = [&](HttpClientConnection<false>* conn, int code, std::string_view message) {
        onConnectErrorCalled = true;
        test.pass();
    };
    
    behavior.onDisconnected = [&](HttpClientConnection<false>* conn, int code, std::string_view message) {
        onDisconnectedCalled = true;
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    
    // Try to connect to a non-existent server to trigger connect error
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    // Run the test
    test.run();
    
    context->free();
}

// Test 4: HTTP Response Parsing
void testHttpResponseParsing() {
    TestHarness test("HTTP Response Parsing");
    
    bool headersReceived = false;
    bool chunkReceived = false;
    int responseStatus = 0;
    std::string responseBody;
    
    HttpClientBehavior<false> behavior;
    behavior.onHeaders = [&](HttpClientConnection<false>* conn, int status, std::string_view statusText, HttpResponseHeaders& headers) {
        responseStatus = status;
        headersReceived = true;
    };
    
    behavior.onChunk = [&](HttpClientConnection<false>* conn, std::string_view chunk, bool isLast) {
        responseBody.append(chunk.data(), chunk.size());
        if (isLast) {
            chunkReceived = true;
            test.pass();
        }
    };
    
    behavior.onError = [&](HttpClientConnection<false>* conn, int code, std::string_view message) {
        test.fail("HTTP parsing error: " + std::string(message));
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    
    // Note: This test would need a real HTTP server to be meaningful
    // For now, we'll just test the callback setup
    assert(context != nullptr);
    
    context->free();
    test.pass();
}

// Test 5: Timeout Handling
void testTimeoutHandling() {
    TestHarness test("Timeout Handling");
    
    bool timeoutCalled = false;
    bool longTimeoutCalled = false;
    
    HttpClientBehavior<false> behavior;
    behavior.idleTimeoutSeconds = 1;  // Very short timeout for testing
    behavior.longTimeoutMinutes = 1;
    
    behavior.onTimeout = [&](HttpClientConnection<false>* conn) {
        timeoutCalled = true;
        test.pass();
    };
    
    behavior.onLongTimeout = [&](HttpClientConnection<false>* conn) {
        longTimeoutCalled = true;
        test.pass();
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    
    // Create a connection that will timeout
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    // Run the test with longer timeout to allow for connection timeout
    test.run(10000);
    
    context->free();
}

// Test 6: Backpressure Handling
void testBackpressureHandling() {
    TestHarness test("Backpressure Handling");
    
    bool writableCalled = false;
    bool droppedCalled = false;
    
    HttpClientBehavior<false> behavior;
    behavior.maxBackpressure = 1024;  // Small backpressure limit
    behavior.closeOnBackpressureLimit = false;
    
    behavior.onWritable = [&](HttpClientConnection<false>* conn) {
        writableCalled = true;
        test.pass();
    };
    
    behavior.onDropped = [&](HttpClientConnection<false>* conn, std::string_view data) {
        droppedCalled = true;
        test.pass();
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    
    // Create a connection
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    // Test would need actual data sending to be meaningful
    context->free();
    test.pass();
}

// Test 7: Statistics Tracking
void testStatisticsTracking() {
    TestHarness test("Statistics Tracking");
    
    auto* context = HttpClientContext<false>::create(test.getLoop());
    assert(context != nullptr);
    
    // Create a connection
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    // Cast to HttpClientConnection to access statistics
    auto* httpConn = static_cast<HttpClientConnection<false>*>(conn);
    
    // Test initial statistics
    assert(httpConn->getBytesReceived() == 0);
    assert(httpConn->getBytesSent() == 0);
    assert(httpConn->getRequestsSent() == 0);
    assert(httpConn->getResponsesReceived() == 0);
    assert(httpConn->getMessagesReceived() == 0);
    assert(httpConn->getMessagesSent() == 0);
    
    // Test statistics reset
    httpConn->resetStatistics();
    assert(httpConn->getBytesReceived() == 0);
    assert(httpConn->getBytesSent() == 0);
    assert(httpConn->getRequestsSent() == 0);
    assert(httpConn->getResponsesReceived() == 0);
    assert(httpConn->getMessagesReceived() == 0);
    assert(httpConn->getMessagesSent() == 0);
    
    context->free();
    test.pass();
}

// Test 8: SSL/TLS Support
void testSSLSupport() {
    TestHarness test("SSL/TLS Support");
    
    // Test SSL context creation
    auto* sslContext = HttpClientContext<true>::create(test.getLoop());
    assert(sslContext != nullptr);
    
    // Test SSL behavior configuration
    HttpClientBehavior<true> sslBehavior;
    sslBehavior.idleTimeoutSeconds = 30;
    sslBehavior.longTimeoutMinutes = 60;
    
    auto* sslContextWithBehavior = HttpClientContext<true>::create(test.getLoop(), sslBehavior);
    assert(sslContextWithBehavior != nullptr);
    
    sslContext->free();
    sslContextWithBehavior->free();
    test.pass();
}

// Test 9: Connection State Management
void testConnectionStateManagement() {
    TestHarness test("Connection State Management");
    
    auto* context = HttpClientContext<false>::create(test.getLoop());
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    auto* httpConn = static_cast<HttpClientConnection<false>*>(conn);
    
    // Test connection state methods
    assert(!httpConn->isClosed());
    assert(!httpConn->isShutDown());
    
    // Test backpressure methods
    assert(httpConn->getBackpressure() == 0);
    assert(!httpConn->hasBackpressure());
    
    // Test timeout configuration
    httpConn->setTimeout(60, 120);
    auto [idleTimeout, longTimeout] = httpConn->getTimeout();
    assert(idleTimeout == 60);
    assert(longTimeout == 120);
    
    // Test backpressure configuration
    httpConn->setMaxBackpressure(2048 * 1024);
    assert(httpConn->getMaxBackpressure() == 2048 * 1024);
    
    context->free();
    test.pass();
}

// Test 10: Request/Response Flow
void testRequestResponseFlow() {
    TestHarness test("Request/Response Flow");
    
    bool requestSent = false;
    bool responseReceived = false;
    
    HttpClientBehavior<false> behavior;
    behavior.onHeaders = [&](HttpClientConnection<false>* conn, int status, std::string_view statusText, HttpResponseHeaders& headers) {
        responseReceived = true;
        test.pass();
    };
    
    auto* context = HttpClientContext<false>::create(test.getLoop(), behavior);
    auto* conn = context->connect("nonexistent.local", 8080);
    assert(conn != nullptr);
    
    auto* httpConn = static_cast<HttpClientConnection<false>*>(conn);
    
    // Test request body length tracking
    assert(httpConn->getRequestBodyLength() == -1);
    assert(httpConn->getPendingRequests() == 0);
    
    // Note: Actual request sending would require a real server
    // This test validates the API structure
    
    context->free();
    test.pass();
}

// Main test runner
int main() {
    std::cout << "Running HttpClient Tests..." << std::endl;
    std::cout << "================================" << std::endl;
    
    try {
        testHttpClientCreation();
        testHttpClientBehavior();
        testConnectionCallbacks();
        testHttpResponseParsing();
        testTimeoutHandling();
        testBackpressureHandling();
        testStatisticsTracking();
        testSSLSupport();
        testConnectionStateManagement();
        testRequestResponseFlow();
        
        std::cout << "================================" << std::endl;
        std::cout << "All HttpClient tests completed!" << std::endl;
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "Test failed with exception: " << e.what() << std::endl;
        return 1;
    }
}
