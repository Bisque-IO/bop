/*
 * Authored by Clay Molocznik, 2025.
 * Intellectual property of third-party.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "TCPApp.h"
#include "TCPConnection.h"
#include "TCPContext.h"
#include "TCPBehavior.h"
#include <iostream>

using namespace uWS;

int main() {
    std::cout << "=== Unified TCP Server and Client Example ===" << std::endl;

    /* Example 1: Simple TCP Server */
    std::cout << "\n--- Example 1: Simple TCP Server ---" << std::endl;
    
    TCPApp server;
    
    /* Create a TCP context with behavior configuration */
    TCPBehavior<false> serverBehavior;
    serverBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        std::cout << "Server: New connection from " << connection->getRemoteAddress() << std::endl;
        std::cout << "Server: Connection is " << (connection->isClient() ? "client" : "server") << std::endl;
        
        /* Set up connection event handlers */
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "Server: Received: " << data << std::endl;
            
            /* Get the connection object */
            TCPConnection<false, void>* conn = (TCPConnection<false, void>*)connData;
            
            /* Echo the data back */
            conn->send("Echo: " + std::string(data));
        });
        
        connection->onDisconnected([](auto* connData, int code, void* reason) {
            std::cout << "Server: Connection closed with code " << code << std::endl;
        });
    };
    
    /* Create context and start listening */
    auto* context = server.createTCPContext(serverBehavior);
    server.listen(3000, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "Server: Listening on port 3000" << std::endl;
        } else {
            std::cout << "Server: Failed to listen on port 3000" << std::endl;
        }
    });

    /* Example 2: Simple TCP Client */
    std::cout << "\n--- Example 2: Simple TCP Client ---" << std::endl;
    
    TCPApp client;
    
    /* Create a client context */
    TCPBehavior<false> clientBehavior;
    clientBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        std::cout << "Client: Connected to " << connection->getRemoteAddress() << std::endl;
        std::cout << "Client: Connection is " << (connection->isClient() ? "client" : "server") << std::endl;
        
        /* Set up data handler */
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "Client: Received: " << data << std::endl;
        });
        
        /* Send a test message */
        connection->send("Hello, Server!");
    };
    
    auto* clientContext = client.createTCPContext(clientBehavior);
    
    /* Connect to the server */
    auto* clientSocket = client.connect("localhost", 3000);
    if (clientSocket) {
        std::cout << "Client: Connecting to localhost:3000" << std::endl;
    }

    /* Example 3: Advanced TCP Server with Multiple Contexts */
    std::cout << "\n--- Example 3: Advanced TCP Server ---" << std::endl;
    
    TCPApp advancedServer;
    
    /* Create multiple contexts for different purposes */
    TCPBehavior<false> apiBehavior;
    apiBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        std::cout << "API Server: New API connection" << std::endl;
        
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "API Server: API request: " << data << std::endl;
            
            TCPConnection<false, void>* conn = (TCPConnection<false, void>*)connData;
            conn->send("API Response: " + std::string(data));
        });
    };
    
    TCPBehavior<false> dataBehavior;
    dataBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        std::cout << "Data Server: New data connection" << std::endl;
        
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "Data Server: Data request: " << data << std::endl;
            
            TCPConnection<false, void>* conn = (TCPConnection<false, void>*)connData;
            conn->send("Data Response: " + std::string(data));
        });
    };
    
    /* Create contexts */
    auto* apiContext = advancedServer.createTCPContext(apiBehavior);
    auto* dataContext = advancedServer.createTCPContext(dataBehavior);
    
    /* Set default context for simple listen calls */
    advancedServer.setDefaultContext(apiContext);
    
    /* Listen on different ports */
    advancedServer.listen(3001, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "Advanced Server: API listening on port 3001" << std::endl;
        }
    });
    
    /* Use specific context for data port */
    advancedServer.setDefaultContext(dataContext);
    advancedServer.listen(3002, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "Advanced Server: Data listening on port 3002" << std::endl;
        }
    });

    /* Example 4: Connection Configuration */
    std::cout << "\n--- Example 4: Connection Configuration ---" << std::endl;
    
    TCPBehavior<false> configBehavior;
    configBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        std::cout << "Config Server: New connection" << std::endl;
        
        /* Configure connection settings */
        connection->setTimeout(60, 120);  // 60 seconds idle, 120 minutes long timeout
        connection->setMaxBackpressure(1024 * 1024);  // 1MB max backpressure
        connection->setResetIdleTimeoutOnSend(true);
        connection->setCloseOnBackpressureLimit(false);
        
        /* Set up event handlers */
        connection->onTimeout([](auto* connData) {
            std::cout << "Config Server: Connection timed out" << std::endl;
        });
        
        connection->onLongTimeout([](auto* connData) {
            std::cout << "Config Server: Connection long timed out" << std::endl;
        });
        
        connection->onDrain([](auto* connData) {
            std::cout << "Config Server: Backpressure drained" << std::endl;
        });
        
        connection->onDropped([](auto* connData, std::string_view data) {
            std::cout << "Config Server: Message dropped due to backpressure" << std::endl;
        });
        
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "Config Server: Received: " << data << std::endl;
            
            TCPConnection<false, void>* conn = (TCPConnection<false, void>*)connData;
            
            /* Use corking for multiple sends */
            conn->cork([conn, data]() {
                conn->send("Response 1: " + std::string(data));
                conn->send("Response 2: " + std::string(data));
                conn->send("Response 3: " + std::string(data));
            });
        });
    };
    
    auto* configContext = advancedServer.createTCPContext(configBehavior);
    advancedServer.setDefaultContext(configContext);
    
    advancedServer.listen(3003, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "Advanced Server: Config listening on port 3003" << std::endl;
        }
    });

    /* Example 5: SSL TCP Server and Client */
    std::cout << "\n--- Example 5: SSL TCP Server and Client ---" << std::endl;
    
    SSLTCPApp sslServer;
    SSLTCPApp sslClient;
    
    /* Create SSL contexts */
    auto* sslServerContext = sslServer.createTCPContext();
    auto* sslClientContext = sslClient.createTCPContext();
    
    /* Set up SSL server behavior */
    TCPBehavior<true> sslServerBehavior;
    sslServerBehavior.onConnection = [](TCPConnection<true, void>* connection) {
        std::cout << "SSL Server: New SSL connection" << std::endl;
        
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "SSL Server: Received: " << data << std::endl;
            
            TCPConnection<true, void>* conn = (TCPConnection<true, void>*)connData;
            conn->send("SSL Echo: " + std::string(data));
        });
    };
    
    /* Configure SSL server context */
    sslServer.setDefaultContext(sslServerContext);
    
    /* Set up SSL client behavior */
    TCPBehavior<true> sslClientBehavior;
    sslClientBehavior.onConnection = [](TCPConnection<true, void>* connection) {
        std::cout << "SSL Client: Connected to " << connection->getRemoteAddress() << std::endl;
        
        connection->onData([](auto* connData, std::string_view data) {
            std::cout << "SSL Client: Received: " << data << std::endl;
        });
        
        /* Send a test message */
        connection->send("Hello, SSL Server!");
    };
    
    auto* sslClientContext2 = sslClient.createTCPContext(sslClientBehavior);
    
    /* Listen and connect */
    sslServer.listen(3004, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "SSL Server: Listening on port 3004" << std::endl;
        }
    });
    
    auto* sslClientSocket = sslClient.connect("localhost", 3004);
    if (sslClientSocket) {
        std::cout << "SSL Client: Connecting to localhost:3004" << std::endl;
    }

    /* Example 6: Context Management */
    std::cout << "\n--- Example 6: Context Management ---" << std::endl;
    
    TCPApp managedServer;
    
    /* Create and manage contexts */
    auto* context1 = managedServer.createTCPContext();
    auto* context2 = managedServer.createTCPContext();
    
    std::cout << "Created contexts: " << (context1 ? "context1" : "none") 
              << ", " << (context2 ? "context2" : "none") << std::endl;
    
    /* Check if server has context */
    std::cout << "Server has context: " << (managedServer.hasContext() ? "yes" : "no") << std::endl;
    
    /* Delete a context */
    managedServer.deleteContext(context1);
    std::cout << "Deleted context1" << std::endl;
    
    /* Clear all contexts */
    managedServer.clear();
    std::cout << "Cleared all contexts" << std::endl;

    /* Example 7: Unified Server and Client in Same App */
    std::cout << "\n--- Example 7: Unified Server and Client ---" << std::endl;
    
    TCPApp unifiedApp;
    
    /* Create a context that handles both server and client connections */
    TCPBehavior<false> unifiedBehavior;
    unifiedBehavior.onConnection = [](TCPConnection<false, void>* connection) {
        if (connection->isClient()) {
            std::cout << "Unified: Client connection established" << std::endl;
            
            connection->onData([](auto* connData, std::string_view data) {
                std::cout << "Unified Client: Received: " << data << std::endl;
            });
            
            /* Send a message from client */
            connection->send("Hello from client!");
        } else {
            std::cout << "Unified: Server connection from " << connection->getRemoteAddress() << std::endl;
            
            connection->onData([](auto* connData, std::string_view data) {
                std::cout << "Unified Server: Received: " << data << std::endl;
                
                TCPConnection<false, void>* conn = (TCPConnection<false, void>*)connData;
                conn->send("Hello from server!");
            });
        }
    };
    
    auto* unifiedContext = unifiedApp.createTCPContext(unifiedBehavior);
    
    /* Start server */
    unifiedApp.listen(3005, [](auto* listen_socket) {
        if (listen_socket) {
            std::cout << "Unified: Server listening on port 3005" << std::endl;
        }
    });
    
    /* Connect as client */
    auto* unifiedClientSocket = unifiedApp.connect("localhost", 3005);
    if (unifiedClientSocket) {
        std::cout << "Unified: Client connecting to localhost:3005" << std::endl;
    }

    std::cout << "\n=== Unified TCP Example Complete ===" << std::endl;
    
    /* Note: In a real application, you would run the event loop here */
    /* For this example, we're just demonstrating the API */
    
    return 0;
}
