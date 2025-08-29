/*
 * Example demonstrating the lean HttpClient architecture
 * with proper HTTP streaming support (headers and body in both directions)
 */

#include "ClientApp.h"
#include "HttpClientRequest.h"
#include "Loop.h"
#include <iostream>

using namespace uWS;

int main() {
    /* Create client app */
    ClientApp app;
    
    /* Create HTTP request to server */
    auto* request = app.request("httpbin.org", 80);
    if (!request) {
        std::cerr << "Failed to create request" << std::endl;
        return 1;
    }
    
    /* Configure the request */
    request->setRequest("POST", "/post");
    request->addHeader("Content-Type", "application/json");
    request->addHeader("User-Agent", "uWS/1.0");
    
    /* Set request body */
    request->setBody("{\"message\": \"Hello from uWS!\"}");
    
    /* Set up request handlers */
    request->onHeaders([](HttpClientRequestData<false>* reqData, int status, const HttpResponseHeaders::Header* headers, size_t headerCount) {
        std::cout << "Response headers received: " << status << std::endl;
        
        /* Print response headers */
        for (size_t i = 0; i < headerCount; i++) {
            std::cout << "  " << headers[i].name << ": " << headers[i].value << std::endl;
        }
        
        reqData->responseReceived = true;
    });
    
    request->onChunk([](HttpClientRequestData<false>* reqData, std::string_view data, bool isLast) {
        std::cout << "Response body chunk: " << data.size() << " bytes" << (isLast ? " (last)" : "") << std::endl;
        std::cout << "Data: " << data << std::endl;
        
        if (isLast) {
            reqData->responseBodyComplete = true;
            std::cout << "Response complete" << std::endl;
        }
    });
    
    request->onError([](HttpClientRequestData<false>* reqData, int code) {
        std::cout << "Request error: " << code << std::endl;
    });
    
    /* Send the request */
    SendStatus status = request->sendRequest();
    if (status != SendStatus::SUCCESS && status != SendStatus::BACKPRESSURE) {
        std::cerr << "Failed to send request: " << (int)status << std::endl;
        return 1;
    }
    
    /* Example with chunked encoding */
    auto* chunkedRequest = app.request("httpbin.org", 80);
    if (chunkedRequest) {
        chunkedRequest->setRequest("POST", "/post");
        chunkedRequest->addHeader("Content-Type", "text/plain");
        chunkedRequest->enableChunkedEncoding();
        
        /* Set up handlers */
        chunkedRequest->onHeaders([](HttpClientRequestData<false>* reqData, int status, const HttpResponseHeaders::Header* headers, size_t headerCount) {
            std::cout << "Chunked request response: " << status << std::endl;
        });
        
        chunkedRequest->onChunk([](HttpClientRequestData<false>* reqData, std::string_view data, bool isLast) {
            std::cout << "Chunked response data: " << data.size() << " bytes" << (isLast ? " (last)" : "") << std::endl;
        });
        
        /* Send headers first */
        chunkedRequest->sendHeaders();
        
        /* Send body in chunks */
        chunkedRequest->sendBodyChunk("Hello");
        chunkedRequest->sendBodyChunk(" ");
        chunkedRequest->sendBodyChunk("World");
        chunkedRequest->sendBodyChunk("!");
        
        /* End the body */
        chunkedRequest->endBody();
    }
    
    /* Example with content-length streaming */
    auto* streamingRequest = app.request("httpbin.org", 80);
    if (streamingRequest) {
        streamingRequest->setRequest("POST", "/post");
        streamingRequest->addHeader("Content-Type", "text/plain");
        streamingRequest->setContentLength(11); // "Hello World"
        
        /* Set up handlers */
        streamingRequest->onHeaders([](HttpClientRequestData<false>* reqData, int status, const HttpResponseHeaders::Header* headers, size_t headerCount) {
            std::cout << "Streaming request response: " << status << std::endl;
        });
        
        streamingRequest->onChunk([](HttpClientRequestData<false>* reqData, std::string_view data, bool isLast) {
            std::cout << "Streaming response data: " << data.size() << " bytes" << (isLast ? " (last)" : "") << std::endl;
        });
        
        /* Send headers first */
        streamingRequest->sendHeaders();
        
        /* Send body in chunks */
        streamingRequest->sendBodyChunk("Hello");
        streamingRequest->sendBodyChunk(" ");
        streamingRequest->sendBodyChunk("World");
    }
    
    /* Example with GET request */
    auto* getRequest = app.request("httpbin.org", 80);
    if (getRequest) {
        getRequest->setRequest("GET", "/get");
        getRequest->addHeader("User-Agent", "uWS/1.0");
        
        /* Set up handlers */
        getRequest->onHeaders([](HttpClientRequestData<false>* reqData, int status, const HttpResponseHeaders::Header* headers, size_t headerCount) {
            std::cout << "GET request response: " << status << std::endl;
        });
        
        getRequest->onChunk([](HttpClientRequestData<false>* reqData, std::string_view data, bool isLast) {
            std::cout << "GET response data: " << data.size() << " bytes" << (isLast ? " (last)" : "") << std::endl;
        });
        
        /* Send GET request (no body) */
        getRequest->sendRequest();
    }
    
    /* Run the event loop */
    Loop::get()->run();
    
    return 0;
}
