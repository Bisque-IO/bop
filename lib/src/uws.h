// uws_wrapper.h
#ifndef UWS_WRAPPER_H
#define UWS_WRAPPER_H

#include <cstddef>
#include <cstdint>
#ifdef __cplusplus
extern "C" {
#endif

#include "lib.h"

typedef void* uws_app_t;
typedef void* uws_res_t;
typedef void* uws_req_t;
typedef void* uws_web_socket_t;
typedef void* uws_loop_t;

typedef struct uws_ssl_options_s {
    const char* key_file_name;
    const char* cert_file_name;
    const char* passphrase;
    const char* dh_params_file_name;
    const char* ca_file_name;
    int ssl_prefer_low_memory;
} uws_ssl_options_t;

BOP_API uws_app_t uws_create_app(void);
BOP_API uws_app_t uws_create_ssl_app(uws_ssl_options_t options);
BOP_API void uws_app_destroy(uws_app_t app);
BOP_API bool uws_app_is_ssl(uws_app_t app);

typedef void (*uws_http_handler_t)(void* user_data, uws_res_t res, uws_req_t req);

BOP_API void uws_app_get(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_post(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_put(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_del(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_patch(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_options(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_any(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

typedef enum {
    UWS_OPCODE_CONTINUE = 0,
    UWS_OPCODE_TEXT = 1,
    UWS_OPCODE_BINARY = 2,
    UWS_OPCODE_CLOSE = 8,
    UWS_OPCODE_PING = 9,
    UWS_OPCODE_PONG = 10
} uws_opcode_t;

typedef void* (*uws_ws_upgrade_handler_t)(uws_res_t res, uws_req_t req, void* context, void* per_pattern_user_data);
typedef void* (*uws_ws_create_user_data_t)(void* per_pattern_user_data);
typedef void (*uws_ws_open_handler_t)(uws_web_socket_t ws, void* user_data);
typedef void (*uws_ws_message_handler_t)(uws_web_socket_t ws, void* user_data, const char* message, size_t length, uws_opcode_t opcode);
typedef void (*uws_ws_drain_handler_t)(uws_web_socket_t ws, void* user_data);
typedef void (*uws_ws_ping_handler_t)(uws_web_socket_t ws, void* user_data, const char* message, size_t length);
typedef void (*uws_ws_pong_handler_t)(uws_web_socket_t ws, void* user_data, const char* message, size_t length);
typedef void (*uws_ws_close_handler_t)(uws_web_socket_t ws, void* user_data, int code, const char* message, size_t length);
typedef void (*uws_ws_subscription_handler_t)(uws_web_socket_t ws, void* user_data, const char* topic, int subscriptions, int old_subscriptions);
typedef void (*uws_ws_destroy_user_data_t)(void* user_data);

typedef struct uws_ws_behavior_s {
    uint16_t compression;  // uWS::CompressOptions, cast to int
    uint16_t idle_timeout;
    uint32_t max_payload_length;
    uint32_t max_backpressure;
    uint16_t max_lifetime;
    bool close_on_backpressure_limit;
    bool reset_idle_timeout_on_send;
    bool send_pings_automatically;
    bool reserved;
    uint16_t reserved_2;
    uint32_t reserved_3;
    uws_ws_upgrade_handler_t upgrade;
    uws_ws_create_user_data_t create_user_data;
    uws_ws_open_handler_t open;
    uws_ws_message_handler_t message;
    uws_ws_message_handler_t dropped;
    uws_ws_drain_handler_t drain;
    uws_ws_ping_handler_t ping;
    uws_ws_pong_handler_t pong;
    uws_ws_close_handler_t close;
    uws_ws_subscription_handler_t subscription;
    uws_ws_destroy_user_data_t destroy_user_data;
    void* per_pattern_user_data;
} uws_ws_behavior_t;

BOP_API void uws_app_ws(uws_app_t app, const char* pattern, size_t pattern_length, uws_ws_behavior_t behavior);

typedef void (*uws_listen_handler_t)(void* listen_socket, void* user_data);

BOP_API void uws_app_listen(
    uws_app_t app,
    void* user_data,
    const char* host,
    size_t host_length,
    int port,
    int options,
    uws_listen_handler_t handler
);

BOP_API void uws_app_listen_unix(
    uws_app_t app,
    void* user_data,
    const char* path,
    size_t path_length,
    int options,
    uws_listen_handler_t handler
);

BOP_API uws_loop_t uws_app_loop(uws_app_t app);
BOP_API void uws_app_run(uws_app_t app);

typedef void (*uws_loop_defer_handler_t)(uws_loop_t loop, void* user_data);

BOP_API void uws_loop_defer(uws_loop_t loop, void* user_data, uws_loop_defer_handler_t callback);

// HTTP Response functions

BOP_API void uws_res_upgrade(
    uws_res_t res,
    void* user_data,
    const char* sec_web_socket_key,
    size_t sec_web_socket_key_length,
    const char* sec_web_socket_protocol,
    size_t sec_web_socket_protocol_length,
    const char* sec_web_socket_extensions,
    size_t sec_web_socket_extensions_length,
    struct us_socket_context_t* web_socket_context
);

BOP_API uws_loop_t uws_res_loop(uws_res_t res);

typedef void (*uws_res_defer_handler_t)(uws_loop_t loop, uws_res_t res, void* user_data);
BOP_API void uws_res_defer(uws_res_t res, void* user_data, uws_res_defer_handler_t callback);

BOP_API void uws_res_close(uws_res_t res);
BOP_API void uws_res_pause(uws_res_t res);
BOP_API void uws_res_resume(uws_res_t res);
BOP_API void* uws_res_native_handle(uws_res_t res);
BOP_API const char* uws_res_remote_address(uws_res_t res, size_t *length);

BOP_API void uws_res_write_continue(uws_res_t res);
BOP_API void uws_res_write_status(uws_res_t res, const char* status, size_t status_length);
BOP_API void uws_res_write_header(uws_res_t res, const char* key, size_t key_length, const char* value, size_t value_length);
BOP_API void uws_res_write_header_int(uws_res_t res, const char* key, size_t key_length, uint64_t value);
BOP_API void uws_res_write(uws_res_t res, const char* data, size_t length);
BOP_API void uws_res_end_without_body(uws_res_t res, size_t reported_content_length, bool reported_content_length_is_set, bool close_connection);
BOP_API void uws_res_end(uws_res_t res, const char* data, size_t length, bool close_connection);
BOP_API bool uws_res_try_end(uws_res_t res, const char* data, size_t length, size_t total_size, bool close_connection, bool *has_responded);
BOP_API size_t uws_res_write_offset(uws_res_t res);
BOP_API void uws_res_override_write_offset(uws_res_t res, size_t offset);
BOP_API bool uws_res_has_responded(uws_res_t res);

typedef void (*uws_res_cork_handler_t)(uws_res_t res, void* user_data);
typedef bool (*uws_res_on_writable_handler_t)(uws_res_t res, void* user_data, uint64_t m);
typedef void (*uws_res_on_aborted_handler_t)(uws_res_t res, void* user_data);
typedef void (*uws_res_on_data_handler_t)(uws_res_t res, void* user_data, const char* data, size_t data_length, bool fin);

BOP_API void uws_res_cork(uws_res_t res, void* user_data, uws_res_cork_handler_t handler);
BOP_API void uws_res_on_writable(uws_res_t res, void* user_data, uws_res_on_writable_handler_t handler);
BOP_API void uws_res_on_aborted(uws_res_t res, void* user_data, uws_res_on_aborted_handler_t handler);
BOP_API void uws_res_on_data(uws_res_t res, void* user_data, uws_res_on_data_handler_t handler);

// HTTP Request functions
BOP_API const char* uws_req_get_method(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_url(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_query(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_header(uws_req_t req, const char* lower_case_name, size_t* length);

typedef struct uws_string_view_t {
    const char* data;
    size_t length;
} uws_string_view_t;

typedef struct uws_http_header_t {
    uws_string_view_t key, value;
} uws_header_t;

BOP_API size_t uws_req_get_header_count(uws_req_t req);

BOP_API void uws_req_get_header_at(uws_req_t req, size_t index, uws_http_header_t* header);

BOP_API size_t uws_req_get_headers(uws_req_t req, uws_http_header_t headers[], size_t max_headers);

enum uws_ws_send_status : int {
    BACKPRESSURE,
    SUCCESS,
    DROPPED
};

// WebSocket functions
BOP_API uws_loop_t uws_ws_loop(uws_web_socket_t ws);
BOP_API bool uws_ws_send(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress, bool fin);
BOP_API uws_ws_send_status uws_ws_send_first_fragment(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress);
BOP_API uws_ws_send_status uws_ws_send_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress);
BOP_API uws_ws_send_status uws_ws_send_last_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress);
BOP_API bool uws_ws_has_negotiated_compression(uws_web_socket_t ws);
BOP_API void uws_ws_end(uws_web_socket_t ws, int code, const char* message, size_t length);
BOP_API void uws_ws_close(uws_web_socket_t ws, int code, const char* message, size_t length);
BOP_API void* uws_ws_get_user_data(uws_web_socket_t ws);
typedef void (*uws_ws_cork_handler_t)(void* user_data, uws_web_socket_t ws);
BOP_API void uws_ws_cork(uws_web_socket_t ws, void* user_data, uws_ws_cork_handler_t handler);
BOP_API bool uws_ws_subscribe(uws_web_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_unsubscribe(uws_web_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_is_subscribed(uws_web_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_publish(uws_web_socket_t ws, const char* topic, size_t topic_length, const char* message, size_t message_length, uws_opcode_t opcode, bool compress);
BOP_API uint32_t uws_ws_get_buffered_amount(uws_web_socket_t ws);
BOP_API const char* uws_ws_get_remote_address(uws_web_socket_t ws, size_t* length);  // Allocates buffer, user must free

// App publish
BOP_API bool uws_app_publish(uws_app_t app, const char* topic, const char* message, size_t length, uws_opcode_t opcode, bool compress);

#ifdef __cplusplus
}
#endif

#endif
