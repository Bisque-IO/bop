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

#include "../lib/src/uws/TCPContext.h"
#include "../lib/src/uws/TCPConnection.h"
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
        loop = Loop::get();
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

        std::thread([&]() {
            loop->run();
        }).detach();

        std::mutex mu{};
        mu.lock();
        loop->defer([&]() {
            while (!testPassed) {
                auto now = std::chrono::steady_clock::now();
                auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(now - start);
                if (elapsed.count() > timeoutMs) {
                    fail("Test timeout");
                    return false;
                }
                std::this_thread::sleep_for(std::chrono::milliseconds(10));
            }

            mu.unlock();
        });
        
        return testPassed;
    }
};

// Test 1: Basic TCP Context Creation and Configuration
void testTCPContextCreation() {
    TestHarness test("TCP Context Creation");
    
    auto* context = TCPContext<false>::create(test.getLoop());
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

// // Test 2: TCP Behavior Configuration
// void testTCPBehavior() {
//     TestHarness test("TCP Behavior Configuration");
    
//     TCPBehavior<false> behavior;
//     behavior.idleTimeoutSeconds = 45;
//     behavior.longTimeoutMinutes = 90;
//     behavior.maxBackpressure = 512 * 1024;
//     behavior.resetIdleTimeoutOnSend = false;
//     behavior.closeOnBackpressureLimit = true;
    
//     // Test callback assignment
//     bool connectedCalled = false;
//     bool dataReceivedCalled = false;
//     bool errorCalled = false;
    
//     behavior.onConnection = [&](TCPConnection<false>* conn) {
//         connectedCalled = true;
//     };
    
//     behavior.onData = [&](TCPConnection<false>* conn, std::string_view data) {
//         dataReceivedCalled = true;
//     };
    
//     // behavior.onError = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//     //     errorCalled = true;
//     // };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
//     assert(context != nullptr);
    
//     // Verify behavior was applied
//     auto [idleTimeout, longTimeout] = context->getTimeout();
//     assert(idleTimeout == 45);
//     assert(longTimeout == 90);
//     assert(context->getMaxBackpressure() == 512 * 1024);
//     assert(context->getResetIdleTimeoutOnSend() == false);
//     assert(context->getCloseOnBackpressureLimit() == true);
    
//     context->free();
//     test.pass();
// }

// // Test 3: TCP Connection Event Callbacks
// void testTCPConnectionCallbacks() {
//     TestHarness test("TCP Connection Callbacks");
    
//     bool onConnectionCalled = false;
//     bool onConnectErrorCalled = false;
//     bool onDisconnectedCalled = false;
    
//     TCPBehavior<false> behavior;
//     behavior.onConnection = [&](TCPConnection<false>* conn) {
//         onConnectionCalled = true;
//         test.pass();
//     };
    
//     behavior.onConnectError = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//         onConnectErrorCalled = true;
//         test.pass();
//     };
    
//     behavior.onDisconnected = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//         onDisconnectedCalled = true;
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Try to connect to a non-existent server to trigger connect error
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Run the test
//     test.run();
    
//     context->free();
// }

// // Test 4: TCP Data Transmission
// void testTCPDataTransmission() {
//     TestHarness test("TCP Data Transmission");
    
//     bool dataReceived = false;
//     std::string receivedData;
    
//     TCPBehavior<false> behavior;
//     behavior.onData = [&](TCPConnection<false>* conn, std::string_view data) {
//         receivedData.append(data.data(), data.size());
//         dataReceived = true;
//         test.pass();
//     };
    
//     // behavior.onError = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//     //     test.fail("TCP error: " + std::string(message));
//     // };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Note: This test would need a real TCP server to be meaningful
//     // For now, we'll just test the callback setup
//     assert(context != nullptr);
    
//     context->free();
//     test.pass();
// }

// // Test 5: TCP Timeout Handling
// void testTCPTimeoutHandling() {
//     TestHarness test("TCP Timeout Handling");
    
//     bool timeoutCalled = false;
//     bool longTimeoutCalled = false;
    
//     TCPBehavior<false> behavior;
//     behavior.idleTimeoutSeconds = 1;  // Very short timeout for testing
//     behavior.longTimeoutMinutes = 1;
    
//     behavior.onTimeout = [&](TCPConnection<false>* conn) {
//         timeoutCalled = true;
//         test.pass();
//     };
    
//     behavior.onLongTimeout = [&](TCPConnection<false>* conn) {
//         longTimeoutCalled = true;
//         test.pass();
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Create a connection that will timeout
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Run the test with longer timeout to allow for connection timeout
//     test.run(10000);
    
//     context->free();
// }

// // Test 6: TCP Backpressure Handling
// void testTCPBackpressureHandling() {
//     TestHarness test("TCP Backpressure Handling");
    
//     bool writableCalled = false;
//     bool droppedCalled = false;
    
//     TCPBehavior<false> behavior;
//     behavior.maxBackpressure = 1024;  // Small backpressure limit
//     behavior.closeOnBackpressureLimit = false;
    
//     behavior.onWritable = std::move([&](TCPConnection<false>* conn) {
//         writableCalled = true;
//         test.pass();
//     });
    
//     behavior.onDropped = std::move([&](TCPConnection<false>* conn, std::string_view data) {
//         droppedCalled = true;
//         test.pass();
//     });
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Create a connection
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Test would need actual data sending to be meaningful
//     context->free();
//     test.pass();
// }

// // Test 7: TCP Statistics Tracking
// void testTCPStatisticsTracking() {
//     TestHarness test("TCP Statistics Tracking");
    
//     auto* context = TCPContext<false>::create(test.getLoop());
//     assert(context != nullptr);
    
//     // Create a connection
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Cast to TCPConnection to access statistics
//     auto* tcpConn = reinterpret_cast<TCPConnection<false>*>(conn);
    
//     // Test initial statistics
//     assert(tcpConn->getBytesReceived() == 0);
//     assert(tcpConn->getBytesSent() == 0);
//     assert(tcpConn->getMessagesReceived() == 0);
//     assert(tcpConn->getMessagesSent() == 0);
    
//     // Test statistics reset
//     tcpConn->resetStatistics();
//     assert(tcpConn->getBytesReceived() == 0);
//     assert(tcpConn->getBytesSent() == 0);
//     assert(tcpConn->getMessagesReceived() == 0);
//     assert(tcpConn->getMessagesSent() == 0);
    
//     context->free();
//     test.pass();
// }

// // Test 8: TCP SSL/TLS Support
// void testTCPSSLSupport() {
//     TestHarness test("TCP SSL/TLS Support");
    
//     // Test SSL context creation
//     auto* sslContext = TCPContext<true>::create(test.getLoop());
//     assert(sslContext != nullptr);
    
//     // Test SSL behavior configuration
//     TCPBehavior<true> sslBehavior;
//     sslBehavior.idleTimeoutSeconds = 30;
//     sslBehavior.longTimeoutMinutes = 60;
    
//     auto* sslContextWithBehavior = TCPContext<true>::create(test.getLoop(), sslBehavior);
//     assert(sslContextWithBehavior != nullptr);
    
//     sslContext->free();
//     sslContextWithBehavior->free();
//     test.pass();
// }

// // Test 9: TCP Connection State Management
// void testTCPConnectionStateManagement() {
//     TestHarness test("TCP Connection State Management");
    
//     auto* context = TCPContext<false>::create(test.getLoop());
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     auto* tcpConn = static_cast<TCPConnection<false>*>(conn);
    
//     // Test connection state methods
//     assert(!tcpConn->isClosed());
//     assert(!tcpConn->isShutDown());
    
//     // Test backpressure methods
//     assert(tcpConn->getBackpressure() == 0);
//     assert(!tcpConn->hasBackpressure());
    
//     // Test timeout configuration
//     tcpConn->setTimeout(60, 120);
//     auto [idleTimeout, longTimeout] = tcpConn->getTimeout();
//     assert(idleTimeout == 60);
//     assert(longTimeout == 120);
    
//     // Test backpressure configuration
//     tcpConn->setMaxBackpressure(2048 * 1024);
//     assert(tcpConn->getMaxBackpressure() == 2048 * 1024);
    
//     context->free();
//     test.pass();
// }

// // Test 10: TCP Send/Receive Operations
// void testTCPSendReceiveOperations() {
//     TestHarness test("TCP Send/Receive Operations");
    
//     bool dataReceived = false;
//     std::string receivedData;
    
//     TCPBehavior<false> behavior;
//     behavior.onData = [&](TCPConnection<false>* conn, std::string_view data) {
//         receivedData.append(data.data(), data.size());
//         dataReceived = true;
//         test.pass();
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     auto* tcpConn = static_cast<TCPConnection<false>*>(conn);
    
//     // Test send operations (would need real server for actual testing)
//     std::string testData = "Hello, TCP!";
    
//     // Note: Actual sending would require a real server
//     // This test validates the API structure
    
//     context->free();
//     test.pass();
// }

// // Test 11: TCP Server Functionality
// void testTCPServerFunctionality() {
//     TestHarness test("TCP Server Functionality");
    
//     bool serverStarted = false;
//     bool clientConnected = false;
    
//     TCPBehavior<false> serverBehavior;
//     serverBehavior.onConnection = [&](TCPConnection<false>* conn) {
//         clientConnected = true;
//         test.pass();
//     };
    
//     auto* serverContext = TCPContext<false>::create(test.getLoop(), serverBehavior);
//     assert(serverContext != nullptr);
    
//     // Note: Server listening would require proper setup
//     // This test validates the server API structure
    
//     serverContext->free();
//     test.pass();
// }

// // Test 12: TCP Connection Lifecycle
// void testTCPConnectionLifecycle() {
//     TestHarness test("TCP Connection Lifecycle");
    
//     bool connectionEstablished = false;
//     bool connectionClosed = false;
    
//     TCPBehavior<false> behavior;
//     behavior.onConnection = [&](TCPConnection<false>* conn) {
//         connectionEstablished = true;
        
//         // Test connection methods
//         assert(!conn->isClosed());
//         assert(!conn->isShutDown());
        
//         // Close the connection
//         conn->close();
//     };
    
//     behavior.onDisconnected = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//         connectionClosed = true;
//         test.pass();
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Create a connection
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Run the test
//     test.run();
    
//     context->free();
// }

// // Test 13: TCP Error Handling
// void testTCPErrorHandling() {
//     TestHarness test("TCP Error Handling");
    
//     bool errorHandled = false;
//     int errorCode = 0;
//     std::string errorMessage;
    
//     TCPBehavior<false> behavior;
//     behavior.onError = [&](TCPConnection<false>* conn, int code, std::string_view message) {
//         errorCode = code;
//         errorMessage = std::string(message);
//         errorHandled = true;
//         test.pass();
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
    
//     // Create a connection that will likely fail
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     // Run the test
//     test.run();
    
//     context->free();
// }

// // Test 14: TCP Performance and Stress Testing
// void testTCPPerformance() {
//     TestHarness test("TCP Performance Testing");
    
//     int messagesReceived = 0;
//     int messagesSent = 0;
    
//     TCPBehavior<false> behavior;
//     behavior.onData = [&](TCPConnection<false>* conn, std::string_view data) {
//         messagesReceived++;
//     };
    
//     auto* context = TCPContext<false>::create(test.getLoop(), behavior);
//     auto* conn = context->connect("nonexistent.local", 8080);
//     assert(conn != nullptr);
    
//     auto* tcpConn = static_cast<TCPConnection<false>*>(conn);
    
//     // Test performance-related methods
//     assert(tcpConn->getBackpressure() == 0);
//     assert(!tcpConn->hasBackpressure());
    
//     // Note: Actual performance testing would require real server/client setup
    
//     context->free();
//     test.pass();
// }

// // Test 15: TCP Configuration Validation
// void testTCPConfigurationValidation() {
//     TestHarness test("TCP Configuration Validation");
    
//     auto* context = TCPContext<false>::create(test.getLoop());
//     assert(context != nullptr);
    
//     // Test various configuration combinations
//     context->setTimeout(0, 0);  // Zero timeouts
//     auto [idleTimeout, longTimeout] = context->getTimeout();
//     assert(idleTimeout == 0);
//     assert(longTimeout == 0);
    
//     context->setMaxBackpressure(0);  // No backpressure limit
//     assert(context->getMaxBackpressure() == 0);
    
//     context->setMaxBackpressure(1);  // Very small backpressure limit
//     assert(context->getMaxBackpressure() == 1);
    
//     context->setMaxBackpressure(UINTMAX_MAX);  // Very large backpressure limit
//     assert(context->getMaxBackpressure() == UINTMAX_MAX);
    
//     context->free();
//     test.pass();
// }

// Main test runner
int main() {
    std::cout << "Running TCP Tests..." << std::endl;
    std::cout << "================================" << std::endl;
    
    try {
        testTCPContextCreation();
        // testTCPBehavior();
        // testTCPConnectionCallbacks();
        // testTCPDataTransmission();
        // testTCPTimeoutHandling();
        // testTCPBackpressureHandling();
        // testTCPStatisticsTracking();
        // testTCPSSLSupport();
        // testTCPConnectionStateManagement();
        // testTCPSendReceiveOperations();
        // testTCPServerFunctionality();
        // testTCPConnectionLifecycle();
        // testTCPErrorHandling();
        // testTCPPerformance();
        // testTCPConfigurationValidation();
        
        std::cout << "================================" << std::endl;
        std::cout << "All TCP tests completed!" << std::endl;
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "Test failed with exception: " << e.what() << std::endl;
        return 1;
    }
}
