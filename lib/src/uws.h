// // uws_wrapper.h
// #ifndef UWS_WRAPPER_H
// #define UWS_WRAPPER_H

// #ifdef __cplusplus
// extern "C" {
// #endif

// #include "lib.h"

// /**
//  * The app type.
//  */
// typedef void* uws_app_t;
// /**
//  * The response type.
//  */
// typedef void* uws_res_t;
// /**
//  * The request type.
//  */
// typedef void* uws_req_t;
// /**
//  * The web socket type.
//  */
// typedef void* uws_web_socket_t;
// /**
//  * The loop type.
//  */
// typedef void* uws_loop_t;
// /**
//  * The client app type.
//  */
// typedef void* uws_client_app_t;
// /**
//  * The client connection type.
//  */
// typedef void* uws_client_conn_t;
// /**
//  * The client request type.
//  */
// typedef void* uws_client_req_t;
// /**
//  * The client response type.
//  */
// typedef void* uws_client_res_t;

// /**
//  * The TCP client app type.
//  */
// typedef void* uws_tcp_client_app_t;
// /**
//  * The TCP server app type.
//  */
// typedef void* uws_tcp_server_app_t;
// /**
//  * The TCP connection type.
//  */
// typedef void* uws_tcp_conn_t;

// /**
//  * The SSL options type.
//  */
// typedef struct uws_ssl_options_s {
//     const char* key_file_name;
//     const char* cert_file_name;
//     const char* passphrase;
//     const char* dh_params_file_name;
//     const char* ca_file_name;
//     int ssl_prefer_low_memory;
// } uws_ssl_options_t;

// /**
//  * Creates an app.
//  */
// BOP_API uws_app_t uws_create_app(void);

// /**
//  * Creates an SSL app.
//  */
// BOP_API uws_app_t uws_create_ssl_app(uws_ssl_options_t options);

// /**
//  * Destroys an app.
//  */
// BOP_API void uws_app_destroy(uws_app_t app);

// /**
//  * Checks if an app is SSL.
//  */
// BOP_API bool uws_app_is_ssl(uws_app_t app);

// typedef void (*uws_http_handler_t)(void* user_data, uws_res_t res, uws_req_t req);

// /**
//  * Adds a GET route to the app.
//  */
// BOP_API void uws_app_get(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a POST route to the app.
//  */
// BOP_API void uws_app_post(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a PUT route to the app.
//  */
// BOP_API void uws_app_put(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a DELETE route to the app.
//  */
// BOP_API void uws_app_del(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a PATCH route to the app.
//  */
// BOP_API void uws_app_patch(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
// /**
//  * Adds a PATCH route to the app.
//  */
// BOP_API void uws_app_patch(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a OPTIONS route to the app.
//  */
// BOP_API void uws_app_options(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * Adds a ANY route to the app.
//  */
// BOP_API void uws_app_any(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

// /**
//  * The WebSocket opcode.
//  */
// typedef enum {
//     UWS_OPCODE_CONTINUE = 0,
//     UWS_OPCODE_TEXT = 1,
//     UWS_OPCODE_BINARY = 2,
//     UWS_OPCODE_CLOSE = 8,
//     UWS_OPCODE_PING = 9,
//     UWS_OPCODE_PONG = 10
// } uws_opcode_t;

// /**
//  * The WebSocket behavior.
//  */
// typedef struct uws_ws_behavior_s {
//     uint16_t compression;  /* uWS::CompressOptions, cast to int */
//     uint16_t idle_timeout;
//     uint32_t max_payload_length;
//     uint32_t max_backpressure;
//     uint16_t max_lifetime;
//     bool close_on_backpressure_limit;
//     bool reset_idle_timeout_on_send;
//     bool send_pings_automatically;
//     bool reserved;
//     uint16_t reserved_2;
//     uint32_t reserved_3;
//     void* (*upgrade)(uws_res_t res, uws_req_t req, void* context, void* per_pattern_user_data);
//     void* (*create_user_data)(void* per_pattern_user_data);
//     void (*open)(uws_web_socket_t ws, void* user_data);
//     void (*message)(uws_web_socket_t ws, void* user_data, const char* message, size_t length, uws_opcode_t opcode);
//     void (*dropped)(uws_web_socket_t ws, void* user_data, const char* message, size_t length, uws_opcode_t opcode);
//     void (*drain)(uws_web_socket_t ws, void* user_data);
//     void (*ping)(uws_web_socket_t ws, void* user_data, const char* message, size_t length);
//     void (*pong)(uws_web_socket_t ws, void* user_data, const char* message, size_t length);
//     void (*close)(uws_web_socket_t ws, void* user_data, int code, const char* message, size_t length);
//     void (*subscription)(uws_web_socket_t ws, void* user_data, const char* topic, int subscriptions, int old_subscriptions);
//     void (*destroy_user_data)(void* user_data);
//     void* per_pattern_user_data;
// } uws_ws_behavior_t;

// BOP_API void uws_app_ws(uws_app_t app, const char* pattern, size_t pattern_length, uws_ws_behavior_t behavior);

// BOP_API void uws_app_listen(
//     uws_app_t app,
//     void* user_data,
//     const char* host,
//     size_t host_length,
//     int port,
//     int options,
//     void (*handler)(void* listen_socket, void* user_data)
// );

// BOP_API void uws_app_listen_unix(
//     uws_app_t app,
//     void* user_data,
//     const char* path,
//     size_t path_length,
//     int options,
//     void (*handler)(void* listen_socket, void* user_data)
// );

// BOP_API uws_loop_t uws_app_loop(uws_app_t app);

// BOP_API void uws_app_run(uws_app_t app);

// BOP_API void uws_loop_defer(uws_loop_t loop, void* user_data, void (*callback)(uws_loop_t loop, void* user_data));


// /**
//  * Upgrades the response to a WebSocket.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param sec_web_socket_key The sec web socket key.
//  * @param sec_web_socket_key_length The sec web socket key length.
//  * @param sec_web_socket_protocol The sec web socket protocol.
//  * @param sec_web_socket_protocol_length The sec web socket protocol length.
//  * @param sec_web_socket_extensions The sec web socket extensions.
//  * @param sec_web_socket_extensions_length The sec web socket extensions length.
//  * @param web_socket_context The web socket context.
//  */
// BOP_API void uws_res_upgrade(
//     uws_res_t res,
//     void* user_data,
//     const char* sec_web_socket_key,
//     size_t sec_web_socket_key_length,
//     const char* sec_web_socket_protocol,
//     size_t sec_web_socket_protocol_length,
//     const char* sec_web_socket_extensions,
//     size_t sec_web_socket_extensions_length,
//     struct us_socket_context_t* web_socket_context
// );

// /**
//  * Gets the loop of the response.
//  *
//  * @param res The response.
//  * @return The loop.
//  */
// BOP_API uws_loop_t uws_res_loop(uws_res_t res);

// /**
//  * Defers a handler for the response.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param callback The callback.
//  */
// BOP_API void uws_res_defer(uws_res_t res, void* user_data, void (*callback)(uws_loop_t loop, uws_res_t res, void* user_data));

// /**
//  * Closes the response.
//  *
//  * @param res The response.
//  */
// BOP_API void uws_res_close(uws_res_t res);

// /**
//  * Pauses the response.
//  *
//  * @param res The response.
//  */
// BOP_API void uws_res_pause(uws_res_t res);

// /**
//  * Resumes the response.
//  *
//  * @param res The response.
//  */
// BOP_API void uws_res_resume(uws_res_t res);

// /**
//  * Gets the native handle of the response.
//  *
//  * @param res The response.
//  * @return The native handle.
//  */
// BOP_API void* uws_res_native_handle(uws_res_t res);

// /**
//  * Gets the remote address of the response.
//  *
//  * @param res The response.
//  * @param length The length of the remote address.
//  * @return The remote address.
//  */
// BOP_API const char* uws_res_remote_address(uws_res_t res, size_t *length);

// /**
//  * Writes a continue to the response.
//  *
//  * @param res The response.
//  */
// BOP_API void uws_res_write_continue(uws_res_t res);

// /**
//  * Writes a status to the response.
//  *
//  * @param res The response.
//  * @param status The status.
//  * @param status_length The length of the status.
//  */
// BOP_API void uws_res_write_status(uws_res_t res, const char* status, size_t status_length);

// /**
//  * Writes a header to the response.
//  *
//  * @param res The response.
//  * @param key The key.
//  * @param key_length The length of the key.
//  * @param value The value.
//  * @param value_length The length of the value.
//  */
// BOP_API void uws_res_write_header(uws_res_t res, const char* key, size_t key_length, const char* value, size_t value_length);

// /**
//  * Writes a header to the response.
//  *
//  * @param res The response.
//  * @param key The key.
//  * @param key_length The length of the key.
//  * @param value The value.
//  */
// BOP_API void uws_res_write_header_int(uws_res_t res, const char* key, size_t key_length, uint64_t value);

// /**
//  * Writes data to the response.
//  *
//  * @param res The response.
//  * @param data The data.
//  * @param length The length of the data.
//  */
// BOP_API void uws_res_write(uws_res_t res, const char* data, size_t length);

// /**
//  * Ends the response without a body.
//  *
//  * @param res The response.
//  * @param reported_content_length The reported content length.
//  * @param reported_content_length_is_set The reported content length is set.
//  * @param close_connection The close connection.
//  */
// BOP_API void uws_res_end_without_body(uws_res_t res, size_t reported_content_length, bool reported_content_length_is_set, bool close_connection);


// /**
//  * Ends the response.
//  *
//  * @param res The response.
//  * @param data The data.
//  * @param length The length of the data.
//  * @param close_connection The close connection.
//  */
// BOP_API void uws_res_end(uws_res_t res, const char* data, size_t length, bool close_connection);

// /**
//  * Tries to end the response.
//  *
//  * @param res The response.
//  * @param data The data.
//  * @param length The length of the data.
//  * @param total_size The total size.
//  * @param close_connection The close connection.
//  * @param has_responded The has responded.
//  */
// BOP_API bool uws_res_try_end(uws_res_t res, const char* data, size_t length, size_t total_size, bool close_connection, bool *has_responded);

// /**
//  * Writes the offset of the response.
//  *
//  * @param res The response.
//  * @return The offset.
//  */
// BOP_API size_t uws_res_write_offset(uws_res_t res);

// /**
//  * Overwrites the offset of the response.
//  *
//  * @param res The response.
//  * @param offset The offset.
//  */
// BOP_API void uws_res_override_write_offset(uws_res_t res, size_t offset);

// /**
//  * Checks if the response has responded.
//  *
//  * @param res The response.
//  * @return The has responded.
//  */
// BOP_API bool uws_res_has_responded(uws_res_t res);

// /**
//  * Corks the response.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param handler The handler.
//  */
// BOP_API void uws_res_cork(uws_res_t res, void* user_data, void (*handler)(uws_res_t res, void* user_data));

// /**
//  * On writable.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param handler The handler.
//  */
// BOP_API void uws_res_on_writable(uws_res_t res, void* user_data, bool (*handler)(uws_res_t res, void* user_data, uint64_t m));

// /**
//  * On aborted.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param handler The handler.
//  */
// BOP_API void uws_res_on_aborted(uws_res_t res, void* user_data, void (*handler)(uws_res_t res, void* user_data));

// /**
//  * On data.
//  *
//  * @param res The response.
//  * @param user_data The user data.
//  * @param handler The handler.
//  */
// BOP_API void uws_res_on_data(uws_res_t res, void* user_data, void (*handler)(uws_res_t res, void* user_data, const char* data, size_t data_length, bool fin));

// // HTTP Request functions
// /**
//  * Gets the method of the request.
//  *
//  * @param req The request.
//  * @param length The length of the method.
//  * @return The method.
//  */
// BOP_API const char* uws_req_get_method(uws_req_t req, size_t* length);

// /**
//  * Gets the URL of the request.
//  *
//  * @param req The request.
//  * @param length The length of the URL.
//  * @return The URL.
//  */
// BOP_API const char* uws_req_get_url(uws_req_t req, size_t* length);

// /**
//  * Gets the query of the request.
//  *
//  * @param req The request.
//  * @param length The length of the query.
//  * @return The query.
//  */
// BOP_API const char* uws_req_get_query(uws_req_t req, size_t* length);

// /**
//  * Gets the header of the request.
//  *
//  * @param req The request.
//  * @param lower_case_name The lower case name of the header.
//  * @param length The length of the header.
//  * @return The header.
//  */
// BOP_API const char* uws_req_get_header(uws_req_t req, const char* lower_case_name, size_t* length);

// /**
//  * Gets the header count of the request.
//  *
//  * @param req The request.
//  * @return The header count.
//  */
// BOP_API size_t uws_req_get_header_count(uws_req_t req);

// /**
//  * A string view.
//  *
//  * @param data The data.
//  * @param length The length.
//  */
// typedef struct uws_string_view_t {
//     const char* data;
//     size_t length;
// } uws_string_view_t;

// /**
//  * The HTTP header.
//  *
//  * @param key The key.
//  * @param value The value.
//  */
// typedef struct {
//     uws_string_view_t key, value;
// } uws_http_header_t;

// /**
//  * Gets the header count of the request.
//  *
//  * @param req The request.
//  * @return The header count.
//  */
// BOP_API size_t uws_req_get_header_count(uws_req_t req);

// /**
//  * Gets the header at the index of the request.
//  *
//  * @param req The request.
//  * @param index The index.
//  * @param header The header.
//  */
// BOP_API void uws_req_get_header_at(uws_req_t req, size_t index, uws_http_header_t* header);

// /**
//  * Gets the headers of the request.
//  *
//  * @param req The request.
//  * @param headers The headers.
//  * @param max_headers The maximum headers.
//  */
// BOP_API size_t uws_req_get_headers(uws_req_t req, uws_http_header_t headers[], size_t max_headers);

// /**
//  * The WebSocket send status.
//  */
// enum uws_ws_send_status : int {
//     BACKPRESSURE,
//     SUCCESS,
//     DROPPED
// };

// // WebSocket functions
// /**
//  * Gets the loop of the web socket.
//  *
//  * @param ws The web socket.
//  * @return The loop.
//  */
// BOP_API uws_loop_t uws_ws_loop(uws_web_socket_t ws);

// /**
//  * Sends a message to the web socket.
//  *
//  * @param ws The web socket.
//  * @param message The message.
//  * @param length The length of the message.
//  * @param opcode The opcode.
//  * @param compress The compress.
//  * @param fin The fin.
//  */
// BOP_API bool uws_ws_send(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress, bool fin);

// /**
//  * Sends the first fragment of a message to the web socket.
//  *
//  * @param ws The web socket.
//  * @param message The message.
//  * @param length The length of the message.
//  * @param opcode The opcode.
//  * @param compress The compress.
//  */
// BOP_API enum uws_ws_send_status uws_ws_send_first_fragment(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress);

// /**
//  * Sends a fragment of a message to the web socket.
//  *
//  * @param ws The web socket.
//  * @param message The message.
//  * @param length The length of the message.
//  * @param compress The compress.
//  */
// BOP_API enum uws_ws_send_status uws_ws_send_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress);

// /**
//  * Sends the last fragment of a message to the web socket.
//  *
//  * @param ws The web socket.
//  * @param message The message.
//  * @param length The length of the message.
//  * @param compress The compress.
//  */ 
// BOP_API enum uws_ws_send_status uws_ws_send_last_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress);

// /**
//  * Checks if the web socket has negotiated compression.
//  *
//  * @param ws The web socket.
//  * @return The compression status.
//  */
// BOP_API bool uws_ws_has_negotiated_compression(uws_web_socket_t ws);

// /**
//  * Ends the web socket.
//  *
//  * @param ws The web socket.
//  * @param code The code.
//  * @param message The message.
//  * @param length The length of the message.
//  */
// BOP_API void uws_ws_end(uws_web_socket_t ws, int code, const char* message, size_t length);

// /**
//  * Closes the web socket.
//  *
//  * @param ws The web socket.
//  * @param code The code.
//  * @param message The message.
//  * @param length The length of the message.
//  */
// BOP_API void uws_ws_close(uws_web_socket_t ws, int code, const char* message, size_t length);

// /**
//  * Gets the user data of the web socket.
//  *
//  * @param ws The web socket.
//  * @return The user data.
//  */
// BOP_API void* uws_ws_get_user_data(uws_web_socket_t ws);

// /**
//  * The cork handler type.
//  */
// typedef void (*uws_ws_cork_handler_t)(void* user_data, uws_web_socket_t ws);

// /**
//  * Corks the web socket.
//  *
//  * @param ws The web socket.
//  * @param user_data The user data.
//  * @param handler The cork handler.
//  */
// BOP_API void uws_ws_cork(uws_web_socket_t ws, void* user_data, uws_ws_cork_handler_t handler);

// /**
//  * Subscribes to a topic.
//  *
//  * @param ws The web socket.
//  * @param topic The topic.
//  * @param topic_length The length of the topic.
//  */
// BOP_API bool uws_ws_subscribe(uws_web_socket_t ws, const char* topic, size_t topic_length);

// /**
//  * Unsubscribes from a topic.
//  *
//  * @param ws The web socket.
//  * @param topic The topic.
//  * @param topic_length The length of the topic.
//  */
// BOP_API bool uws_ws_unsubscribe(uws_web_socket_t ws, const char* topic, size_t topic_length);

// /**
//  * Checks if the web socket is subscribed to a topic.
//  *
//  * @param ws The web socket.
//  * @param topic The topic.
//  * @param topic_length The length of the topic.
//  */
// BOP_API bool uws_ws_is_subscribed(uws_web_socket_t ws, const char* topic, size_t topic_length);

// /**
//  * Publishes a message to a topic.
//  *
//  * @param ws The web socket.
//  * @param topic The topic.
//  * @param topic_length The length of the topic.
//  * @param message The message.
//  * @param message_length The length of the message.
//  * @param opcode The opcode.
//  * @param compress The compress.
//  */
// BOP_API bool uws_ws_publish(uws_web_socket_t ws, const char* topic, size_t topic_length, const char* message, size_t message_length, uws_opcode_t opcode, bool compress);

// /**
//  * Gets the buffered amount of the web socket.
//  *
//  * @param ws The web socket.
//  * @return The buffered amount.
//  */
// BOP_API uint32_t uws_ws_get_buffered_amount(uws_web_socket_t ws);

// /**
//  * Gets the remote address of the web socket.
//  *
//  * @param ws The web socket.
//  * @param length The length of the remote address.
//  * @return The remote address.
//  */
// BOP_API const char* uws_ws_get_remote_address(uws_web_socket_t ws, size_t* length);  // Allocates buffer, user must free

// /**
// * Publishes a message to a topic.
// *
// * @param app The app.
// * @param topic The topic.
// * @param message The message.
// * @param length The length of the message.
// * @param opcode The opcode.
// * @param compress The compress.
// */
// BOP_API bool uws_app_publish(uws_app_t app, const char* topic, const char* message, size_t length, uws_opcode_t opcode, bool compress);

// /**
//  * The TCP behavior type.
//  */
// typedef struct uws_tcp_behavior_s {
//     uint32_t idle_timeout_seconds;
//     uint32_t long_timeout_minutes;
//     uint32_t max_backpressure_bytes;
//     uint16_t conn_user_data_size;
//     bool close_on_backpressure_limit;
//     bool reset_idle_timeout_on_send;
//     void (*on_open)(uws_tcp_conn_t conn);
//     void (*on_close)(uws_tcp_conn_t conn, int code, void* data);
//     void (*on_connect_error)(uws_tcp_conn_t conn, int code, const char* message, size_t length);
//     void (*on_data)(uws_tcp_conn_t conn, const char* data, size_t length);
//     void (*on_writable)(uws_tcp_conn_t conn, uint64_t remaining);
//     void (*on_drain)(uws_tcp_conn_t conn);
//     void (*on_dropped)(uws_tcp_conn_t conn, const char* data, size_t length);
//     void (*on_end)(uws_tcp_conn_t conn);
//     void (*on_timeout)(uws_tcp_conn_t conn);
//     void (*on_long_timeout)(uws_tcp_conn_t conn);
//     void (*on_server_name)(uws_tcp_conn_t conn, const char* server_name);
// } uws_tcp_behavior_t;

// /**
//  * Creates a TCP server app.
//  *
//  * @param behavior The behavior of the TCP server.
//  * @param listener_host The host to listen on.
//  * @param listener_host_length The length of the listener host.
//  * @param listener_port The port to listen on.
//  * @param listener_options The options for the listener.
//  */
// BOP_API uws_tcp_server_app_t uws_create_tcp_server_app(
//     uws_tcp_behavior_t behavior,
//     const char* listener_host,
//     size_t listener_host_length,
//     int listener_port,
//     int listener_options
// );

// /**
//  * Creates a TCP server SSL app.
//  *
//  * @param behavior The behavior of the TCP server.
//  * @param listener_host The host to listen on.
//  * @param listener_host_length The length of the listener host.
//  * @param listener_port The port to listen on.
//  * @param listener_options The options for the listener.
//  * @param ssl_options The SSL options.
//  * @param server_names The server names.
//  * @param server_names_length The length of the server names.
//  */
// BOP_API uws_tcp_server_app_t uws_create_tcp_server_ssl_app(
//     uws_tcp_behavior_t behavior,
//     const char* listener_host,
//     size_t listener_host_length,
//     int listener_port,
//     int listener_options,
//     uws_ssl_options_t ssl_options,
//     const char* server_names[],
//     size_t server_names_length
// );

// /**
//  * Creates a TCP client app.
//  *
//  * @param behavior The behavior of the TCP client.
//  */
// BOP_API uws_tcp_client_app_t uws_create_tcp_client_app(uws_tcp_behavior_t behavior);

// /**
//  * Creates a TCP client SSL app.
//  *
//  * @param behavior The behavior of the TCP client.
//  * @param ssl_options The SSL options.
//  */
// BOP_API uws_tcp_client_app_t uws_create_tcp_client_ssl_app(uws_tcp_behavior_t behavior, uws_ssl_options_t ssl_options);

// #ifdef __cplusplus
// }
// #endif

// #endif
