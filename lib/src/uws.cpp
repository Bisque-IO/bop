// uws_wrapper.cpp
#include "lib.h"
#include "uws.h"
#include "uws/App.h"
#include "uws/WebSocket.h"
#include <cstddef>
#include <string_view>

using namespace uWS;

struct InternalApp {
    bool is_ssl;
    union {
        App* app;
        SSLApp* ssl_app;
    } u;
};

struct InternalRes {
    void* res;
    bool is_ssl;
};

struct InternalSocket {
    void* ws;
    bool is_ssl;
};

BOP_API uws_app_t uws_create_app(void) {
    auto* handle = new InternalApp();
    handle->is_ssl = false;
    handle->u.app = new App();
    return handle;
}

BOP_API uws_app_t uws_create_ssl_app(uws_ssl_options_t options) {
    auto* handle = new InternalApp();
    handle->is_ssl = true;
    SocketContextOptions sco;
    sco.key_file_name = options.key_file_name;
    sco.cert_file_name = options.cert_file_name;
    sco.passphrase = options.passphrase;
    sco.dh_params_file_name = options.dh_params_file_name;
    sco.ca_file_name = options.ca_file_name;
    sco.ssl_prefer_low_memory_usage = options.ssl_prefer_low_memory;
    handle->u.ssl_app = new SSLApp(std::move(sco));
    return handle;
}

BOP_API void uws_app_destroy(uws_app_t app) {
    auto* handle = static_cast<InternalApp *>(app);
    if (handle->is_ssl && handle->u.ssl_app) {
        delete handle->u.ssl_app;
        handle->u.ssl_app = nullptr;
    }
    else if (handle->u.app) {
        delete handle->u.app;
        handle->u.app = nullptr;
    }
    delete handle;
}

static void add_http_route(void* user_data, InternalApp* handle, std::string_view method, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    auto h = [handler, user_data, is_ssl = handle->is_ssl](auto* res, HttpRequest* req) {
        InternalRes ires = {res, is_ssl};
        handler(user_data, &ires, req);
    };

    if (!handle->is_ssl) {
        if (method == "get") handle->u.app->get({pattern, pattern_length}, std::move(h));
        else if (method == "post") handle->u.app->post({pattern, pattern_length}, std::move(h));
        else if (method == "put") handle->u.app->put({pattern, pattern_length}, std::move(h));
        else if (method == "del") handle->u.app->del({pattern, pattern_length}, std::move(h));
        else if (method == "patch") handle->u.app->patch({pattern, pattern_length}, std::move(h));
        else if (method == "options") handle->u.app->options({pattern, pattern_length}, std::move(h));
        else if (method == "any") handle->u.app->any({pattern, pattern_length}, std::move(h));
    } else {
        if (method == "get") handle->u.ssl_app->get({pattern, pattern_length}, std::move(h));
        else if (method == "post") handle->u.ssl_app->post({pattern, pattern_length}, std::move(h));
        else if (method == "put") handle->u.ssl_app->put({pattern, pattern_length}, std::move(h));
        else if (method == "del") handle->u.ssl_app->del({pattern, pattern_length}, std::move(h));
        else if (method == "patch") handle->u.ssl_app->patch({pattern, pattern_length}, std::move(h));
        else if (method == "options") handle->u.ssl_app->options({pattern, pattern_length}, std::move(h));
        else if (method == "any") handle->u.ssl_app->any({pattern, pattern_length}, std::move(h));
    }
}

BOP_API void uws_app_get(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "get", pattern, pattern_length, handler);
}

BOP_API void uws_app_post(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "post", pattern, pattern_length, handler);
}

BOP_API void uws_app_put(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "put", pattern, pattern_length, handler);
}

BOP_API void uws_app_del(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "del", pattern, pattern_length, handler);
}

BOP_API void uws_app_patch(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "patch", pattern, pattern_length, handler);
}

BOP_API void uws_app_options(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "options", pattern, pattern_length, handler);
}

BOP_API void uws_app_any(void* user_data, uws_app_t app, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(user_data, static_cast<InternalApp *>(app), "any", pattern, pattern_length, handler);
}

template <bool SSL>
static void uws_app_ws_impl(InternalApp* handle, const char* pattern, size_t pattern_len, uws_ws_behavior_t c_behavior) {
    using WSB = TemplatedApp<SSL>::template WebSocketBehavior<void*>;
    WSB behavior = {
        .compression = static_cast<CompressOptions>(c_behavior.compression),
        .maxPayloadLength = c_behavior.max_payload_length,
        .idleTimeout = c_behavior.idle_timeout,
        .maxBackpressure = c_behavior.max_backpressure,
        .closeOnBackpressureLimit = c_behavior.close_on_backpressure_limit,
        .resetIdleTimeoutOnSend = c_behavior.reset_idle_timeout_on_send,
        .sendPingsAutomatically = c_behavior.send_pings_automatically,
        .maxLifetime = c_behavior.max_lifetime,
    };

    behavior.upgrade = [
        upgrade = c_behavior.upgrade,
        per_pattern_user_data = c_behavior.per_pattern_user_data
    ](HttpResponse<SSL>* res, auto* req, us_socket_context_t* context) {
        InternalRes ires = {res, false};
        if (upgrade != nullptr) {
            void* ud = upgrade(&ires, req, context, per_pattern_user_data);
            res->template upgrade<void*>(
                std::move(ud),
                req->getHeader("sec-websocket-key"),
                req->getHeader("sec-websocket-protocol"),
                req->getHeader("sec-websocket-extensions"),
                context
            );
        }
    };

    behavior.open = [
        open_fn = c_behavior.open,
        create_user_data = c_behavior.create_user_data,
        per_pattern_user_data = c_behavior.per_pattern_user_data
    ](WebSocket<SSL, true, void*>* ws) {
        void* ud = create_user_data ? create_user_data(per_pattern_user_data) : nullptr;
        *ws->getUserData() = ud;
        if (open_fn) {
            open_fn(ud, ws);
        }
    };

    behavior.message = [
        message = c_behavior.message
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg, OpCode op) {
        if (message) {
            message(*ws->getUserData(), ws, msg.data(), msg.length(), static_cast<uws_opcode_t>(op));
        }
    };

    behavior.dropped = [
        dropped = c_behavior.dropped
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg, OpCode op) {
        if (dropped) {
            dropped(*ws->getUserData(), ws, msg.data(), msg.length(), static_cast<uws_opcode_t>(op));
        }
    };

    behavior.drain = [drain = c_behavior.drain](WebSocket<SSL, true, void*>* ws) {
        if (drain) {
            drain(*ws->getUserData(), ws);
        }
    };

    behavior.ping = [
        ping = c_behavior.ping
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg) {
        if (ping) {
            ping(*ws->getUserData(), ws, msg.data(), msg.length());
        }
    };

    behavior.pong = [
        pong = c_behavior.pong
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg) {
        if (pong) {
            pong(*ws->getUserData(), ws, msg.data(), msg.length());
        }
    };

    behavior.subscription = [
        subscription = c_behavior.subscription
    ](WebSocket<SSL, true, void*>* ws, std::string_view topic, int subs, int old_subs) {
        if (subscription) {
            subscription(*ws->getUserData(), ws, topic.data(), subs, old_subs);
        }
    };

    behavior.close = [
        close_fn = c_behavior.close,
        destroy_user_data = c_behavior.destroy_user_data
    ](WebSocket<SSL, true, void*>* ws, int code, std::string_view msg) {
        void* ud = *ws->getUserData();
        if (close_fn) {
            close_fn(ud, ws, code, msg.data(), msg.length());
        }
        if (destroy_user_data) {
            destroy_user_data(ud);
        }
    };

    if constexpr (SSL) {
        handle->u.ssl_app->ws<void*>({pattern, pattern_len}, std::move(behavior));
    } else {
        handle->u.app->ws<void*>({pattern, pattern_len}, std::move(behavior));
    }
}

BOP_API void uws_app_ws(uws_app_t app, const char* pattern, size_t pattern_len, uws_ws_behavior_t c_behavior) {
    auto* handle = static_cast<InternalApp *>(app);
    if (handle->is_ssl) {
        uws_app_ws_impl<true>(handle, pattern, pattern_len, c_behavior);
    } else {
        uws_app_ws_impl<false>(handle, pattern, pattern_len, c_behavior);
    }
}

BOP_API void uws_app_listen(void* user_data, uws_app_t app, int port, uws_listen_handler_t handler) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    auto lh = [handler, user_data](auto* ls) {
        if (handler) handler(user_data, ls);
    };
    if (handle->is_ssl) {
        handle->u.ssl_app->listen(port, std::move(lh));
    } else {
        handle->u.app->listen(port, std::move(lh));
    }
}

BOP_API void uws_app_run(uws_app_t app) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    if (handle->is_ssl) {
        handle->u.ssl_app->run();
    } else {
        handle->u.app->run();
    }
}

BOP_API void uws_res_write_status(uws_res_t res, const char* status, size_t status_length) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->writeStatus({status, status_length});
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->writeStatus({status, status_length});
    }
}

BOP_API void uws_res_write_header(uws_res_t res, const char* key, size_t key_length, const char* value, size_t value_length) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->writeHeader({key, key_length}, {value, value_length});
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->writeHeader({key, key_length}, {value, value_length});
    }
}

BOP_API void uws_res_write(uws_res_t res, const char* data, size_t length) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->write({data, length});
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->write({data, length});
    }
}

void uws_res_end(uws_res_t res, const char* data, size_t length, bool close_connection) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->end({data, length}, close_connection);
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->end({data, length}, close_connection);
    }
}

BOP_API const char* uws_req_get_method(uws_req_t req, size_t* length) {
    auto method = static_cast<HttpRequest *>(req)->getMethod();
    *length = method.size();
    return method.data();
}

BOP_API const char* uws_req_get_url(uws_req_t req, size_t* length) {
    auto url = static_cast<HttpRequest *>(req)->getUrl();
    *length = url.size();
    return url.data();
}

BOP_API const char* uws_req_get_query(uws_req_t req, size_t* length) {
    auto query = static_cast<HttpRequest *>(req)->getQuery();
    *length = query.size();
    return query.data();
}

BOP_API const char* uws_req_get_header(uws_req_t req, const char* lower_case_name, size_t* length) {
    auto sv = static_cast<HttpRequest *>(req)->getHeader(lower_case_name);
    if (sv.empty()) return nullptr;
    *length = sv.length();
    return sv.data();
}

BOP_API bool uws_ws_send(uws_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress, bool fin) {
    InternalSocket* is = (InternalSocket*) ws;
    std::string_view mv(message, length);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->send(mv, (OpCode) opcode, compress, fin);
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->send(mv, (OpCode) opcode, compress, fin);
    }
}

BOP_API void uws_ws_close(uws_socket_t ws, int code, const char* message, size_t length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    std::string_view mv(message, length);
    if (is->is_ssl) {
        static_cast<WebSocket<true, true, void *> *>(is->ws)->end(code, mv);
    } else {
        static_cast<WebSocket<false, true, void *> *>(is->ws)->end(code, mv);
    }
}

BOP_API void* uws_ws_get_user_data(uws_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->getUserData();
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->getUserData();
    }
}

BOP_API bool uws_ws_subscribe(uws_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->subscribe({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->subscribe({topic, topic_length});
    }
}

BOP_API bool uws_ws_unsubscribe(uws_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->unsubscribe({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->unsubscribe({topic, topic_length});
    }
}

BOP_API bool uws_ws_is_subscribed(uws_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->isSubscribed({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->isSubscribed({topic, topic_length});
    }
}

BOP_API bool uws_ws_publish(uws_socket_t ws, const char* topic, size_t topic_length, const char* message, size_t length, uws_opcode_t opcode, bool compress) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->publish({topic, topic_length}, {message, length}, (OpCode) opcode, compress);
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->publish({topic, topic_length}, {message, length}, (OpCode) opcode, compress);
    }
}

BOP_API uint32_t uws_ws_get_buffered_amount(uws_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->getBufferedAmount();
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->getBufferedAmount();
    }
}

BOP_API const char* uws_ws_get_remote_address(uws_socket_t ws, size_t* length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    std::string_view sv;
    if (is->is_ssl) {
        sv = static_cast<WebSocket<true, true, void *> *>(is->ws)->getRemoteAddressAsText();
    } else {
        sv = static_cast<WebSocket<false, true, void *> *>(is->ws)->getRemoteAddressAsText();
    }
    *length = sv.length();
    char* buf = (char*) malloc(sv.length() + 1);
    memcpy(buf, sv.data(), sv.length());
    buf[sv.length()] = '\0';
    return buf;
}

BOP_API bool uws_app_publish(uws_app_t app, const char* topic, const char* message, size_t length, uws_opcode_t opcode, bool compress) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    std::string_view mv(message, length);
    if (handle->is_ssl) {
        return handle->u.ssl_app->publish(topic, mv, (OpCode) opcode, compress);
    } else {
        return handle->u.app->publish(topic, mv, (OpCode) opcode, compress);
    }
}
