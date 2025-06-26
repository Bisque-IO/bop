#include <iostream>
#include <boost/asio/ssl/stream_base.hpp>

#include "./lib.h"
#include "state_machine.hxx"

#include "libnuraft/nuraft.hxx"

extern "C" {
//    #define LIBMDBX_API BOP_API
//    #define LIBMDBX_EXPORTS BOP_API
//    #define SQLITE_API BOP_API
#include <openssl/ssl.h>
#include <mdbx.h>
#include <sqlite3.h>
#include "../hash/rapidhash.h"
#include "../hash/xxh3.h"
#include "./raft.h"


////////////////////////////////////////////////////////////////////////////////////
/// NuRaft C API
////////////////////////////////////////////////////////////////////////////////////

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::buffer
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_buffer_ptr {
    nuraft::ptr<nuraft::buffer> buf;

    explicit bop_raft_buffer_ptr(const nuraft::ptr<nuraft::buffer> &buf)
        : buf(buf) {
    }
};

BOP_API bop_raft_buffer *bop_raft_buffer_alloc(const size_t size) {
    return reinterpret_cast<bop_raft_buffer *>(nuraft::buffer::alloc_unique(size));
}

// Do not call this when passing to a function that takes ownership.
BOP_API void bop_raft_buffer_free(bop_raft_buffer *buf) {
    delete[] reinterpret_cast<char *>(buf);
}

BOP_API unsigned char *bop_raft_buffer_data(bop_raft_buffer *buf) {
    return reinterpret_cast<nuraft::buffer *>(buf)->data_begin();
}

BOP_API size_t bop_raft_buffer_container_size(bop_raft_buffer *buf) {
    return reinterpret_cast<nuraft::buffer *>(buf)->container_size();
}

BOP_API size_t bop_raft_buffer_size(bop_raft_buffer *buf) {
    return reinterpret_cast<nuraft::buffer *>(buf)->size();
}

BOP_API size_t bop_raft_buffer_pos(bop_raft_buffer *buf) {
    return reinterpret_cast<nuraft::buffer *>(buf)->pos();
}

BOP_API void bop_raft_buffer_set_pos(bop_raft_buffer *buf, size_t pos) {
    reinterpret_cast<nuraft::buffer *>(buf)->pos(pos);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<uint64_t>
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_cmd_result_uint64_t {
    void *user_data{nullptr};
    bop_raft_cmd_result_uint64_when_ready_func when_ready{nullptr};
    nuraft::ptr<nuraft::cmd_result<uint64_t> > cmd_result{nullptr};

    nuraft::cmd_result<uint64_t>::handler_type on_ready = [this](uint64_t result, nuraft::ptr<std::exception> err) {
        if (err) {
            when_ready(user_data, result, err->what());
        } else {
            when_ready(user_data, result, nullptr);
        }
    };

    void set(nuraft::ptr<nuraft::cmd_result<uint64_t> > cmd) {
        cmd->when_ready(on_ready);
        cmd_result = std::move(cmd);
    }
};

BOP_API bop_raft_cmd_result_uint64_t *bop_raft_cmd_result_uint64_create(
    void *user_data,
    bop_raft_cmd_result_uint64_when_ready_func when_ready
) {
    auto result = new bop_raft_cmd_result_uint64_t;
    result->user_data = user_data;
    result->when_ready = when_ready;
    return result;
}


BOP_API void bop_raft_cmd_result_uint64_delete(const bop_raft_cmd_result_uint64_t *self) {
    if (self) {
        delete self;
    }
}

BOP_API void *bop_raft_cmd_result_uint64_get_user_data(
    const bop_raft_cmd_result_uint64_t *self
) {
    return self->user_data;
}

BOP_API bop_raft_cmd_result_uint64_when_ready_func bop_raft_cmd_result_uint64_get_when_ready(
    const bop_raft_cmd_result_uint64_t *self
) {
    return self->when_ready;
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<nuraft::ptr<nuraft::buffer>>
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_cmd_result_buffer_t {
    void *user_data{nullptr};
    bop_raft_cmd_result_buffer_when_ready_func when_ready{nullptr};
    nuraft::ptr<nuraft::cmd_result<nuraft::ptr<nuraft::buffer> > > cmd_result{nullptr};

    nuraft::cmd_result<nuraft::ptr<nuraft::buffer> >::handler_type on_ready = [this](
        nuraft::ptr<nuraft::buffer> result,
        nuraft::ptr<std::exception> err
    ) {
        if (err) {
            when_ready(user_data, result.get(), err->what());
        } else {
            when_ready(user_data, result.get(), nullptr);
        }
    };

    void set(nuraft::ptr<nuraft::cmd_result<nuraft::ptr<nuraft::buffer> > > cmd) {
        cmd->when_ready(on_ready);
        cmd_result = std::move(cmd);
    }
};

BOP_API bop_raft_cmd_result_buffer_t *bop_raft_cmd_result_buffer_create(
    void *user_data,
    bop_raft_cmd_result_buffer_when_ready_func when_ready
) {
    auto result = new bop_raft_cmd_result_buffer_t;
    result->user_data = user_data;
    result->when_ready = when_ready;
    return result;
}


BOP_API void bop_raft_cmd_result_buffer_delete(const bop_raft_cmd_result_buffer_t *self) {
    if (self) {
        delete self;
    }
}

BOP_API void *bop_raft_cmd_result_buffer_get_user_data(
    bop_raft_cmd_result_buffer_t *self
) {
    return self->user_data;
}

BOP_API bop_raft_cmd_result_buffer_when_ready_func bop_raft_cmd_result_buffer_get_when_ready(
    bop_raft_cmd_result_buffer_t *self
) {
    return self->when_ready;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::snapshot
///////////////////////////////////////////////////////////////////////////////////////////////////

static bop_raft_snapshot *bop_raft_snapshot_deserialize0(nuraft::buffer_serializer &bs) {
    nuraft::snapshot::type snp_type = static_cast<nuraft::snapshot::type>(bs.get_u8());
    uint64_t last_log_idx = bs.get_u64();
    uint64_t last_log_term = bs.get_u64();
    uint64_t size = bs.get_u64();
    auto last_config = nuraft::cluster_config::deserialize(bs);
    return reinterpret_cast<bop_raft_snapshot *>(new nuraft::snapshot(last_log_idx, last_log_term, last_config, size,
                                                                      snp_type));
}

BOP_API bop_raft_buffer *bop_raft_snapshot_serialize(bop_raft_snapshot *snapshot) {
    auto conf_buf = reinterpret_cast<nuraft::snapshot *>(snapshot)->get_last_config()->serialize();
    nuraft::buffer *buf = nuraft::buffer::alloc_unique(conf_buf->size() + 8 * 3 + 1);
    buf->put(reinterpret_cast<nuraft::snapshot *>(snapshot)->get_type());
    buf->put(reinterpret_cast<nuraft::snapshot *>(snapshot)->get_last_log_idx());
    buf->put(reinterpret_cast<nuraft::snapshot *>(snapshot)->get_last_log_term());
    buf->put(reinterpret_cast<nuraft::snapshot *>(snapshot)->size());
    buf->put(*conf_buf);
    buf->pos(0);
    return reinterpret_cast<bop_raft_buffer *>(buf);
}

static nuraft::buffer *bop_raft_snapshot_serialize_by_ref(const nuraft::snapshot &snapshot) {
    auto conf_buf = snapshot.get_last_config()->serialize();
    nuraft::buffer *buf = nuraft::buffer::alloc_unique(conf_buf->size() + 8 * 3 + 1);
    buf->put(snapshot.get_type());
    buf->put(snapshot.get_last_log_idx());
    buf->put(snapshot.get_last_log_term());
    buf->put(snapshot.size());
    buf->put(*conf_buf);
    buf->pos(0);
    return buf;
}

BOP_API bop_raft_snapshot *bop_raft_snapshot_deserialize(bop_raft_buffer *buf) {
    nuraft::buffer_serializer bs(*reinterpret_cast<nuraft::buffer *>(buf));
    return bop_raft_snapshot_deserialize0(bs);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cluster_config
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_cluster_config;

struct bop_raft_cluster_config_ptr {
    nuraft::ptr<nuraft::cluster_config> config;

    bop_raft_cluster_config_ptr(const nuraft::ptr<nuraft::cluster_config> &config) : config(config) {
    }
};

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_alloc() {
    return reinterpret_cast<bop_raft_cluster_config *>(new nuraft::cluster_config());
}

BOP_API bop_raft_cluster_config_ptr *bop_raft_cluster_config_alloc_ptr(bop_raft_cluster_config *config) {
    return new bop_raft_cluster_config_ptr(nuraft::ptr<nuraft::cluster_config>(
            reinterpret_cast<nuraft::cluster_config *>(config))
    );
}

BOP_API void bop_raft_cluster_config_delete(const bop_raft_cluster_config *config) {
    if (config) delete reinterpret_cast<const nuraft::cluster_config *>(config);
}

BOP_API void bop_raft_cluster_config_ptr_delete(const bop_raft_cluster_config_ptr *config) {
    if (config) delete config;
}

BOP_API bop_raft_buffer *bop_raft_cluster_config_serialize(bop_raft_cluster_config *conf) {
    size_t sz = 2 * 8 + 4 + 1;
    std::vector<nuraft::ptr<nuraft::buffer> > srv_buffs;
    for (auto it = reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().cbegin();
         it != reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().cend(); ++it) {
        nuraft::ptr<nuraft::buffer> buf = (*it)->serialize();
        srv_buffs.push_back(buf);
        sz += buf->size();
    }
    // For aux string.
    sz += 4;
    sz += reinterpret_cast<nuraft::cluster_config *>(conf)->get_user_ctx().size();

    nuraft::buffer *result = nuraft::buffer::alloc_unique(sz);
    result->put(reinterpret_cast<nuraft::cluster_config *>(conf)->get_log_idx());
    result->put(reinterpret_cast<nuraft::cluster_config *>(conf)->get_prev_log_idx());
    result->put(static_cast<uint8_t>(reinterpret_cast<nuraft::cluster_config *>(conf)->is_async_replication() ? 1 : 0));
    result->put(reinterpret_cast<uint8_t *>(reinterpret_cast<nuraft::cluster_config *>(conf)->get_user_ctx().data()),
                reinterpret_cast<nuraft::cluster_config *>(conf)->get_user_ctx().size());
    result->put(static_cast<int32_t>(reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().size()));
    for (size_t i = 0; i < srv_buffs.size(); ++i) {
        result->put(*srv_buffs[i]);
    }

    result->pos(0);
    return reinterpret_cast<bop_raft_buffer *>(result);
}

static nuraft::cluster_config *bop_raft_cluster_config_deserialize0(nuraft::buffer_serializer &bs) {
    uint64_t log_idx = bs.get_u64();
    uint64_t prev_log_idx = bs.get_u64();

    uint8_t ec_byte = bs.get_u8();
    bool ec = ec_byte ? true : false;

    size_t ctx_len;
    const uint8_t *ctx_data = (const uint8_t *) bs.get_bytes(ctx_len);
    std::string user_ctx = std::string((const char *) ctx_data, ctx_len);

    int32_t cnt = bs.get_i32();

    nuraft::cluster_config *conf = new nuraft::cluster_config(log_idx, prev_log_idx, ec);
    while (cnt-- > 0) {
        conf->get_servers().push_back(nuraft::srv_config::deserialize(bs));
    }

    conf->set_user_ctx(user_ctx);

    return conf;
}

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_deserialize(bop_raft_buffer *buf) {
    nuraft::buffer_serializer bs(*reinterpret_cast<nuraft::buffer *>(buf));
    return reinterpret_cast<bop_raft_cluster_config *>(bop_raft_cluster_config_deserialize0(bs));
}

// Log index number of current config.
BOP_API uint64_t bop_raft_cluster_config_log_idx(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->get_log_idx();
}

// Log index number of previous config.
BOP_API uint64_t bop_raft_cluster_config_prev_log_idx(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->get_prev_log_idx();
}

// `true` if asynchronous replication mode is on.
BOP_API bool bop_raft_cluster_config_is_async_replication(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->is_async_replication();
}

// Custom config data given by user.
BOP_API const char *bop_raft_cluster_config_user_ctx(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->get_user_ctx().data();
}

BOP_API size_t bop_raft_cluster_config_user_ctx_size(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->get_user_ctx().size();
}

// Number of servers.
BOP_API size_t bop_raft_cluster_config_servers_size(bop_raft_cluster_config *cfg) {
    return reinterpret_cast<nuraft::cluster_config *>(cfg)->get_servers().size();
}

// Server config at index
BOP_API bop_raft_srv_config *bop_raft_cluster_config_server(bop_raft_cluster_config *cfg, int idx) {
    auto svr_cfg = reinterpret_cast<nuraft::cluster_config *>(cfg)->get_server(idx);
    return (svr_cfg) ? reinterpret_cast<bop_raft_srv_config *>(svr_cfg.get()) : nullptr;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::srv_config
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_srv_config_vec {
    std::vector<nuraft::ptr<nuraft::srv_config> > configs;

    bop_raft_srv_config_vec() = default;
};

BOP_API bop_raft_srv_config_vec *bop_raft_srv_config_vec_create() {
    bop_raft_srv_config_vec *vec = new bop_raft_srv_config_vec();
    vec->configs.reserve(16);
    return vec;
}

BOP_API void bop_raft_srv_config_vec_delete(const bop_raft_srv_config_vec *vec) {
    if (vec) {
        delete vec;
    }
}

BOP_API size_t bop_raft_srv_config_vec_size(bop_raft_srv_config_vec *vec) {
    return vec->configs.size();
}

BOP_API nuraft::srv_config *bop_raft_srv_config_vec_get(bop_raft_srv_config_vec *vec, size_t idx) {
    return vec->configs[idx].get();
}

struct bop_raft_srv_config_ptr {
    nuraft::ptr<nuraft::srv_config> config;

    explicit bop_raft_srv_config_ptr(const nuraft::ptr<nuraft::srv_config> &config) : config(config) {
    }
};

BOP_API bop_raft_srv_config_ptr *bop_raft_srv_config_ptr_alloc(bop_raft_srv_config *config) {
    return new bop_raft_srv_config_ptr(nuraft::ptr<nuraft::srv_config>(reinterpret_cast<nuraft::srv_config *>(config)));
}

BOP_API void bop_raft_srv_config_ptr_delete(const bop_raft_srv_config_ptr *config) {
    if (config) delete config;
}

BOP_API void bop_raft_srv_config_delete(const bop_raft_srv_config *config) {
    if (config) delete reinterpret_cast<const nuraft::srv_config *>(config);
}

// ID of this server, should be positive number.
BOP_API int32_t bop_raft_srv_config_id(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_id();
}

// ID of datacenter where this server is located. 0 if not used.
BOP_API int32_t bop_raft_srv_config_dc_id(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_dc_id();
}

// Endpoint (address + port).
BOP_API const char *bop_raft_srv_config_endpoint(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_endpoint().data();
}

// Size of Endpoint (address + port).
BOP_API size_t bop_raft_srv_config_endpoint_size(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_endpoint().size();
}

/**
 * Custom string given by user.
 * WARNING: It SHOULD NOT contain NULL character,
 *          as it will be stored as a C-style string.
 */
BOP_API const char *bop_raft_srv_config_aux(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_aux().data();
}

BOP_API size_t bop_raft_srv_config_aux_size(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_aux().size();
}

/**
 * `true` if this node is learner.
 * Learner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_learner(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->is_learner();
}

/**
 * `true` if this node is a new joiner, but not yet fully synced.
 * New joiner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_new_joiner(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->is_new_joiner();
}

/**
 * Priority of this node.
 * 0 will never be a leader.
 */
BOP_API int32_t bop_raft_srv_config_priority(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_priority();
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::svr_state
///////////////////////////////////////////////////////////////////////////////////////////////////

BOP_API bop_raft_srv_state *bop_raft_svr_state_deserialize(bop_raft_buffer *buf) {
    nuraft::buffer_serializer bs(*reinterpret_cast<nuraft::buffer *>(buf));
    uint8_t ver = bs.get_u8();

    uint64_t term = bs.get_u64();
    int voted_for = bs.get_i32();
    bool et_allowed = (bs.get_u8() == 1);

    bool catching_up = false;
    if (ver >= 2 && bs.pos() < reinterpret_cast<nuraft::buffer *>(buf)->size()) {
        catching_up = (bs.get_u8() == 1);
    }

    bool receiving_snapshot = false;
    if (ver >= 2 && bs.pos() < reinterpret_cast<nuraft::buffer *>(buf)->size()) {
        receiving_snapshot = (bs.get_u8() == 1);
    }

    return reinterpret_cast<bop_raft_srv_state *>(new nuraft::srv_state(
            term,
            voted_for,
            et_allowed,
            catching_up,
            receiving_snapshot)
    );
}

BOP_API bop_raft_buffer *bop_raft_svr_state_serialize(bop_raft_srv_state *state) {
    if (!state) return nullptr;
    uint8_t version = 2;
    //   << Format >>
    // version              1 byte
    // term                 8 bytes
    // voted_for            4 bytes
    // election timer       1 byte      (since v1)
    // catching up          1 byte      (since v2)
    // receiving snapshot   1 byte      (since v2)

    size_t buf_len = sizeof(uint8_t) +
                     sizeof(uint64_t) +
                     sizeof(int32_t) +
                     sizeof(uint8_t);
    if (version >= 2) {
        buf_len += sizeof(uint8_t);
        buf_len += sizeof(uint8_t);
    }
    bop_raft_buffer *buf = bop_raft_buffer_alloc(buf_len);
    nuraft::buffer_serializer bs(*reinterpret_cast<nuraft::buffer *>(buf));
    bs.put_u8(version);
    bs.put_u64(reinterpret_cast<nuraft::srv_state *>(state)->get_term());
    bs.put_i32(reinterpret_cast<nuraft::srv_state *>(state)->get_voted_for());
    bs.put_u8(reinterpret_cast<nuraft::srv_state *>(state)->is_election_timer_allowed() ? 1 : 0);
    if (version >= 2) {
        bs.put_u8(reinterpret_cast<nuraft::srv_state *>(state)->is_catching_up() ? 1 : 0);
        bs.put_u8(reinterpret_cast<nuraft::srv_state *>(state)->is_receiving_snapshot() ? 1 : 0);
    }
    return buf;
}

BOP_API void bop_raft_svr_state_delete(const bop_raft_srv_state *state) {
    if (state) delete reinterpret_cast<const nuraft::srv_state *>(state);
}

/**
 * Term
 */
BOP_API uint64_t bop_raft_svr_state_term(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->get_term();
}

/**
 * Server ID that this server voted for.
 * `-1` if not voted.
 */
BOP_API int bop_raft_svr_state_voted_for(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->get_voted_for();
}

/**
 * `true` if election timer is allowed.
 */
BOP_API int bop_raft_svr_state_is_election_timer_allowed(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->is_election_timer_allowed();
}

/**
 * true if this server has joined the cluster but has not yet
 * fully caught up with the latest log. While in the catch-up status,
 * this server will not receive normal append_entries requests.
 */
BOP_API int bop_raft_svr_state_is_catching_up(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->is_catching_up();
}

/**
 * `true` if this server is receiving a snapshot.
 * Same as `catching_up_`, it must be a durable flag so as not to be
 * reset after restart. While this flag is set, this server will neither
 * receive normal append_entries requests nor initiate election.
 */
BOP_API int bop_raft_svr_state_is_receiving_snapshot(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->is_receiving_snapshot();
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::logger
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_logger_ptr {
    nuraft::ptr<nuraft::logger> logger;

    bop_raft_logger_ptr(const nuraft::ptr<nuraft::logger> &logger) : logger(logger) {
    }
};

struct bop_raft_logger final : nuraft::logger {
    void *user_data;
    bop_raft_logger_put_details_func put_details_{};

    bop_raft_logger(void *user_data, bop_raft_logger_put_details_func put_details) : user_data(user_data),
        put_details_(put_details) {
    }

    /**
     * Put a log with level, line number, function name,
     * and file name.
     *
     * Log level info:
     *    Trace:    6
     *    Debug:    5
     *    Info:     4
     *    Warning:  3
     *    Error:    2
     *    Fatal:    1
     *
     * @param level Level of given log.
     * @param source_file Name of file where the log is located.
     * @param func_name Name of function where the log is located.
     * @param line_number Line number of the log.
     * @param log_line Contents of the log.
     */
    void put_details(
        int level,
        const char *source_file,
        const char *func_name,
        size_t line_number,
        const std::string &log_line
    ) override {
        put_details_(user_data, level, source_file, func_name, line_number, log_line.data(), log_line.size());
    }
};

BOP_API bop_raft_logger_ptr *bop_raft_logger_create(void *user_data, bop_raft_logger_put_details_func callback) {
    return new bop_raft_logger_ptr(std::make_shared<bop_raft_logger>(user_data, callback));
}

BOP_API void bop_raft_logger_delete(const bop_raft_logger_ptr *logger) {
    if (logger) delete logger;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::asio_service
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_asio_service_options;


struct bop_raft_asio_service_ptr {
    nuraft::ptr<nuraft::asio_service> service;
    nuraft::asio_service_options options;

    bop_raft_asio_service_ptr(
        const nuraft::ptr<nuraft::asio_service> &asio_service,
        const nuraft::asio_service_options &options
    ) : service(asio_service), options(options) {
    }
};

BOP_API bop_raft_asio_service_ptr *bop_raft_asio_service_create(
    /**
     * Number of ASIO worker threads.
     * If zero, it will be automatically set to number of cores.
     */
    size_t thread_pool_size,

    /**
     * Lifecycle callback function on worker thread start.
     */
    void *worker_start_user_data,
    /**
     * Lifecycle callback function on worker thread start.
     */
    bop_raft_asio_service_worker_start_fn worker_start,
    /**
     * Lifecycle callback function on worker thread start.
     */
    void *worker_stop_user_data,
    /**
     * Lifecycle callback function on worker thread start.
     */
    bop_raft_asio_service_worker_stop_fn worker_stop,
    /**
     * If `true`, enable SSL/TLS secure connection.
     */
    bool enable_ssl,

    /**
     * If `true`, skip certificate verification.
     */
    bool skip_verification,

    /**
     * Path to server certificate file.
     */
    char *server_cert_file,

    /**
     * Path to server key file.
     */
    char *server_key_file,

    /**
     * Path to root certificate file.
     */
    char *root_cert_file,

    /**
     * If `true`, it will invoke `read_req_meta_` even though
     * the received meta is empty.
     */
    bool invoke_req_cb_on_empty_meta,

    /**
     * If `true`, it will invoke `read_resp_meta_` even though
     * the received meta is empty.
     */
    bool invoke_resp_cb_on_empty_meta,

    void *verify_sn_user_data,
    bop_raft_asio_service_verify_sn_fn verify_sn,

    void *ssl_context_provider_server_user_data,

    /**
     * Callback function that provides pre-configured SSL_CTX.
     * Asio takes ownership of the provided object
     * and disposes it later with SSL_CTX_free.
     *
     * No configuration changes are applied to the provided context,
     * so callback must return properly configured and operational SSL_CTX.
     *
     * Note that it might be unsafe to share SSL_CTX with other threads,
     * consult with your OpenSSL library documentation/guidelines.
     */
    bop_raft_asio_service_ssl_ctx_provider_fn ssl_context_provider_server,

    void *ssl_context_provider_client_user_data,

    /**
     * Callback function that provides pre-configured SSL_CTX.
     * Asio takes ownership of the provided object
     * and disposes it later with SSL_CTX_free.
     *
     * No configuration changes are applied to the provided context,
     * so callback must return properly configured and operational SSL_CTX.
     *
     * Note that it might be unsafe to share SSL_CTX with other threads,
     * consult with your OpenSSL library documentation/guidelines.
     */
    bop_raft_asio_service_ssl_ctx_provider_fn ssl_context_provider_client,

    void *custom_resolver_user_data,

    /**
     * Custom IP address resolver. If given, it will be invoked
     * before the connection is established.
     *
     * If you want to selectively bypass some hosts, just pass the given
     * host and port to the response function as they are.
     */
    bop_raft_asio_service_custom_resolver_fn custom_resolver,

    /**
     * If `true`, each log entry will contain timestamp when it was generated
     * by the leader, and those timestamps will be replicated to all followers
     * so that they will see the same timestamp for the same log entry.
     *
     * To support this feature, the log store implementation should be able to
     * restore the timestamp when it reads log entries.
     *
     * This feature is not backward compatible. To enable this feature, there
     * should not be any member running with old version before supporting
     * this flag.
     */
    bool replicate_log_timestamp,

    /**
     * If `true`, NuRaft will validate the entire message with CRC.
     * Otherwise, it validates the header part only.
     */
    bool crc_on_entire_message,

    /**
     * If `true`, each log entry will contain a CRC checksum of the entry's
     * payload.
     *
     * To support this feature, the log store implementation should be able to
     * store and retrieve the CRC checksum when it reads log entries.
     *
     * This feature is not backward compatible. To enable this feature, there
     * should not be any member running with the old version before supporting
     * this flag.
     */
    bool crc_on_payload,

    void *corrupted_msg_handler_user_data,
    /**
     * Callback function that will be invoked when the received message is corrupted.
     * The first `buffer` contains the raw binary of message header,
     * and the second `buffer` contains the user payload including metadata,
     * if it is not null.
     */
    bop_raft_asio_service_corrupted_msg_handler_fn corrupted_msg_handler,

    /**
     * If `true`,  NuRaft will use streaming mode, which allows it to send
     * subsequent requests without waiting for the response to previous requests.
     * The order of responses will be identical to the order of requests.
     */
    bool streaming_mode,

    bop_raft_logger_ptr *logger
) {
    nuraft::asio_service_options opts{};
    opts.thread_pool_size_ = thread_pool_size;
    if (worker_start != nullptr) {
        opts.worker_start_ = [worker_start, worker_start_user_data](uint32_t value) {
            worker_start(worker_start_user_data, value);
        };
    }
    if (worker_stop != nullptr) {
        opts.worker_stop_ = [worker_stop_user_data, worker_stop](uint32_t value) {
            worker_stop(worker_stop_user_data, value);
        };
    }
    opts.enable_ssl_ = enable_ssl;
    opts.skip_verification_ = skip_verification;
    opts.server_cert_file_ = std::string(server_cert_file);
    opts.server_key_file_ = std::string(server_key_file);
    opts.root_cert_file_ = std::string(root_cert_file);
    opts.invoke_req_cb_on_empty_meta_ = invoke_req_cb_on_empty_meta;
    opts.invoke_resp_cb_on_empty_meta_ = invoke_resp_cb_on_empty_meta;
    if (verify_sn != nullptr) {
        opts.verify_sn_ = [verify_sn, verify_sn_user_data](const std::string &value) {
            return verify_sn(verify_sn_user_data, value.data(), value.size());
        };
    }
    if (ssl_context_provider_server != nullptr) {
        opts.ssl_context_provider_server_ = [ssl_context_provider_server, ssl_context_provider_server_user_data]() {
            return ssl_context_provider_server(ssl_context_provider_server_user_data);
        };
    }
    if (ssl_context_provider_client != nullptr) {
        opts.ssl_context_provider_client_ = [ssl_context_provider_client, ssl_context_provider_client_user_data]() {
            return ssl_context_provider_client(ssl_context_provider_client_user_data);
        };
    }
    if (custom_resolver != nullptr) {
        opts.custom_resolver_ = [custom_resolver, custom_resolver_user_data](
            const std::string &v1, const std::string &v2, nuraft::asio_service_custom_resolver_response response) {
                    return custom_resolver(custom_resolver_user_data, &response, v1.data(), v1.size(), v2.data(),
                                           v2.size(),
                                           [](void *response_impl, const char *v1_, size_t v1_size_, const char *v2_,
                                              size_t v2_size_, int error_code) {
                                               const auto v1_str = std::string(v1_, v1_size_);
                                               const auto v2_str = std::string(v2_, v2_size_);
                                               std::error_code ec;
                                               if (error_code != 0) {
                                                   ec.assign(error_code, std::generic_category());
                                               }
                                               reinterpret_cast<nuraft::asio_service_custom_resolver_response &>(
                                                   response_impl)(v1_str, v2_str, ec);
                                           });
                };
    }
    opts.replicate_log_timestamp_ = replicate_log_timestamp;
    opts.crc_on_entire_message_ = crc_on_entire_message;
    opts.crc_on_payload_ = crc_on_payload;

    if (corrupted_msg_handler != nullptr) {
        opts.corrupted_msg_handler_ = [corrupted_msg_handler, corrupted_msg_handler_user_data](
            std::shared_ptr<nuraft::buffer> header,
            std::shared_ptr<nuraft::buffer> payload
        ) {
                    corrupted_msg_handler(
                        corrupted_msg_handler_user_data,
                        header->data_begin(), header->size(),
                        payload->data_begin(), payload->size()
                    );
                };
    }
    opts.streaming_mode_ = streaming_mode;
    auto result = new bop_raft_asio_service_ptr(std::make_shared<nuraft::asio_service>(opts, logger->logger), opts);
    return result;
}

BOP_API void bop_raft_asio_service_delete(const bop_raft_asio_service_ptr *asio_service) {
    delete asio_service;
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_params
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_params_ptr {
    nuraft::ptr<nuraft::raft_params> params;

    bop_raft_params_ptr(const nuraft::ptr<nuraft::raft_params> &params) : params(params) {
    }
};

// static void bop_raft_params_copy_to(const nuraft::raft_params &from, bop_raft_params &to) {
//     to.election_timeout_upper_bound_ = from.election_timeout_upper_bound_;
//     to.election_timeout_lower_bound_ = from.election_timeout_lower_bound_;
//     to.heart_beat_interval_ = from.heart_beat_interval_;
//     to.rpc_failure_backoff_ = from.rpc_failure_backoff_;
//     to.log_sync_batch_size_ = from.log_sync_batch_size_;
//     to.log_sync_stop_gap_ = from.log_sync_stop_gap_;
//     to.snapshot_distance_ = from.snapshot_distance_;
//     to.snapshot_block_size_ = from.snapshot_block_size_;
//     to.snapshot_sync_ctx_timeout_ = from.snapshot_sync_ctx_timeout_;
//     to.enable_randomized_snapshot_creation_ = from.enable_randomized_snapshot_creation_;
//     to.max_append_size_ = from.max_append_size_;
//     to.reserved_log_items_ = from.reserved_log_items_;
//     to.client_req_timeout_ = from.client_req_timeout_;
//     to.fresh_log_gap_ = from.fresh_log_gap_;
//     to.stale_log_gap_ = from.stale_log_gap_;
//     to.custom_commit_quorum_size_ = from.custom_commit_quorum_size_;
//     to.custom_election_quorum_size_ = from.custom_election_quorum_size_;
//     to.leadership_expiry_ = from.leadership_expiry_;
//     to.leadership_transfer_min_wait_time_ = from.leadership_transfer_min_wait_time_;
//     to.allow_temporary_zero_priority_leader_ = from.allow_temporary_zero_priority_leader_;
//     to.auto_forwarding_ = from.auto_forwarding_;
//     to.auto_forwarding_max_connections_ = from.auto_forwarding_max_connections_;
//     to.use_bg_thread_for_snapshot_io_ = from.use_bg_thread_for_snapshot_io_;
//     to.use_bg_thread_for_urgent_commit_ = from.use_bg_thread_for_urgent_commit_;
//     to.exclude_snp_receiver_from_quorum_ = from.exclude_snp_receiver_from_quorum_;
//     to.auto_adjust_quorum_for_small_cluster_ = from.auto_adjust_quorum_for_small_cluster_;
//     to.locking_method_type_ = static_cast<bop_raft_params_locking_method_type>(from.locking_method_type_);
//     to.return_method_ = static_cast<bop_raft_params_return_method_type>(from.return_method_);
//     to.auto_forwarding_req_timeout_ = from.auto_forwarding_req_timeout_;
//     to.grace_period_of_lagging_state_machine_ = from.grace_period_of_lagging_state_machine_;
//     to.use_new_joiner_type_ = from.use_new_joiner_type_;
//     to.use_bg_thread_for_snapshot_io_ = from.use_bg_thread_for_snapshot_io_;
//     to.use_full_consensus_among_healthy_members_ = from.use_full_consensus_among_healthy_members_;
//     to.parallel_log_appending_ = from.parallel_log_appending_;
//     to.max_log_gap_in_stream_ = from.max_log_gap_in_stream_;
//     to.max_bytes_in_flight_in_stream_ = from.max_bytes_in_flight_in_stream_;
// }

static_assert(
    offsetof(nuraft::raft_params, election_timeout_upper_bound_) ==
    offsetof(bop_raft_params, election_timeout_upper_bound)
);
static_assert(
    offsetof(nuraft::raft_params, election_timeout_lower_bound_) ==
    offsetof(bop_raft_params, election_timeout_lower_bound)
);
static_assert(
    offsetof(nuraft::raft_params, heart_beat_interval_) ==
    offsetof(bop_raft_params, heart_beat_interval)
);
static_assert(
    offsetof(nuraft::raft_params, rpc_failure_backoff_) ==
    offsetof(bop_raft_params, rpc_failure_backoff)
);
static_assert(
    offsetof(nuraft::raft_params, log_sync_batch_size_) ==
    offsetof(bop_raft_params, log_sync_batch_size)
);
static_assert(
    offsetof(nuraft::raft_params, log_sync_stop_gap_) ==
    offsetof(bop_raft_params, log_sync_stop_gap)
);
static_assert(
    offsetof(nuraft::raft_params, snapshot_distance_) ==
    offsetof(bop_raft_params, snapshot_distance)
);
static_assert(
    offsetof(nuraft::raft_params, snapshot_block_size_) ==
    offsetof(bop_raft_params, snapshot_block_size)
);
static_assert(
    offsetof(nuraft::raft_params, snapshot_sync_ctx_timeout_) ==
    offsetof(bop_raft_params, snapshot_sync_ctx_timeout)
);
static_assert(
    offsetof(nuraft::raft_params, enable_randomized_snapshot_creation_) ==
    offsetof(bop_raft_params, enable_randomized_snapshot_creation)
);
static_assert(
    offsetof(nuraft::raft_params, max_append_size_) ==
    offsetof(bop_raft_params, max_append_size)
);
static_assert(
    offsetof(nuraft::raft_params, reserved_log_items_) ==
    offsetof(bop_raft_params, reserved_log_items)
);
static_assert(
    offsetof(nuraft::raft_params, client_req_timeout_) ==
    offsetof(bop_raft_params, client_req_timeout)
);
static_assert(
    offsetof(nuraft::raft_params, fresh_log_gap_) ==
    offsetof(bop_raft_params, fresh_log_gap)
);
static_assert(
    offsetof(nuraft::raft_params, stale_log_gap_) ==
    offsetof(bop_raft_params, stale_log_gap)
);
static_assert(
    offsetof(nuraft::raft_params, custom_commit_quorum_size_) ==
    offsetof(bop_raft_params, custom_commit_quorum_size)
);
static_assert(
    offsetof(nuraft::raft_params, custom_election_quorum_size_) ==
    offsetof(bop_raft_params, custom_election_quorum_size)
);
static_assert(
    offsetof(nuraft::raft_params, leadership_expiry_) ==
    offsetof(bop_raft_params, leadership_expiry)
);
static_assert(
    offsetof(nuraft::raft_params, leadership_transfer_min_wait_time_) ==
    offsetof(bop_raft_params, leadership_transfer_min_wait_time)
);
static_assert(
    offsetof(nuraft::raft_params, allow_temporary_zero_priority_leader_) ==
    offsetof(bop_raft_params, allow_temporary_zero_priority_leader)
);
static_assert(
    offsetof(nuraft::raft_params, auto_forwarding_) ==
    offsetof(bop_raft_params, auto_forwarding)
);
static_assert(
    offsetof(nuraft::raft_params, auto_forwarding_max_connections_) ==
    offsetof(bop_raft_params, auto_forwarding_max_connections)
);
static_assert(
    offsetof(nuraft::raft_params, use_bg_thread_for_snapshot_io_) ==
    offsetof(bop_raft_params, use_bg_thread_for_snapshot_io)
);
static_assert(
    offsetof(nuraft::raft_params, use_bg_thread_for_urgent_commit_) ==
    offsetof(bop_raft_params, use_bg_thread_for_urgent_commit)
);
static_assert(
    offsetof(nuraft::raft_params, exclude_snp_receiver_from_quorum_) ==
    offsetof(bop_raft_params, exclude_snp_receiver_from_quorum)
);
static_assert(
    offsetof(nuraft::raft_params, auto_adjust_quorum_for_small_cluster_) ==
    offsetof(bop_raft_params, auto_adjust_quorum_for_small_cluster)
);
static_assert(
    offsetof(nuraft::raft_params, locking_method_type_) ==
    offsetof(bop_raft_params, locking_method_type)
);
static_assert(
    offsetof(nuraft::raft_params, return_method_) ==
    offsetof(bop_raft_params, return_method)
);
static_assert(
    offsetof(nuraft::raft_params, auto_forwarding_req_timeout_) ==
    offsetof(bop_raft_params, auto_forwarding_req_timeout)
);
static_assert(
    offsetof(nuraft::raft_params, grace_period_of_lagging_state_machine_) ==
    offsetof(bop_raft_params, grace_period_of_lagging_state_machine)
);
static_assert(
    offsetof(nuraft::raft_params, use_new_joiner_type_) ==
    offsetof(bop_raft_params, use_new_joiner_type)
);
static_assert(
    offsetof(nuraft::raft_params, use_bg_thread_for_snapshot_io_) ==
    offsetof(bop_raft_params, use_bg_thread_for_snapshot_io)
);
static_assert(
    offsetof(nuraft::raft_params, use_full_consensus_among_healthy_members_) ==
    offsetof(bop_raft_params, use_full_consensus_among_healthy_members)
);
static_assert(
    offsetof(nuraft::raft_params, parallel_log_appending_) ==
    offsetof(bop_raft_params, parallel_log_appending)
);
static_assert(
    offsetof(nuraft::raft_params, max_log_gap_in_stream_) ==
    offsetof(bop_raft_params, max_log_gap_in_stream)
);
static_assert(
    offsetof(nuraft::raft_params, max_bytes_in_flight_in_stream_) ==
    offsetof(bop_raft_params, max_bytes_in_flight_in_stream)
);

static_assert(sizeof(nuraft::raft_params) == sizeof(bop_raft_params));

BOP_API bop_raft_params *bop_raft_params_alloc() {
    nuraft::raft_params params{};
    bop_raft_params *result = new bop_raft_params;
    *result = *reinterpret_cast<bop_raft_params *>(&params);
    return result;
}

BOP_API void bop_raft_params_delete(const bop_raft_params *params) {
    if (params) delete params;
}

static void bop_raft_params_to_nuraft(bop_raft_params &from, nuraft::raft_params &to) {
    to = *reinterpret_cast<nuraft::raft_params *>(&from);
}

static bop_raft_params bop_raft_params_from_nuraft(nuraft::raft_params &params) {
    bop_raft_params result{};
    result = *reinterpret_cast<bop_raft_params *>(&params);
    return result;
}

BOP_API bop_raft_params_ptr *bop_raft_params_ptr_create(bop_raft_params *params) {
    return new bop_raft_params_ptr(nuraft::ptr<nuraft::raft_params>(reinterpret_cast<nuraft::raft_params *>(params)));
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_machine
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_fsm_ptr {
    nuraft::ptr<nuraft::state_machine> sm;

    bop_raft_fsm_ptr(const nuraft::ptr<nuraft::state_machine> &sm) : sm(sm) {
    }
};

BOP_API uint64_t bop_raft_fsm_adjust_commit_index_peer_index(
    bop_raft_fsm_adjust_commit_index_params *params, const int peerID
) {
    return reinterpret_cast<nuraft::state_machine::adjust_commit_index_params *>(params)->peer_index_map_[peerID];
}

BOP_API void bop_raft_fsm_adjust_commit_index_peer_indexes(
    bop_raft_fsm_adjust_commit_index_params *params, uint64_t *peers[16]
) {
    for (const auto &[peer_id, index]: reinterpret_cast<nuraft::state_machine::adjust_commit_index_params *>(params)->
         peer_index_map_) {
        if (peer_id >= 0 && peer_id < 16) {
            *peers[peer_id] = index;
        }
    }
}

struct bop_raft_state_machine : nuraft::state_machine {
    std::mutex snapshots_mu_{};
    void *user_data_{nullptr};
    bool async_snapshot_{true};
    nuraft::ptr<nuraft::cluster_config> current_conf{nullptr};
    nuraft::ptr<nuraft::cluster_config> rollback_conf{nullptr};
    bop_raft_fsm_commit commit_{nullptr};
    bop_raft_fsm_cluster_config commit_config_{nullptr};
    bop_raft_fsm_commit pre_commit_{nullptr};
    bop_raft_fsm_rollback rollback_{nullptr};
    bop_raft_fsm_cluster_config rollback_config_{nullptr};
    bop_raft_fsm_get_next_batch_size_hint_in_bytes get_next_batch_size_hint_in_bytes_{nullptr};
    bop_raft_fsm_snapshot_save save_snapshot_{nullptr};
    bop_raft_fsm_snapshot_apply apply_snapshot_{nullptr};
    bop_raft_fsm_snapshot_read read_snapshot_{nullptr};
    bop_raft_fsm_free_user_snapshot_ctx free_snapshot_user_ctx_{nullptr};
    bop_raft_fsm_last_snapshot last_snapshot_{nullptr};
    bop_raft_fsm_last_commit_index last_commit_index_{nullptr};
    bop_raft_fsm_create_snapshot create_snapshot_{nullptr};
    bop_raft_fsm_chk_create_snapshot chk_create_snapshot_{nullptr};
    bop_raft_fsm_allow_leadership_transfer allow_leadership_transfer_{nullptr};
    bop_raft_fsm_adjust_commit_index adjust_commit_index_{nullptr};

    bop_raft_state_machine(
        void *user_data,
        const nuraft::ptr<nuraft::cluster_config> &current_conf,
        const nuraft::ptr<nuraft::cluster_config> &rollback_conf,
        bop_raft_fsm_commit commit,
        bop_raft_fsm_cluster_config commit_config,
        bop_raft_fsm_commit pre_commit,
        bop_raft_fsm_rollback rollback,
        bop_raft_fsm_cluster_config rollback_config,
        bop_raft_fsm_get_next_batch_size_hint_in_bytes get_next_batch_size_hint_in_bytes,
        bop_raft_fsm_snapshot_save save_snapshot,
        bop_raft_fsm_snapshot_apply apply_snapshot,
        bop_raft_fsm_snapshot_read read_snapshot,
        bop_raft_fsm_free_user_snapshot_ctx free_snapshot_user_ctx,
        bop_raft_fsm_last_snapshot last_snapshot,
        bop_raft_fsm_last_commit_index last_commit_index,
        bop_raft_fsm_create_snapshot create_snapshot,
        bop_raft_fsm_chk_create_snapshot chk_create_snapshot,
        bop_raft_fsm_allow_leadership_transfer allow_leadership_transfer,
        bop_raft_fsm_adjust_commit_index adjust_commit_index)
        : user_data_(user_data),
          current_conf(current_conf),
          rollback_conf(rollback_conf),
          commit_(commit),
          commit_config_(commit_config),
          pre_commit_(pre_commit),
          rollback_(rollback),
          rollback_config_(rollback_config),
          get_next_batch_size_hint_in_bytes_(get_next_batch_size_hint_in_bytes),
          save_snapshot_(save_snapshot),
          apply_snapshot_(apply_snapshot),
          read_snapshot_(read_snapshot),
          free_snapshot_user_ctx_(free_snapshot_user_ctx),
          last_snapshot_(last_snapshot),
          last_commit_index_(last_commit_index),
          create_snapshot_(create_snapshot),
          chk_create_snapshot_(chk_create_snapshot),
          allow_leadership_transfer_(allow_leadership_transfer),
          adjust_commit_index_(adjust_commit_index) {
    }

    /**
     * Commit the given Raft log.
     *
     * NOTE:
     *   Given memory buffer is owned by caller, so that
     *   commit implementation should clone it if user wants to
     *   use the memory even after the commit call returns.
     *
     *   Here provide a default implementation for facilitating the
     *   situation when application does not care its implementation.
     *
     * @param log_idx Raft log number to commit.
     * @param data Payload of the Raft log.
     * @return Result value of state machine.
     */
    nuraft::ptr<nuraft::buffer> commit(
        const nuraft::ulong log_idx,
        nuraft::buffer &data
    ) override {
        nuraft::buffer *result{nullptr};
        commit_(user_data_, log_idx, data.data_begin(), data.size(), &result);
        return result != nullptr ? nuraft::buffer::make_shared(result) : nullptr;
    }

    /**
     * (Optional)
     * Handler on the commit of a configuration change.
     *
     * @param log_idx Raft log number of the configuration change.
     * @param new_conf New cluster configuration.
     */
    void commit_config(const nuraft::ulong log_idx, nuraft::ptr<nuraft::cluster_config> &new_conf) override {
        current_conf = new_conf;
        commit_config_(user_data_, log_idx, new_conf.get());
    }

    /**
     * Pre-commit the given Raft log.
     *
     * Pre-commit is called after appending Raft log,
     * before getting acks from quorum nodes.
     * Users can ignore this function if not needed.
     *
     * Same as `commit()`, memory buffer is owned by caller.
     *
     * @param log_idx Raft log number to commit.
     * @param data Payload of the Raft log.
     * @return Result value of state machine.
     */
    nuraft::ptr<nuraft::buffer> pre_commit(const nuraft::ulong log_idx,
                                           nuraft::buffer &data) override {
        const auto cb = pre_commit_;
        if (!cb) return nullptr;
        nuraft::buffer *result{nullptr};
        cb(user_data_, log_idx, data.data_begin(), data.size(), &result);
        return result != nullptr ? nuraft::buffer::make_shared(result) : nullptr;
    }

    /**
     * Rollback the state machine to given Raft log number.
     *
     * It will be called for uncommitted Raft logs only,
     * so that users can ignore this function if they don't
     * do anything on pre-commit.
     *
     * Same as `commit()`, memory buffer is owned by caller.
     *
     * @param log_idx Raft log number to commit.
     * @param data Payload of the Raft log.
     */
    void rollback(const nuraft::ulong log_idx,
                  nuraft::buffer &data) override {
        rollback_(user_data_, log_idx, data.data_begin(), data.size());
    }

    /**
     * (Optional)
     * Handler on the rollback of a configuration change.
     * The configuration can be either committed or uncommitted one,
     * and that can be checked by the given `log_idx`, comparing it with
     * the current `cluster_config`'s log index.
     *
     * @param log_idx Raft log number of the configuration change.
     * @param conf The cluster configuration to be rolled back.
     */
    void rollback_config(const nuraft::ulong log_idx, nuraft::ptr<nuraft::cluster_config> &conf) override {
        rollback_conf = conf;
        rollback_config_(user_data_, log_idx, conf.get());
    }

    /**
     * (Optional)
     * Return a hint about the preferred size (in number of bytes)
     * of the next batch of logs to be sent from the leader.
     *
     * Only applicable on followers.
     *
     * @return The preferred size of the next log batch.
     *         `0` indicates no preferred size (any size is good).
     *         `positive value` indicates at least one log can be sent,
     *         (the size of that log may be bigger than this hint size).
     *         `negative value` indicates no log should be sent since this
     *         follower is busy handling pending logs.
     */
    nuraft::int64 get_next_batch_size_hint_in_bytes() override {
        return get_next_batch_size_hint_in_bytes_(user_data_);
    }

    /**
     * Save the given snapshot object to local snapshot.
     * This API is for snapshot receiver (i.e., follower).
     *
     * This is an optional API for users who want to use logical
     * snapshot. Instead of splitting a snapshot into multiple
     * physical chunks, this API uses logical objects corresponding
     * to a unique object ID. Users are responsible for defining
     * what object is: it can be a key-value pair, a set of
     * key-value pairs, or whatever.
     *
     * Same as `commit()`, memory buffer is owned by caller.
     *
     * @param s Snapshot instance to save.
     * @param obj_id[in,out]
     *     Object ID.
     *     As a result of this API call, the next object ID
     *     that reciever wants to get should be set to
     *     this parameter.
     * @param data Payload of given object.
     * @param is_first_obj `true` if this is the first object.
     * @param is_last_obj `true` if this is the last object.
     */
    void save_logical_snp_obj(nuraft::snapshot &s,
                              nuraft::ulong &obj_id,
                              nuraft::buffer &data,
                              bool is_first_obj,
                              bool is_last_obj) override {
        save_snapshot_(
            user_data_,
            s.get_last_log_idx(),
            s.get_last_log_term(),
            reinterpret_cast<bop_raft_cluster_config *>(s.get_last_config().get()),
            s.size(),
            s.get_type(),
            is_first_obj,
            is_last_obj,
            data.data(),
            data.size()
        );
    }

    /**
     * Apply received snapshot to state machine.
     *
     * @param s Snapshot instance to apply.
     * @returm `true` on success.
     */
    bool apply_snapshot(nuraft::snapshot &s) override {
        return apply_snapshot_(
            user_data_,
            s.get_last_log_idx(),
            s.get_last_log_term(),
            reinterpret_cast<bop_raft_cluster_config *>(s.get_last_config().get()),
            s.size(),
            s.get_type()
        );
    }

    /**
     * Read the given snapshot object.
     * This API is for snapshot sender (i.e., leader).
     *
     * Same as above, this is an optional API for users who want to
     * use logical snapshot.
     *
     * @param s Snapshot instance to read.
     * @param[in,out] user_snp_ctx
     *     User-defined instance that needs to be passed through
     *     the entire snapshot read. It can be a pointer to
     *     state machine specific iterators, or whatever.
     *     On the first `read_logical_snp_obj` call, it will be
     *     set to `null`, and this API may return a new pointer if necessary.
     *     Returned pointer will be passed to next `read_logical_snp_obj`
     *     call.
     * @param obj_id Object ID to read.
     * @param[out] data_out Buffer where the read object will be stored.
     * @param[out] is_last_obj Set `true` if this is the last object.
     * @return Negative number if failed.
     */
    int read_logical_snp_obj(
        nuraft::snapshot &s,
        void *&user_snp_ctx,
        nuraft::ulong obj_id,
        nuraft::ptr<nuraft::buffer> &data_out,
        bool &is_last_obj
    ) override {
        bop_raft_buffer *data{nullptr};
        int result = read_snapshot_(
            user_data_,
            static_cast<void **>(user_snp_ctx),
            obj_id,
            &data,
            reinterpret_cast<bool *>(is_last_obj)
        );
        if (data == nullptr) {
            is_last_obj = true;
        } else {
            data_out = nuraft::buffer::make_shared(reinterpret_cast<nuraft::buffer *>(data));
        }
        return result;
    }

    /**
     * Free user-defined instance that is allocated by
     * `read_logical_snp_obj`.
     * This is an optional API for users who want to use logical snapshot.
     *
     * @param user_snp_ctx User-defined instance to free.
     */
    void free_user_snp_ctx(void *&user_snp_ctx) override {
        free_snapshot_user_ctx_(user_data_, static_cast<void **>(user_snp_ctx));
    }

    /**
     * Get the latest snapshot instance.
     *
     * This API will be invoked at the initialization of Raft server,
     * so that the last last snapshot should be durable for server restart,
     * if you want to avoid unnecessary catch-up.
     *
     * @return Pointer to the latest snapshot.
     */
    nuraft::ptr<nuraft::snapshot> last_snapshot() override {
        return nuraft::ptr<nuraft::snapshot>(reinterpret_cast<nuraft::snapshot *>(last_snapshot_(user_data_)));
    }

    /**
     * Get the last committed Raft log number.
     *
     * This API will be invoked at the initialization of Raft server
     * to identify what the last committed point is, so that the last
     * committed index number should be durable for server restart,
     * if you want to avoid unnecessary catch-up.
     *
     * @return Last committed Raft log number.
     */
    nuraft::ulong last_commit_index() override {
        return last_commit_index_(user_data_);
    }

    /**
     * Create a snapshot corresponding to the given info.
     *
     * @param s Snapshot info to create.
     * @param when_done Callback function that will be called after
     *                  snapshot creation is done.
     */
    void create_snapshot(
        nuraft::snapshot &s,
        nuraft::async_result<bool>::handler_type &when_done
    ) override {
        // Clone snapshot from `s`.
        nuraft::ptr<nuraft::buffer> snp_buf = s.serialize();
        nuraft::ptr<nuraft::snapshot> snp = nuraft::snapshot::deserialize(*snp_buf);

        if (async_snapshot_) {
            std::thread snp_thread([this, snp, snp_buf, when_done] {
                try {
                    create_snapshot_(
                        user_data_,
                        reinterpret_cast<bop_raft_snapshot *>(snp.get()),
                        reinterpret_cast<bop_raft_buffer *>(snp_buf.get()),
                        snp_buf->data(),
                        snp_buf->size()
                    );
                    bool done = true;
                    nuraft::ptr<std::exception> err{nullptr};
                    when_done(done, err);
                } catch (std::exception e) {
                    bool done = true;
                    nuraft::ptr<std::exception> err = nuraft::cs_new<std::exception>(e);
                    when_done(done, err);
                }
            });
        } else {
            try {
                create_snapshot_(
                    user_data_,
                    reinterpret_cast<bop_raft_snapshot *>(snp.get()),
                    reinterpret_cast<bop_raft_buffer *>(snp_buf.get()),
                    snp_buf->data(),
                    snp_buf->size()
                );
                bool done = true;
                nuraft::ptr<std::exception> err{nullptr};
                when_done(done, err);
            } catch (std::exception e) {
                bool done = true;
                nuraft::ptr<std::exception> err = nuraft::cs_new<std::exception>(e);
                when_done(done, err);
            }
        }
    }

    /**
     * Decide to create snapshot or not.
     * Once the pre-defined condition is satisfied, Raft core will invoke
     * this function to ask if it needs to create a new snapshot.
     * If user-defined state machine does not want to create snapshot
     * at this time, this function will return `false`.
     *
     * @return `true` if wants to create snapshot.
     *         `false` if does not want to create snapshot.
     */
    bool chk_create_snapshot() override {
        return chk_create_snapshot_(user_data_);
    }

    /**
     * Decide to transfer leadership.
     * Once the other conditions are met, Raft core will invoke
     * this function to ask if it is allowed to transfer the
     * leadership to other member.
     *
     * @return `true` if wants to transfer leadership.
     *         `false` if not.
     */
    bool allow_leadership_transfer() override {
        return allow_leadership_transfer_(user_data_);
    }

    /**
     * This function will be called when Raft succeeds in replicating logs
     * to an arbitrary follower and attempts to commit logs. Users can manually
     * adjust the commit index. The adjusted commit index should be equal to
     * or greater than the given `current_commit_index`. Otherwise, no log
     * will be committed.
     *
     * @param params Parameters.
     * @return Adjusted commit index.
     */
    uint64_t adjust_commit_index(const adjust_commit_index_params &params) override {
        return adjust_commit_index_(
            user_data_,
            params.current_commit_index_,
            params.expected_commit_index_,
            reinterpret_cast<const bop_raft_fsm_adjust_commit_index_params *>(&params)
        );
    }
};

BOP_API bop_raft_fsm_ptr *bop_raft_fsm_create(
    void *user_data,
    bop_raft_cluster_config *current_conf,
    bop_raft_cluster_config *rollback_conf,
    bop_raft_fsm_commit commit,
    bop_raft_fsm_cluster_config commit_config,
    bop_raft_fsm_commit pre_commit,
    bop_raft_fsm_rollback rollback,
    bop_raft_fsm_cluster_config rollback_config,
    bop_raft_fsm_get_next_batch_size_hint_in_bytes get_next_batch_size_hint_in_bytes,
    bop_raft_fsm_snapshot_save save_snapshot,
    bop_raft_fsm_snapshot_apply apply_snapshot,
    bop_raft_fsm_snapshot_read read_snapshot,
    bop_raft_fsm_free_user_snapshot_ctx free_snapshot_user_ctx,
    bop_raft_fsm_last_snapshot last_snapshot,
    bop_raft_fsm_last_commit_index last_commit_index,
    bop_raft_fsm_create_snapshot create_snapshot,
    bop_raft_fsm_chk_create_snapshot chk_create_snapshot,
    bop_raft_fsm_allow_leadership_transfer allow_leadership_transfer,
    bop_raft_fsm_adjust_commit_index adjust_commit_index
) {
    if (!commit) return nullptr;
    if (!commit_config) return nullptr;
    if (!pre_commit) return nullptr;
    if (!rollback) return nullptr;
    if (!rollback_conf) return nullptr;
    if (!get_next_batch_size_hint_in_bytes) return nullptr;
    if (!save_snapshot) return nullptr;
    if (!apply_snapshot) return nullptr;
    if (!read_snapshot) return nullptr;
    if (!free_snapshot_user_ctx) return nullptr;
    if (!last_snapshot) return nullptr;
    if (!last_commit_index) return nullptr;
    if (!create_snapshot) return nullptr;
    if (!chk_create_snapshot) return nullptr;
    if (!allow_leadership_transfer) return nullptr;
    if (!adjust_commit_index) return nullptr;
    return new bop_raft_fsm_ptr(std::make_shared<bop_raft_state_machine>(
        user_data,
        nuraft::ptr<nuraft::cluster_config>(reinterpret_cast<nuraft::cluster_config*>(current_conf)),
        nuraft::ptr<nuraft::cluster_config>(reinterpret_cast<nuraft::cluster_config*>(rollback_conf)),
        commit,
        commit_config,
        pre_commit,
        rollback,
        rollback_config,
        get_next_batch_size_hint_in_bytes,
        save_snapshot,
        apply_snapshot,
        read_snapshot,
        free_snapshot_user_ctx,
        last_snapshot,
        last_commit_index,
        create_snapshot,
        chk_create_snapshot,
        allow_leadership_transfer,
        adjust_commit_index
    ));
}

BOP_API void bop_raft_fsm_delete(const bop_raft_fsm_ptr *fsm) {
    if (fsm) delete fsm;
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_mgr
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_state_mgr_t {
    nuraft::ptr<nuraft::state_mgr> state_mgr;

    bop_raft_state_mgr_t(const nuraft::ptr<nuraft::state_mgr> &state_mgr) : state_mgr(state_mgr) {
    }
};

typedef nuraft::cluster_config * (*bop_raft_state_mgr_load_config)(void *user_data);

typedef void (*bop_raft_state_mgr_save_config)(
    void *user_data,
    const nuraft::cluster_config *config
);

typedef void (*bop_raft_state_mgr_save_state)(
    void *user_data,
    const nuraft::srv_state *state
);

typedef nuraft::srv_state * (*bop_raft_state_mgr_read_state)(
    void *user_data
);

typedef nuraft::log_store * (*bop_raft_state_mgr_load_log_store)(
    void *user_data
);

typedef int32_t (*bop_raft_state_mgr_server_id)(
    void *user_data
);

typedef void (*bop_raft_state_mgr_system_exit)(
    void *user_data,
    const int exit_code
);


struct bop_raft_state_mgr : nuraft::state_mgr {
    void *user_data_;
    bop_raft_state_mgr_load_config load_config_;
    bop_raft_state_mgr_save_config save_config_;
    bop_raft_state_mgr_save_state save_state_;
    bop_raft_state_mgr_read_state read_state_;
    bop_raft_state_mgr_load_log_store load_log_store_;
    bop_raft_state_mgr_server_id server_id_;
    bop_raft_state_mgr_system_exit system_exit_;

    bop_raft_state_mgr(
        void *user_data,
        bop_raft_state_mgr_load_config load_config,
        bop_raft_state_mgr_save_config save_config,
        bop_raft_state_mgr_save_state save_state,
        bop_raft_state_mgr_read_state read_state,
        bop_raft_state_mgr_load_log_store load_log_store,
        bop_raft_state_mgr_server_id server_id,
        bop_raft_state_mgr_system_exit system_exit)
        : user_data_(user_data),
          load_config_(load_config),
          save_config_(save_config),
          save_state_(save_state),
          read_state_(read_state),
          load_log_store_(load_log_store),
          server_id_(server_id),
          system_exit_(system_exit) {
    }

    /**
     * Load the last saved cluster config.
     * This function will be invoked on initialization of
     * Raft server.
     *
     * Even at the very first initialization, it should
     * return proper initial cluster config, not `nullptr`.
     * The initial cluster config must include the server itself.
     *
     * @return Cluster config.
     */
    nuraft::ptr<nuraft::cluster_config> load_config() override {
        return nuraft::ptr<nuraft::cluster_config>(load_config_(user_data_));
    }

    /**
     * Save given cluster config.
     *
     * @param config Cluster config to save.
     */
    void save_config(const nuraft::cluster_config &config) override {
        save_config_(user_data_, &config);
    }

    /**
     * Save given server state.
     *
     * @param state Server state to save.
     */
    void save_state(const nuraft::srv_state &state) override {
        save_state_(user_data_, &state);
    }

    /**
     * Load the last saved server state.
     * This function will be invoked on initialization of
     * Raft server
     *
     * At the very first initialization, it should return
     * `nullptr`.
     *
     * @param Server state.
     */
    nuraft::ptr<nuraft::srv_state> read_state() override {
        return nuraft::ptr<nuraft::srv_state>(read_state_(user_data_));
    }

    /**
     * Get instance of user-defined Raft log store.
     *
     * @param Raft log store instance.
     */
    nuraft::ptr<nuraft::log_store> load_log_store() override {
        return nuraft::ptr<nuraft::log_store>(load_log_store_(user_data_));
    }

    /**
     * Get ID of this Raft server.
     *
     * @return Server ID.
     */
    nuraft::int32 server_id() override {
        return server_id_(user_data_);
    }

    /**
     * System exit handler. This function will be invoked on
     * abnormal termination of Raft server.
     *
     * @param exit_code Error code.
     */
    void system_exit(const int exit_code) override {
        system_exit_(user_data_, exit_code);
    }
};

BOP_API bop_raft_state_mgr_t *bop_raft_state_mgr_create(
    void *user_data,
    bop_raft_state_mgr_load_config load_config,
    bop_raft_state_mgr_save_config save_config,
    bop_raft_state_mgr_save_state save_state,
    bop_raft_state_mgr_read_state read_state,
    bop_raft_state_mgr_load_log_store load_log_store,
    bop_raft_state_mgr_server_id server_id,
    bop_raft_state_mgr_system_exit system_exit
) {
    if (!load_config) return nullptr;
    if (!save_config) return nullptr;
    if (!save_state) return nullptr;
    if (!read_state) return nullptr;
    if (!load_log_store) return nullptr;
    if (!server_id) return nullptr;
    if (!system_exit) return nullptr;
    return new bop_raft_state_mgr_t(
        nuraft::cs_new<bop_raft_state_mgr>(
            user_data,
            load_config,
            save_config,
            save_state,
            read_state,
            load_log_store,
            server_id,
            system_exit
        )
    );
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::log_store
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_log_store_t {
    nuraft::ptr<nuraft::log_store> log_store;

    bop_raft_log_store_t(const nuraft::ptr<nuraft::log_store> &log_store) : log_store(log_store) {
    }
};

typedef uint64_t (*bop_raft_log_store_next_slot)(void *user_data);

typedef uint64_t (*bop_raft_log_store_start_index)(void *user_data);

typedef nuraft::log_entry * (*bop_raft_log_store_last_entry)(void *user_data);

typedef uint64_t (*bop_raft_log_store_append)(
    void *user_data,
    uint64_t term,
    uint8_t *data,
    size_t data_size,
    uint64_t log_timestamp,
    bool has_crc32,
    uint32_t crc32
);

typedef void (*bop_raft_log_store_write_at)(
    void *user_data,
    uint64_t index,
    uint64_t term,
    uint8_t *data,
    size_t data_size,
    uint64_t log_timestamp,
    bool has_crc32,
    uint32_t crc32
);

typedef void (*bop_raft_log_store_end_of_append_batch)(
    void *user_data,
    uint64_t start,
    uint64_t cnt
);

struct bop_raft_log_entry_vector {
    std::vector<nuraft::ptr<nuraft::log_entry> > *log_entries;

    bop_raft_log_entry_vector(std::vector<nuraft::ptr<nuraft::log_entry> > *log_entries): log_entries(log_entries) {
    }
};

BOP_API void bop_raft_log_entry_vector_push(
    const bop_raft_log_entry_vector *vec,
    uint64_t term,
    nuraft::buffer *data,
    uint64_t timestamp,
    bool has_crc32,
    uint32_t crc32
) {
    const auto buf = nuraft::ptr<nuraft::buffer>(data);
    vec->log_entries->push_back(std::make_shared<nuraft::log_entry>(
        term, std::move(buf), nuraft::log_val_type::app_log, timestamp, has_crc32, crc32
    ));
}

typedef bool (*bop_raft_log_store_log_entries)(
    void *user_data,
    bop_raft_log_entry_vector *vec,
    uint64_t start,
    uint64_t end
);

typedef nuraft::log_entry * (*bop_raft_log_store_entry_at)(
    void *user_data,
    uint64_t index
);

BOP_API nuraft::log_entry *bop_raft_log_entry_create(
    uint64_t term,
    nuraft::buffer *data,
    uint64_t timestamp,
    bool has_crc32,
    uint32_t crc32
) {
    const auto buf = nuraft::ptr<nuraft::buffer>(data);
    return new nuraft::log_entry(
        term, std::move(buf), nuraft::log_val_type::app_log, timestamp, has_crc32, crc32
    );
}

typedef uint64_t (*bop_raft_log_store_term_at)(
    void *user_data,
    uint64_t index
);

typedef nuraft::buffer * (*bop_raft_log_store_pack)(
    void *user_data,
    uint64_t index,
    int32_t cnt
);

typedef void (*bop_raft_log_store_apply_pack)(
    void *user_data,
    uint64_t index,
    nuraft::buffer *pack
);

typedef bool (*bop_raft_log_store_compact)(void *user_data, uint64_t last_log_index);

typedef bool (*bop_raft_log_store_compact_async)(void *user_data, uint64_t last_log_index);

typedef bool (*bop_raft_log_store_flush)(void *user_data);

typedef uint64_t (*bop_raft_log_store_last_durable_index)(void *user_data);

struct bop_raft_log_store : nuraft::log_store {
    void *user_data_;
    bop_raft_log_store_next_slot next_slot_;
    bop_raft_log_store_start_index start_index_;
    bop_raft_log_store_last_entry last_entry_;
    bop_raft_log_store_append append_;
    bop_raft_log_store_write_at write_at_;
    bop_raft_log_store_end_of_append_batch end_of_append_batch_;
    bop_raft_log_store_log_entries log_entries_;
    bop_raft_log_store_entry_at entry_at_;
    bop_raft_log_store_term_at term_at_;
    bop_raft_log_store_pack pack_;
    bop_raft_log_store_apply_pack apply_pack_;
    bop_raft_log_store_compact compact_;
    bop_raft_log_store_compact_async compact_async_;
    bop_raft_log_store_flush flush_;
    bop_raft_log_store_last_durable_index last_durable_index_;

    bop_raft_log_store(
        void *user_data,
        bop_raft_log_store_next_slot next_slot,
        bop_raft_log_store_start_index start_index,
        bop_raft_log_store_last_entry last_entry,
        bop_raft_log_store_append append,
        bop_raft_log_store_write_at write_at,
        bop_raft_log_store_end_of_append_batch end_of_append_batch,
        bop_raft_log_store_log_entries log_entries,
        bop_raft_log_store_entry_at entry_at,
        bop_raft_log_store_term_at term_at,
        bop_raft_log_store_pack pack,
        bop_raft_log_store_apply_pack apply_pack,
        bop_raft_log_store_compact compact,
        bop_raft_log_store_compact_async compact_async,
        bop_raft_log_store_flush flush,
        bop_raft_log_store_last_durable_index last_durable_index)
        : user_data_(user_data),
          next_slot_(next_slot),
          start_index_(start_index),
          last_entry_(last_entry),
          append_(append),
          write_at_(write_at),
          end_of_append_batch_(end_of_append_batch),
          log_entries_(log_entries),
          entry_at_(entry_at),
          term_at_(term_at),
          pack_(pack),
          apply_pack_(apply_pack),
          compact_(compact),
          compact_async_(compact_async),
          flush_(flush),
          last_durable_index_(last_durable_index) {
    }

    /**
     * The first available slot of the store, starts with 1
     *
     * @return Last log index number + 1
     */
    nuraft::ulong next_slot() const override {
        return next_slot_(user_data_);
    }

    /**
     * The start index of the log store, at the very beginning, it must be 1.
     * However, after some compact actions, this could be anything equal to or
     * greater than one
     */
    nuraft::ulong start_index() const override {
        return start_index_(user_data_);
    }

    /**
     * The last log entry in store.
     *
     * @return If no log entry exists: a dummy constant entry with
     *         value set to null and term set to zero.
     */
    nuraft::ptr<nuraft::log_entry> last_entry() const override {
        return nuraft::ptr<nuraft::log_entry>(last_entry_(user_data_));
    }

    /**
     * Append a log entry to store.
     *
     * @param entry Log entry
     * @return Log index number.
     */
    nuraft::ulong append(nuraft::ptr<nuraft::log_entry> &entry) override {
        return append_(
            user_data_,
            entry->get_term(),
            entry->get_buf().data_begin(),
            entry->get_buf().size(),
            entry->get_timestamp(),
            entry->has_crc32(),
            entry->get_crc32()
        );
    }

    /**
     * Overwrite a log entry at the given `index`.
     * This API should make sure that all log entries
     * after the given `index` should be truncated (if exist),
     * as a result of this function call.
     *
     * @param index Log index number to overwrite.
     * @param entry New log entry to overwrite.
     */
    void write_at(nuraft::ulong index, nuraft::ptr<nuraft::log_entry> &entry) override {
        write_at_(
            user_data_,
            index,
            entry->get_term(),
            entry->get_buf().data_begin(),
            entry->get_buf().size(),
            entry->get_timestamp(),
            entry->has_crc32(),
            entry->get_crc32()
        );
    }

    /**
     * Invoked after a batch of logs is written as a part of
     * a single append_entries request.
     *
     * @param start The start log index number (inclusive)
     * @param cnt The number of log entries written.
     */
    void end_of_append_batch(nuraft::ulong start, nuraft::ulong cnt) override {
        end_of_append_batch_(user_data_, start, cnt);
    }

    /**
     * Get log entries with index [start, end).
     *
     * Return nullptr to indicate error if any log entry within the requested range
     * could not be retrieved (e.g. due to external log truncation).
     *
     * @param start The start log index number (inclusive).
     * @param end The end log index number (exclusive).
     * @return The log entries between [start, end).
     */
    nuraft::ptr<std::vector<nuraft::ptr<nuraft::log_entry> > >
    log_entries(nuraft::ulong start, nuraft::ulong end) override {
        try {
            auto result = nuraft::cs_new<std::vector<nuraft::ptr<nuraft::log_entry> > >();
            result->reserve(end - start);
            bop_raft_log_entry_vector vec{result.get()};
            if (!log_entries_(user_data_, &vec, start, end)) {
                return nullptr;
            }
            return result;
        } catch (...) {
            // TODO: Log it
            return nullptr;
        }
    }

    /**
     * (Optional)
     * Get log entries with index [start, end).
     *
     * The total size of the returned entries is limited by batch_size_hint.
     *
     * Return nullptr to indicate error if any log entry within the requested range
     * could not be retrieved (e.g. due to external log truncation).
     *
     * @param start The start log index number (inclusive).
     * @param end The end log index number (exclusive).
     * @param batch_size_hint_in_bytes Total size (in bytes) of the returned entries,
     *        see the detailed comment at
     *        `state_machine::get_next_batch_size_hint_in_bytes()`.
     * @return The log entries between [start, end) and limited by the total size
     *         given by the batch_size_hint_in_bytes.
     */
    nuraft::ptr<std::vector<nuraft::ptr<nuraft::log_entry> > > log_entries_ext(
        nuraft::ulong start, nuraft::ulong end, nuraft::int64 batch_size_hint_in_bytes = 0) override {
        return log_entries(start, end);
    }

    /**
     * Get the log entry at the specified log index number.
     *
     * @param index Should be equal to or greater than 1.
     * @return The log entry or null if index >= this->next_slot().
     */
    nuraft::ptr<nuraft::log_entry> entry_at(nuraft::ulong index) override {
        return nuraft::ptr<nuraft::log_entry>(entry_at_(user_data_, index));
    }

    /**
     * Get the term for the log entry at the specified index.
     * Suggest to stop the system if the index >= this->next_slot()
     *
     * @param index Should be equal to or greater than 1.
     * @return The term for the specified log entry, or
     *         0 if index < this->start_index().
     */
    nuraft::ulong term_at(nuraft::ulong index) override {
        return term_at_(user_data_, index);
    }

    /**
     * Pack the given number of log items starting from the given index.
     *
     * @param index The start log index number (inclusive).
     * @param cnt The number of logs to pack.
     * @return Packed (encoded) logs.
     */
    nuraft::ptr<nuraft::buffer> pack(nuraft::ulong index, nuraft::int32 cnt) override {
        return nuraft::ptr<nuraft::buffer>(pack_(user_data_, index, cnt));
    }

    /**
     * Apply the log pack to current log store, starting from index.
     *
     * @param index The start log index number (inclusive).
     * @param Packed logs.
     */
    void apply_pack(nuraft::ulong index, nuraft::buffer &pack) override {
        apply_pack_(user_data_, index, &pack);
    }

    /**
     * Compact the log store by purging all log entries,
     * including the given log index number.
     *
     * If current maximum log index is smaller than given `last_log_index`,
     * set start log index to `last_log_index + 1`.
     *
     * @param last_log_index Log index number that will be purged up to (inclusive).
     * @return `true` on success.
     */
    bool compact(nuraft::ulong last_log_index) override {
        return compact_(user_data_, last_log_index);
    }

    /**
     * Compact the log store by purging all log entries,
     * including the given log index number.
     *
     * Unlike `compact`, this API allows to execute the log compaction in background
     * asynchronously, aiming at reducing the client-facing latency caused by the
     * log compaction.
     *
     * This function call may return immediately, but after this function
     * call, following `start_index` should return `last_log_index + 1` even
     * though the log compaction is still in progress. In the meantime, the
     * actual job incurring disk IO can run in background. Once the job is done,
     * `when_done` should be invoked.
     *
     * @param last_log_index Log index number that will be purged up to (inclusive).
     * @param when_done Callback function that will be called after
     *                  the log compaction is done.
     */
    void compact_async(
        nuraft::ulong last_log_index,
        const nuraft::async_result<bool>::handler_type &when_done
    ) override {
        std::thread tr([this, last_log_index, when_done]() {
            bool rc = compact_async_(user_data_, last_log_index);
            nuraft::ptr<std::exception> exp(nullptr);
            when_done(rc, exp);
        });
        tr.detach();
    }

    /**
     * Synchronously flush all log entries in this log store to the backing storage
     * so that all log entries are guaranteed to be durable upon process crash.
     *
     * @return `true` on success.
     */
    bool flush() override {
        return flush_(user_data_);
    }

    /**
     * (Experimental)
     * This API is used only when `raft_params::parallel_log_appending_` flag is set.
     * Please refer to the comment of the flag.
     *
     * @return The last durable log index.
     */
    nuraft::ulong last_durable_index() override {
        return last_durable_index_(user_data_);
    }
};

BOP_API bop_raft_log_store_t *bop_raft_log_store_create(
    void *user_data,
    bop_raft_log_store_next_slot next_slot,
    bop_raft_log_store_start_index start_index,
    bop_raft_log_store_last_entry last_entry,
    bop_raft_log_store_append append,
    bop_raft_log_store_write_at write_at,
    bop_raft_log_store_end_of_append_batch end_of_append_batch,
    bop_raft_log_store_log_entries log_entries,
    bop_raft_log_store_entry_at entry_at,
    bop_raft_log_store_term_at term_at,
    bop_raft_log_store_pack pack,
    bop_raft_log_store_apply_pack apply_pack,
    bop_raft_log_store_compact compact,
    bop_raft_log_store_compact_async compact_async,
    bop_raft_log_store_flush flush,
    bop_raft_log_store_last_durable_index last_durable_index
) {
    if (!next_slot) return nullptr;
    if (!start_index) return nullptr;
    if (!last_entry) return nullptr;
    if (!append) return nullptr;
    if (!write_at) return nullptr;
    if (!end_of_append_batch) return nullptr;
    if (!log_entries) return nullptr;
    if (!entry_at) return nullptr;
    if (!term_at) return nullptr;
    if (!pack) return nullptr;
    if (!apply_pack) return nullptr;
    if (!compact) return nullptr;
    if (!compact_async) return nullptr;
    if (!flush) return nullptr;
    if (!last_durable_index) return nullptr;
    return new bop_raft_log_store_t(std::make_shared<bop_raft_log_store>(
        user_data, next_slot, start_index, last_entry, append, write_at, end_of_append_batch, log_entries,
        entry_at, term_at, pack, apply_pack, compact, compact_async, flush, last_durable_index
    ));
}

BOP_API void bop_raft_log_store_delete(const bop_raft_log_store_t *log_store) {
    if (log_store) delete log_store;
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::context
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_context_t {
    nuraft::ptr<nuraft::context> ctx;
};

BOP_API bop_raft_context_t *bop_raft_context_create() {
    return new bop_raft_context_t;
}

BOP_API void bop_raft_context_delete(const bop_raft_context_t *context) {
    if (context) delete context;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_server
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_server_ptr {
    nuraft::ptr<nuraft::raft_server> raft_server;
    nuraft::ptr<nuraft::asio_service> asio_service;
    nuraft::ptr<nuraft::rpc_listener> asio_listener;
    nuraft::ptr<nuraft::log_store> log_store;

    bop_raft_server_ptr(const nuraft::ptr<nuraft::raft_server> &raft_server,
                        const nuraft::ptr<nuraft::asio_service> &asio_service,
                        const nuraft::ptr<nuraft::rpc_listener> &asio_listener,
                        const nuraft::ptr<nuraft::log_store> &log_store)
        : raft_server(raft_server),
          asio_service(asio_service),
          asio_listener(asio_listener),
          log_store(log_store) {
    }
};

BOP_API bop_raft_server_ptr *bop_raft_server_launch(
    bop_raft_fsm_ptr *fsm,
    bop_raft_state_mgr_t *state_mgr,
    bop_raft_logger_ptr *logger,
    int port_number,
    bop_raft_asio_service_ptr *asio_service,
    nuraft::raft_params *params_given,
    nuraft::raft_server::init_options *init_options
) {
    auto asio_listener = asio_service->service->create_rpc_listener(port_number, logger->logger);
    if (!asio_listener) return nullptr;

    nuraft::ptr<nuraft::delayed_task_scheduler> scheduler = asio_service->service;
    nuraft::ptr<nuraft::rpc_client_factory> rpc_client_factory = asio_service->service;

    nuraft::context *ctx = new nuraft::context(
        state_mgr->state_mgr,
        fsm->sm,
        asio_listener,
        logger->logger,
        rpc_client_factory,
        scheduler,
        *params_given
    );

    nuraft::ptr<nuraft::raft_server> raft_server = nuraft::cs_new<nuraft::raft_server>(ctx, *init_options);
    asio_listener->listen(raft_server);

    return new bop_raft_server_ptr(raft_server, asio_service->service, asio_listener, raft_server->get_log_store());
}

BOP_API bool bop_raft_server_stop(bop_raft_server_ptr *server, size_t time_limit_sec) {
    if (!server->raft_server) return false;

    server->raft_server->shutdown();
    server->raft_server.reset();

    if (server->asio_listener) {
        server->asio_listener->stop();
        server->asio_listener->shutdown();
    }

    if (server->asio_service) {
        server->asio_service->stop();
        size_t count = 0;
        while (server->asio_service->get_active_workers() && count < time_limit_sec * 100) {
            nuraft::timer_helper::sleep_ms(10);
            count++;
        }
    }
    if (server->asio_service->get_active_workers()) {
        return false;
    }
    return true;
}

/**
 * Check if this server is ready to serve operation.
 *
 * @return `true` if it is ready.
 */
BOP_API bool bop_raft_server_is_initialized(
    const bop_raft_server_ptr *server
) {
    if (!server) return false;
    return server->raft_server->is_initialized();
}

/**
 * Check if this server is catching up the current leader
 * to join the cluster.
 *
 * @return `true` if it is in catch-up mode.
 */
BOP_API bool bop_raft_server_is_catching_up(
    const bop_raft_server_ptr *server
) {
    if (!server) return false;
    return server->raft_server->is_catching_up();
}

/**
 * Check if this server is receiving snapshot from leader.
 *
 * @return `true` if it is receiving snapshot.
 */
BOP_API bool bop_raft_server_is_receiving_snapshot(
    const bop_raft_server_ptr *server
) {
    if (!server) return false;
    return server->raft_server->is_receiving_snapshot();
}

/**
 * Add a new server to the current cluster.
 * Only leader will accept this operation.
 * Note that this is an asynchronous task so that needs more network
 * communications. Returning this function does not guarantee
 * adding the server.
 *
 * @param srv Configuration of server to add.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_add_srv(
    const bop_raft_server_ptr *server,
    const bop_raft_srv_config_ptr *srv,
    bop_raft_cmd_result_buffer_t *handler
) {
    if (!server) return false;
    if (!srv) return false;
    if (!handler) return false;
    if (!srv->config) return false;
    handler->set(server->raft_server->add_srv(*srv->config));
    return true;
}

/**
 * Remove a server from the current cluster.
 * Only leader will accept this operation.
 * The same as `add_srv`, this is also an asynchronous task.
 *
 * @param srv_id ID of server to remove.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_remove_srv(
    const bop_raft_server_ptr *server,
    int srv_id,
    bop_raft_cmd_result_buffer_t *handler
) {
    if (!server) return false;
    if (!handler) return false;
    handler->set(server->raft_server->remove_srv(srv_id));
    return true;
}

/**
 * Flip learner flag of given server.
 * Learner will be excluded from the quorum.
 * Only leader will accept this operation.
 * This is also an asynchronous task.
 *
 * @param srv_id ID of the server to set as a learner.
 * @param to If `true`, set the server as a learner, otherwise, clear learner flag.
 * @return `ret->get_result_code()` will be OK on success.
 */
BOP_API bool bop_raft_server_flip_learner_flag(
    const bop_raft_server_ptr *server,
    const int32_t srv_id,
    const bool to,
    bop_raft_cmd_result_buffer_t *handler
) {
    if (!server) return false;
    if (!handler) return false;
    handler->set(server->raft_server->flip_learner_flag(srv_id, to));
    return true;
}

struct bop_raft_append_entries_t {
    std::vector<nuraft::ptr<nuraft::buffer> > logs{};
};

BOP_API bop_raft_append_entries_t *bop_raft_append_entries_create() {
    return new bop_raft_append_entries_t;
}

BOP_API void bop_raft_append_entries_delete(const bop_raft_append_entries_t *self) {
    if (self) {
        delete self;
    }
}

BOP_API size_t bop_raft_append_entries_size(bop_raft_append_entries_t *self) {
    return self->logs.size();
}

BOP_API size_t bop_raft_append_entries_push(bop_raft_append_entries_t *self, nuraft::buffer *buf) {
    if (!self) return 0;
    if (!buf) return 0;
    self->logs.emplace_back(buf);
    return self->logs.size();
}

/**
 * Append and replicate the given logs.
 * Only leader will accept this operation.
 *
 * @param logs Set of logs to replicate.
 * @return
 *     In blocking mode, it will be blocked during replication, and
 *     return `cmd_result` instance which contains the commit results from
 *     the state machine.
 *     In async mode, this function will return immediately, and the
 *     commit results will be set to returned `cmd_result` instance later.
 */
BOP_API bool bop_raft_server_append_entries(
    const bop_raft_server_ptr *server,
    bop_raft_append_entries_t *entries,
    bop_raft_cmd_result_buffer_t *handler
) {
    if (!server || !entries) return false;
    if (!handler) {
        server->raft_server->append_entries(std::move(entries->logs));
    } else {
        handler->set(server->raft_server->append_entries(std::move(entries->logs)));
    }
    return true;
}

/**
 * Update the priority of given server.
 *
 * @param server local nuraft::raft_server instance
 * @param srv_id ID of server to update priority.
 * @param new_priority
 *     Priority value, greater than or equal to 0.
 *     If priority is set to 0, this server will never be a leader.
 * @param broadcast_when_leader_exists
 *     If we're not a leader and a leader exists, broadcast priority change to other
 *     peers. If false, set_priority does nothing. Please note that setting this
 *     option to true may possibly cause cluster config to diverge.
 * @return SET If we're a leader and we have committed priority change.
 * @return BROADCAST
 *     If either there's no live leader now, or we're a leader and we want to set our
 *     priority to 0, or we're not a leader and broadcast_when_leader_exists = true.
 *     We have sent messages to other peers about priority change but haven't
 *     committed this change.
 * @return IGNORED If we're not a leader and broadcast_when_leader_exists = false. We
 *     ignored the request.
 */
BOP_API int bop_raft_server_set_priority(
    const bop_raft_server_ptr *server,
    const int srv_id,
    const int new_priority,
    bool broadcast_when_leader_exists = false
) {
    auto result = server->raft_server->set_priority(srv_id, new_priority, broadcast_when_leader_exists);
    switch (result) {
        case nuraft::raft_server::PrioritySetResult::SET:
            return 0;
        case nuraft::raft_server::PrioritySetResult::BROADCAST:
            return 1;
        case nuraft::raft_server::PrioritySetResult::IGNORED:
            return 2;
    }
    return -1;
}

/**
 * Broadcast the priority change of given server to all peers.
 * This function should be used only when there is no live leader
 * and leader election is blocked by priorities of live followers.
 * In that case, we are not able to change priority by using
 * normal `set_priority` operation.
 *
 * @param server local nuraft::raft_server instance
 * @param srv_id ID of server to update priority.
 * @param new_priority New priority.
 */
BOP_API void bop_raft_server_broadcast_priority_change(
    const bop_raft_server_ptr *server,
    const int srv_id,
    const int new_priority
) {
    server->raft_server->broadcast_priority_change(srv_id, new_priority);
}

/**
 * Yield current leadership and becomes a follower. Only a leader
 * will accept this operation.
 *
 * If given `immediate_yield` flag is `true`, it will become a
 * follower immediately. The subsequent leader election will be
 * totally random so that there is always a chance that this
 * server becomes the next leader again.
 *
 * Otherwise, this server will pause write operations first, wait
 * until the successor (except for this server) finishes the
 * catch-up of the latest log, and then resign. In such a case,
 * the next leader will be much more predictable.
 *
 * Users can designate the successor. If not given, this API will
 * automatically choose the highest priority server as a successor.
 *
 * @param server local nuraft::raft_server instance
 * @param immediate_yield If `true`, yield immediately.
 * @param successor_id The server ID of the successor.
 *                     If `-1`, the successor will be chosen
 *                     automatically.
 */
BOP_API void bop_raft_server_yield_leadership(
    const bop_raft_server_ptr *server,
    bool immediate_yield,
    int successor_id
) {
    server->raft_server->yield_leadership(immediate_yield, successor_id);
}

/**
 * Send a request to the current leader to yield its leadership,
 * and become the next leader.
 *
 * @return `true` on success. But it does not guarantee to become
 *         the next leader due to various failures.
 */
BOP_API bool bop_raft_server_request_leadership(const bop_raft_server_ptr *server) {
    return server->raft_server->request_leadership();
}

/**
 * Start the election timer on this server, if this server is a follower.
 * It will allow the election timer permanently, if it was disabled
 * by state manager.
 */
BOP_API void bop_raft_server_restart_election_timer(const bop_raft_server_ptr *server) {
    server->raft_server->restart_election_timer();
}

/**
 * Set custom context to Raft cluster config.
 * It will create a new configuration log and replicate it.
 *
 * @param ctx Custom context.
 */
BOP_API void bop_raft_server_set_user_ctx(
    const bop_raft_server_ptr *server,
    const char *data,
    size_t size
) {
    const std::string_view view(data, size);
    const std::string ctx(view);
    server->raft_server->set_user_ctx(ctx);
}

/**
 * Get custom context from the current cluster config.
 *
 * @return Custom context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_user_ctx(const bop_raft_server_ptr *server) {
    auto ctx = server->raft_server->get_user_ctx();
    auto buf = bop_raft_buffer_alloc(ctx.size());
    reinterpret_cast<nuraft::buffer *>(buf)->put(ctx);
    reinterpret_cast<nuraft::buffer *>(buf)->pos(0);
    return buf;
}

/**
* Get timeout for snapshot_sync_ctx
*
* @return snapshot_sync_ctx_timeout.
*/
BOP_API int32_t bop_raft_server_get_snapshot_sync_ctx_timeout(const bop_raft_server_ptr *server) {
    return server->raft_server->get_snapshot_sync_ctx_timeout();
}

/**
 * Get ID of this server.
 *
 * @return Server ID.
 */
BOP_API int32_t bop_raft_server_get_id(const bop_raft_server_ptr *server) {
    return server->raft_server->get_snapshot_sync_ctx_timeout();
}

/**
 * Get the current term of this server.
 *
 * @return Term.
 */
BOP_API uint64_t bop_raft_server_get_term(const bop_raft_server_ptr *server) {
    return server->raft_server->get_term();
}

/**
 * Get the term of given log index number.
 *
 * @param log_idx Log index number
 * @return Term of given log.
 */
BOP_API uint64_t bop_raft_server_get_log_term(const bop_raft_server_ptr *server, uint64_t log_idx) {
    return server->raft_server->get_log_term(log_idx);
}

/**
 * Get the term of the last log.
 *
 * @return Term of the last log.
 */
BOP_API uint64_t bop_raft_server_get_last_log_term(const bop_raft_server_ptr *server) {
    return server->raft_server->get_last_log_term();
}

/**
 * Get the last log index number.
 *
 * @return Last log index number.
 */
BOP_API uint64_t bop_raft_server_get_last_log_idx(const bop_raft_server_ptr *server) {
    return server->raft_server->get_last_log_idx();
}

/**
 * Get the last committed log index number of state machine.
 *
 * @return Last committed log index number of state machine.
 */
BOP_API uint64_t bop_raft_server_get_committed_log_idx(const bop_raft_server_ptr *server) {
    return server->raft_server->get_committed_log_idx();
}

/**
 * Get the target log index number we are required to commit.
 *
 * @return Target committed log index number.
 */
BOP_API uint64_t bop_raft_server_get_target_committed_log_idx(const bop_raft_server_ptr *server) {
    return server->raft_server->get_target_committed_log_idx();
}

/**
 * Get the leader's last committed log index number.
 *
 * @return The leader's last committed log index number.
 */
BOP_API uint64_t bop_raft_server_get_leader_committed_log_idx(const bop_raft_server_ptr *server) {
    return server->raft_server->get_leader_committed_log_idx();
}

/**
 * Get the log index of the first config when this server became a leader.
 * This API can be used for checking if the state machine is fully caught up
 * with the latest log after a leader election, so that the new leader can
 * guarantee strong consistency.
 *
 * It will return 0 if this server is not a leader.
 *
 * @return The log index of the first config when this server became a leader.
 */
BOP_API uint64_t bop_raft_server_get_log_idx_at_becoming_leader(const bop_raft_server_ptr *server) {
    return server->raft_server->get_log_idx_at_becoming_leader();
}

/**
 * Calculate the log index to be committed
 * from current peers' matched indexes.
 *
 * @return Expected committed log index.
 */
BOP_API uint64_t bop_raft_server_get_expected_committed_log_idx(const bop_raft_server_ptr *server) {
    return server->raft_server->get_expected_committed_log_idx();
}

/**
 * Get the current Raft cluster config.
 *
 * @param server raft server instance
 * @param cluster_config Wrapper for holding nuraft::ptr<nuraft::cluster_config>
 */
BOP_API void bop_raft_server_get_config(const bop_raft_server_ptr *server,
                                        bop_raft_cluster_config_ptr *cluster_config) {
    if (!server) return;
    if (!cluster_config) return;
    cluster_config->config = server->raft_server->get_config();
}

/**
 * Get data center ID of the given server.
 *
 * @param srv_id Server ID.
 * @return -1 if given server ID does not exist.
 *          0 if data center ID was not assigned.
 */
BOP_API int32_t bop_raft_server_get_dc_id(const bop_raft_server_ptr *server, int32_t srv_id) {
    return server->raft_server->get_dc_id(srv_id);
}

/**
 * Get auxiliary context stored in the server config.
 *
 * @param srv_id Server ID.
 * @return Auxiliary context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_aux(const bop_raft_server_ptr *server, int32_t srv_id) {
    auto aux = server->raft_server->get_aux(srv_id);
    auto buf = bop_raft_buffer_alloc(aux.size());
    reinterpret_cast<nuraft::buffer *>(buf)->put_raw(reinterpret_cast<uint8_t *>(aux.data()), aux.size());
    return buf;
}

/**
 * Get the ID of current leader.
 *
 * @return Leader ID
 *         -1 if there is no live leader.
 */
BOP_API int32_t bop_raft_server_get_leader(const bop_raft_server_ptr *server) {
    return server->raft_server->get_leader();
}

/**
 * Check if this server is leader.
 *
 * @return `true` if it is leader.
 */
BOP_API bool bop_raft_server_is_leader(const bop_raft_server_ptr *server) {
    return server->raft_server->is_leader();
}

/**
 * Check if there is live leader in the current cluster.
 *
 * @return `true` if live leader exists.
 */
BOP_API bool bop_raft_server_is_leader_alive(const bop_raft_server_ptr *server) {
    return server->raft_server->is_leader_alive();
}

/**
 * Get the configuration of given server.
 *
 * @param srv_id Server ID.
 * @return Server configuration.
 */
BOP_API void bop_raft_server_get_srv_config(
    const bop_raft_server_ptr *server,
    bop_raft_srv_config_ptr *svr_config,
    int32_t srv_id
) {
    if (!server) return;
    if (!svr_config) return;
    svr_config->config = server->raft_server->get_srv_config(srv_id);
}

/**
 * Get the configuration of all servers.
 *
 * @param[out] configs_out Set of server configurations.
 */
BOP_API void bop_raft_server_get_srv_config_all(
    const bop_raft_server_ptr *server,
    bop_raft_srv_config_vec *configs_out
) {
    if (!server) return;
    if (!configs_out) return;
    server->raft_server->get_srv_config_all(configs_out->configs);
}

/**
 * Update the server configuration, only leader will accept this operation.
 * This function will update the current cluster config
 * and replicate it to all peers.
 *
 * We don't allow changing multiple server configurations at once,
 * due to safety reason.
 *
 * Change on endpoint will not be accepted (should be removed and then re-added).
 * If the server is in new joiner state, it will be rejected.
 * If the server ID does not exist, it will also be rejected.
 *
 * @param new_config Server configuration to update.
 * @return `true` on success, `false` if rejected.
 */
BOP_API void bop_raft_server_update_srv_config(
    const bop_raft_server_ptr *server,
    bop_raft_srv_config_ptr *new_config
) {
    if (!server) return;
    if (!new_config) return;
    server->raft_server->update_srv_config(*new_config->config);
}

struct bop_raft_server_peer_info {
    bop_raft_server_peer_info()
        : id(-1), last_log_idx(0), last_succ_resp_us(0) {
    }

    /**
     * Peer ID.
     */
    int32_t id;

    /**
     * The last log index that the peer has, from this server's point of view.
     */
    uint64_t last_log_idx;

    /**
     * The elapsed time since the last successful response from this peer,
     * in microsecond.
     */
    uint64_t last_succ_resp_us;
};

static_assert(sizeof(bop_raft_server_peer_info) == sizeof(nuraft::raft_server::peer_info));
static_assert(offsetof(bop_raft_server_peer_info, id) == offsetof(nuraft::raft_server::peer_info, id_));
static_assert(
    offsetof(bop_raft_server_peer_info, last_log_idx) == offsetof(nuraft::raft_server::peer_info, last_log_idx_));
static_assert(
    offsetof(
        bop_raft_server_peer_info,
        last_succ_resp_us
    ) == offsetof(
        nuraft::raft_server::peer_info,
        last_succ_resp_us_
    ));

struct bop_raft_server_peer_info_vec {
    std::vector<nuraft::raft_server::peer_info> peers;

    bop_raft_server_peer_info_vec() = default;
};

BOP_API bop_raft_server_peer_info_vec *bop_raft_server_peer_info_vec_create() {
    bop_raft_server_peer_info_vec *vec = new bop_raft_server_peer_info_vec();
    vec->peers.reserve(16);
    return vec;
}

BOP_API void bop_raft_server_peer_info_vec_delete(const bop_raft_server_peer_info_vec *vec) {
    if (vec) {
        delete vec;
    }
}

BOP_API size_t bop_raft_server_peer_info_vec_size(bop_raft_server_peer_info_vec *vec) {
    return vec->peers.size();
}

BOP_API bop_raft_server_peer_info *bop_raft_server_peer_info_vec_get(bop_raft_server_peer_info_vec *vec, size_t idx) {
    if (idx >= vec->peers.size()) {
        return nullptr;
    }
    return reinterpret_cast<bop_raft_server_peer_info *>(&vec->peers[idx]);
}

/**
 * Get the peer info of the given ID. Only leader will return peer info.
 *
 * @param srv_id Server ID.
 * @return Peer info.
 */
BOP_API bool bop_raft_server_get_peer_info(
    const bop_raft_server_ptr *server,
    int32_t srv_id,
    bop_raft_server_peer_info *peer
) {
    if (!server) return false;
    if (!peer) return false;
    auto info = server->raft_server->get_peer_info(srv_id);
    peer->id = info.id_;
    peer->last_log_idx = info.last_log_idx_;
    peer->last_succ_resp_us = info.last_succ_resp_us_;
    return info.id_ != -1;
}

/**
 * Get the info of all peers. Only leader will return peer info.
 *
 * @return Vector of peer info.
 */
BOP_API void bop_raft_server_get_peer_info_all(
    const bop_raft_server_ptr *server,
    bop_raft_server_peer_info_vec *peers_out
) {
    if (!server) return;
    if (!peers_out) return;
    peers_out->peers = server->raft_server->get_peer_info_all();
}

/**
 * Shut down server instance.
 */
BOP_API void bop_raft_server_shutdown(
    const bop_raft_server_ptr *server
) {
    server->raft_server->shutdown();
}

/**
 *  Start internal background threads, initialize election
 */
BOP_API void bop_raft_server_start_server(
    const bop_raft_server_ptr *server,
    bool skip_initial_election_timeout
) {
    server->raft_server->start_server(skip_initial_election_timeout);
}

/**
 * Stop background commit thread.
 */
BOP_API void bop_raft_server_stop_server(
    const bop_raft_server_ptr *server
) {
    server->raft_server->stop_server();
}

/**
 * Send reconnect request to leader.
 * Leader will re-establish the connection to this server in a few seconds.
 * Only follower will accept this operation.
 */
BOP_API void bop_raft_server_send_reconnect_request(
    const bop_raft_server_ptr *server
) {
    server->raft_server->send_reconnect_request();
}

/**
 * Update Raft parameters.
 *
 * @param new_params Parameters to set.
 */
BOP_API void bop_raft_server_update_params(
    const bop_raft_server_ptr *server,
    nuraft::raft_params *params
) {
    if (!server) return;
    if (!params) return;
    server->raft_server->update_params(*params);
}

/**
 * Get the current Raft parameters.
 * Returned instance is the clone of the original one,
 * so that user can modify its contents.
 *
 * @return Clone of Raft parameters.
 */
BOP_API void bop_raft_server_get_current_params(
    const bop_raft_server_ptr *server,
    nuraft::raft_params *params
) {
    if (!server) return;
    if (!params) return;
    *params = server->raft_server->get_current_params();
}

struct bop_raft_counter_t {
    std::string name;
    uint64_t value;

    bop_raft_counter_t(const std::string &name, uint64_t value)
        : name(name),
          value(value) {
    }
};

BOP_API bop_raft_counter_t *bop_raft_counter_create(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_counter_t(std::string(view), 0);
}

BOP_API void bop_raft_counter_delete(const bop_raft_counter_t *counter) {
    if (counter) {
        delete counter;
    }
}

BOP_API const char *bop_raft_counter_name(const bop_raft_counter_t *counter) {
    return counter->name.c_str();
}

BOP_API uint64_t bop_raft_counter_value(const bop_raft_counter_t *counter) {
    return counter->value;
}


struct bop_raft_gauge_t {
    std::string name;
    int64_t value;

    bop_raft_gauge_t(const std::string &name, int64_t value)
        : name(name),
          value(value) {
    }
};

BOP_API bop_raft_gauge_t *bop_raft_gauge_create(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_gauge_t(std::string(view), 0);
}

BOP_API void bop_raft_gauge_delete(const bop_raft_gauge_t *gauge) {
    if (gauge) {
        delete gauge;
    }
}

BOP_API const char *bop_raft_gauge_name(const bop_raft_gauge_t *gauge) {
    return gauge->name.c_str();
}

BOP_API int64_t bop_raft_gauge_value(const bop_raft_gauge_t *gauge) {
    return gauge->value;
}


struct bop_raft_histogram_t {
    std::string name;
    std::map<double, uint64_t> value{};

    bop_raft_histogram_t(const std::string &name)
        : name(name) {
    }
};

BOP_API bop_raft_histogram_t *bop_raft_histogram_create(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_histogram_t(std::string(view));
}

BOP_API void bop_raft_histogram_delete(const bop_raft_histogram_t *histogram) {
    if (histogram) {
        delete histogram;
    }
}

BOP_API const char *bop_raft_histogram_name(const bop_raft_histogram_t *histogram) {
    return histogram->name.c_str();
}

BOP_API size_t bop_raft_histogram_size(const bop_raft_histogram_t *histogram) {
    return histogram->value.size();
}

BOP_API uint64_t bop_raft_histogram_get(const bop_raft_histogram_t *histogram, double key) {
    if (auto it = histogram->value.find(key); it != histogram->value.end()) {
        return it->second;
    } else {
        return 0;
    }
}


/**
 * Get the counter number of given stat name.
 *
 * @param name Stat name to retrieve.
 * @return Counter value.
 */
BOP_API uint64_t bop_raft_server_get_stat_counter(
    const bop_raft_server_ptr *server,
    bop_raft_counter_t *counter
) {
    if (!server) return 0;
    if (!counter) return 0;
    return server->raft_server->get_stat_counter(counter->name);
}

/**
 * Get the gauge number of given stat name.
 *
 * @param name Stat name to retrieve.
 * @return Gauge value.
 */
BOP_API int64_t bop_raft_server_get_stat_gauge(
    const bop_raft_server_ptr *server,
    bop_raft_gauge_t *gauge
) {
    if (!server) return 0;
    if (!gauge) return 0;
    return server->raft_server->get_stat_gauge(gauge->name);
}

/**
 * Get the histogram of given stat name.
 *
 * @param name Stat name to retrieve.
 * @param[out] histogram_out
 *     Histogram as a map. Key is the upper bound of a bucket, and
 *     value is the counter of that bucket.
 * @return `true` on success.
 *         `false` if stat does not exist, or is not histogram type.
 */
BOP_API bool bop_raft_server_get_stat_histogram(
    const bop_raft_server_ptr *server,
    bop_raft_histogram_t *histogram
) {
    if (!server) return false;
    if (!histogram) return false;
    return server->raft_server->get_stat_histogram(histogram->name, histogram->value);
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_counter(
    const bop_raft_server_ptr *server,
    bop_raft_counter_t *counter
) {
    if (!server) return;
    if (!counter) return;
    server->raft_server->reset_stat(counter->name);
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_gauge(
    const bop_raft_server_ptr *server,
    bop_raft_gauge_t *gauge
) {
    if (!server) return;
    if (!gauge) return;
    server->raft_server->reset_stat(gauge->name);
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_histogram(
    const bop_raft_server_ptr *server,
    bop_raft_histogram_t *histogram
) {
    if (!server) return;
    if (!histogram) return;
    server->raft_server->reset_stat(histogram->name);
}

/**
 * Reset all existing stats to zero.
 */
BOP_API void bop_raft_server_reset_all_stats(
    const bop_raft_server_ptr *server
) {
    if (!server) return;
    server->raft_server->reset_all_stats();
}

typedef uint64_t (*bop_raft_inc_term_func)(void *user_data, uint64_t current_term);

/**
 * Set a custom callback function for increasing term.
 */
BOP_API void bop_raft_server_set_inc_term_func(
    const bop_raft_server_ptr *server,
    void *user_data,
    bop_raft_inc_term_func func
) {
    if (!server) return;
    if (!func) return;
    server->raft_server->set_inc_term_func([user_data, func](uint64_t current_term) {
        return func(user_data, current_term);
    });
}

/**
 * Pause the background execution of the state machine.
 * If an operation execution is currently happening, the state
 * machine may not be paused immediately.
 *
 * @param timeout_ms If non-zero, this function will be blocked until
 *                   either it completely pauses the state machine execution
 *                   or reaches the given time limit in milliseconds.
 *                   Otherwise, this function will return immediately, and
 *                   there is a possibility that the state machine execution
 *                   is still happening.
 */
BOP_API void bop_raft_server_pause_state_machine_execution(
    const bop_raft_server_ptr *server,
    size_t timeout_ms
) {
    if (!server) return;
    server->raft_server->pause_state_machine_execution(timeout_ms);
}

/**
 * Resume the background execution of state machine.
 */
BOP_API void bop_raft_server_resume_state_machine_execution(
    const bop_raft_server_ptr *server
) {
    if (!server) return;
    server->raft_server->resume_state_machine_execution();
}

/**
 * Check if the state machine execution is paused.
 *
 * @return `true` if paused.
 */
BOP_API bool bop_raft_server_is_state_machine_execution_paused(
    const bop_raft_server_ptr *server
) {
    if (!server) return false;
    return server->raft_server->is_state_machine_execution_paused();
}

/**
 * Block the current thread and wake it up when the state machine
 * execution is paused.
 *
 * @param timeout_ms If non-zero, wake up after the given amount of time
 *                   even though the state machine is not paused yet.
 * @return `true` if the state machine is paused.
 */
BOP_API bool bop_raft_server_wait_for_state_machine_pause(
    const bop_raft_server_ptr *server,
    size_t timeout_ms
) {
    if (!server) return false;
    return server->raft_server->wait_for_state_machine_pause(timeout_ms);
}

/**
 * (Experimental)
 * This API is used when `raft_params::parallel_log_appending_` is set.
 * Everytime an asynchronous log appending job is done, users should call
 * this API to notify Raft server to handle the log.
 * Note that calling this API once for multiple logs is acceptable
 * and recommended.
 *
 * @param ok `true` if appending succeeded.
 */
BOP_API void bop_raft_server_notify_log_append_completion(
    const bop_raft_server_ptr *server,
    bool ok
) {
    if (!server) return;
    server->raft_server->notify_log_append_completion(ok);
}

/**
 * Manually create a snapshot based on the latest committed
 * log index of the state machine.
 *
 * Note that snapshot creation will fail immediately if the previous
 * snapshot task is still running.
 *
 * @params options Options for snapshot creation.
 * @return Log index number of the created snapshot or`0` if failed.
 */
BOP_API uint64_t bop_raft_server_create_snapshot(
    const bop_raft_server_ptr *server,
    /**
         * If `true`, the background commit will be blocked until `create_snapshot`
         * returns. However, it will not block the commit for the entire duration
         * of the snapshot creation process, as long as your state machine creates
         * the snapshot asynchronously.
         *
         * The purpose of this flag is to ensure that the log index used for
         * the snapshot creation is the most recent one.
         */
    bool serialize_commit
) {
    if (!server) 0;
    nuraft::raft_server::create_snapshot_options options;
    options.serialize_commit_ = serialize_commit;
    return server->raft_server->create_snapshot(options);
}

BOP_API void bop_raft_cmd_result_uint64_set_when_ready(
    bop_raft_cmd_result_uint64_t *self,
    void *user_data,
    bop_raft_cmd_result_uint64_when_ready_func when_ready
) {
    self->user_data = user_data;
    self->when_ready = when_ready;
}

/**
 * Manually and asynchronously create a snapshot on the next earliest
 * available commited log index.
 *
 * Unlike `create_snapshot`, if the previous snapshot task is running,
 * it will wait until the previous task is done. Once the snapshot
 * creation is finished, it will be notified via the returned
 * `cmd_result` with the log index number of the snapshot.
 *
 * @return `cmd_result` instance.
 *         `nullptr` if there is already a scheduled snapshot creation.
 */
BOP_API void bop_raft_server_schedule_snapshot_creation(
    const bop_raft_server_ptr *server,
    bop_raft_cmd_result_uint64_t *result_handler
) {
    if (!server) return;
    result_handler->set(server->raft_server->schedule_snapshot_creation());
}

/**
 * Get the log index number of the last snapshot.
 *
 * @return Log index number of the last snapshot.
 *         `0` if snapshot does not exist.
 */
BOP_API uint64_t bop_raft_server_get_last_snapshot_idx(
    const bop_raft_server_ptr *server
) {
    if (!server) return 0;
    return server->raft_server->get_last_snapshot_idx();
}
}

