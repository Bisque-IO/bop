// uws_wrapper.h
#ifndef UWS_WRAPPER_H
#define UWS_WRAPPER_H

#ifdef __cplusplus
extern "C" {
#endif

#include "lib.h"

typedef void* uws_app_t;
typedef void* uws_res_t;
typedef void* uws_req_t;
typedef void* uws_socket_t;

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

typedef void (*uws_http_handler_t)(void* user_data, uws_res_t res, uws_req_t req);

BOP_API void uws_app_get(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_post(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_put(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_del(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_patch(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_options(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);
BOP_API void uws_app_any(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler);

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
typedef void (*uws_ws_open_handler_t)(void* user_data, uws_socket_t ws);
typedef void (*uws_ws_message_handler_t)(void* user_data, uws_socket_t ws, const char* message, size_t length, uws_opcode_t opcode);
typedef void (*uws_ws_drain_handler_t)(void* user_data, uws_socket_t ws);
typedef void (*uws_ws_ping_handler_t)(void* user_data, uws_socket_t ws, const char* message, size_t length);
typedef void (*uws_ws_pong_handler_t)(void* user_data, uws_socket_t ws, const char* message, size_t length);
typedef void (*uws_ws_close_handler_t)(void* user_data, uws_socket_t ws, int code, const char* message, size_t length);
typedef void (*uws_ws_subscription_handler_t)(void* user_data, uws_socket_t ws, const char* topic, int subscriptions, int old_subscriptions);
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

typedef void (*uws_listen_handler_t)(void* user_data, void* listen_socket);

BOP_API void uws_app_listen(void* user_data, uws_app_t app, int port, uws_listen_handler_t handler);

BOP_API void uws_app_run(uws_app_t app);

// HTTP Response functions
BOP_API void uws_res_write_status(uws_res_t res, const char* status, size_t status_length);
BOP_API void uws_res_write_header(uws_res_t res, const char* key, size_t key_length, const char* value, size_t value_length);
BOP_API void uws_res_write(uws_res_t res, const char* data, size_t length);
BOP_API void uws_res_end(uws_res_t res, const char* data, size_t length, bool close_connection);

// HTTP Request functions
BOP_API const char* uws_req_get_method(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_url(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_query(uws_req_t req, size_t* length);
BOP_API const char* uws_req_get_header(uws_req_t req, const char* lower_case_name, size_t* length);

typedef struct uws_header_t {
    const char* key;
    size_t key_length;
    const char* value;
    size_t value_length;
} uws_header_t;

typedef struct uws_req_header_iterator_t {} uws_req_header_iterator_t;
// BOP_API void 

// WebSocket functions
BOP_API bool uws_ws_send(uws_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress, bool fin);
BOP_API void uws_ws_close(uws_socket_t ws, int code, const char* message, size_t length);
BOP_API void* uws_ws_get_user_data(uws_socket_t ws);
BOP_API bool uws_ws_subscribe(uws_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_unsubscribe(uws_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_is_subscribed(uws_socket_t ws, const char* topic, size_t topic_length);
BOP_API bool uws_ws_publish(uws_socket_t ws, const char* topic, size_t topic_length, const char* message, size_t message_length, uws_opcode_t opcode, bool compress);
BOP_API uint32_t uws_ws_get_buffered_amount(uws_socket_t ws);
BOP_API const char* uws_ws_get_remote_address(uws_socket_t ws, size_t* length);  // Allocates buffer, user must free

// App publish
BOP_API bool uws_app_publish(uws_app_t app, const char* topic, const char* message, size_t length, uws_opcode_t opcode, bool compress);

#ifdef __cplusplus
}
#endif

#endif
