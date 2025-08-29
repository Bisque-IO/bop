// uws_wrapper.cpp
#include "lib.h"
#include "uws.h"
#include "uws/App.h"
#include "uws/WebSocket.h"
#include <cstddef>
#include <cstdint>
#include <optional>
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

BOP_API bool uws_app_is_ssl(uws_app_t app) {
    return static_cast<InternalApp *>(app)->is_ssl;
}

static void add_http_route(InternalApp* handle, void* user_data, std::string_view method, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
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

BOP_API void uws_app_get(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "get", pattern, pattern_length, handler);
}

BOP_API void uws_app_post(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "post", pattern, pattern_length, handler);
}

BOP_API void uws_app_put(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "put", pattern, pattern_length, handler);
}

BOP_API void uws_app_del(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "del", pattern, pattern_length, handler);
}

BOP_API void uws_app_patch(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "patch", pattern, pattern_length, handler);
}

BOP_API void uws_app_options(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "options", pattern, pattern_length, handler);
}

BOP_API void uws_app_any(uws_app_t app, void* user_data, const char* pattern, size_t pattern_length, uws_http_handler_t handler) {
    add_http_route(static_cast<InternalApp *>(app), user_data, "any", pattern, pattern_length, handler);
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
            open_fn(ws, ud);
        }
    };

    behavior.message = [
        message = c_behavior.message
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg, OpCode op) {
        if (message) {
            message(ws, *ws->getUserData(), msg.data(), msg.length(), static_cast<uws_opcode_t>(op));
        }
    };

    behavior.dropped = [
        dropped = c_behavior.dropped
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg, OpCode op) {
        if (dropped) {
            dropped(ws, *ws->getUserData(), msg.data(), msg.length(), static_cast<uws_opcode_t>(op));
        }
    };

    behavior.drain = [drain = c_behavior.drain](WebSocket<SSL, true, void*>* ws) {
        if (drain) {
            drain(ws, *ws->getUserData());
        }
    };

    behavior.ping = [
        ping = c_behavior.ping
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg) {
        if (ping) {
            ping(ws, *ws->getUserData(), msg.data(), msg.length());
        }
    };

    behavior.pong = [
        pong = c_behavior.pong
    ](WebSocket<SSL, true, void*>* ws, std::string_view msg) {
        if (pong) {
            pong(ws, *ws->getUserData(), msg.data(), msg.length());
        }
    };

    behavior.subscription = [
        subscription = c_behavior.subscription
    ](WebSocket<SSL, true, void*>* ws, std::string_view topic, int subs, int old_subs) {
        if (subscription) {
            subscription(ws, *ws->getUserData(), topic.data(), subs, old_subs);
        }
    };

    behavior.close = [
        close_fn = c_behavior.close,
        destroy_user_data = c_behavior.destroy_user_data
    ](WebSocket<SSL, true, void*>* ws, int code, std::string_view msg) {
        void* ud = *ws->getUserData();
        if (close_fn) {
            close_fn(ws, ud, code, msg.data(), msg.length());
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

BOP_API void uws_app_listen(
    uws_app_t app,
    void* user_data,
    const char* host,
    size_t host_length,
    int port,
    int options,
    uws_listen_handler_t handler
) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    auto lh = [user_data, handler](auto* ls) {
        if (handler) handler(ls, user_data);
    };
    const char* h = nullptr;
    if (handle->is_ssl) {
        handle->u.ssl_app->listen(
            (!host || host_length == 0) ? "" : std::string(host, host_length),
            port,
            options,
            std::move(lh)
        );
    } else {
        handle->u.app->listen(
            (!host || host_length == 0) ? "" : std::string(host, host_length),
            port,
            options,
            std::move(lh)
        );
    }
}

BOP_API void uws_app_listen_unix(
    uws_app_t app,
    void* user_data,
    const char* path,
    size_t path_length,
    int options,
    uws_listen_handler_t handler
) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    auto lh = [user_data, handler](auto* ls) {
        if (handler) handler(ls, user_data);
    };
    if (handle->is_ssl) {
        handle->u.ssl_app->listen(options, std::move(lh), std::string(path, path_length));
    } else {
        handle->u.app->listen(options, std::move(lh), std::string(path, path_length));
    }
}

BOP_API uws_loop_t uws_app_loop(uws_app_t app) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    if (handle->is_ssl) {
        return reinterpret_cast<uws_loop_t>(handle->u.ssl_app->getLoop());
    } else {
        return reinterpret_cast<uws_loop_t>(handle->u.app->getLoop());
    }
}

BOP_API void uws_loop_defer(uws_loop_t loop, void* user_data, uws_loop_defer_handler_t callback) {
    reinterpret_cast<Loop*>(loop)->defer([loop, user_data, callback]() {
        callback(loop, user_data);
    });
}

BOP_API void uws_app_run(uws_app_t app) {
    InternalApp* handle = static_cast<InternalApp *>(app);
    if (handle->is_ssl) {
        handle->u.ssl_app->run();
    } else {
        handle->u.app->run();
    }
}

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
) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->upgrade(
            std::move(user_data),
            {sec_web_socket_key, sec_web_socket_key_length},
            {sec_web_socket_protocol, sec_web_socket_protocol_length},
            {sec_web_socket_extensions, sec_web_socket_extensions_length},
            web_socket_context
        );
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->upgrade(
            std::move(user_data),
            {sec_web_socket_key, sec_web_socket_key_length},
            {sec_web_socket_protocol, sec_web_socket_protocol_length},
            {sec_web_socket_extensions, sec_web_socket_extensions_length},
            web_socket_context
        );
    }
}

BOP_API uws_loop_t uws_res_loop(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        return us_socket_context_loop(1, us_socket_context(1, (us_socket_t *) static_cast<HttpResponse<true>*>(ir->res)));
    } else {
        return us_socket_context_loop(0, us_socket_context(0, (us_socket_t *) static_cast<HttpResponse<false>*>(ir->res)));
    }
}

BOP_API void uws_res_defer(uws_res_t res, void* user_data, uws_res_defer_handler_t handler) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        auto response = static_cast<HttpResponse<true>*>(ir->res);
        auto loop = reinterpret_cast<Loop*>(us_socket_context_loop(1, us_socket_context(1, (us_socket_t *)response)));
        loop->defer([loop, res, user_data, handler]() {
            handler(reinterpret_cast<uws_loop_t>(loop), res, user_data);
        });
    } else {
        auto response = static_cast<HttpResponse<false>*>(ir->res);
        auto loop = reinterpret_cast<Loop*>(us_socket_context_loop(0, us_socket_context(0, (us_socket_t *)response)));
        loop->defer([loop, res, user_data, handler]() {
            handler(reinterpret_cast<uws_loop_t>(loop), res, user_data);
        });
    }
}

BOP_API void uws_res_close(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->close();
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->close();
    }
}

BOP_API void uws_res_pause(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->pause();
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->pause();
    }
}

BOP_API void uws_res_resume(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->resume();
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->resume();
    }
}

BOP_API void* uws_res_native_handle(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        return static_cast<HttpResponse<true> *>(ir->res)->getNativeHandle();
    } else {
        return static_cast<HttpResponse<false> *>(ir->res)->getNativeHandle();
    }
}

BOP_API const char* uws_res_remote_address(uws_res_t res, size_t *length) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    std::string_view address;
    if (ir->is_ssl) {
        address = static_cast<HttpResponse<true> *>(ir->res)->getRemoteAddress();
    } else {
        address = static_cast<HttpResponse<false> *>(ir->res)->getRemoteAddress();
    }
    if (length) [[likely]] *length = address.size();
    return address.data();
}

BOP_API void uws_res_write_continue(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->writeContinue();
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->writeContinue();
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

BOP_API void uws_res_write_header_int(uws_res_t res, const char* key, size_t key_length, uint64_t value) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->writeHeader({key, key_length}, value);
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->writeHeader({key, key_length}, value);
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

BOP_API void uws_res_end_without_body(uws_res_t res, size_t reported_content_length, bool reported_content_length_is_set, bool close_connection) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    std::optional<size_t> size = reported_content_length_is_set ? std::optional(reported_content_length) : std::nullopt;
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->endWithoutBody(size, close_connection);
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->endWithoutBody(size, close_connection);
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

BOP_API bool uws_res_try_end(uws_res_t res, const char* data, size_t length, size_t total_size, bool close_connection, bool *has_responded) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        auto [ok, hasResponded] = static_cast<HttpResponse<true> *>(ir->res)->tryEnd({data, length}, static_cast<uintmax_t>(total_size), close_connection);
        if (has_responded) [[likely]] *has_responded = hasResponded;
        return ok;
    } else {
        auto [ok, hasResponded] = static_cast<HttpResponse<false> *>(ir->res)->tryEnd({data, length}, static_cast<uintmax_t>(total_size), close_connection);
        if (has_responded) [[likely]] *has_responded = hasResponded;
        return ok;
    }
}

BOP_API size_t uws_res_write_offset(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        return static_cast<size_t>(static_cast<HttpResponse<true> *>(ir->res)->getWriteOffset());
    } else {
        return static_cast<size_t>(static_cast<HttpResponse<false> *>(ir->res)->getWriteOffset());
    }
}

BOP_API void uws_res_override_write_offset(uws_res_t res, size_t offset) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->overrideWriteOffset(static_cast<uintmax_t>(offset));
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->overrideWriteOffset(static_cast<uintmax_t>(offset));
    }
}

BOP_API bool uws_res_has_responded(uws_res_t res) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        return static_cast<HttpResponse<true> *>(ir->res)->hasResponded();
    } else {
        return static_cast<HttpResponse<false> *>(ir->res)->hasResponded();
    }
}

BOP_API void uws_res_cork(uws_res_t res, void* user_data, uws_res_cork_handler_t cb) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->cork([res, user_data, cb]() {
            cb(res, user_data);
        });
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->cork([user_data, res, cb]() {
            cb(res, user_data);
        });
    }
}

BOP_API void uws_res_on_writable(uws_res_t res, void* user_data, uws_res_on_writable_handler_t cb) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->onWritable([res, user_data, cb](uintmax_t m) {
            return cb(res, user_data, (uint64_t)m);
        });
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->onWritable([res, user_data, cb](uintmax_t m) {
            return cb(res, user_data, (uint64_t)m);
        });
    }
}

BOP_API void uws_res_on_aborted(uws_res_t res, void* user_data, uws_res_on_aborted_handler_t cb) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->onAborted([res, user_data, cb]() {
            cb(res, user_data);
        });
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->onAborted([res, user_data, cb]() {
            cb(res, user_data);
        });
    }
}

BOP_API void uws_res_on_data(uws_res_t res, void* user_data, uws_res_on_data_handler_t cb) {
    InternalRes* ir = static_cast<InternalRes *>(res);
    if (ir->is_ssl) {
        static_cast<HttpResponse<true> *>(ir->res)->onData([res, user_data, cb](std::string_view data, bool fin) {
            cb(res, user_data, data.data(), data.size(), fin);
        });
    } else {
        static_cast<HttpResponse<false> *>(ir->res)->onData([res, user_data, cb](std::string_view data, bool fin) {
            cb(res, user_data, data.data(), data.size(), fin);
        });
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

BOP_API size_t uws_req_get_header_count(uws_req_t req) {
    return static_cast<HttpRequest *>(req)->getHeaderCount();
}

BOP_API void uws_req_get_header_at(uws_req_t req, size_t index, uws_http_header_t* header) {
    auto from_header = static_cast<HttpRequest *>(req)->getHeader(index);
    header->key = {from_header->key.data(), from_header->key.size()};
    header->value = {from_header->value.data(), from_header->value.size()};
}

BOP_API size_t uws_req_get_headers(uws_req_t req, uws_http_header_t headers[], size_t max_headers) {
    auto request = static_cast<HttpRequest *>(req);
    size_t count = 0;

    for (auto [key, value] : *request) {
        if (count == max_headers) {
            break;
        }
        headers[count].key.data = key.data();
        headers[count].key.length = key.length();
        headers[count].value.data = value.data();
        headers[count].value.length = value.length();
        count++;
    }

    return count;
}

BOP_API uws_loop_t uws_ws_loop(uws_web_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return reinterpret_cast<uws_loop_t>(us_socket_context_loop(1, us_socket_context(1, (us_socket_t *) static_cast<WebSocket<true, true, void *> *>(is->ws))));
    } else {
        return reinterpret_cast<uws_loop_t>(us_socket_context_loop(0, us_socket_context(0, (us_socket_t *) static_cast<WebSocket<false, true, void *> *>(is->ws))));
    }
}

BOP_API bool uws_ws_send(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress, bool fin) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->send({message, length}, (OpCode) opcode, compress, fin);
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->send({message, length}, (OpCode) opcode, compress, fin);
    }
}

BOP_API uws_ws_send_status uws_ws_send_first_fragment(uws_web_socket_t ws, const char* message, size_t length, uws_opcode_t opcode, bool compress) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<true, true, void *> *>(is->ws)->sendFirstFragment({message, length}, (OpCode) opcode, compress));
    } else {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<false, true, void *> *>(is->ws)->sendFirstFragment({message, length}, (OpCode) opcode, compress));
    }
}

BOP_API uws_ws_send_status uws_ws_send_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<true, true, void *> *>(is->ws)->sendFragment({message, length}, compress));
    } else {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<false, true, void *> *>(is->ws)->sendFragment({message, length}, compress));
    }
}

BOP_API uws_ws_send_status uws_ws_send_last_fragment(uws_web_socket_t ws, const char* message, size_t length, bool compress) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<true, true, void *> *>(is->ws)->sendLastFragment({message, length}, compress));
    } else {
        return static_cast<uws_ws_send_status>(static_cast<WebSocket<false, true, void *> *>(is->ws)->sendLastFragment({message, length}, compress));
    }
}

BOP_API bool uws_ws_has_negotiated_compression(uws_web_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->hasNegotiatedCompression();
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->hasNegotiatedCompression();
    }
}

BOP_API void uws_ws_end(uws_web_socket_t ws, int code, const char* message, size_t length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        static_cast<WebSocket<true, true, void *> *>(is->ws)->end(code, {message, length});
    } else {
        static_cast<WebSocket<false, true, void *> *>(is->ws)->end(code, {message, length});
    }
}

BOP_API void uws_ws_close(uws_web_socket_t ws, int code, const char* message, size_t length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        static_cast<WebSocket<true, true, void *> *>(is->ws)->end(code, {message, length});
    } else {
        static_cast<WebSocket<false, true, void *> *>(is->ws)->end(code, {message, length});
    }
}

BOP_API void* uws_ws_get_user_data(uws_web_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->getUserData();
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->getUserData();
    }
}

BOP_API void uws_ws_cork(uws_web_socket_t ws, void* user_data, uws_ws_cork_handler_t handler) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        static_cast<WebSocket<true, true, void *> *>(is->ws)->cork([ws, user_data, handler]() {
            handler(ws, user_data);
        });
    } else {
        static_cast<WebSocket<false, true, void *> *>(is->ws)->cork([ws, user_data, handler]() {
            handler(ws, user_data);
        });
    }
}

BOP_API bool uws_ws_subscribe(uws_web_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->subscribe({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->subscribe({topic, topic_length});
    }
}

BOP_API bool uws_ws_unsubscribe(uws_web_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->unsubscribe({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->unsubscribe({topic, topic_length});
    }
}

BOP_API bool uws_ws_is_subscribed(uws_web_socket_t ws, const char* topic, size_t topic_length) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->isSubscribed({topic, topic_length});
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->isSubscribed({topic, topic_length});
    }
}

BOP_API bool uws_ws_publish(uws_web_socket_t ws, const char* topic, size_t topic_length, const char* message, size_t length, uws_opcode_t opcode, bool compress) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->publish({topic, topic_length}, {message, length}, (OpCode) opcode, compress);
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->publish({topic, topic_length}, {message, length}, (OpCode) opcode, compress);
    }
}

BOP_API uint32_t uws_ws_get_buffered_amount(uws_web_socket_t ws) {
    InternalSocket* is = static_cast<InternalSocket *>(ws);
    if (is->is_ssl) {
        return static_cast<WebSocket<true, true, void *> *>(is->ws)->getBufferedAmount();
    } else {
        return static_cast<WebSocket<false, true, void *> *>(is->ws)->getBufferedAmount();
    }
}

BOP_API const char* uws_ws_get_remote_address(uws_web_socket_t ws, size_t* length) {
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
    if (handle->is_ssl) {
        return handle->u.ssl_app->publish(topic, {message, length}, (OpCode) opcode, compress);
    } else {
        return handle->u.app->publish(topic, {message, length}, (OpCode) opcode, compress);
    }
}
