
#ifdef BOOST_ASIO_USE_WOLFSSL
extern "C" {
#include <wolfssl/options.h>
#include <wolfssl/wolfssl/ssl.h>
#include <wolfssl/openssl/ssl.h>
#include <wolfssl/openssl/bio.h>
#include <wolfssl/openssl/err.h>
#include <wolfssl/openssl/dh.h>
}
#endif

#include <cstring>
#include <iostream>
#include <memory>

#include "./lib.h"

#include "libnuraft/nuraft.hxx"
#include "libnuraft/asio_service_options.hxx"

extern "C" {
//    #define LIBMDBX_API BOP_API
//    #define LIBMDBX_EXPORTS BOP_API
//    #define SQLITE_API BOP_API

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

    explicit bop_raft_buffer_ptr(const nuraft::ptr<nuraft::buffer> &buf) : buf(buf) {
    }
};

BOP_API bop_raft_buffer *bop_raft_buffer_new(const size_t size) {
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

struct bop_raft_async_uint64_ptr {
    void *user_data{nullptr};
    bop_raft_async_uint64_when_ready when_ready{nullptr};
    nuraft::ptr<nuraft::cmd_result<uint64_t> > cmd_result{nullptr};

    nuraft::cmd_result<uint64_t>::handler_type on_ready =
            [this](uint64_t result, nuraft::ptr<std::exception> err) {
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

BOP_API bop_raft_async_uint64_ptr *bop_raft_async_u64_make(
    void *user_data, bop_raft_async_uint64_when_ready when_ready
) {
    auto result = new bop_raft_async_uint64_ptr;
    result->user_data = user_data;
    result->when_ready = when_ready;
    return result;
}

BOP_API void bop_raft_async_u64_delete(const bop_raft_async_uint64_ptr *self) {
    if (self) {
        delete self;
    }
}

BOP_API void *bop_raft_async_u64_get_user_data(const bop_raft_async_uint64_ptr *self) {
    if (!self) return nullptr;
    return self->user_data;
}

BOP_API void bop_raft_async_u64_set_user_data(bop_raft_async_uint64_ptr *self, void *user_data) {
    if (!self) return;
    self->user_data = user_data;
}

BOP_API bop_raft_async_uint64_when_ready
bop_raft_async_u64_get_when_ready(const bop_raft_async_uint64_ptr *self) {
    if (!self) return nullptr;
    return self->when_ready;
}

BOP_API void bop_raft_async_u64_set_when_ready(
    bop_raft_async_uint64_ptr *self, void *user_data,
    bop_raft_async_uint64_when_ready when_ready
) {
    if (!self) return;
    self->user_data = user_data;
    self->when_ready = when_ready;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<nuraft::ptr<nuraft::buffer>>
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_async_buffer_ptr {
    void *user_data{nullptr};
    bop_raft_async_buffer_when_ready when_ready{nullptr};
    nuraft::ptr<nuraft::cmd_result<nuraft::ptr<nuraft::buffer> > > cmd_result{nullptr};

    nuraft::cmd_result<nuraft::ptr<nuraft::buffer> >::handler_type on_ready =
            [this](nuraft::ptr<nuraft::buffer> result, nuraft::ptr<std::exception> err) {
        if (err) {
            when_ready(
                user_data, reinterpret_cast<bop_raft_buffer *>(result.get()), err->what()
            );
        } else {
            when_ready(user_data, reinterpret_cast<bop_raft_buffer *>(result.get()), nullptr);
        }
    };

    void set(nuraft::ptr<nuraft::cmd_result<nuraft::ptr<nuraft::buffer> > > cmd) {
        cmd->when_ready(on_ready);
        cmd_result = std::move(cmd);
    }
};

BOP_API bop_raft_async_buffer_ptr *bop_raft_async_buffer_make(
    void *user_data, bop_raft_async_buffer_when_ready when_ready
) {
    auto result = new bop_raft_async_buffer_ptr;
    result->user_data = user_data;
    result->when_ready = when_ready;
    return result;
}

BOP_API void bop_raft_async_buffer_delete(const bop_raft_async_buffer_ptr *self) {
    if (self) {
        delete self;
    }
}

BOP_API void *bop_raft_async_buffer_get_user_data(bop_raft_async_buffer_ptr *self) {
    if (!self) return nullptr;
    return self->user_data;
}

BOP_API void bop_raft_async_buffer_set_user_data(bop_raft_async_buffer_ptr *self, void *user_data) {
    if (!self) return;
    self->user_data = user_data;
}

BOP_API bop_raft_async_buffer_when_ready
bop_raft_async_buffer_get_when_ready(bop_raft_async_buffer_ptr *self) {
    if (!self) return nullptr;
    return self->when_ready;
}

BOP_API void bop_raft_async_buffer_set_when_ready(
    bop_raft_async_buffer_ptr *self, void *user_data,
    bop_raft_async_buffer_when_ready when_ready
) {
    if (!self) return;
    self->user_data = user_data;
    self->when_ready = when_ready;
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
    return reinterpret_cast<bop_raft_snapshot *>(
        new nuraft::snapshot(last_log_idx, last_log_term, last_config, size, snp_type)
    );
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

    bop_raft_cluster_config_ptr(const nuraft::ptr<nuraft::cluster_config> &config)
        : config(config) {
    }
};

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_new() {
    return reinterpret_cast<bop_raft_cluster_config *>(new nuraft::cluster_config());
}

BOP_API bop_raft_cluster_config_ptr *
bop_raft_cluster_config_ptr_make(bop_raft_cluster_config *config) {
    return new bop_raft_cluster_config_ptr(
        nuraft::ptr<nuraft::cluster_config>(reinterpret_cast<nuraft::cluster_config *>(config))
    );
}

BOP_API void bop_raft_cluster_config_free(const bop_raft_cluster_config *config) {
    if (config)
        delete reinterpret_cast<const nuraft::cluster_config *>(config);
}

BOP_API void bop_raft_cluster_config_ptr_delete(const bop_raft_cluster_config_ptr *config) {
    if (config)
        delete config;
}

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_ptr_get(bop_raft_cluster_config_ptr *conf) {
    if (!conf) return nullptr;
    return reinterpret_cast<bop_raft_cluster_config *>(conf->config.get());
}

BOP_API bop_raft_buffer *bop_raft_cluster_config_serialize(bop_raft_cluster_config *conf) {
    size_t sz = 2 * 8 + 4 + 1;
    std::vector<nuraft::ptr<nuraft::buffer> > srv_buffs;
    for (auto it = reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().cbegin();
         it != reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().cend();
         ++it) {
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
    result->put(
        static_cast<uint8_t>(
            reinterpret_cast<nuraft::cluster_config *>(conf)->is_async_replication() ? 1 : 0
        )
    );
    result->put(
        reinterpret_cast<uint8_t *>(
            reinterpret_cast<nuraft::cluster_config *>(conf)->get_user_ctx().data()
        ),
        reinterpret_cast<nuraft::cluster_config *>(conf)->get_user_ctx().size()
    );
    result->put(
        static_cast<int32_t>(reinterpret_cast<nuraft::cluster_config *>(conf)->get_servers().size())
    );
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
BOP_API void bop_raft_cluster_config_user_ctx(
    bop_raft_cluster_config *cfg, char *out_data, size_t out_data_size
) {
    if (!out_data)
        return;
    auto data = reinterpret_cast<nuraft::cluster_config *>(cfg)->get_user_ctx();
    if (out_data_size > data.size()) {
        out_data_size = data.size();
    }
    std::memcpy((void *) out_data, (const void *) data.data(), out_data_size);
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

BOP_API bop_raft_srv_config_vec *bop_raft_srv_config_vec_make() {
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

BOP_API bop_raft_srv_config *bop_raft_srv_config_vec_get(bop_raft_srv_config_vec *vec, size_t idx) {
    return reinterpret_cast<bop_raft_srv_config *>(vec->configs[idx].get());
}

struct bop_raft_srv_config_ptr {
    nuraft::ptr<nuraft::srv_config> config;

    explicit bop_raft_srv_config_ptr(const nuraft::ptr<nuraft::srv_config> &config)
        : config(config) {
    }
};

BOP_API bop_raft_srv_config_ptr *bop_raft_srv_config_ptr_make(bop_raft_srv_config *config) {
    return new bop_raft_srv_config_ptr(
        nuraft::ptr<nuraft::srv_config>(reinterpret_cast<nuraft::srv_config *>(config))
    );
}

BOP_API void bop_raft_srv_config_ptr_delete(const bop_raft_srv_config_ptr *config) {
    if (config)
        delete config;
}

BOP_API bop_raft_srv_config *bop_raft_srv_config_make(
    int32_t id,
    int32_t dc_id,
    const char *endpoint,
    size_t endpoint_size,
    const char *aux,
    size_t aux_size,
    bool learner,
    int32_t priority
) {
    return reinterpret_cast<bop_raft_srv_config *>(
        new nuraft::srv_config(
            id,
            dc_id,
            std::string(endpoint, endpoint_size),
            std::string(aux, aux_size),
            learner,
            priority
        )
    );
}

BOP_API void bop_raft_srv_config_delete(const bop_raft_srv_config *config) {
    if (config)
        delete reinterpret_cast<const nuraft::srv_config *>(config);
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
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_endpoint().c_str();
}

// Size of Endpoint (address + port).
BOP_API size_t bop_raft_srv_config_endpoint_size(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_endpoint().size();
}

/**
 * Custom string given by user.
 * WARNING: It SHOULD NOT contain NULL character, as it will be stored as a C-style string.
 */
BOP_API const char *bop_raft_srv_config_aux(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_aux().c_str();
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

BOP_API void bop_raft_srv_config_set_is_learner(bop_raft_srv_config *cfg, bool learner) {
    reinterpret_cast<nuraft::srv_config *>(cfg)->set_learner(learner);
}

/**
 * `true` if this node is a new joiner, but not yet fully synced.
 * New joiner will not
 * initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_new_joiner(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->is_new_joiner();
}

BOP_API void bop_raft_srv_config_set_new_joiner(bop_raft_srv_config *cfg, bool new_joiner) {
    reinterpret_cast<nuraft::srv_config *>(cfg)->set_new_joiner(new_joiner);
}

/**
 * Priority of this node.
 * 0 will never be a leader.
 */
BOP_API int32_t bop_raft_srv_config_priority(bop_raft_srv_config *cfg) {
    return reinterpret_cast<nuraft::srv_config *>(cfg)->get_priority();
}

BOP_API void bop_raft_srv_config_set_priority(bop_raft_srv_config *cfg, int32_t priority) {
    reinterpret_cast<nuraft::srv_config *>(cfg)->set_priority(priority);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::svr_state
///////////////////////////////////////////////////////////////////////////////////////////////////

BOP_API bop_raft_buffer *bop_raft_srv_state_serialize(bop_raft_srv_state *state) {
    if (!state)
        return nullptr;
    uint8_t version = 2;
    //   << Format >>
    // version              1 byte
    // term                 8 bytes
    // voted_for            4 bytes
    // election timer       1 byte      (since v1)
    // catching up          1 byte      (since v2)
    // receiving snapshot   1 byte      (since v2)

    size_t buf_len = sizeof(uint8_t) + sizeof(uint64_t) + sizeof(int32_t) + sizeof(uint8_t);
    if (version >= 2) {
        buf_len += sizeof(uint8_t);
        buf_len += sizeof(uint8_t);
    }
    bop_raft_buffer *buf = bop_raft_buffer_new(buf_len);
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

BOP_API bop_raft_srv_state *bop_raft_srv_state_deserialize(bop_raft_buffer *buf) {
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

    return reinterpret_cast<bop_raft_srv_state *>(
        new nuraft::srv_state(term, voted_for, et_allowed, catching_up, receiving_snapshot)
    );
}

BOP_API void bop_raft_srv_state_delete(const bop_raft_srv_state *state) {
    if (state)
        delete reinterpret_cast<const nuraft::srv_state *>(state);
}

/**
 * Term
 */
BOP_API uint64_t bop_raft_srv_state_term(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->get_term();
}

/**
 * Server ID that this server voted for.
 * `-1` if not voted.
 */
BOP_API int32_t bop_raft_srv_state_voted_for(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->get_voted_for();
}

/**
 * `true` if election timer is allowed.
 */
BOP_API bool bop_raft_srv_state_is_election_timer_allowed(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->is_election_timer_allowed();
}

/**
 * true if this server has joined the cluster but has not yet
 * fully caught up with the latest log. While in the catch-up status,
 * this server will not receive normal append_entries requests.
 */
BOP_API bool bop_raft_srv_state_is_catching_up(const bop_raft_srv_state *state) {
    return reinterpret_cast<const nuraft::srv_state *>(state)->is_catching_up();
}

/**
 * `true` if this server is receiving a snapshot.
 * Same as `catching_up_`, it must be a durable flag so as not to be
 * reset after restart. While this flag is set, this server will neither
 * receive normal append_entries requests nor initiate election.
 */
BOP_API bool bop_raft_srv_state_is_receiving_snapshot(const bop_raft_srv_state *state) {
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

    bop_raft_logger(void *user_data, bop_raft_logger_put_details_func put_details)
        : user_data(user_data),
          put_details_(put_details) {
        logger::set_level(6);
    }

    /**
     * Put a log with level, line number, function name,
     * and file name.
     *
     *
     * Log level info:
     *    Trace:    6
     *    Debug:    5
     *    Info:     4
     *    Warning:  3
     *    Error:    2
     *    Fatal:    1
     *
     * @param level Level of
     * given log.
     * @param source_file Name of file where the log is located.
     * @param
     * func_name Name of function where the log is located.
     * @param line_number Line number of
     * the log.
     * @param log_line Contents of the log.
     */
    void put_details(
        int level, const char *source_file, const char *func_name, size_t line_number,
        const std::string &log_line
    ) override {
        put_details_(
            user_data, level, source_file, func_name, line_number, log_line.data(), log_line.size()
        );
    }
};

BOP_API bop_raft_logger_ptr *
bop_raft_logger_make(void *user_data, bop_raft_logger_put_details_func callback) {
    return new bop_raft_logger_ptr(std::make_shared<bop_raft_logger>(user_data, callback));
}

BOP_API void bop_raft_logger_delete(const bop_raft_logger_ptr *logger) {
    if (logger)
        delete logger;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::asio_service
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_asio_service_options;

struct bop_raft_asio_service_ptr {
    nuraft::ptr<nuraft::asio_service> service;
    nuraft::asio_service_options options;
    bool stopped{false};

    bop_raft_asio_service_ptr(
        const nuraft::ptr<nuraft::asio_service> &asio_service,
        const nuraft::asio_service_options &options
    )
        : service(asio_service),
          options(options) {
    }
};

BOP_API bop_raft_asio_service_ptr *bop_raft_asio_service_make(
    bop_raft_asio_options *options,
    bop_raft_logger_ptr *logger
) {
    if (!options) return nullptr;
    nuraft::asio_service_options opts{};
    opts.thread_pool_size_ = options->thread_pool_size;
    if (options->worker_start != nullptr) {
        opts.worker_start_ = [worker_start = options->worker_start, worker_start_user_data = options->
                    worker_start_user_data](uint32_t value) {
                    worker_start(worker_start_user_data, value);
                };
    }
    if (options->worker_stop != nullptr) {
        opts.worker_stop_ = [worker_stop = options->worker_stop, worker_stop_user_data = options->worker_stop_user_data
                ](uint32_t value) {
                    worker_stop(worker_stop_user_data, value);
                };
    }
    opts.enable_ssl_ = options->enable_ssl;
    opts.skip_verification_ = options->skip_verification;
    opts.server_cert_file_ = options->server_cert_file ? std::string(options->server_cert_file) : "";
    opts.server_key_file_ = options->server_key_file ? std::string(options->server_key_file) : "";
    opts.root_cert_file_ = options->root_cert_file ? std::string(options->root_cert_file) : "";
    opts.invoke_req_cb_on_empty_meta_ = options->invoke_req_cb_on_empty_meta;
    opts.invoke_resp_cb_on_empty_meta_ = options->invoke_resp_cb_on_empty_meta;
    if (options->verify_sn != nullptr) {
        opts.verify_sn_ = [verify_sn = options->verify_sn, verify_sn_user_data = options->verify_sn_user_data
                ](const std::string &value) {
                    return verify_sn(verify_sn_user_data, value.data(), value.size());
                };
    }
    if (options->ssl_context_provider_server != nullptr) {
        opts.ssl_context_provider_server_ = [ssl_context_provider_server = options->ssl_context_provider_server,
                    ssl_context_provider_server_user_data = options->ssl_context_provider_server_user_data]() {
                    return ssl_context_provider_server(ssl_context_provider_server_user_data);
                };
    }
    if (options->ssl_context_provider_client != nullptr) {
        opts.ssl_context_provider_client_ = [ssl_context_provider_client = options->ssl_context_provider_client,
                    ssl_context_provider_client_user_data = options->ssl_context_provider_client_user_data]() {
                    return ssl_context_provider_client(ssl_context_provider_client_user_data);
                };
    }
    if (options->custom_resolver != nullptr) {
        opts.custom_resolver_ = [custom_resolver = options->custom_resolver, custom_resolver_user_data = options->
                    custom_resolver_user_data](
            const std::string &v1,
            const std::string &v2,
            nuraft::asio_service_custom_resolver_response response
        ) {
                    return custom_resolver(
                        custom_resolver_user_data,
                        &response,
                        v1.data(),
                        v1.size(),
                        v2.data(),
                        v2.size(),
                        [](void *response_impl,
                           const char *v1_,
                           size_t v1_size_,
                           const char *v2_,
                           size_t v2_size_,
                           int error_code) {
                            const auto v1_str = std::string(v1_, v1_size_);
                            const auto v2_str = std::string(v2_, v2_size_);
                            std::error_code ec;
                            if (error_code != 0) {
                                ec.assign(error_code, std::generic_category());
                            }
                            reinterpret_cast<nuraft::asio_service_custom_resolver_response &>(response_impl)(
                                v1_str, v2_str, ec
                            );
                        }
                    );
                };
    }
    opts.replicate_log_timestamp_ = options->replicate_log_timestamp;
    opts.crc_on_entire_message_ = options->crc_on_entire_message;
    opts.crc_on_payload_ = options->crc_on_payload;

    if (options->corrupted_msg_handler != nullptr) {
        opts.corrupted_msg_handler_ =
                [corrupted_msg_handler = options->corrupted_msg_handler, corrupted_msg_handler_user_data = options->
                    corrupted_msg_handler_user_data](
            std::shared_ptr<nuraft::buffer> header, std::shared_ptr<nuraft::buffer> payload
        ) {
                    corrupted_msg_handler(
                        corrupted_msg_handler_user_data,
                        header->data_begin(),
                        header->size(),
                        payload->data_begin(),
                        payload->size()
                    );
                };
    }
    opts.streaming_mode_ = options->streaming_mode;
    auto result = new bop_raft_asio_service_ptr(
        std::make_shared<nuraft::asio_service>(opts, logger->logger), opts
    );
    return result;
}

BOP_API void bop_raft_asio_service_delete(const bop_raft_asio_service_ptr *asio_service) {
    delete asio_service;
}

BOP_API void bop_raft_asio_service_stop(bop_raft_asio_service_ptr *asio_service) {
    if (!asio_service || !asio_service->service || asio_service->stopped) return;
    try {
        asio_service->service->stop();
    } catch (...) {
    }
    if (asio_service) asio_service->stopped = true;
}

BOP_API uint32_t bop_raft_asio_service_get_active_workers(bop_raft_asio_service_ptr *asio_service) {
    if (!asio_service || !asio_service->service) return 0;
    return asio_service->service->get_active_workers();
}

struct bop_raft_delayed_task final : nuraft::delayed_task {
    void *user_data;
    bop_raft_delayed_task_func exec_cb;

    bop_raft_delayed_task(
        void *user_data,
        int32_t type,
        bop_raft_delayed_task_func exec_cb,
        bop_raft_delayed_task_func del_cb
    ) : nuraft::delayed_task(type), user_data(user_data), exec_cb(exec_cb) {
        set_impl_context(user_data, del_cb);
    };

protected:
    void exec() override {
        if (exec_cb) {
            exec_cb(user_data);
        }
    }
};

struct bop_raft_delayed_task_ptr {
    nuraft::ptr<nuraft::delayed_task> task;

    explicit bop_raft_delayed_task_ptr(const nuraft::ptr<nuraft::delayed_task> &task)
        : task(task) {
    }
};


BOP_API bop_raft_delayed_task_ptr *bop_raft_delayed_task_make(
    void *user_data,
    int32_t type,
    bop_raft_delayed_task_func exec_func,
    bop_raft_delayed_task_func deleter_func
) {
    if (!exec_func) return nullptr;
    auto ptr = new
            bop_raft_delayed_task_ptr(nuraft::cs_new<bop_raft_delayed_task>(
                user_data, type, exec_func, deleter_func
            ));
    return ptr;
}

BOP_API void bop_raft_delayed_task_delete(const bop_raft_delayed_task_ptr *task) {
    if (!task) return;
    delete task;
}

BOP_API void bop_raft_delayed_task_cancel(bop_raft_delayed_task_ptr *task) {
    if (!task || !task->task) return;
    task->task->cancel();
}

BOP_API void bop_raft_delayed_task_reset(bop_raft_delayed_task_ptr *task) {
    if (!task || !task->task) return;
    task->task->reset();
}

BOP_API int32_t bop_raft_delayed_task_type(bop_raft_delayed_task_ptr *task) {
    if (!task || !task->task) return -1;
    return task->task->get_type();
}

BOP_API void *bop_raft_delayed_task_user_data(bop_raft_delayed_task_ptr *task) {
    if (!task || !task->task) return nullptr;
    return task->task->get_impl_context();
}

BOP_API void bop_raft_asio_service_schedule(
    bop_raft_asio_service_ptr *asio_service,
    bop_raft_delayed_task_ptr *delayed_task,
    int32_t milliseconds
) {
    if (!asio_service || !asio_service->service || !delayed_task) return;
    asio_service->service->schedule(delayed_task->task, milliseconds);
}

struct bop_raft_rpc_listener_ptr {
    nuraft::ptr<nuraft::rpc_listener> rpc_listener;

    explicit bop_raft_rpc_listener_ptr(const nuraft::ptr<nuraft::rpc_listener> &rpc_listener)
        : rpc_listener(rpc_listener) {
    }
};

BOP_API bop_raft_rpc_listener_ptr *bop_raft_asio_rpc_listener_make(
    bop_raft_asio_service_ptr *asio_service,
    uint16_t listening_port,
    bop_raft_logger_ptr *logger
) {
    if (!asio_service || !asio_service->service || !logger || !logger->logger) return nullptr;
    auto listener = asio_service->service->create_rpc_listener(listening_port, logger->logger);
    if (!listener) return nullptr;
    return new bop_raft_rpc_listener_ptr(listener);
}

BOP_API void bop_raft_asio_rpc_listener_delete(const bop_raft_rpc_listener_ptr *rpc_listener) {
    if (!rpc_listener) return;
    delete rpc_listener;
}

struct bop_raft_rpc_client_ptr {
    nuraft::ptr<nuraft::rpc_client> rpc_client;

    explicit bop_raft_rpc_client_ptr(const nuraft::ptr<nuraft::rpc_client> &rpc_client)
        : rpc_client(rpc_client) {
    }
};

BOP_API bop_raft_rpc_client_ptr *bop_raft_asio_rpc_client_make(
    bop_raft_asio_service_ptr *asio_service,
    const char *endpoint,
    size_t endpoint_size
) {
    if (!asio_service || !asio_service->service || !endpoint || endpoint_size == 0) return nullptr;
    auto rpc_client = asio_service->service->create_client(std::string(endpoint, endpoint_size));
    if (!rpc_client) return nullptr;
    return new bop_raft_rpc_client_ptr(rpc_client);
}

BOP_API void bop_raft_asio_rpc_client_delete(const bop_raft_rpc_client_ptr *rpc_client) {
    if (!rpc_client) return;
    delete rpc_client;
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
//     to.locking_method_type_ =
//     static_cast<bop_raft_params_locking_method_type>(from.locking_method_type_);
//     to.return_method_ = static_cast<bop_raft_params_return_method_type>(from.return_method_);
//     to.auto_forwarding_req_timeout_ = from.auto_forwarding_req_timeout_;
//     to.grace_period_of_lagging_state_machine_ = from.grace_period_of_lagging_state_machine_;
//     to.use_new_joiner_type_ = from.use_new_joiner_type_;
//     to.use_bg_thread_for_snapshot_io_ = from.use_bg_thread_for_snapshot_io_;
//     to.use_full_consensus_among_healthy_members_ =
//     from.use_full_consensus_among_healthy_members_; to.parallel_log_appending_ =
//     from.parallel_log_appending_; to.max_log_gap_in_stream_ = from.max_log_gap_in_stream_;
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
    offsetof(nuraft::raft_params, max_append_size_) == offsetof(bop_raft_params, max_append_size)
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
    offsetof(nuraft::raft_params, fresh_log_gap_) == offsetof(bop_raft_params, fresh_log_gap)
);
static_assert(
    offsetof(nuraft::raft_params, stale_log_gap_) == offsetof(bop_raft_params, stale_log_gap)
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
    offsetof(nuraft::raft_params, auto_forwarding_) == offsetof(bop_raft_params, auto_forwarding)
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
    offsetof(nuraft::raft_params, return_method_) == offsetof(bop_raft_params, return_method)
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

BOP_API bop_raft_params *bop_raft_params_make() {
    nuraft::raft_params params{};
    bop_raft_params *result = new bop_raft_params;
    *result = *reinterpret_cast<bop_raft_params *>(&params);
    return result;
}

BOP_API void bop_raft_params_delete(const bop_raft_params *params) {
    if (params)
        delete params;
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
    return new bop_raft_params_ptr(
        nuraft::ptr<nuraft::raft_params>(reinterpret_cast<nuraft::raft_params *>(params))
    );
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
    return reinterpret_cast<nuraft::state_machine::adjust_commit_index_params *>(params)
            ->peer_index_map_[peerID];
}

BOP_API void bop_raft_fsm_adjust_commit_index_peer_indexes(
    bop_raft_fsm_adjust_commit_index_params *params, uint64_t *peers[16]
) {
    for (const auto &[peer_id, index]:
         reinterpret_cast<nuraft::state_machine::adjust_commit_index_params *>(params)
         ->peer_index_map_) {
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
        void *user_data, const nuraft::ptr<nuraft::cluster_config> &current_conf,
        const nuraft::ptr<nuraft::cluster_config> &rollback_conf, bop_raft_fsm_commit commit,
        bop_raft_fsm_cluster_config commit_config, bop_raft_fsm_commit pre_commit,
        bop_raft_fsm_rollback rollback, bop_raft_fsm_cluster_config rollback_config,
        bop_raft_fsm_get_next_batch_size_hint_in_bytes get_next_batch_size_hint_in_bytes,
        bop_raft_fsm_snapshot_save save_snapshot, bop_raft_fsm_snapshot_apply apply_snapshot,
        bop_raft_fsm_snapshot_read read_snapshot,
        bop_raft_fsm_free_user_snapshot_ctx free_snapshot_user_ctx,
        bop_raft_fsm_last_snapshot last_snapshot, bop_raft_fsm_last_commit_index last_commit_index,
        bop_raft_fsm_create_snapshot create_snapshot,
        bop_raft_fsm_chk_create_snapshot chk_create_snapshot,
        bop_raft_fsm_allow_leadership_transfer allow_leadership_transfer,
        bop_raft_fsm_adjust_commit_index adjust_commit_index
    )
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
     *   Given memory buffer is owned
     * by caller, so that
     *   commit implementation should clone it if user wants to
     * use
     * the memory even after the commit call returns.
     *
     *   Here provide a default
     * implementation for facilitating the
     *   situation when application does not care its
     * implementation.
     *
     * @param log_idx Raft log number to commit.
     * @param data
     * Payload of the Raft log.
     * @return Result value of state machine.
     */
    nuraft::ptr<nuraft::buffer> commit(const nuraft::ulong log_idx, nuraft::buffer &data) override {
        nuraft::buffer *result{nullptr};
        commit_(user_data_, log_idx, data.data_begin(), data.size(), &result);
        return result != nullptr ? nuraft::buffer::make_shared(result) : nullptr;
    }

    /**
     * (Optional)
     * Handler on the commit of a configuration change.
     *
     *
     * @param log_idx Raft log number of the configuration change.
     * @param new_conf New
     * cluster configuration.
     */
    void commit_config(
        const nuraft::ulong log_idx, nuraft::ptr<nuraft::cluster_config> &new_conf
    ) override {
        current_conf = new_conf;
        commit_config_(user_data_, log_idx, new_conf.get());
    }

    /**
     * Pre-commit the given Raft log.
     *
     * Pre-commit is called after appending
     * Raft log,
     * before getting acks from quorum nodes.
     * Users can ignore this function
     * if not needed.
     *
     * Same as `commit()`, memory buffer is owned by caller.
     *

     * * @param log_idx Raft log number to commit.
     * @param data Payload of the Raft log.

     * * @return Result value of state machine.
     */
    nuraft::ptr<nuraft::buffer>
    pre_commit(const nuraft::ulong log_idx, nuraft::buffer &data) override {
        const auto cb = pre_commit_;
        if (!cb)
            return nullptr;
        nuraft::buffer *result{nullptr};
        cb(user_data_, log_idx, data.data_begin(), data.size(), &result);
        return result != nullptr ? nuraft::buffer::make_shared(result) : nullptr;
    }

    /**
     * Rollback the state machine to given Raft log number.
     *
     * It will be called
     * for uncommitted Raft logs only,
     * so that users can ignore this function if they don't

     * * do anything on pre-commit.
     *
     * Same as `commit()`, memory buffer is owned by
     * caller.
     *
     * @param log_idx Raft log number to commit.
     * @param data Payload of
     * the Raft log.
     */
    void rollback(const nuraft::ulong log_idx, nuraft::buffer &data) override {
        rollback_(user_data_, log_idx, data.data_begin(), data.size());
    }

    /**
     * (Optional)
     * Handler on the rollback of a configuration change.
     * The
     * configuration can be either committed or uncommitted one,
     * and that can be checked by
     * the given `log_idx`, comparing it with
     * the current `cluster_config`'s log index.
 *

     * * @param log_idx Raft log number of the configuration change.
     * @param conf The cluster
     * configuration to be rolled back.
     */
    void rollback_config(
        const nuraft::ulong log_idx, nuraft::ptr<nuraft::cluster_config> &conf
    ) override {
        rollback_conf = conf;
        rollback_config_(user_data_, log_idx, conf.get());
    }

    /**
     * (Optional)
     * Return a hint about the preferred size (in number of bytes)
     *
     * of the next batch of logs to be sent from the leader.
     *
     * Only applicable on
     * followers.
     *
     * @return The preferred size of the next log batch.
     *         `0`
     * indicates no preferred size (any size is good).
     *         `positive value` indicates at
     * least one log can be sent,
     *         (the size of that log may be bigger than this hint
     * size).
     *         `negative value` indicates no log should be sent since this
     *
     * follower is busy handling pending logs.
     */
    nuraft::int64 get_next_batch_size_hint_in_bytes() override {
        return get_next_batch_size_hint_in_bytes_(user_data_);
    }

    /**
     * Save the given snapshot object to local snapshot.
     * This API is for snapshot
     * receiver (i.e., follower).
     *
     * This is an optional API for users who want to use
     * logical
     * snapshot. Instead of splitting a snapshot into multiple
     * physical
     * chunks, this API uses logical objects corresponding
     * to a unique object ID. Users are
     * responsible for defining
     * what object is: it can be a key-value pair, a set of
     *
     * key-value pairs, or whatever.
     *
     * Same as `commit()`, memory buffer is owned by
     * caller.
     *
     * @param s Snapshot instance to save.
     * @param obj_id[in,out]
     *
     * Object ID.
     *     As a result of this API call, the next object ID
     *     that
     * reciever wants to get should be set to
     *     this parameter.
     * @param data Payload
     * of given object.
     * @param is_first_obj `true` if this is the first object.
     * @param
     * is_last_obj `true` if this is the last object.
     */
    void save_logical_snp_obj(
        nuraft::snapshot &s, nuraft::ulong &obj_id, nuraft::buffer &data, bool is_first_obj,
        bool is_last_obj
    ) override {
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
     * @param s Snapshot instance to
     * apply.
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
     * This API is for snapshot sender (i.e.,
     * leader).
     *
     * Same as above, this is an optional API for users who want to
     *
     * use logical snapshot.
     *
     * @param s Snapshot instance to read.
     * @param[in,out]
     * user_snp_ctx
     *     User-defined instance that needs to be passed through
     *     the
     * entire snapshot read. It can be a pointer to
     *     state machine specific iterators, or
     * whatever.
     *     On the first `read_logical_snp_obj` call, it will be
     *     set to
     * `null`, and this API may return a new pointer if necessary.
     *     Returned pointer will
     * be passed to next `read_logical_snp_obj`
     *     call.
     * @param obj_id Object ID to
     * read.
     * @param[out] data_out Buffer where the read object will be stored.
     *
     * @param[out] is_last_obj Set `true` if this is the last object.
     * @return Negative number
     * if failed.
     */
    int read_logical_snp_obj(
        nuraft::snapshot &s, void *&user_snp_ctx, nuraft::ulong obj_id,
        nuraft::ptr<nuraft::buffer> &data_out, bool &is_last_obj
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
     *
     * This is an optional API for users who want to use logical snapshot.
     *
     * @param
     * user_snp_ctx User-defined instance to free.
     */
    void free_user_snp_ctx(void *&user_snp_ctx) override {
        free_snapshot_user_ctx_(user_data_, static_cast<void **>(user_snp_ctx));
    }

    /**
     * Get the latest snapshot instance.
     *
     * This API will be invoked at the
     * initialization of Raft server,
     * so that the last last snapshot should be durable for
     * server restart,
     * if you want to avoid unnecessary catch-up.
     *
     * @return
     * Pointer to the latest snapshot.
     */
    nuraft::ptr<nuraft::snapshot> last_snapshot() override {
        return nuraft::ptr<nuraft::snapshot>(
            reinterpret_cast<nuraft::snapshot *>(last_snapshot_(user_data_))
        );
    }

    /**
     * Get the last committed Raft log number.
     *
     * This API will be invoked at the
     * initialization of Raft server
     * to identify what the last committed point is, so that
     * the last
     * committed index number should be durable for server restart,
     * if you
     * want to avoid unnecessary catch-up.
     *
     * @return Last committed Raft log number.
 */
    nuraft::ulong last_commit_index() override {
        return last_commit_index_(user_data_);
    }

    /**
     * Create a snapshot corresponding to the given info.
     *
     * @param s Snapshot
     * info to create.
     * @param when_done Callback function that will be called after
     *
     * snapshot creation is done.
     */
    void create_snapshot(
        nuraft::snapshot &s, nuraft::async_result<bool>::handler_type &when_done
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
     * Once the pre-defined condition is satisfied,
     * Raft core will invoke
     * this function to ask if it needs to create a new snapshot.
 * If
     * user-defined state machine does not want to create snapshot
     * at this time, this
     * function will return `false`.
     *
     * @return `true` if wants to create snapshot.
 *
     * `false` if does not want to create snapshot.
     */
    bool chk_create_snapshot() override {
        return chk_create_snapshot_(user_data_);
    }

    /**
     * Decide to transfer leadership.
     * Once the other conditions are met, Raft core
     * will invoke
     * this function to ask if it is allowed to transfer the
     * leadership to
     * other member.
     *
     * @return `true` if wants to transfer leadership.
     * `false` if
     * not.
     */
    bool allow_leadership_transfer() override {
        return allow_leadership_transfer_(user_data_);
    }

    /**
     * This function will be called when Raft succeeds in replicating logs
     * to an
     * arbitrary follower and attempts to commit logs. Users can manually
     * adjust the commit
     * index. The adjusted commit index should be equal to
     * or greater than the given
     * `current_commit_index`. Otherwise, no log
     * will be committed.
     *
     * @param
     * params Parameters.
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

BOP_API bop_raft_fsm_ptr *bop_raft_fsm_make(
    void *user_data, bop_raft_cluster_config *current_conf, bop_raft_cluster_config *rollback_conf,
    bop_raft_fsm_commit commit, bop_raft_fsm_cluster_config commit_config,
    bop_raft_fsm_commit pre_commit, bop_raft_fsm_rollback rollback,
    bop_raft_fsm_cluster_config rollback_config,
    bop_raft_fsm_get_next_batch_size_hint_in_bytes get_next_batch_size_hint_in_bytes,
    bop_raft_fsm_snapshot_save save_snapshot, bop_raft_fsm_snapshot_apply apply_snapshot,
    bop_raft_fsm_snapshot_read read_snapshot,
    bop_raft_fsm_free_user_snapshot_ctx free_snapshot_user_ctx,
    bop_raft_fsm_last_snapshot last_snapshot, bop_raft_fsm_last_commit_index last_commit_index,
    bop_raft_fsm_create_snapshot create_snapshot,
    bop_raft_fsm_chk_create_snapshot chk_create_snapshot,
    bop_raft_fsm_allow_leadership_transfer allow_leadership_transfer,
    bop_raft_fsm_adjust_commit_index adjust_commit_index
) {
    if (!commit)
        return nullptr;
    if (!commit_config)
        return nullptr;
    if (!pre_commit)
        return nullptr;
    if (!rollback)
        return nullptr;
    if (!get_next_batch_size_hint_in_bytes)
        return nullptr;
    if (!save_snapshot)
        return nullptr;
    if (!apply_snapshot)
        return nullptr;
    if (!read_snapshot)
        return nullptr;
    if (!free_snapshot_user_ctx)
        return nullptr;
    if (!last_snapshot)
        return nullptr;
    if (!last_commit_index)
        return nullptr;
    if (!create_snapshot)
        return nullptr;
    if (!chk_create_snapshot)
        return nullptr;
    if (!allow_leadership_transfer)
        return nullptr;
    if (!adjust_commit_index)
        return nullptr;
    return new bop_raft_fsm_ptr(
        std::make_shared<bop_raft_state_machine>(
            user_data,
            nuraft::ptr<nuraft::cluster_config>(
                reinterpret_cast<nuraft::cluster_config *>(current_conf)
            ),
            nuraft::ptr<nuraft::cluster_config>(
                reinterpret_cast<nuraft::cluster_config *>(rollback_conf)
            ),
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
        )
    );
}

BOP_API void bop_raft_fsm_delete(const bop_raft_fsm_ptr *fsm) {
    if (fsm)
        delete fsm;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_mgr
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_state_mgr_ptr {
    nuraft::ptr<nuraft::state_mgr> state_mgr;

    bop_raft_state_mgr_ptr(const nuraft::ptr<nuraft::state_mgr> &state_mgr)
        : state_mgr(state_mgr) {
    }
};

struct bop_raft_log_store_ptr {
    nuraft::ptr<nuraft::log_store> log_store;

    bop_raft_log_store_ptr(const nuraft::ptr<nuraft::log_store> &log_store)
        : log_store(log_store) {
    }
};

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
        void *user_data, bop_raft_state_mgr_load_config load_config,
        bop_raft_state_mgr_save_config save_config, bop_raft_state_mgr_save_state save_state,
        bop_raft_state_mgr_read_state read_state, bop_raft_state_mgr_load_log_store load_log_store,
        bop_raft_state_mgr_server_id server_id, bop_raft_state_mgr_system_exit system_exit
    )
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
     * This function will be invoked on
     * initialization of
     * Raft server.
     *
     * Even at the very first initialization, it
     * should
     * return proper initial cluster config, not `nullptr`.
     * The initial cluster
     * config must include the server itself.
     *
     * @return Cluster config.
     */
    nuraft::ptr<nuraft::cluster_config> load_config() override {
        return nuraft::ptr<nuraft::cluster_config>(
            reinterpret_cast<nuraft::cluster_config *>(load_config_(user_data_))
        );
    }

    /**
     * Save given cluster config.
     *
     * @param config Cluster config to save.
 */
    void save_config(const nuraft::cluster_config &config) override {
        save_config_(user_data_, reinterpret_cast<const bop_raft_cluster_config *>(&config));
    }

    /**
     * Save given server state.
     *
     * @param state Server state to save.
     */
    void save_state(const nuraft::srv_state &state) override {
        save_state_(user_data_, reinterpret_cast<const bop_raft_srv_state *>(&state));
    }

    /**
     * Load the last saved server state.
     * This function will be invoked on
     * initialization of
     * Raft server
     *
     * At the very first initialization, it
     * should return
     * `nullptr`.
     *
     * @param Server state.
     */
    nuraft::ptr<nuraft::srv_state> read_state() override {
        return nuraft::ptr<nuraft::srv_state>(
            reinterpret_cast<nuraft::srv_state *>(read_state_(user_data_))
        );
    }

    /**
     * Get instance of user-defined Raft log store.
     *
     * @param Raft log store
     * instance.
     */
    nuraft::ptr<nuraft::log_store> load_log_store() override {
        return load_log_store_(user_data_)->log_store;
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
     * abnormal termination of
     * Raft server.
     *
     * @param exit_code Error code.
     */
    void system_exit(const int exit_code) override {
        system_exit_(user_data_, exit_code);
    }
};

BOP_API bop_raft_state_mgr_ptr *bop_raft_state_mgr_make(
    void *user_data,
    bop_raft_state_mgr_load_config load_config,
    bop_raft_state_mgr_save_config save_config,
    bop_raft_state_mgr_read_state read_state,
    bop_raft_state_mgr_save_state save_state,
    bop_raft_state_mgr_load_log_store load_log_store,
    bop_raft_state_mgr_server_id server_id,
    bop_raft_state_mgr_system_exit system_exit
) {
    if (!load_config)
        return nullptr;
    if (!save_config)
        return nullptr;
    if (!save_state)
        return nullptr;
    if (!read_state)
        return nullptr;
    if (!load_log_store)
        return nullptr;
    if (!server_id)
        return nullptr;
    if (!system_exit)
        return nullptr;
    return new bop_raft_state_mgr_ptr(
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

BOP_API void bop_raft_state_mgr_delete(const bop_raft_state_mgr_ptr *sm) {
    if (sm)
        delete sm;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::log_store
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_log_entry;

const size_t BOP_RAFT_LOG_ENTRY_PTR_SIZE = sizeof(nuraft::ptr<nuraft::log_entry>);

static_assert(sizeof(nuraft::ptr<nuraft::log_entry>) == 16);

static_assert(sizeof(bop_raft_log_entry_ptr) == 16);

size_t bop_raft_log_entry_ptr_use_count(bop_raft_log_entry_ptr *log_entry_ptr) {
    return reinterpret_cast<nuraft::ptr<nuraft::log_entry>*>(log_entry_ptr)->use_count();
}

void bop_raft_log_entry_ptr_retain(bop_raft_log_entry_ptr* log_entry_ptr) {
    char data[BOP_RAFT_LOG_ENTRY_PTR_SIZE];
    new(data) nuraft::ptr(*reinterpret_cast<nuraft::ptr<nuraft::log_entry>*>(log_entry_ptr));
}

void bop_raft_log_entry_ptr_release(bop_raft_log_entry_ptr *log_entry_ptr) {
    reinterpret_cast<nuraft::ptr<nuraft::log_entry>*>(log_entry_ptr)->~shared_ptr();
}

// typedef uint64_t (*bop_raft_log_store_append_async)(
//     void *user_data,
//     int64_t batch_size,
//     int64_t batch_index,
//     uint64_t term,
//     uint8_t *data,
//     size_t data_size,
//     uint64_t log_timestamp,
//     bool has_crc32,
//     uint32_t crc32
// );
//
// typedef void (*bop_raft_log_store_append_async_batch_done)(void *user_data);
//
// BOP_API int64_t bop_raft_log_entry_queue_pop(
//     void *user_data,
//     bop_raft_log_entry_queue *queue,
//     int64_t timeout_micros,
//     bop_raft_log_store_append_async on_entry,
//     bop_raft_log_store_append_async_batch_done on_batch_done
// ) {
//     if (!queue) return -1;
//     if (!on_entry) return -1;
//
//     bop_raft_log_store_append a;
//
//     nuraft::ptr<nuraft::log_entry> entries[128];
//
//     int64_t size = queue->queue_.wait_dequeue_bulk_timed(entries, 128, timeout_micros);
//
//     if (size == 0) return 0;
//
//     for (int64_t i = 0; i < size; i++) {
//         auto entry = entries[i];
//         entries[i] = nullptr;
//         on_entry(
//             user_data,
//             size,
//             i,
//             entry->get_term(),
//             entry->get_buf().data(),
//             entry->get_buf().size(),
//             entry->get_timestamp(),
//             entry->has_crc32(),
//             entry->get_crc32()
//         );
//     }
//
//     if (on_batch_done) on_batch_done(user_data);
//
//     return size;
// }

struct bop_raft_log_entry_vector {
    std::vector<nuraft::ptr<nuraft::log_entry> > *log_entries;

    bop_raft_log_entry_vector(std::vector<nuraft::ptr<nuraft::log_entry> > *log_entries)
        : log_entries(log_entries) {
    }
};

BOP_API void bop_raft_log_entry_vec_push(
    const bop_raft_log_entry_vector *vec, uint64_t term, nuraft::buffer *data, uint64_t timestamp,
    bool has_crc32, uint32_t crc32
) {
    const auto buf = nuraft::ptr<nuraft::buffer>(data);
    vec->log_entries->push_back(
        std::make_shared<nuraft::log_entry>(
            term, std::move(buf), nuraft::log_val_type::app_log, timestamp, has_crc32, crc32
        )
    );
}

BOP_API bop_raft_log_entry *bop_raft_log_entry_make(
    uint64_t term, nuraft::buffer *data, uint64_t timestamp, bool has_crc32, uint32_t crc32
) {
    const auto buf = nuraft::ptr<nuraft::buffer>(data);
    return reinterpret_cast<bop_raft_log_entry *>(new nuraft::log_entry(
        term, std::move(buf), nuraft::log_val_type::app_log, timestamp, has_crc32, crc32
    ));
}

BOP_API void bop_raft_log_entry_delete(const bop_raft_log_entry *entry) {
    if (entry)
        delete reinterpret_cast<const nuraft::log_entry *>(entry);
}

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
        void *user_data, bop_raft_log_store_next_slot next_slot,
        bop_raft_log_store_start_index start_index, bop_raft_log_store_last_entry last_entry,
        bop_raft_log_store_append append, bop_raft_log_store_write_at write_at,
        bop_raft_log_store_end_of_append_batch end_of_append_batch,
        bop_raft_log_store_log_entries log_entries, bop_raft_log_store_entry_at entry_at,
        bop_raft_log_store_term_at term_at, bop_raft_log_store_pack pack,
        bop_raft_log_store_apply_pack apply_pack, bop_raft_log_store_compact compact,
        bop_raft_log_store_compact_async compact_async, bop_raft_log_store_flush flush,
        bop_raft_log_store_last_durable_index last_durable_index
    )
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
     * @return Last log
     * index number + 1
     */
    nuraft::ulong next_slot() const override {
        return next_slot_(user_data_);
    }

    /**
     * The start index of the log store, at the very beginning, it must be 1.
     *
     * However, after some compact actions, this could be anything equal to or
     * greater than
     * one
     */
    nuraft::ulong start_index() const override {
        return start_index_(user_data_);
    }

    /**
     * The last log entry in store.
     *
     * @return If no log entry exists: a dummy
     * constant entry with
     *         value set to null and term set to zero.
     */
    [[nodiscard]] nuraft::ptr<nuraft::log_entry> last_entry() const override {
        return nuraft::ptr<nuraft::log_entry>(
            reinterpret_cast<nuraft::log_entry *>(last_entry_(user_data_))
        );
    }

    /**
     * Append a log entry to store.
     *
     * @param entry Log entry
     * @return Log
     * index number.
     */
    nuraft::ulong append(nuraft::ptr<nuraft::log_entry> &entry) override {
        return append_(
            user_data_,
            *reinterpret_cast<bop_raft_log_entry_ptr *>(&entry),
            entry->get_term(),
            entry->get_buf().data_begin(),
            entry->get_buf().size(),
            entry->get_timestamp(),
            entry->has_crc32(),
            entry->get_crc32()
        );
    }

    /**
     * Overwrite a log entry at the given `index`. This API should make sure
     * that all log entries after the given `index` should be truncated (if exist),
     * as a result of this function call.
     *
     * @param index Log index number to overwrite.
     *
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
     * a single append_entries
     * request.
     *
     * @param start The start log index number (inclusive)
     * @param cnt
     * The number of log entries written.
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

     * * @param end The end log index number (exclusive).
     * @return The log entries between
     * [start, end).
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
     * The total
     * size of the returned entries is limited by batch_size_hint.
     *
     * Return nullptr to
     * indicate error if any log entry within the requested range
     * could not be retrieved
     * (e.g. due to external log truncation).
     *
     * @param start The start log index number
     * (inclusive).
     * @param end The end log index number (exclusive).
     * @param
     * batch_size_hint_in_bytes Total size (in bytes) of the returned entries,
     *        see the
     * detailed comment at
     *        `state_machine::get_next_batch_size_hint_in_bytes()`.

     * * @return The log entries between [start, end) and limited by the total size
     * given by
     * the batch_size_hint_in_bytes.
     */
    nuraft::ptr<std::vector<nuraft::ptr<nuraft::log_entry> > > log_entries_ext(
        nuraft::ulong start, nuraft::ulong end, nuraft::int64 batch_size_hint_in_bytes = 0
    ) override {
        return log_entries(start, end);
    }

    /**
     * Get the log entry at the specified log index number.
     *
     * @param index
     * Should be equal to or greater than 1.
     * @return The log entry or null if index >=
     * this->next_slot().
     */
    nuraft::ptr<nuraft::log_entry> entry_at(nuraft::ulong index) override {
        return nuraft::ptr<nuraft::log_entry>(reinterpret_cast<nuraft::log_entry *>(entry_at_(user_data_, index)));
    }

    /**
     * Get the term for the log entry at the specified index.
     * Suggest to stop the system if the index >= this->next_slot()
     *
     * @param index Should be equal to or
     * greater than 1.
     * @return The term for the specified log entry, or
     *         0 if
     * index < this->start_index().
     */
    nuraft::ulong term_at(nuraft::ulong index) override {
        return term_at_(user_data_, index);
    }

    /**
     * Pack the given number of log items starting from the given index.
     *
     *
     * @param index The start log index number (inclusive).
     * @param cnt The number of logs to
     * pack.
     * @return Packed (encoded) logs.
     */
    nuraft::ptr<nuraft::buffer> pack(nuraft::ulong index, nuraft::int32 cnt) override {
        return nuraft::ptr<nuraft::buffer>(reinterpret_cast<nuraft::buffer *>(pack_(user_data_, index, cnt)));
    }

    /**
     * Apply the log pack to current log store, starting from index.
     *
     * @param
     * index The start log index number (inclusive).
     * @param Packed logs.
     */
    void apply_pack(nuraft::ulong index, nuraft::buffer &pack) override {
        apply_pack_(user_data_, index, reinterpret_cast<bop_raft_buffer *>(&pack));
    }

    /**
     * Compact the log store by purging all log entries,
     * including the given log
     * index number.
     *
     * If current maximum log index is smaller than given
     * `last_log_index`,
     * set start log index to `last_log_index + 1`.
     *
     * @param
     * last_log_index Log index number that will be purged up to (inclusive).
     * @return `true`
     * on success.
     */
    bool compact(nuraft::ulong last_log_index) override {
        return compact_(user_data_, last_log_index);
    }

    /**
     * Compact the log store by purging all log entries,
     * including the given log
     * index number.
     *
     * Unlike `compact`, this API allows to execute the log compaction
     * in background
     * asynchronously, aiming at reducing the client-facing latency caused by
     * the
     * log compaction.
     *
     * This function call may return immediately, but after
     * this function
     * call, following `start_index` should return `last_log_index + 1` even

     * * though the log compaction is still in progress. In the meantime, the
     * actual job
     * incurring disk IO can run in background. Once the job is done,
     * `when_done` should be
     * invoked.
     *
     * @param last_log_index Log index number that will be purged up to
     * (inclusive).
     * @param when_done Callback function that will be called after
     * the
     * log compaction is done.
     */
    void compact_async(
        nuraft::ulong last_log_index, const nuraft::async_result<bool>::handler_type &when_done
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
     *
     * so that all log entries are guaranteed to be durable upon process crash.
     *
     *
     * @return `true` on success.
     */
    bool flush() override {
        return flush_(user_data_);
    }

    /**
     * (Experimental)
     * This API is used only when
     * `raft_params::parallel_log_appending_` flag is set.
     * Please refer to the comment of the
     * flag.
     *
     * @return The last durable log index.
     */
    nuraft::ulong last_durable_index() override {
        return last_durable_index_(user_data_);
    }
};

BOP_API bop_raft_log_store_ptr *bop_raft_log_store_make(
    void *user_data, bop_raft_log_store_next_slot next_slot,
    bop_raft_log_store_start_index start_index, bop_raft_log_store_last_entry last_entry,
    bop_raft_log_store_append append, bop_raft_log_store_write_at write_at,
    bop_raft_log_store_end_of_append_batch end_of_append_batch,
    bop_raft_log_store_log_entries log_entries, bop_raft_log_store_entry_at entry_at,
    bop_raft_log_store_term_at term_at, bop_raft_log_store_pack pack,
    bop_raft_log_store_apply_pack apply_pack, bop_raft_log_store_compact compact,
    bop_raft_log_store_compact_async compact_async, bop_raft_log_store_flush flush,
    bop_raft_log_store_last_durable_index last_durable_index
) {
    if (!next_slot)
        return nullptr;
    if (!start_index)
        return nullptr;
    if (!last_entry)
        return nullptr;
    if (!append)
        return nullptr;
    if (!write_at)
        return nullptr;
    if (!end_of_append_batch)
        return nullptr;
    if (!log_entries)
        return nullptr;
    if (!entry_at)
        return nullptr;
    if (!term_at)
        return nullptr;
    if (!pack)
        return nullptr;
    if (!apply_pack)
        return nullptr;
    if (!compact)
        return nullptr;
    if (!compact_async)
        return nullptr;
    if (!flush)
        return nullptr;
    if (!last_durable_index)
        return nullptr;
    return new bop_raft_log_store_ptr(
        std::make_shared<bop_raft_log_store>(
            user_data,
            next_slot,
            start_index,
            last_entry,
            append,
            write_at,
            end_of_append_batch,
            log_entries,
            entry_at,
            term_at,
            pack,
            apply_pack,
            compact,
            compact_async,
            flush,
            last_durable_index
        )
    );
}

BOP_API void bop_raft_log_store_delete(const bop_raft_log_store_ptr *log_store) {
    if (log_store)
        delete log_store;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::context
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_context_ptr {
    nuraft::ptr<nuraft::context> ctx;
};

BOP_API bop_raft_context_ptr *bop_raft_context_create() {
    return new bop_raft_context_ptr;
}

BOP_API void bop_raft_context_delete(const bop_raft_context_ptr *context) {
    if (context)
        delete context;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::counter
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_counter {
    std::string name;
    uint64_t value;

    bop_raft_counter(const std::string &name, uint64_t value) : name(name), value(value) {
    }
};

BOP_API bop_raft_counter *bop_raft_counter_make(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_counter(std::string(view), 0);
}

BOP_API void bop_raft_counter_delete(const bop_raft_counter *counter) {
    if (counter) {
        delete counter;
    }
}

BOP_API const char *bop_raft_counter_name(const bop_raft_counter *counter) {
    return counter->name.c_str();
}

BOP_API uint64_t bop_raft_counter_value(const bop_raft_counter *counter) {
    return counter->value;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::gauge
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_gauge {
    std::string name;
    int64_t value;

    bop_raft_gauge(const std::string &name, int64_t value) : name(name), value(value) {
    }
};

BOP_API bop_raft_gauge *bop_raft_gauge_make(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_gauge(std::string(view), 0);
}

BOP_API void bop_raft_gauge_delete(const bop_raft_gauge *gauge) {
    if (gauge) {
        delete gauge;
    }
}

BOP_API const char *bop_raft_gauge_name(const bop_raft_gauge *gauge) {
    return gauge->name.c_str();
}

BOP_API int64_t bop_raft_gauge_value(const bop_raft_gauge *gauge) {
    return gauge->value;
}

struct bop_raft_histogram {
    std::string name;
    std::map<double, uint64_t> value{};

    bop_raft_histogram(const std::string &name) : name(name) {
    }
};

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::histogram
///////////////////////////////////////////////////////////////////////////////////////////////////

BOP_API bop_raft_histogram *bop_raft_histogram_make(const char *name, size_t name_size) {
    std::string_view view(name, name_size);
    return new bop_raft_histogram(std::string(view));
}

BOP_API void bop_raft_histogram_delete(const bop_raft_histogram *histogram) {
    if (histogram) {
        delete histogram;
    }
}

BOP_API const char *bop_raft_histogram_name(const bop_raft_histogram *histogram) {
    return histogram->name.c_str();
}

BOP_API size_t bop_raft_histogram_size(const bop_raft_histogram *histogram) {
    return histogram->value.size();
}

BOP_API uint64_t bop_raft_histogram_get(const bop_raft_histogram *histogram, double key) {
    if (auto it = histogram->value.find(key); it != histogram->value.end()) {
        return it->second;
    } else {
        return 0;
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_server_peer_info
///////////////////////////////////////////////////////////////////////////////////////////////////

static_assert(sizeof(bop_raft_server_peer_info) == sizeof(nuraft::raft_server::peer_info));
static_assert(
    offsetof(bop_raft_server_peer_info, id) == offsetof(nuraft::raft_server::peer_info, id_)
);
static_assert(
    offsetof(bop_raft_server_peer_info, last_log_idx) ==
    offsetof(nuraft::raft_server::peer_info, last_log_idx_)
);
static_assert(
    offsetof(bop_raft_server_peer_info, last_succ_resp_us) ==
    offsetof(nuraft::raft_server::peer_info, last_succ_resp_us_)
);

static_assert(sizeof(bop_raft_server_peer_info) == sizeof(nuraft::raft_server::peer_info));

struct bop_raft_server_peer_info_vec {
    std::vector<nuraft::raft_server::peer_info> peers;

    bop_raft_server_peer_info_vec() = default;
};

BOP_API bop_raft_server_peer_info_vec *bop_raft_server_peer_info_vec_make() {
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

BOP_API bop_raft_server_peer_info *
bop_raft_server_peer_info_vec_get(bop_raft_server_peer_info_vec *vec, size_t idx) {
    if (idx >= vec->peers.size()) {
        return nullptr;
    }
    return reinterpret_cast<bop_raft_server_peer_info *>(&vec->peers[idx]);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_append_entries_ptr
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_append_entries_ptr {
    std::vector<nuraft::ptr<nuraft::buffer> > logs{};
};

BOP_API bop_raft_append_entries_ptr *bop_raft_append_entries_create() {
    return new bop_raft_append_entries_ptr;
}

BOP_API void bop_raft_append_entries_delete(const bop_raft_append_entries_ptr *self) {
    if (self) {
        delete self;
    }
}

BOP_API size_t bop_raft_append_entries_size(bop_raft_append_entries_ptr *self) {
    return self->logs.size();
}

BOP_API size_t
bop_raft_append_entries_push(bop_raft_append_entries_ptr *self, bop_raft_buffer *buf) {
    if (!self)
        return 0;
    if (!buf)
        return 0;
    self->logs.emplace_back(reinterpret_cast<nuraft::buffer *>(buf));
    return self->logs.size();
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_server
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_server_ptr {
    nuraft::ptr<nuraft::raft_server> raft_server;
    nuraft::ptr<nuraft::asio_service> asio_service;
    nuraft::ptr<nuraft::rpc_listener> asio_listener;
    nuraft::ptr<nuraft::log_store> log_store;

    bop_raft_server_ptr(
        const nuraft::ptr<nuraft::raft_server> &raft_server,
        const nuraft::ptr<nuraft::asio_service> &asio_service,
        const nuraft::ptr<nuraft::rpc_listener> &asio_listener,
        const nuraft::ptr<nuraft::log_store> &log_store
    )
        : raft_server(raft_server),
          asio_service(asio_service),
          asio_listener(asio_listener),
          log_store(log_store) {
    }
};

BOP_API bop_raft_server_ptr *bop_raft_server_launch(
    void *user_data,
    bop_raft_fsm_ptr *fsm,
    bop_raft_state_mgr_ptr *state_mgr,
    bop_raft_logger_ptr *logger,
    int32_t port_number,
    const bop_raft_asio_service_ptr *asio_service,
    bop_raft_params *params_given,
    bool skip_initial_election_timeout,
    bool start_server_in_constructor,
    bool test_mode_flag,
    bop_raft_cb_func cb_func
) {
    if (!fsm || !state_mgr || !logger || !asio_service || !params_given)
        return nullptr;

    auto asio_listener = asio_service->service->create_rpc_listener(port_number, logger->logger);
    if (!asio_listener)
        return nullptr;

    nuraft::ptr<nuraft::delayed_task_scheduler> scheduler = asio_service->service;
    nuraft::ptr<nuraft::rpc_client_factory> rpc_client_factory = asio_service->service;

    auto *ctx = new nuraft::context(
        state_mgr->state_mgr,
        fsm->sm,
        asio_listener,
        logger->logger,
        rpc_client_factory,
        scheduler,
        *reinterpret_cast<nuraft::raft_params *>(params_given)
    );

    nuraft::raft_server::init_options init_options;
    init_options.skip_initial_election_timeout_ = skip_initial_election_timeout;
    init_options.start_server_in_constructor_ = start_server_in_constructor;
    init_options.test_mode_flag_ = test_mode_flag;
    if (cb_func) {
        init_options.raft_callback_ = [user_data, cb_func](
            nuraft::cb_func::Type type, nuraft::cb_func::Param *param
        ) -> nuraft::cb_func::ReturnCode {
                    return static_cast<nuraft::cb_func::ReturnCode>(cb_func(
                        user_data,
                        static_cast<bop_raft_cb_type>(type),
                        reinterpret_cast<bop_raft_cb_param *>(param)
                    ));
                };
    }

    nuraft::ptr<nuraft::raft_server> raft_server =
            nuraft::cs_new<nuraft::raft_server>(ctx, init_options);
    asio_listener->listen(raft_server);

    return new bop_raft_server_ptr(
        raft_server, asio_service->service, asio_listener, raft_server->get_log_store()
    );
}

BOP_API bool bop_raft_server_stop(bop_raft_server_ptr *server, size_t time_limit_sec) {
    if (!server->raft_server)
        return false;

    server->raft_server->shutdown();
    server->raft_server.reset();

    if (server->asio_listener) {
        server->asio_listener->stop();
        server->asio_listener->shutdown();
        server->asio_listener = nullptr;
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
    server->raft_server = nullptr;
    server->asio_listener = nullptr;
    server->asio_service = nullptr;
    return true;
}

BOP_API void bop_raft_server_delete(bop_raft_server_ptr *server) {
    if (!server) return;
    delete server;
}

BOP_API bop_raft_server *bop_raft_server_get(bop_raft_server_ptr *s) {
    if (!s)
        return nullptr;
    return reinterpret_cast<bop_raft_server *>(s->raft_server.get());
}

/**
 * Check if this server is ready to serve operation.
 *
 * @return `true` if it is ready.
 */
BOP_API bool bop_raft_server_is_initialized(const bop_raft_server *rs) {
    if (!rs)
        return false;
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_initialized();
}

/**
 * Check if this server is catching up the current leader
 * to join the cluster.
 *
 * @return
 * `true` if it is in catch-up mode.
 */
BOP_API bool bop_raft_server_is_catching_up(const bop_raft_server *rs) {
    if (!rs)
        return false;
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_catching_up();
}

/**
 * Check if this server is receiving snapshot from leader.
 *
 * @return `true` if it is
 * receiving snapshot.
 */
BOP_API bool bop_raft_server_is_receiving_snapshot(const bop_raft_server *rs) {
    if (!rs)
        return false;
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_receiving_snapshot();
}

/**
 * Add a new server to the current cluster.
 * Only leader will accept this operation.
 * Note
 * that this is an asynchronous task so that needs more network
 * communications. Returning this
 * function does not guarantee
 * adding the server.
 *
 * @param srv Configuration of server to
 * add.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_add_srv(
    bop_raft_server *rs, const bop_raft_srv_config_ptr *srv, bop_raft_async_buffer_ptr *handler
) {
    if (!rs)
        return false;
    if (!srv)
        return false;
    if (!handler)
        return false;
    if (!srv->config)
        return false;
    handler->set(reinterpret_cast<nuraft::raft_server *>(rs)->add_srv(*srv->config));
    return true;
}

/**
 * Remove a server from the current cluster.
 * Only leader will accept this operation.
 * The
 * same as `add_srv`, this is also an asynchronous task.
 *
 * @param srv_id ID of server to
 * remove.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_remove_srv(
    bop_raft_server *rs, int32_t srv_id, bop_raft_async_buffer_ptr *handler
) {
    if (!rs)
        return false;
    if (!handler)
        return false;
    handler->set(reinterpret_cast<nuraft::raft_server *>(rs)->remove_srv(srv_id));
    return true;
}

/**366
 * Flip learner flag of given server.
 * Learner will be excluded from the quorum.
 * Only
 * leader will accept this operation.
 * This is also an asynchronous task.
 *
 * @param srv_id ID
 * of the server to set as a learner.
 * @param to If `true`, set the server as a learner,
 * otherwise, clear learner flag.
 * @return `ret->get_result_code()` will be OK on success.
 */
BOP_API bool bop_raft_server_flip_learner_flag(
    bop_raft_server *rs, const int32_t srv_id, const bool to,
    bop_raft_async_buffer_ptr *handler
) {
    if (!rs)
        return false;
    if (!handler)
        return false;
    handler->set(reinterpret_cast<nuraft::raft_server *>(rs)->flip_learner_flag(srv_id, to));
    return true;
}

/**
 * Append and replicate the given logs.
 * Only leader will accept this operation.
 *
 * @param
 * logs Set of logs to replicate.
 * @return
 *     In blocking mode, it will be blocked during
 * replication, and
 *     return `cmd_result` instance which contains the commit results from
 *
 * the state machine.
 *     In async mode, this function will return immediately, and the
 * commit
 * results will be set to returned `cmd_result` instance later.
 */
BOP_API bool bop_raft_server_append_entries(
    bop_raft_server *rs, bop_raft_append_entries_ptr *entries,
    bop_raft_async_buffer_ptr *handler
) {
    if (!rs || !entries)
        return false;
    if (!handler) {
        reinterpret_cast<nuraft::raft_server *>(rs)->append_entries(std::move(entries->logs));
    } else {
        handler->set(
            reinterpret_cast<nuraft::raft_server *>(rs)->append_entries(std::move(entries->logs))
        );
    }
    return true;
}

/**
 * Update the priority of given server.
 *
 * @param rs local nuraft::raft_server instance
 *
 * @param srv_id ID of server to update priority.
 * @param new_priority
 *     Priority value,
 * greater than or equal to 0.
 *     If priority is set to 0, this server will never be a leader.

 * * @param broadcast_when_leader_exists
 *     If we're not a leader and a leader exists, broadcast
 * priority change to other
 *     peers. If false, set_priority does nothing. Please note that
 * setting this
 *     option to true may possibly cause cluster config to diverge.
 * @return SET
 * If we're a leader and we have committed priority change.
 * @return BROADCAST
 *     If either
 * there's no live leader now, or we're a leader and we want to set our
 *     priority to 0, or
 * we're not a leader and broadcast_when_leader_exists = true.
 *     We have sent messages to other
 * peers about priority change but haven't
 *     committed this change.
 * @return IGNORED If we're
 * not a leader and broadcast_when_leader_exists = false. We
 *     ignored the request.
 */
BOP_API bop_raft_server_priority_set_result bop_raft_server_set_priority(
    bop_raft_server *rs,
    const int32_t srv_id,
    const int32_t new_priority,
    bool broadcast_when_leader_exists
) {
    auto result = reinterpret_cast<nuraft::raft_server *>(rs)->set_priority(
        srv_id, new_priority, broadcast_when_leader_exists
    );
    switch (result) {
        case nuraft::raft_server::PrioritySetResult::SET:
            return bop_raft_server_priority_set_result_set;
        case nuraft::raft_server::PrioritySetResult::BROADCAST:
            return bop_raft_server_priority_set_result_broadcast;
        case nuraft::raft_server::PrioritySetResult::IGNORED:
            return bop_raft_server_priority_set_result_ignored;
    }
    return bop_raft_server_priority_set_result_ignored;
}

/**
 * Broadcast the priority change of given server to all peers.
 * This function should be used
 * only when there is no live leader
 * and leader election is blocked by priorities of live
 * followers.
 * In that case, we are not able to change priority by using
 * normal `set_priority`
 * operation.
 *
 * @param rs local nuraft::raft_server instance
 * @param srv_id ID of server to
 * update priority.
 * @param new_priority New priority.
 */
BOP_API void bop_raft_server_broadcast_priority_change(
    bop_raft_server *rs,
    const int32_t srv_id,
    const int32_t new_priority
) {
    reinterpret_cast<nuraft::raft_server *>(rs)->broadcast_priority_change(srv_id, new_priority);
}

/**
 * Yield current leadership and becomes a follower. Only a leader
 * will accept this
 * operation.
 *
 * If given `immediate_yield` flag is `true`, it will become a
 * follower
 * immediately. The subsequent leader election will be
 * totally random so that there is always a
 * chance that this
 * server becomes the next leader again.
 *
 * Otherwise, this server will pause
 * write operations first, wait
 * until the successor (except for this server) finishes the
 *
 * catch-up of the latest log, and then resign. In such a case,
 * the next leader will be much more
 * predictable.
 *
 * Users can designate the successor. If not given, this API will
 *
 * automatically choose the highest priority server as a successor.
 *
 * @param rs local
 * nuraft::raft_server instance
 * @param immediate_yield If `true`, yield immediately.
 * @param
 * successor_id The server ID of the successor.
 *                     If `-1`, the successor will
 * be chosen
 *                     automatically.
 */
BOP_API void
bop_raft_server_yield_leadership(
    bop_raft_server *rs,
    bool immediate_yield,
    int32_t successor_id
) {
    reinterpret_cast<nuraft::raft_server *>(rs)->yield_leadership(
        immediate_yield, static_cast<int>(successor_id)
    );
}

/**
 * Send a request to the current leader to yield its leadership,
 * and become the next leader.

 * *
 * @return `true` on success. But it does not guarantee to become
 *         the next leader
 * due to various failures.
 */
BOP_API bool bop_raft_server_request_leadership(bop_raft_server *rs) {
    return reinterpret_cast<nuraft::raft_server *>(rs)->request_leadership();
}

/**
 * Start the election timer on this server, if this server is a follower.
 * It will allow the
 * election timer permanently, if it was disabled
 * by state manager.
 */
BOP_API void bop_raft_server_restart_election_timer(bop_raft_server *rs) {
    reinterpret_cast<nuraft::raft_server *>(rs)->restart_election_timer();
}

/**
 * Set custom context to Raft cluster config.
 * It will create a new configuration log and
 * replicate it.
 *
 * @param ctx Custom context.
 */
BOP_API void bop_raft_server_set_user_ctx(bop_raft_server *rs, const char *data, size_t size) {
    const std::string_view view(data, size);
    const std::string ctx(view);
    reinterpret_cast<nuraft::raft_server *>(rs)->set_user_ctx(ctx);
}

/**
 * Get custom context from the current cluster config.
 *
 * @return Custom context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_user_ctx(bop_raft_server *rs) {
    auto ctx = reinterpret_cast<nuraft::raft_server *>(rs)->get_user_ctx();
    auto buf = bop_raft_buffer_new(ctx.size());
    reinterpret_cast<nuraft::buffer *>(buf)->put(ctx);
    reinterpret_cast<nuraft::buffer *>(buf)->pos(0);
    return buf;
}

/**
* Get timeout for snapshot_sync_ctx
*
* @return snapshot_sync_ctx_timeout.
*/
BOP_API int32_t bop_raft_server_get_snapshot_sync_ctx_timeout(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_snapshot_sync_ctx_timeout();
}

/**
 * Get ID of this server.
 *
 * @return Server ID.
 */
BOP_API int32_t bop_raft_server_get_id(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_snapshot_sync_ctx_timeout();
}

/**
 * Get the current term of this server.
 *
 * @return Term.
 */
BOP_API uint64_t bop_raft_server_get_term(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_term();
}

/**
 * Get the term of given log index number.
 *
 * @param log_idx Log index number
 * @return Term
 * of given log.
 */
BOP_API uint64_t bop_raft_server_get_log_term(const bop_raft_server *rs, uint64_t log_idx) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_log_term(log_idx);
}

/**
 * Get the term of the last log.
 *
 * @return Term of the last log.
 */
BOP_API uint64_t bop_raft_server_get_last_log_term(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_last_log_term();
}

/**
 * Get the last log index number.
 *
 * @return Last log index number.
 */
BOP_API uint64_t bop_raft_server_get_last_log_idx(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_last_log_idx();
}

/**
 * Get the last committed log index number of state machine.
 *
 * @return Last committed log
 * index number of state machine.
 */
BOP_API uint64_t bop_raft_server_get_committed_log_idx(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_committed_log_idx();
}

/**
 * Get the target log index number we are required to commit.
 *
 * @return Target committed log
 * index number.
 */
BOP_API uint64_t bop_raft_server_get_target_committed_log_idx(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_target_committed_log_idx();
}

/**
 * Get the leader's last committed log index number.
 *
 * @return The leader's last committed
 * log index number.
 */
BOP_API uint64_t bop_raft_server_get_leader_committed_log_idx(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_leader_committed_log_idx();
}

/**
 * Get the log index of the first config when this server became a leader.
 * This API can be
 * used for checking if the state machine is fully caught up
 * with the latest log after a leader
 * election, so that the new leader can
 * guarantee strong consistency.
 *
 * It will return 0 if
 * this server is not a leader.
 *
 * @return The log index of the first config when this server
 * became a leader.
 */
BOP_API uint64_t bop_raft_server_get_log_idx_at_becoming_leader(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_log_idx_at_becoming_leader();
}

/**
 * Calculate the log index to be committed
 * from current peers' matched indexes.
 *
 * @return
 * Expected committed log index.
 */
BOP_API uint64_t bop_raft_server_get_expected_committed_log_idx(bop_raft_server *rs) {
    return reinterpret_cast<nuraft::raft_server *>(rs)->get_expected_committed_log_idx();
}

/**
 * Get the current Raft cluster config.
 *
 * @param rs raft server instance
 * @param
 * cluster_config Wrapper for holding nuraft::ptr<nuraft::cluster_config>
 */
BOP_API void
bop_raft_server_get_config(const bop_raft_server *rs, bop_raft_cluster_config_ptr *cluster_config) {
    if (!rs)
        return;
    if (!cluster_config)
        return;
    cluster_config->config = reinterpret_cast<const nuraft::raft_server *>(rs)->get_config();
}

/**
 * Get data center ID of the given server.
 *
 * @param srv_id Server ID.
 * @return -1 if given
 * server ID does not exist.
 *          0 if data center ID was not assigned.
 */
BOP_API int32_t bop_raft_server_get_dc_id(const bop_raft_server *rs, int32_t srv_id) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_dc_id(srv_id);
}

/**
 * Get auxiliary context stored in the server config.
 *
 * @param srv_id Server ID.
 * @return
 * Auxiliary context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_aux(const bop_raft_server *rs, int32_t srv_id) {
    auto aux = reinterpret_cast<const nuraft::raft_server *>(rs)->get_aux(srv_id);
    auto buf = bop_raft_buffer_new(aux.size());
    reinterpret_cast<nuraft::buffer *>(buf)->put_raw(
        reinterpret_cast<uint8_t *>(aux.data()), aux.size()
    );
    return buf;
}

/**
 * Get the ID of current leader.
 *
 * @return Leader ID
 *         -1 if there is no live
 * leader.
 */
BOP_API int32_t bop_raft_server_get_leader(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_leader();
}

/**
 * Check if this server is leader.
 *
 * @return `true` if it is leader.
 */
BOP_API bool bop_raft_server_is_leader(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_leader();
}

/**
 * Check if there is live leader in the current cluster.
 *
 * @return `true` if live leader
 * exists.
 */
BOP_API bool bop_raft_server_is_leader_alive(const bop_raft_server *rs) {
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_leader_alive();
}

/**
 * Get the configuration of given server.
 *
 * @param srv_id Server ID.
 * @return Server
 * configuration.
 */
BOP_API void bop_raft_server_get_srv_config(
    const bop_raft_server *rs, bop_raft_srv_config_ptr *svr_config, int32_t srv_id
) {
    if (!rs)
        return;
    if (!svr_config)
        return;
    svr_config->config = reinterpret_cast<const nuraft::raft_server *>(rs)->get_srv_config(srv_id);
}

/**
 * Get the configuration of all servers.
 *
 * @param[out] configs_out Set of server
 * configurations.
 */
BOP_API void bop_raft_server_get_srv_config_all(
    const bop_raft_server *rs, bop_raft_srv_config_vec *configs_out
) {
    if (!rs)
        return;
    if (!configs_out)
        return;
    reinterpret_cast<const nuraft::raft_server *>(rs)->get_srv_config_all(configs_out->configs);
}

/**
 * Update the server configuration, only leader will accept this operation.
 * This function
 * will update the current cluster config
 * and replicate it to all peers.
 *
 * We don't allow
 * changing multiple server configurations at once,
 * due to safety reason.
 *
 * Change on
 * endpoint will not be accepted (should be removed and then re-added).
 * If the server is in new
 * joiner state, it will be rejected.
 * If the server ID does not exist, it will also be rejected.

 * *
 * @param new_config Server configuration to update.
 * @return `true` on success, `false` if
 * rejected.
 */
BOP_API void
bop_raft_server_update_srv_config(bop_raft_server *rs, bop_raft_srv_config_ptr *new_config) {
    if (!rs)
        return;
    if (!new_config)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->update_srv_config(*new_config->config);
}

/**
 * Get the peer info of the given ID. Only leader will return peer info.
 *
 * @param srv_id
 * Server ID.
 * @return Peer info.
 */
BOP_API bool bop_raft_server_get_peer_info(
    bop_raft_server *rs, int32_t srv_id, bop_raft_server_peer_info *peer
) {
    if (!rs)
        return false;
    if (!peer)
        return false;
    auto info = reinterpret_cast<nuraft::raft_server *>(rs)->get_peer_info(srv_id);
    peer->id = info.id_;
    peer->last_log_idx = info.last_log_idx_;
    peer->last_succ_resp_us = info.last_succ_resp_us_;
    return info.id_ != -1;
}

/**
 * Get the info of all peers. Only leader will return peer info.
 *
 * @return Vector of peer
 * info.
 */
BOP_API void bop_raft_server_get_peer_info_all(
    const bop_raft_server *rs, bop_raft_server_peer_info_vec *peers_out
) {
    if (!rs)
        return;
    if (!peers_out)
        return;
    peers_out->peers = reinterpret_cast<const nuraft::raft_server *>(rs)->get_peer_info_all();
}

/**
 * Shut down server instance.
 */
BOP_API void bop_raft_server_shutdown(bop_raft_server *rs) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->shutdown();
}

/**
 *  Start internal background threads, initialize election
 */
BOP_API void bop_raft_server_start_server(bop_raft_server *rs, bool skip_initial_election_timeout) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->start_server(skip_initial_election_timeout);
}

/**
 * Stop background commit thread.
 */
BOP_API void bop_raft_server_stop_server(bop_raft_server *rs) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->stop_server();
}

/**
 * Send reconnect request to leader.
 * Leader will re-establish the connection to this server
 * in a few seconds.
 * Only follower will accept this operation.
 */
BOP_API void bop_raft_server_send_reconnect_request(bop_raft_server *rs) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->send_reconnect_request();
}

/**
 * Update Raft parameters.
 *
 * @param new_params Parameters to set.
 */
BOP_API void bop_raft_server_update_params(bop_raft_server *rs, bop_raft_params *params) {
    if (!rs)
        return;
    if (!params)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->update_params(
        *reinterpret_cast<nuraft::raft_params *>(params)
    );
}

/**
 * Get the current Raft parameters.
 * Returned instance is the clone of the original one,
 * so
 * that user can modify its contents.
 *
 * @return Clone of Raft parameters.
 */
BOP_API void
bop_raft_server_get_current_params(const bop_raft_server *rs, bop_raft_params *params) {
    if (!rs)
        return;
    if (!params)
        return;
    auto current_params = reinterpret_cast<const nuraft::raft_server *>(rs)->get_current_params();
    *params = *reinterpret_cast<bop_raft_params *>(&current_params);
}

/**
 * Get the counter number of given stat name.
 *
 * @param name Stat name to retrieve.
 *
 * @return Counter value.
 */
BOP_API uint64_t bop_raft_server_get_stat_counter(bop_raft_server *rs, bop_raft_counter *counter) {
    if (!rs)
        return 0;
    if (!counter)
        return 0;
    return reinterpret_cast<nuraft::raft_server *>(rs)->get_stat_counter(counter->name);
}

/**
 * Get the gauge number of given stat name.
 *
 * @param name Stat name to retrieve.
 * @return
 * Gauge value.
 */
BOP_API int64_t bop_raft_server_get_stat_gauge(bop_raft_server *rs, bop_raft_gauge *gauge) {
    if (!rs)
        return 0;
    if (!gauge)
        return 0;
    return reinterpret_cast<nuraft::raft_server *>(rs)->get_stat_gauge(gauge->name);
}

/**
 * Get the histogram of given stat name.
 *
 * @param name Stat name to retrieve.
 * @param[out]
 * histogram_out
 *     Histogram as a map. Key is the upper bound of a bucket, and
 *     value is
 * the counter of that bucket.
 * @return `true` on success.
 *         `false` if stat does not
 * exist, or is not histogram type.
 */
BOP_API bool
bop_raft_server_get_stat_histogram(bop_raft_server *rs, bop_raft_histogram *histogram) {
    if (!rs)
        return false;
    if (!histogram)
        return false;
    return reinterpret_cast<nuraft::raft_server *>(rs)->get_stat_histogram(
        histogram->name, histogram->value
    );
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_counter(bop_raft_server *rs, bop_raft_counter *counter) {
    if (!rs)
        return;
    if (!counter)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->reset_stat(counter->name);
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_gauge(bop_raft_server *rs, bop_raft_gauge *gauge) {
    if (!rs)
        return;
    if (!gauge)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->reset_stat(gauge->name);
}

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_histogram(bop_raft_server *rs, bop_raft_histogram *histogram) {
    if (!rs)
        return;
    if (!histogram)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->reset_stat(histogram->name);
}

/**
 * Reset all existing stats to zero.
 */
BOP_API void bop_raft_server_reset_all_stats(bop_raft_server *rs) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->reset_all_stats();
}

/**
 * Set a custom callback function for increasing term.
 */
BOP_API void bop_raft_server_set_inc_term_func(
    bop_raft_server *rs, void *user_data, bop_raft_inc_term_func func
) {
    if (!rs)
        return;
    if (!func)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->set_inc_term_func(
        [user_data, func](uint64_t current_term) { return func(user_data, current_term); }
    );
}

/**
 * Pause the background execution of the state machine.
 * If an operation execution is
 * currently happening, the state
 * machine may not be paused immediately.
 *
 * @param timeout_ms
 * If non-zero, this function will be blocked until
 *                   either it completely pauses
 * the state machine execution
 *                   or reaches the given time limit in
 * milliseconds.
 *                   Otherwise, this function will return immediately, and
 * there
 * is a possibility that the state machine execution
 *                   is still happening.
 */
BOP_API void bop_raft_server_pause_state_machine_execution(bop_raft_server *rs, size_t timeout_ms) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->pause_state_machine_execution(timeout_ms);
}

/**
 * Resume the background execution of state machine.
 */
BOP_API void bop_raft_server_resume_state_machine_execution(bop_raft_server *rs) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->resume_state_machine_execution();
}

/**
 * Check if the state machine execution is paused.
 *
 * @return `true` if paused.
 */
BOP_API bool bop_raft_server_is_state_machine_execution_paused(const bop_raft_server *rs) {
    if (!rs)
        return false;
    return reinterpret_cast<const nuraft::raft_server *>(rs)->is_state_machine_execution_paused();
}

/**
 * Block the current thread and wake it up when the state machine
 * execution is paused.
 *
 *
 * @param timeout_ms If non-zero, wake up after the given amount of time
 *                   even
 * though the state machine is not paused yet.
 * @return `true` if the state machine is paused.
 */
BOP_API bool bop_raft_server_wait_for_state_machine_pause(bop_raft_server *rs, size_t timeout_ms) {
    if (!rs)
        return false;
    return reinterpret_cast<nuraft::raft_server *>(rs)->wait_for_state_machine_pause(timeout_ms);
}

/**
 * (Experimental)
 * This API is used when `raft_params::parallel_log_appending_` is set.
 *
 * Everytime an asynchronous log appending job is done, users should call
 * this API to notify Raft
 * server to handle the log.
 * Note that calling this API once for multiple logs is acceptable
 *
 * and recommended.
 *
 * @param ok `true` if appending succeeded.
 */
BOP_API void bop_raft_server_notify_log_append_completion(bop_raft_server *rs, bool ok) {
    if (!rs)
        return;
    reinterpret_cast<nuraft::raft_server *>(rs)->notify_log_append_completion(ok);
}

/**
 * Manually create a snapshot based on the latest committed
 * log index of the state machine.

 * *
 * Note that snapshot creation will fail immediately if the previous
 * snapshot task is still
 * running.
 *
 * @params options Options for snapshot creation.
 * @return Log index number of the
 * created snapshot or`0` if failed.
 */
BOP_API uint64_t bop_raft_server_create_snapshot(
    bop_raft_server *rs,
    /**
     * If `true`, the background commit will be blocked until `create_snapshot`
     *
       returns. However, it will not block the commit for the entire duration
     * of the snapshot
       creation process, as long as your state machine creates
     * the snapshot asynchronously.

       *
     * The purpose of this flag is to ensure that the log index used for
     * the
       snapshot creation is the most recent one.
     */
    bool serialize_commit
) {
    if (!rs)
        return 0;
    nuraft::raft_server::create_snapshot_options options;
    options.serialize_commit_ = serialize_commit;
    return reinterpret_cast<nuraft::raft_server *>(rs)->create_snapshot(options);
}

/**
 * Manually and asynchronously create a snapshot on the next earliest
 * available commited log
 * index.
 *
 * Unlike `create_snapshot`, if the previous snapshot task is running,
 * it will wait
 * until the previous task is done. Once the snapshot
 * creation is finished, it will be notified
 * via the returned
 * `cmd_result` with the log index number of the snapshot.
 *
 * @return
 * `cmd_result` instance.
 *         `nullptr` if there is already a scheduled snapshot creation.

 */
BOP_API void bop_raft_server_schedule_snapshot_creation(
    bop_raft_server *rs, bop_raft_async_uint64_ptr *result_handler
) {
    if (!rs)
        return;
    result_handler->set(reinterpret_cast<nuraft::raft_server *>(rs)->schedule_snapshot_creation());
}

/**
 * Get the log index number of the last snapshot.
 *
 * @return Log index number of the last
 * snapshot.
 *         `0` if snapshot does not exist.
 */
BOP_API uint64_t bop_raft_server_get_last_snapshot_idx(const bop_raft_server *rs) {
    if (!rs)
        return 0;
    return reinterpret_cast<const nuraft::raft_server *>(rs)->get_last_snapshot_idx();
}

BOP_API void bop_raft_cb_get_req_msg(bop_raft_cb_req_resp *req_resp, bop_raft_cb_req_msg *req_msg) {
    if (!req_resp || !req_msg)
        return;
    auto req = reinterpret_cast<nuraft::cb_func::ReqResp *>(req_resp)->req;
    req_msg->term = req->get_term();
    req_msg->type = static_cast<bop_raft_msg_type>(req->get_type());
    req_msg->src = req->get_src();
    req_msg->dst = req->get_dst();
    req_msg->last_log_term = req->get_last_log_term();
    req_msg->last_log_idx = req->get_last_log_idx();
    req_msg->commit_idx = req->get_commit_idx();
}

BOP_API size_t bop_raft_cb_get_req_msg_entries_size(bop_raft_cb_req_resp *req_resp) {
    if (!req_resp)
        return 0;
    auto req = reinterpret_cast<nuraft::cb_func::ReqResp *>(req_resp)->req;
    if (!req)
        return 0;
    return req->log_entries().size();
}

BOP_API bop_raft_log_entry *
bop_raft_cb_get_req_msg_get_entry(bop_raft_cb_req_resp *req_resp, size_t idx) {
    if (!req_resp)
        return 0;
    auto req = reinterpret_cast<nuraft::cb_func::ReqResp *>(req_resp)->req;
    if (!req)
        return 0;
    auto size = req->log_entries().size();
    if (idx >= size)
        return nullptr;
    return reinterpret_cast<bop_raft_log_entry *>(req->log_entries()[idx].get());
}

BOP_API void
bop_raft_cb_get_resp_msg(bop_raft_cb_req_resp *req_resp, bop_raft_cb_resp_msg *resp_msg) {
    if (!req_resp || !resp_msg)
        return;
    auto resp = reinterpret_cast<nuraft::cb_func::ReqResp *>(req_resp)->resp.get();
    if (!resp)
        return;
    resp_msg->term = resp->get_term();
    resp_msg->type = static_cast<bop_raft_msg_type>(resp->get_type());
    resp_msg->src = resp->get_src();
    resp_msg->dst = resp->get_dst();
    resp_msg->next_idx = resp->get_next_idx();
    resp_msg->next_batch_size_hint_in_bytes = resp->get_next_batch_size_hint_in_bytes();
    resp_msg->accepted = resp->get_accepted();
    resp_msg->ctx = reinterpret_cast<bop_raft_buffer *>(resp->get_ctx().get());
    resp_msg->peer = reinterpret_cast<bop_raft_cb_resp_peer *>(resp->get_peer().get());
    resp_msg->result_code = static_cast<bop_raft_cmd_result_code>(resp->get_result_code());
}
}


#include <libnuraft/nuraft.hxx>
#include <tracer.hxx>
#include <expected>
#include <filesystem>
#include <format>
#include <memory>
#include <mutex>
#include "./raft.h"

extern "C" {
#include <mdbx.h>
}

template<typename F>
struct privDefer {
    F f;

    explicit privDefer(F f) : f(f) {
    }

    ~privDefer() {
        f();
    }
};

template<typename F>
privDefer<F> defer_func(F f) {
    return privDefer<F>(f);
}

#define DEFER_1(x, y) x##y
#define DEFER_2(x, y) DEFER_1(x, y)
#define DEFER_3(x) DEFER_2(x, __COUNTER__)
#define DEFER(code) auto DEFER_3(_defer_) = defer_func([&]() { code; })

#if _WIN32
#define INLINE inline
#define FORCE_INLINE
#else
#define INLINE __attribute__((always_inline))
#define FORCE_INLINE INLINE inline
#endif

// #if (defined(_MSC_VER) && _MSVC_LANG>= 202002L)
// #define LIKELY(x) (x) [[likely]]
// #else
// #define LIKELY(x) (__builtin_expect(!!(x), 1))
// #endif
//
// #if (defined(_MSC_VER) && _MSVC_LANG>= 202002L)// || __cplusplus >= 202002L
// #define UNLIKELY(x) (x) [[unlikely]]
// #else
// #define UNLIKELY(x) (__builtin_expect(!!(x), 0))
// #endif

#if defined(__x86_64__)
#define X86_64
#endif
#if defined(__i386__)
#define p_i386
#endif
#if defined(__aarch64__)
#define ARM64
#endif
#if defined(__arm__)
#define ARM
#endif

#define REMOVE_COPY_CAPABILITY(clazz)                                                              \
  private:                                                                                         \
    clazz(const clazz&) = delete;                                                                  \
    clazz& operator=(const clazz&) = delete;

#define REMOVE_MOVE_CAPABILITY(clazz)                                                              \
  private:                                                                                         \
    clazz(const clazz&&) = delete;                                                                 \
    auto operator=(const clazz&&) = delete;

#define REMOVE_COPY_AND_MOVE_CAPABILITY(clazz)                                                     \
  private:                                                                                         \
    clazz(const clazz&) = delete;                                                                  \
    clazz& operator=(const clazz&) = delete;                                                       \
    clazz(const clazz&&) = delete;                                                                 \
    auto operator=(const clazz&&) = delete;


#define BISQUE_LOG(logger, severity, format_str, ...) \


#define BISQUE_TRACE(logger, format_str, ...)                                  \
if (logger && logger->get_level() >= 6) \
logger->put_details(6, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_DEBUG(format_str, ...)                                          \
if (logger && logger->get_level() >= 5) \
logger->put_details(5, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_INFO(logger, format_str, ...)                                   \
if (logger && logger->get_level() >= 4) \
logger->put_details(4, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_WARN(logger, format_str, ...)                                   \
if (logger && logger->get_level() >= 3) \
logger->put_details(3, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_ERR(logger, format_str, ...)                                    \
if (logger && logger->get_level() >= 2) \
logger->put_details(2, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_CRITICAL(logger, format_str, ...)                               \
if (logger && logger->get_level() >= 1) \
logger->put_details(1, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));

#define BISQUE_FATAL(logger, format_str, ...)                                  \
if (logger && logger->get_level() >= 1) \
logger->put_details(1, __FILE__, __func__, __LINE__, std::format(format_str __VA_OPT__(, ) __VA_ARGS__));


constexpr static uint64_t CLUSTER_CONFIG_KEY = 1;
constexpr static uint64_t SRV_STATE_KEY = 2;

static inline nuraft::ulong size_of_log_entry(nuraft::ptr<nuraft::log_entry> &entry) {
    return sizeof(nuraft::ptr<nuraft::buffer>) + sizeof(nuraft::log_entry) +
           sizeof(nuraft::ptr<nuraft::buffer>) + sizeof(nuraft::buffer) + entry->get_buf().size();
}

class Log_Store : public nuraft::log_store {
public:
    Log_Store(
        MDBX_env *env,
        MDBX_dbi dbi,
        nuraft::ptr<nuraft::logger> logger,
        uint64_t start_index,
        uint64_t last_index,
        nuraft::ptr<nuraft::log_entry> &last_entry,
        std::size_t compact_batch_size);

    ~Log_Store() override;

    REMOVE_COPY_CAPABILITY(Log_Store);

public:
    /**
     * The first available slot of the store, starts with 1
     *
     * @return Last log index number + 1
     */
    nuraft::ulong next_slot() const override;

    /**
     * The start index of the log store, at the very beginning, it must be 1.
     * However, after some compact actions, this could be anything equal to or
     * greater than or equal to one
     */
    nuraft::ulong start_index() const override;

    /**
     * The last log entry in store.
     *
     * @return If no log entry exists: a dummy constant entry with
     *         value set to null and term set to zero.
     */
    nuraft::ptr<nuraft::log_entry> last_entry() const override;

    /**
     * Append a log entry to store.
     *
     * @param entry Log entry
     * @return Log index number.
     */
    nuraft::ulong append(nuraft::ptr<nuraft::log_entry> &entry) override;

    /**
     * Overwrite a log entry at the given `index`.
     * This API should make sure that all log entries
     * after the given `index` should be truncated (if exist),
     * as a result of this function call.
     *
     * @param index Log index number to overwrite.
     * @param entry New log entry to overwrite.
     */
    void write_at(nuraft::ulong index, nuraft::ptr<nuraft::log_entry> &entry) override;

    /**
     * Invoked after a batch of logs is written as a part of
     * a single append_entries request.
     *
     * @param start The start log index number (inclusive)
     * @param cnt The number of log entries written.
     */
    void end_of_append_batch(nuraft::ulong start, nuraft::ulong cnt) override;

    /**
     * Get log entries with index [start, end).
     *
     * Return nullptr to indicate error if any log entry within the requested
     * range could not be retrieved (e.g. due to external log truncation).
     *
     * @param start The start log index number (inclusive).
     * @param end The end log index number (exclusive).
     * @return The log entries between [start, end).
     */
    nuraft::ptr<std::vector<nuraft::ptr<nuraft::log_entry> > >
    log_entries(nuraft::ulong start, nuraft::ulong end) override;

    /**
     * (Optional)
     * Get log entries with index [start, end).
     *
     * The total size of the returned entries is limited by batch_size_hint.
     *
     * Return nullptr to indicate error if any log entry within the requested
     * range could not be retrieved (e.g. due to external log truncation).
     *
     * @param start The start log index number (inclusive).
     * @param end The end log index number (exclusive).
     * @param batch_size_hint_in_bytes Total size (in bytes) of the returned
     * entries, see the detailed comment at
     *        `state_machine::get_next_batch_size_hint_in_bytes()`.
     * @return The log entries between [start, end) and limited by the total size
     *         given by the batch_size_hint_in_bytes.
     */
    nuraft::ptr<std::vector<nuraft::ptr<nuraft::log_entry> > > log_entries_ext(
        nuraft::ulong start,
        nuraft::ulong end,
        nuraft::int64 batch_size_hint_in_bytes = 0) override;

    /**
     * Get the log entry at the specified log index number.
     *
     * @param index Should be equal to or greater than 1.
     * @return The log entry or null if index >= this->next_slot().
     */
    nuraft::ptr<nuraft::log_entry> entry_at(nuraft::ulong index) override;

    /**
     * Get the term for the log entry at the specified index.
     * Suggest to stop the system if the index >= this->next_slot()
     *
     * @param index Should be equal to or greater than 1.
     * @return The term for the specified log entry, or
     *         0 if index < this->start_index().
     */
    nuraft::ulong term_at(nuraft::ulong index) override;

    /**
     * Pack the given number of log items starting from the given index.
     *
     * @param index The start log index number (inclusive).
     * @param cnt The number of logs to pack.
     * @return Packed (encoded) logs.
     */
    nuraft::ptr<nuraft::buffer> pack(nuraft::ulong index, nuraft::int32 cnt) override;

    /**
     * Apply the log pack to current log store, starting from index.
     *
     * @param index The start log index number (inclusive).
     * @param Packed logs.
     */
    void apply_pack(nuraft::ulong index, nuraft::buffer &pack) override;

    /**
     * Compact the log store by purging all log entries,
     * including the given log index number.
     *
     * If current maximum log index is smaller than given `last_log_index`,
     * set start log index to `last_log_index + 1`.
     *
     * @param last_log_index Log index number that will be purged up to
     * (inclusive).
     * @return `true` on success.
     */
    bool compact(nuraft::ulong last_log_index) override;

    /**
     * Compact the log store by purging all log entries,
     * including the given log index number.
     *
     * Unlike `compact`, this API allows to execute the log compaction in
     * background asynchronously, aiming at reducing the client-facing latency
     * caused by the log compaction.
     *
     * This function call may return immediately, but after this function
     * call, following `start_index` should return `last_log_index + 1` even
     * though the log compaction is still in progress. In the meantime, the
     * actual job incurring disk IO can run in background. Once the job is done,
     * `when_done` should be invoked.
     *
     * @param last_log_index Log index number that will be purged up to
     * (inclusive).
     * @param when_done Callback function that will be called after
     *                  the log compaction is done.
     */
    void compact_async(
        nuraft::ulong last_log_index,
        const nuraft::async_result<bool>::handler_type &when_done) override;

    /**
     * Synchronously flush all log entries in this log store to the backing
     * storage so that all log entries are guaranteed to be durable upon process
     * crash.
     *
     * @return `true` on success.
     */
    bool flush() override;

    /**
     * Synchronously flush all log entries in this log store to the backing
     * storage so that all log entries are guaranteed to be durable upon process
     * crash.
     *
     * @return `true` on success.
     */
    bool flush(bool sync);

    /**
     * (Experimental)
     * This API is used only when `raft_params::parallel_log_appending_` flag is
     * set. Please refer to the comment of the flag.
     *
     * @return The last durable log index.
     */
    nuraft::ulong last_durable_index() override;

    inline bool out_of_range(nuraft::ulong index) noexcept {
        return index < start_index() || index > last_durable_index();
    }

public:
    static const nuraft::ptr<nuraft::log_entry> ZERO_ENTRY;

    MDBX_env *env() const noexcept {
        return env_;
    }

    MDBX_dbi dbi() const noexcept {
        return logs_dbi_;
    }

    nuraft::ptr<nuraft::logger> &logger() noexcept {
        return logger_;
    }

private:
    nuraft::ulong last_index() const noexcept {
        return last_index_.load(std::memory_order_relaxed);
    }

protected:
    MDBX_env *env_;
    MDBX_dbi logs_dbi_;
    nuraft::ptr<nuraft::logger> logger_;
    std::atomic<nuraft::ulong> start_index_;
    std::atomic<nuraft::ulong> last_index_;
    nuraft::ptr<nuraft::log_entry> last_entry_;
    mutable std::mutex last_entry_mutex_;
    std::atomic<nuraft::ulong> last_durable_index_;
    uint64_t compact_batch_size_;
    mutable std::mutex write_mutex_;
    // std::unique_ptr<tail_cache> tail_cache_;

    std::vector<nuraft::ptr<nuraft::log_entry> > append_;
    std::vector<nuraft::ptr<nuraft::log_entry> > tail_;
    std::vector<nuraft::ptr<nuraft::log_entry> > write_buf_;
    nuraft::ulong append_start_index_;
    mutable std::mutex tail_mutex_;
    nuraft::ulong tail_start_index_;
    bool write_at_;
};

class State_Mgr : public nuraft::state_mgr {
public:
    /*
     nuraft::ptr<nuraft::srv_config> my_srv_config_;
     std::string dir_;
     std::string path_;
     std::string lck_path_;
     std::string logs_path_;
     std::string logs_lck_path_;
     MDBX_env* env_;
     MDBX_dbi dbi_;
     nuraft::ptr<spdlog::logger> logger_;
     nuraft::ptr<log_store> log_store_;
     std::mutex config_mutex_;
     nuraft::ptr<nuraft::cluster_config> cluster_config_;
     nuraft::ptr<nuraft::srv_state> srv_state_;
    */
    // State_Mgr(
    // 	nuraft::ptr<nuraft::srv_config> &my_srv_config,
    // 	std::string &dir,
    // 	std::string &path,
    // 	std::string &lck_path,
    // 	std::string &logs_path,
    // 	std::string &logs_lck_path,
    // 	MDBX_env *env,
    // 	MDBX_dbi dbi,
    // 	nuraft::ptr<nuraft::logger> logger,
    // 	nuraft::ptr<nuraft::cluster_config> &cluster_config,
    // 	nuraft::ptr<nuraft::srv_state> &srv_state,
    // 	nuraft::ptr<Log_Store> &log_store);

    State_Mgr() = default;

    ~State_Mgr() override;

public:
    [[nodiscard]] inline nuraft::ptr<nuraft::log_store> get_log_store() const {
        return log_store_;
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
    nuraft::ptr<nuraft::cluster_config> load_config() override;

    /**
     * Save given cluster config.
     *
     * @param config Cluster config to save.
     */
    void save_config(const nuraft::cluster_config &config) override;

    /**
     * Save given server state.
     *
     * @param state Server state to save.
     */
    void save_state(const nuraft::srv_state &state) override;

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
    nuraft::ptr<nuraft::srv_state> read_state() override;

    /**
     * Get instance of user-defined Raft log store.
     *
     * @param Raft log store instance.
     */
    nuraft::ptr<nuraft::log_store> load_log_store() override;

    /**
     * Get ID of this Raft server.
     *
     * @return Server ID.
     */
    nuraft::int32 server_id() override;

    /**
     * System exit handler. This function will be invoked on
     * abnormal termination of Raft server.
     *
     * @param exit_code Error code.
     */
    void system_exit(int exit_code) override;

public:
    nuraft::ptr<nuraft::srv_config> my_srv_config_{};
    std::string dir_{};
    std::string path_{};
    std::string lck_path_{};
    MDBX_env *env_{};
    MDBX_dbi dbi_{};
    nuraft::ptr<nuraft::logger> logger_{};
    nuraft::ptr<nuraft::log_store> log_store_{};
    std::mutex config_mutex_{};
    nuraft::ptr<nuraft::cluster_config> cluster_config_{};
    nuraft::ptr<nuraft::srv_state> srv_state_{};
};

constexpr static uint8_t CURRENT_VERSION = 1;

enum state_mgr_open_error {
};

static nuraft::ptr<State_Mgr> raft_mdbx_state_mgr_open(
    nuraft::ptr<nuraft::srv_config> my_srv_config,
    std::string dir,
    nuraft::ptr<nuraft::logger> logger,
    size_t size_lower,
    size_t size_now,
    size_t size_upper,
    size_t growth_step,
    size_t shrink_threshold,
    size_t pagesize,
    uint32_t flags, // MDBX_env_flags_t
    uint16_t mode, // mdbx_mode_t
    nuraft::ptr<nuraft::log_store> log_store
) {
    if (!logger) {
        return nullptr;
        // logger = std::move(bisque::get_logger("raft::state"));
    }

    if (my_srv_config == nullptr) {
        BISQUE_CRITICAL(logger, "my_srv_config was null");
        return nullptr;
    }

    if (!std::filesystem::is_directory(dir)) {
        if (std::filesystem::is_regular_file(dir)) {
            BISQUE_CRITICAL(logger, "supplied directory path is a file: {}", dir);
            return nullptr;
        }
        if (std::filesystem::is_character_file(dir)) {
            BISQUE_CRITICAL(logger, "supplied directory path is a character file: {}", dir);
            return nullptr;
        }
        if (std::filesystem::is_socket(dir)) {
            BISQUE_CRITICAL(logger, "supplied directory path is a socket: {}", dir);
            return nullptr;
        }
        if (std::filesystem::is_block_file(dir)) {
            BISQUE_CRITICAL(logger, "supplied directory path is a block file: {}", dir);
            return nullptr;
        }
        if (std::filesystem::is_fifo(dir)) {
            BISQUE_CRITICAL(logger, "supplied directory path is a fifo file: {}", dir);
            return nullptr;
        }
        if (!std::filesystem::create_directories(dir)) {
            BISQUE_CRITICAL(logger, "std::filesystem::create_directories({}) returned false", dir);
            return nullptr;
        }
        BISQUE_INFO(logger, "successfully created directories for: {}", dir);
    } else {
        BISQUE_INFO(logger, "directory found: {}", dir);
    }

    std::string state_path = (std::filesystem::path(dir) / std::filesystem::path("raft_state.db")).string();
    std::string state_lck_path =
            (std::filesystem::path(dir) / std::filesystem::path("raft_state.db-lck")).string();
    auto log_store_path = (std::filesystem::path(dir) / std::filesystem::path("raft_logs.db")).string();
    std::string log_store_lck_path =
            (std::filesystem::path(dir) / std::filesystem::path("raft_logs.db-lck")).string();

    bool state_exists = std::filesystem::exists(state_path);
    //	bool state_lck_exists = std::filesystem::exists(state_lck_path);
    //	bool logs_exists = std::filesystem::exists(logs_path);
    //	bool logs_lck_exists = std::filesystem::exists(logs_lck_path);

    MDBX_env *env;
    int err = mdbx_env_create(&env);
    if (err != MDBX_SUCCESS) {
        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }

    err = mdbx_env_set_geometry(
        env,
        static_cast<intptr_t>(size_lower),
        static_cast<intptr_t>(size_now),
        static_cast<intptr_t>(size_upper),
        static_cast<intptr_t>(growth_step),
        static_cast<intptr_t>(shrink_threshold),
        static_cast<intptr_t>(pagesize)
    );
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }

    err = mdbx_env_set_maxdbs(env, 2);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }
    err = mdbx_env_set_maxreaders(env, 64);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }

    flags = MDBX_NOSTICKYTHREADS | MDBX_NOSUBDIR | MDBX_NOMEMINIT |
            MDBX_SYNC_DURABLE | MDBX_LIFORECLAIM | MDBX_WRITEMAP;

    BISQUE_DEBUG("opening mdbx state_mgr at {}", state_path);
    err = mdbx_env_open(env, state_path.c_str(),
                        static_cast<MDBX_env_flags_t>(MDBX_NOSTICKYTHREADS | MDBX_NOSUBDIR | MDBX_NOMEMINIT |
                                                      MDBX_SYNC_DURABLE | MDBX_LIFORECLAIM | MDBX_WRITEMAP), mode);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);

        if (!state_exists) {
            mdbx_env_delete(state_path.c_str(), MDBX_ENV_JUST_DELETE);
        }

        BISQUE_CRITICAL(
            logger,
            "mdbx_env_open({}, {}, {}) err: {} reason: {}", state_path, static_cast<int>(flags),
            static_cast<int>(mode), err, mdbx_strerror(err));

        switch (err) {
            // The version of the MDBX library doesn't match
            // the version that created the database environment.
            case MDBX_VERSION_MISMATCH:
                break;

            // The environment file headers are corrupted.
            case MDBX_INVALID:
                break;

            // The directory specified by the path parameter doesn't exist.
            case MDBX_ENOFILE:
                break;

            // The user didn't have permission to access the environment
            // files.
            case MDBX_EACCESS:
                break;

            // The environment was locked by another process.
            case EAGAIN:
                break;

            // The \ref MDBX_EXCLUSIVE flag was specified and the
            // environment is in use by another process,
            // or the current process tries to open environment
            // more than once.
            case MDBX_BUSY:
                break;

            // Environment is already opened by another process,
            // but with different set of \ref MDBX_SAFE_NOSYNC,
            // \ref MDBX_UTTERLY_NOSYNC flags.
            // Or if the database is already exist and parameters
            // specified early by \ref mdbx_env_set_geometry()
            // are incompatible (i.e. different pagesize, etc).
            case MDBX_INCOMPATIBLE:
                break;

            // The \ref MDBX_RDONLY flag was specified but
            // read-write access is required to rollback
            // inconsistent state after a system crash.
            case MDBX_WANNA_RECOVERY:
                break;

            case MDBX_ENOMEM:
                break;

            // Database is too large for this process,
            // i.e. 32-bit process tries to open >4Gb database.
            case MDBX_TOO_LARGE:
                break;

            default:
                break;
        }

        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }

    MDBX_txn *tx = nullptr;
    err = mdbx_txn_begin(env, nullptr, MDBX_TXN_READWRITE, &tx);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        // return std::unexpected(mdbx_strerror(err));
        BISQUE_CRITICAL(
            logger,
            "mdbx_txn_begin() err: {} reason: {}", err, mdbx_strerror(err));
        return nullptr;
    }

    MDBX_dbi dbi;
    err = mdbx_dbi_open(tx, "state", MDBX_INTEGERKEY | MDBX_CREATE, &dbi);
    if (err != MDBX_SUCCESS) {
        mdbx_txn_abort(tx);
        mdbx_env_close(env);
        BISQUE_CRITICAL(
            logger,
            "mdbx_dbi_open() err: {} reason: {}", err, mdbx_strerror(err));
        // return std::unexpected(mdbx_strerror(err));
        return nullptr;
    }

    uint64_t id = CLUSTER_CONFIG_KEY;
    MDBX_val key;
    key.iov_base = reinterpret_cast<void *>(&id);
    key.iov_len = 8;
    MDBX_val value;

    nuraft::ptr<nuraft::cluster_config> cluster_config(nullptr);

    err = mdbx_get(tx, dbi, &key, &value);
    if (err != MDBX_SUCCESS) {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            mdbx_txn_abort(tx);
            mdbx_env_close(env);
            // return std::unexpected(mdbx_strerror(err));
            BISQUE_CRITICAL(
                logger,
                "mdbx_get() err: {} reason: {}", err, mdbx_strerror(err)
            );
            return nullptr;
        }

        // create cluster_config.
        cluster_config = nuraft::cs_new<nuraft::cluster_config>();
        cluster_config->get_servers().push_back(my_srv_config);
    } else {
        // deserialize cluster_config
        nuraft::ptr<nuraft::buffer> buf = nuraft::buffer::alloc(value.iov_len);
        memcpy((void *) buf->data(), value.iov_base, value.iov_len);
        nuraft::buffer_serializer bs(buf);
        cluster_config = nuraft::cluster_config::deserialize(bs);
    }

    id = SRV_STATE_KEY;
    key.iov_base = static_cast<void *>(&id);
    key.iov_len = 8;
    value.iov_base = nullptr;
    value.iov_len = 0;

    nuraft::ptr<nuraft::srv_state> srv_state(nullptr);

    err = mdbx_get(tx, dbi, &key, &value);
    if (err != MDBX_SUCCESS) {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            mdbx_txn_abort(tx);
            mdbx_env_close(env);
            BISQUE_CRITICAL(
                logger,
                "mdbx_get() err: {} reason: {}", err, mdbx_strerror(err)
            );
            return nullptr;
        }

        // first initialization srv_state should be a nullptr
    } else {
        // deserialize srv_state
        nuraft::ptr<nuraft::buffer> buf = nuraft::buffer::alloc(value.iov_len);
        memcpy((void *) buf->data(), value.iov_base, value.iov_len);
        srv_state = nuraft::srv_state::deserialize_v1p(*buf);
    }

    err = mdbx_txn_commit(tx);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        BISQUE_CRITICAL(
            logger,
            "mdbx_txn_commit() err: {} reason: {}", err, mdbx_strerror(err)
        );
        return nullptr;
    }

    auto state_mgr = nuraft::cs_new<State_Mgr>();
    state_mgr->my_srv_config_ = my_srv_config;
    state_mgr->dir_ = dir;
    state_mgr->path_ = state_path;
    state_mgr->lck_path_ = state_lck_path;
    state_mgr->env_ = env;
    state_mgr->dbi_ = dbi;
    state_mgr->logger_ = logger;
    state_mgr->cluster_config_ = cluster_config;
    state_mgr->srv_state_ = srv_state;
    state_mgr->log_store_ = log_store;
    return state_mgr;
}

State_Mgr::~State_Mgr() {
    BISQUE_WARN(logger_, "begin");
    log_store_ = nullptr;
    if (env_ != nullptr) {
        int err = mdbx_env_close(env_);
        env_ = nullptr;
        if (err != MDBX_SUCCESS) {
            BISQUE_CRITICAL(logger_, "mdbx_env_close failed reason: {}", mdbx_strerror(err));
        }
    }
    BISQUE_WARN(logger_, "done");
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
nuraft::ptr<nuraft::cluster_config> State_Mgr::load_config() {
    std::lock_guard guard(config_mutex_);
    return cluster_config_;
}

/**
 * Save given cluster config.
 *
 * @param config Cluster config to save.
 */
void State_Mgr::save_config(const nuraft::cluster_config &config) {
    BISQUE_TRACE(logger_, "state_mgr::save_config");

    auto serialized = config.serialize();
    auto clone = nuraft::cluster_config::deserialize(*serialized);

    MDBX_txn *tx = nullptr;
    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_READWRITE, &tx);
    if (err != MDBX_SUCCESS) {
        BISQUE_CRITICAL(logger_, "mdbx_txn_begin failure reason: {}", mdbx_strerror(err))
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        cluster_config_ = clone;
        return;
    }

    uint64_t id = CLUSTER_CONFIG_KEY;
    MDBX_val key;
    key.iov_base = static_cast<void *>(&id);
    key.iov_len = 8;
    MDBX_val value;
    value.iov_len = serialized->size();

    err = mdbx_put(tx, dbi_, &key, &value, MDBX_UPSERT | MDBX_RESERVE);
    if (err != MDBX_SUCCESS) {
        mdbx_txn_abort(tx);
        BISQUE_CRITICAL(logger_,
                        "mdbx_put(CLUSTER_CONFIG_KEY, MDBX_UPSERT|MDBX_RESERVE) failure "
                        "reason: {}",
                        mdbx_strerror(err))
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        cluster_config_ = clone;
        return;
    }

    if (value.iov_len != serialized->size()) {
        mdbx_txn_abort(tx);
        BISQUE_CRITICAL(logger_,
                        "mdbx_put(CLUSTER_CONFIG_KEY, MDBX_UPSERT|MDBX_RESERVE) reserved an "
                        "incorrect number of bytes: expected {} != {}",
                        (uint64_t)serialized->size(), (uint64_t)value.iov_len)
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        cluster_config_ = clone;
        return;
    }

    memcpy(value.iov_base, (void *) serialized->data(), value.iov_len);
    MDBX_commit_latency latency{};
    err = mdbx_txn_commit_ex(tx, &latency);
    if (err != MDBX_SUCCESS) {
        BISQUE_CRITICAL(logger_, "mdbx_txn_commit_ex failure reason: {}", mdbx_strerror(err))
    }
    std::lock_guard<std::mutex> guard(config_mutex_);
    cluster_config_ = clone;
}

/**
 * Save given server state.
 *
 * @param state Server state to save.
 */
void State_Mgr::save_state(const nuraft::srv_state &state) {
    BISQUE_TRACE(logger_, "state_mgr::save_state")

    nuraft::ptr<nuraft::buffer>
            serialized = state.serialize_v1p(CURRENT_VERSION);
    nuraft::ptr<nuraft::srv_state> clone = nuraft::srv_state::deserialize_v1p(*serialized);

    MDBX_txn *tx = nullptr;
    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_READWRITE, &tx);
    if (err != MDBX_SUCCESS) {
        BISQUE_CRITICAL(logger_, "mdbx_txn_begin failure reason: {}", mdbx_strerror(err))
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        srv_state_ = clone;
        return;
    }

    uint64_t id = SRV_STATE_KEY;
    MDBX_val key{(void *) &id, 8};
    MDBX_val value{nullptr, serialized->size()};

    err = mdbx_put(tx, dbi_, &key, &value, MDBX_UPSERT | MDBX_RESERVE);
    if (err != MDBX_SUCCESS) {
        mdbx_txn_abort(tx);
        BISQUE_CRITICAL(logger_,
                        "mdbx_put(SRV_STATE_KEY, MDBX_UPSERT|MDBX_RESERVE) failure reason: "
                        "{}",
                        mdbx_strerror(err))
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        srv_state_ = clone;
        return;
    }

    if (value.iov_len != serialized->size()) {
        mdbx_txn_abort(tx);
        BISQUE_CRITICAL(logger_,
                        "mdbx_put(SRV_STATE_KEY, MDBX_UPSERT|MDBX_RESERVE) reserved an "
                        "incorrect number of bytes: expected {} != {}",
                        (uint64_t)serialized->size(), (uint64_t)value.iov_len)
        std::lock_guard<std::mutex>
                guard(config_mutex_);
        srv_state_ = clone;
        return;
    }

    memcpy(value.iov_base, (void *) serialized->data(), value.iov_len);
    MDBX_commit_latency latency{};
    err = mdbx_txn_commit_ex(tx, &latency);
    if (err != MDBX_SUCCESS) {
        BISQUE_CRITICAL(logger_, "mdbx_txn_commit_ex failure reason: {}", mdbx_strerror(err))
    }
    std::lock_guard<std::mutex> guard(config_mutex_);
    srv_state_ = clone;
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
nuraft::ptr<nuraft::srv_state> State_Mgr::read_state() {
    std::lock_guard guard(config_mutex_);
    return srv_state_;
}

/**
 * Get instance of user-defined Raft log store.
 *
 * @param Raft log store instance.
 */
nuraft::ptr<nuraft::log_store> State_Mgr::load_log_store() {
    return log_store_;
}

/**
 * Get ID of this Raft server.
 *
 * @return Server ID.
 */
int32_t State_Mgr::server_id() {
    return my_srv_config_->get_id();
}

/**
 * System exit handler. This function will be invoked on
 * abnormal termination of Raft server.
 *
 * @param exit_code Error code.
 */
void State_Mgr::system_exit(const int exit_code) {
    BISQUE_TRACE(logger_, "system_exit({})", exit_code);
}


constexpr static size_t LOG_ENTRY_HEADER_SIZE = 24;

using log_entry_ptr = nuraft::ptr<nuraft::log_entry>;

FORCE_INLINE static void serialize_entry(log_entry_ptr &entry, void *to, size_t to_len) {
    *(nuraft::ulong *) to = entry->get_term();
    *(uint64_t *) (((uint8_t *) to) + 8) = entry->get_timestamp();
    *(uint32_t *) (((uint8_t *) to) + 16) = entry->get_crc32();
    *(uint8_t *) (((uint8_t *) to) + 20) = entry->has_crc32() ? 1 : 0;
    *(uint8_t *) (((uint8_t *) to) + 21) = (uint8_t) entry->get_val_type();
    memcpy(
        (void *) (((uint8_t *) to) + LOG_ENTRY_HEADER_SIZE),
        (void *) entry->get_buf().data(),
        entry->get_buf().size()
    );
}

FORCE_INLINE static auto deserialize_entry(void *from, size_t from_len) -> log_entry_ptr {
    nuraft::ulong term = *(nuraft::ulong *) from;
    uint64_t timestamp = *(uint64_t *) (((uint8_t *) from) + 8);
    uint32_t crc32 = *(uint32_t *) (((uint8_t *) from) + 16);
    bool has_crc32 = ((uint8_t) *(((uint8_t *) from) + 20)) > 0;
    auto t = (nuraft::log_val_type) *(((uint8_t *) from) + 21);
    // pad (uint16_t)
    nuraft::ptr<nuraft::buffer> data = nuraft::buffer::alloc(from_len - LOG_ENTRY_HEADER_SIZE);
    memcpy((void *) data->data(), (void *) (((uint8_t *) from) + LOG_ENTRY_HEADER_SIZE), data->size());
    return cs_new<nuraft::log_entry>(term, std::move(data), t, timestamp, has_crc32, crc32, true);
}

const log_entry_ptr Log_Store::ZERO_ENTRY =
        cs_new<nuraft::log_entry>(0, nuraft::buffer::alloc(0));

static nuraft::ptr<Log_Store> raft_mdbx_log_store_open(
    std::string path,
    nuraft::ptr<nuraft::logger> logger,
    intptr_t size_lower,
    intptr_t size_now,
    intptr_t size_upper,
    intptr_t growth_step,
    intptr_t shrink_threshold,
    intptr_t pagesize,
    MDBX_env_flags_t flags,
    mdbx_mode_t mode,
    std::size_t compact_batch_size
) {
    if (!logger) {
        BISQUE_CRITICAL(
            logger,
            "logger was nil"
        );
        return nullptr;
    }

    flags = MDBX_NOSTICKYTHREADS | MDBX_NOSUBDIR | MDBX_NOMEMINIT | MDBX_SYNC_DURABLE | MDBX_LIFORECLAIM |
            MDBX_WRITEMAP;

    auto dir = std::filesystem::path(path);
    auto db_path = (std::filesystem::path(dir) / std::filesystem::path("raft_logs.db"));
    auto lck_path = (std::filesystem::path(dir) / std::filesystem::path("raft_logs.db-lck"));

    bool dir_exists = std::filesystem::is_directory(dir);
    if (!dir_exists) {
        std::filesystem::create_directories(dir);
    }

    bool db_exists = std::filesystem::exists(db_path);
    bool lck_exists = std::filesystem::exists(lck_path);

    MDBX_env *env = nullptr;
    int err = mdbx_env_create(&env);

    if (err != MDBX_SUCCESS) {
        BISQUE_CRITICAL(logger, "mdbx_env_create() err: {}", mdbx_strerror(err));
        return nullptr;
    }

    err = mdbx_env_set_geometry(
        env,
        size_lower,
        size_now,
        size_upper,
        growth_step,
        shrink_threshold,
        pagesize
    );
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_env_set_geometry() err: {}", mdbx_strerror(err));
        return nullptr;
    }

    err = mdbx_env_set_maxdbs(env, 2);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_env_set_maxdbs() err: {}", mdbx_strerror(err));
        return nullptr;
    }
    err = mdbx_env_set_maxreaders(env, 64);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_env_set_maxreaders() err: {}", mdbx_strerror(err));
        return nullptr;
    }

    BISQUE_DEBUG("mdbx log_store opening at {}", db_path.string());
    err = mdbx_env_open(env, db_path.string().c_str(), flags, mode);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);

        if (!db_exists) {
            mdbx_env_delete(db_path.string().c_str(), MDBX_ENV_JUST_DELETE);
        }
        BISQUE_CRITICAL(
            logger,
            "mdbx_env_open({}, {}, {}) err: {} reason: {}",
            db_path.string(),
            static_cast<int>(flags),
            static_cast<int>(mode),
            err,
            mdbx_strerror(err)
        );

        switch (err) {
            // The version of the MDBX library doesn't match
            // the version that created the database environment.
            case MDBX_VERSION_MISMATCH: break;

            // The environment file headers are corrupted.
            case MDBX_INVALID: break;

            // The directory specified by the path parameter
            // doesn't exist.
            case MDBX_ENOFILE: break;

            // The user didn't have permission to access the
            // environment files.
            case MDBX_EACCESS: break;

            // The environment was locked by another process.
            case EAGAIN: break;

            // The \ref MDBX_EXCLUSIVE flag was specified and the
            // environment is in use by another process,
            // or the current process tries to open environment
            // more than once.
            case MDBX_BUSY: break;

            // Environment is already opened by another process,
            // but with different set of \ref MDBX_SAFE_NOSYNC,
            // \ref MDBX_UTTERLY_NOSYNC flags.
            // Or if the database is already exist and parameters
            // specified early by \ref mdbx_env_set_geometry()
            // are incompatible (i.e. different pagesize, etc).
            case MDBX_INCOMPATIBLE: break;

            // The \ref MDBX_RDONLY flag was specified but
            // read-write access is required to rollback
            // inconsistent state after a system crash.
            case MDBX_WANNA_RECOVERY: break;

            case MDBX_ENOMEM: break;

            // Database is too large for this process,
            // i.e. 32-bit process tries to open >4Gb database.
            case MDBX_TOO_LARGE: break;
        }

        return nullptr;
    }

    MDBX_txn *tx = nullptr;
    err = mdbx_txn_begin(env, nullptr, MDBX_TXN_READWRITE, &tx);
    if (err != MDBX_SUCCESS) {
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_txn_begin() err: {} reason: {}", err, mdbx_strerror(err));
        return nullptr;
    }

    MDBX_dbi dbi = 0;
    err = mdbx_dbi_open(tx, "logs", MDBX_INTEGERKEY | MDBX_CREATE, &dbi);
    if (err != MDBX_SUCCESS) {
        mdbx_txn_abort(tx);
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_dbi_open() err: {} reason: {}", err, mdbx_strerror(err));
        return nullptr;
    }

    // Load first.
    MDBX_cursor *cur = nullptr;
    err = mdbx_cursor_open(tx, dbi, &cur);
    if (err != MDBX_SUCCESS) {
        mdbx_txn_abort(tx);
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_cursor_open() err: {} reason: {}", err, mdbx_strerror(err));
        return nullptr;
    }

    nuraft::ulong start_index = 0;
    nuraft::ulong last_index = 0;
    log_entry_ptr last_entry = Log_Store::ZERO_ENTRY;
    MDBX_val key;
    MDBX_val value;
    err = mdbx_cursor_get(cur, &key, &value, MDBX_FIRST);
    if (err == MDBX_SUCCESS) {
        if (key.iov_len == 8) {
            start_index = *static_cast<nuraft::ulong *>(key.iov_base);
        }

        err = mdbx_cursor_get(cur, &key, &value, MDBX_LAST);
        if (err != MDBX_SUCCESS) {
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            mdbx_env_close(env);
            BISQUE_CRITICAL(logger, "mdbx_cursor_get() err: {} reason: {}", err, mdbx_strerror(err));
            return nullptr;
        }

        if (key.iov_len == 8) {
            last_index = *static_cast<nuraft::ulong *>(key.iov_base);
        } else {
            BISQUE_CRITICAL(
                logger, "last entry key size is invalid: {} != {}", (uint64_t)key.iov_len, 8
            );
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            mdbx_env_close(env);
            return nullptr;
        }

        if (value.iov_len >= LOG_ENTRY_HEADER_SIZE) {
            last_entry = deserialize_entry(value.iov_base, value.iov_len);
        } else {
            BISQUE_CRITICAL(
                logger,
                "last entry at: {} has an invalid data size: {}",
                last_index,
                (uint64_t)value.iov_len
            );
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            mdbx_env_close(env);
            return nullptr;
        }
        mdbx_cursor_close(cur);
        err = mdbx_txn_abort(tx);
        if (err != MDBX_SUCCESS) {
            mdbx_env_close(env);
            BISQUE_CRITICAL(logger, "mdbx_txn_abort() err: {} reason: {}", err, mdbx_strerror(err));
            return nullptr;
        }
    } else if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) {
        start_index = 0;

        err = mdbx_txn_commit(tx);
        if (err != MDBX_SUCCESS) {
            mdbx_env_close(env);
            BISQUE_CRITICAL(logger, "mdbx_txn_commit() err: {} reason: {}", err, mdbx_strerror(err));
            return nullptr;
        }
    } else {
        mdbx_cursor_close(cur);
        mdbx_txn_abort(tx);
        mdbx_env_close(env);
        BISQUE_CRITICAL(logger, "mdbx_txn_begin() err: {} reason: {}", err, mdbx_strerror(MDBX_CORRUPTED));
        return nullptr;
    }

    return cs_new<Log_Store>(
        env,
        dbi,
        std::move(logger),
        (uint64_t) start_index,
        (uint64_t) last_index,
        last_entry,
        compact_batch_size
    );
}

Log_Store::Log_Store(
    MDBX_env *env, MDBX_dbi dbi, nuraft::ptr<nuraft::logger> logger, uint64_t start_index,
    uint64_t last_index, log_entry_ptr &last_entry, size_t compact_batch_size
)
    : env_(env),
      logs_dbi_(dbi),
      logger_(std::move(logger)),
      start_index_{start_index},
      last_index_{last_index},
      last_entry_(last_entry),
      compact_batch_size_(compact_batch_size),
      append_start_index_{0},
      tail_start_index_{0},
      write_at_{false} {
    last_durable_index_.store(last_index, std::memory_order_relaxed);
    tail_.reserve(4096);
    append_.reserve(4096);
}

Log_Store::~Log_Store() {
    BISQUE_WARN(logger_, "begin");
    if (env_) {
        int err = mdbx_env_close(env_);
        if (err != MDBX_SUCCESS) {
            BISQUE_ERR(logger_, "mdbx_env_close() failed reason: {}", mdbx_strerror(err));
        }
        env_ = nullptr;
    }
    BISQUE_WARN(logger_, "done");
}

/**
 * The first available slot of the store, starts with 1
 *
 * @return Last log index
 * number


 */
FORCE_INLINE auto Log_Store::next_slot() const -> nuraft::ulong {
    return last_index_.load(std::memory_order_relaxed) + 1;
}

/**
 * The start index of the log store, at the very beginning, it must be 1.
 * However,
 * after

 * * some compact actions,
 * this could be anything equal to or
 * greater than one

 */
FORCE_INLINE auto Log_Store::start_index() const -> nuraft::ulong {
    return start_index_.load(std::memory_order_relaxed);
}

/**
 * The last log entry in store.
 *
 * @return If no log entry exists: a dummy
 * constant
 *
 *
 * with value set to null and
 * term set to zero.
 */
auto Log_Store::last_entry() const -> log_entry_ptr {
    std::lock_guard<std::mutex> guard(last_entry_mutex_);
    return last_entry_;
}

/**
 * Append a log entry to store.
 *
 * @param entry Log entry
 * @return Log index number.
 */
auto Log_Store::append(log_entry_ptr &entry) -> nuraft::ulong {
    nuraft::ulong index = last_index() + 1;
    BISQUE_TRACE(logger_, "append() index={}", index);

    std::lock_guard<std::mutex> lock(tail_mutex_);
    last_index_.store(index, std::memory_order_relaxed);
    // log_entry_ptr new_entry = entry;
    append_.push_back(entry);
    if (append_.size() == 1) {
        append_start_index_ = index;
    }
    return index;
}

/**
 * Overwrite a log entry at the given `index`.
 * This API should make sure that all
 * log
 *
 * entries
 * after the given
 * `index` should be truncated (if exist),
 * as a
 * result of this

 * * function call.
 *
 * @param index Log index number to
 * overwrite.
 *
 * @param entry New log

 * * entry to overwrite.
 */
void Log_Store::write_at(nuraft::ulong index, log_entry_ptr &entry) {
    BISQUE_TRACE(logger_, "write_at(index={})", index);

    std::lock_guard<std::mutex> lock(tail_mutex_);
    write_at_ = true;
    last_index_.store(index, std::memory_order_relaxed);
    append_.push_back(entry);
    if (append_.size() == 1) {
        append_start_index_ = index;
    }
}

/**
 * Invoked after a batch of logs is written as a part of
 * a single append_entries
 *
 * request.

 * *
 * @param start The
 * start log index number (inclusive)
 * @param cnt The
 *
 * number of log
 * entries written.
 */
void Log_Store::end_of_append_batch(nuraft::ulong start, nuraft::ulong cnt) {
    BISQUE_TRACE(logger_, "end_of_append_batch(start={}, cnt={})", start, cnt);

    bool is_write_at = false; {
        std::lock_guard<std::mutex> lock(tail_mutex_);
        is_write_at = write_at_;
        tail_.swap(append_);
        tail_start_index_ = append_start_index_;
        append_.clear();
        append_start_index_ = 0;
        write_at_ = false;
    }

    MDBX_txn *txn = nullptr;
    MDBX_cursor *cur = nullptr;
    MDBX_val key = {nullptr, 0};
    MDBX_val value = {nullptr, 0};

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_READWRITE, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_begin(MDBX_TXN_READWRITE) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return;
    }

    err = mdbx_cursor_open(txn, logs_dbi_, &cur);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        mdbx_txn_abort(txn);
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_open(LOGS_DBI) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return;
    }
    DEFER(mdbx_cursor_close(cur));

    if (is_write_at) {
        key.iov_base = (void *) &start;
        key.iov_len = sizeof(nuraft::ulong);
        err = mdbx_cursor_get(cur, &key, &value, MDBX_SET_LOWERBOUND);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
                mdbx_txn_abort(txn);
                BISQUE_CRITICAL(
                    logger_,
                    "mdbx_cursor_get(MDBX_LAST) err: {} "
                    "reason: {}",
                    err,
                    mdbx_strerror(err)
                );
                return;
            }
        }

        // Delete tail from "start" index.
        err = MDBX_SUCCESS;
        while (err == MDBX_SUCCESS) {
            err = mdbx_cursor_del(cur, MDBX_CURRENT);
            if (err != MDBX_SUCCESS) [[unlikely]] {
                if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
                    mdbx_txn_abort(txn);
                    BISQUE_CRITICAL(
                        logger_,
                        "mdbx_cursor_del(MDBX_CURRENT) err: {} "
                        "reason: {}",
                        err,
                        mdbx_strerror(err)
                    );
                    return;
                }
            }
        }
    }

    err = mdbx_cursor_get(cur, &key, &value, MDBX_LAST);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get(MDBX_LAST) err: {} "
                "reason: {}",
                err,
                mdbx_strerror(err)
            );
            return;
        }
    }

    // if (appending_.size() != cnt) {
    //     l_critical("appending cnt not match expected cnt: {} != {}", appending_.size(),
    //     cnt);
    // }
    for (nuraft::ulong i = 0; i < tail_.size(); i++) {
        auto entry = tail_[i];
        nuraft::ulong index = start + i;

        key.iov_base = (void *) &index;
        key.iov_len = sizeof(index);
        value.iov_len = (size_t) LOG_ENTRY_HEADER_SIZE + entry->get_buf().size();
        err = mdbx_cursor_put(cur, &key, &value, MDBX_APPEND | MDBX_RESERVE);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            last_index_.store(last_durable_index(), std::memory_order_relaxed);
            int abort_err = mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_put({}, MDBX_APPEND) with size: {} err: {} "
                "reason: {}",
                index,
                (uint64_t)value.iov_len,
                err,
                mdbx_strerror(err)
            );
            if (abort_err != MDBX_SUCCESS) {
                BISQUE_CRITICAL(
                    logger_, "mdbx_txn_abort() err: {} reason: {}", err, mdbx_strerror(err)
                );
            }
            return;
        }

        if (value.iov_len < (size_t) LOG_ENTRY_HEADER_SIZE + entry->get_buf().size()) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_put short write buffer {} expected {}",
                (uint64_t)value.iov_len,
                (uint64_t)((size_t)LOG_ENTRY_HEADER_SIZE + entry->get_buf().size())
            );
            return;
        }

        serialize_entry(entry, value.iov_base, value.iov_len);
    }

    err = mdbx_txn_commit(txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        mdbx_txn_abort(txn);
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_commit() err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return;
    } {
        // std::lock_guard<decltype(last_entry_mutex_)> guard(last_entry_mutex_);
        // last_entry_ = tail_[tail_.size() - 1];
    }

    std::lock_guard<decltype(tail_mutex_)> guard(tail_mutex_);
    tail_.clear();

    last_durable_index_.store(start + cnt - 1);
}

/**
 * Get log entries with index [start, end).
 *
 * Return nullptr to indicate error if
 * any
 * log
 * entry within the
 * requested range
 * could not be retrieved (e.g. due to
 * external
 * log
 * truncation).
 *
 * @param start The start log
 * index number
 * (inclusive).
 * @param
 * end The
 * end log index number (exclusive).
 * @return The log
 * entries between
 * [start,
 * end).
 */
auto Log_Store::log_entries(nuraft::ulong start, nuraft::ulong end)
    -> nuraft::ptr<std::vector<log_entry_ptr> > {
    BISQUE_TRACE(logger_, "log_entries(start={}, end={})", start, end);
    return log_entries_ext(start, end, 1024 * 1024 * 128);
}

/**
 * (Optional)
 * Get log entries with index [start, end).
 *
 * The total size of the
 *
 *
 * returned entries is limited by
 * batch_size_hint.
 *
 * Return nullptr to indicate
 * error if

 * * any log entry within the requested range
 * could not be
 * retrieved (e.g.
 * due to external

 * * log truncation).
 *
 * @param start The start log index number
 * (inclusive).
 * @param
 *
 * end
 * The end log index number (exclusive).
 * @param
 * batch_size_hint_in_bytes Total size (in
 * bytes)
 * of the returned
 * entries,
 * see the
 * detailed comment at
 *
 *
 * `state_machine::get_next_batch_size_hint_in_bytes()`.
 *

 * * @return The log entries between
 *
 * [start, end) and limited by the total size
 * given
 * by the
 * batch_size_hint_in_bytes.
 */
auto Log_Store::log_entries_ext(
    nuraft::ulong start, nuraft::ulong end, nuraft::int64 batch_size_hint_in_bytes
) -> nuraft::ptr<std::vector<log_entry_ptr> > {
    BISQUE_TRACE(
        logger_,
        "log_entries_ext(start={}, end={}, "
        "batch_size_hint_in_bytes={})",
        start,
        end,
        batch_size_hint_in_bytes
    );

    MDBX_txn *txn = nullptr;

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_RDONLY, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_begin(MDBX_TXN_READWRITE) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return nullptr;
    }

    DEFER(mdbx_txn_abort(txn));

    MDBX_cursor *cur;
    err = mdbx_cursor_open(txn, logs_dbi_, &cur);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_open(logs_dbi) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return nullptr;
    }
    DEFER(mdbx_cursor_close(cur));

    auto result = nuraft::cs_new<std::vector<log_entry_ptr> >();
    result->reserve(end - start);

    MDBX_val key;
    MDBX_val value;

    err = mdbx_cursor_get(cur, &key, &value, MDBX_SET_RANGE);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
            return result;
        }
        BISQUE_CRITICAL(
            logger_, "mdbx_cursor_get({}, MDBX_SET_RANGE) failed: {}", start, mdbx_strerror(err)
        );
        return nullptr;
    }

    uint64_t id = *(uint64_t *) key.iov_base;
    if (id >= end || id < start) [[unlikely]] {
        return result;
    }
    nuraft::int64 size_in_bytes = (nuraft::int64) value.iov_len;

    do {
        if (size_in_bytes + value.iov_len > (size_t) batch_size_hint_in_bytes) {
            return result;
        }
        if (value.iov_len >= LOG_ENTRY_HEADER_SIZE) [[likely]] {
            result->push_back(deserialize_entry(value.iov_base, value.iov_len));
        } else {
            BISQUE_CRITICAL(
                logger_,
                "log_entry at: {} was too small {} is not at "
                "least {}",
                id,
                value.iov_len,
                LOG_ENTRY_HEADER_SIZE
            );
        }

        err = mdbx_cursor_get(cur, &key, &value, MDBX_NEXT_NODUP);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            // Reached the end?
            if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
                return result;
            }
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_NEXT_NODUP) failed: {}",
                start,
                mdbx_strerror(err)
            );
            return result;
        }

        id = *(uint64_t *) key.iov_base;
        size_in_bytes += (nuraft::int64) value.iov_len;
    } while (id < end && size_in_bytes <= batch_size_hint_in_bytes);

    return result;
}

/**
 * Get the log entry at the specified log index number.
 *
 * @param index Should be
 * equal
 * to
 * or greater than 1.
 *
 * @return The log entry or null if index >=
 * this->next_slot().

 */
log_entry_ptr Log_Store::entry_at(nuraft::ulong index) {
    BISQUE_TRACE(logger_, "entry_at(index={})", index); {
        std::lock_guard<std::mutex> lock(tail_mutex_);
        if (index >= tail_start_index_ && tail_start_index_ > 0) {
            if (index < tail_start_index_ + tail_.size()) {
                auto result = std::move(tail_[index - tail_start_index_]);
                if (result != nullptr) {
                    return result;
                }
            }
        }
        if (index >= append_start_index_ && append_start_index_ > 0) {
            if (index < append_start_index_ + append_.size()) {
                return append_[index - append_start_index_];
            }
        }
    }

    BISQUE_TRACE(logger_, "entry_at(index={}) slow path", index);

    log_entry_ptr result = nullptr;
    MDBX_txn *txn = nullptr;

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_RDONLY, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_begin(MDBX_TXN_READWRITE) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return nullptr;
    }

    DEFER(mdbx_txn_abort(txn));

    MDBX_val key;
    key.iov_base = (void *) &index;
    key.iov_len = sizeof(nuraft::ulong);
    MDBX_val value;
    err = mdbx_get(txn, logs_dbi_, &key, &value);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            BISQUE_CRITICAL(
                logger_,
                "mdbx_get() err: {} "
                "reason: {}",
                err,
                mdbx_strerror(err)
            );
        }
        return nullptr;
    }

    if (value.iov_len >= LOG_ENTRY_HEADER_SIZE) [[likely]] {
        result = deserialize_entry(value.iov_base, value.iov_len);
    } else {
        uint64_t iov_len = (uint64_t) value.iov_len;
        BISQUE_CRITICAL(
            logger_,
            "log_entry at: {} was too small {} is not at least "
            "{}",
            index,
            iov_len,
            LOG_ENTRY_HEADER_SIZE
        );
    }

    return result;
}

/**
 * Get the term for the log entry at the specified index.
 * Suggest to stop the
 * system if
 *
 * the index >=
 * this->next_slot()
 *
 * @param index Should be equal to or
 * greater than 1.
 *

 * * @return The term for the specified log
 * entry, or
 *         0 if
 * index <
 *
 * this->start_index().
 */
nuraft::ulong Log_Store::term_at(nuraft::ulong index) {
    BISQUE_TRACE(logger_, "term_at(index={})", index); {
        std::lock_guard<decltype(last_entry_mutex_)> lock(last_entry_mutex_);
        if (index == this->last_index()) {
            if (last_entry_ != nullptr && last_entry_ != ZERO_ENTRY) {
                return last_entry_->get_term();
            }
        }
    } {
        std::lock_guard<decltype(tail_mutex_)> lock(tail_mutex_);
        if (append_.size() > 0 && append_start_index_ <= index &&
            index < append_start_index_ + append_.size()) {
            return append_[index - append_start_index_]->get_term();
        }
        if (tail_.size() > 0 && tail_start_index_ <= index &&
            index < tail_start_index_ + tail_.size()) {
            return tail_[index - tail_start_index_]->get_term();
        }
    }

    MDBX_txn *txn;
    MDBX_val key;
    MDBX_val value;

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_RDONLY, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            BISQUE_CRITICAL(
                logger_, "mdbx_txn_begin(RDONLY) error: {} {}", err, mdbx_strerror(err)
            );
        }
        return 0;
    }
    DEFER(mdbx_txn_abort(txn));

    key.iov_base = (void *) &index;
    key.iov_len = sizeof(nuraft::ulong);

    err = mdbx_get(txn, logs_dbi_, &key, &value);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) {
            BISQUE_CRITICAL(
                logger_, "mdbx_txn_begin(RDONLY) error: {} {}", err, mdbx_strerror(err)
            );
        }
        return 0;
    }

    nuraft::ulong term = value.iov_len >= 8 ? *(nuraft::ulong *) value.iov_base : (nuraft::ulong) 0;
    return term;
}

/**
 * Pack the given number of log items starting from the given index.
 *
 * @param
 * index The

 * * start log index number
 * (inclusive).
 * @param cnt The number of logs to
 * pack.
 * @return

 * * Packed (encoded) logs.
 */
nuraft::ptr<nuraft::buffer> Log_Store::pack(nuraft::ulong index, nuraft::int32 cnt) {
    BISQUE_TRACE(logger_, "pack(index={}, cnt={})", index, cnt);

    MDBX_txn *txn = nullptr;

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_RDONLY, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_begin(MDBX_TXN_READWRITE) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return nullptr;
    }

    DEFER(mdbx_txn_abort(txn));

    MDBX_cursor *cur;
    err = mdbx_cursor_open(txn, logs_dbi_, &cur);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_open(logs_dbi) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return nullptr;
    }
    DEFER(mdbx_cursor_close(cur));

    nuraft::ulong first_index = index;
    MDBX_val key;
    key.iov_base = (void *) &index;
    key.iov_len = 8;
    MDBX_val value;

    err = mdbx_cursor_get(cur, &key, &value, MDBX_SET_RANGE);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
            return nullptr;
        }
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_get({}, MDBX_SET_RANGE) failed "
            "reason: {}",
            index,
            mdbx_strerror(err)
        );
        return 0;
    }

    if (key.iov_len != 8) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_get({}, MDBX_SET_RANGE) invalid key "
            "length {} != 8",
            index,
            (uint64_t)key.iov_len
        );
        return nullptr;
    }

    index = *(nuraft::ulong *) key.iov_base;
    nuraft::int32 count = 1;
    std::size_t buf_size = value.iov_len + 4 + 4 + LOG_ENTRY_HEADER_SIZE;

    if (first_index < index) {
        BISQUE_WARN(
            logger_, "log_store::pack({}, {}) actual first index: {}", first_index, cnt, index
        )
    }

    do {
        err = mdbx_cursor_get(cur, &key, &value, MDBX_NEXT_NODUP);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
                goto after_calc;
            }
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_NEXT_NODUP) failed "
                "reason: {}",
                index,
                mdbx_strerror(err)
            );
            goto after_calc;
        }

        if (key.iov_len != 8) [[unlikely]] {
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_NEXT_NODUP) invalid "
                "key length {} != "
                "8",
                index,
                (uint64_t)key.iov_len
            );
            goto after_calc;
        }

        if (index + 1 != *(nuraft::ulong *) key.iov_base) [[unlikely]] {
            BISQUE_WARN(
                logger_,
                "log_entry gap {}-{}: mdbx_cursor_get({}, "
                "MDBX_NEXT_NODUP) "
                "next "
                "key is {}",
                index,
                *(nuraft::ulong*)key.iov_base,
                index,
                *(nuraft::ulong*)key.iov_base
            );
        }
        index = *(nuraft::ulong *) key.iov_base;
        count++;
        buf_size += value.iov_len + 4 + LOG_ENTRY_HEADER_SIZE;
    } while (count < cnt);

after_calc:
    // Reset to first index again.
    key.iov_base = (void *) &first_index;
    key.iov_len = 8;
    err = mdbx_cursor_get(cur, &key, &value, MDBX_SET_RANGE);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
            return nullptr;
        }
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_get({}, MDBX_SET_RANGE) failed "
            "reason: {}",
            index,
            mdbx_strerror(err)
        );
        return 0;
    }

    index = *(nuraft::ulong *) key.iov_base;
    count = 1;

    BISQUE_TRACE(
        logger_,
        "log_store::pack({}, {}) allocating buffer sized: {} "
        "containing {} "
        "entries",
        first_index,
        cnt,
        (uint64_t)buf_size,
        count
    );

    nuraft::ptr<nuraft::buffer> buf = nuraft::buffer::alloc(buf_size);
    nuraft::buffer_serializer ser(buf);
    ser.put_u32((uint32_t) count);
    ser.put_u32((uint32_t) value.iov_len);
    ser.put_bytes(value.iov_base, value.iov_len);

    do {
        err = mdbx_cursor_get(cur, &key, &value, MDBX_NEXT_NODUP);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
                return buf;
            }
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_NEXT_NODUP) failed "
                "reason: {}",
                index,
                mdbx_strerror(err)
            );

            return nullptr;
        }
        if (key.iov_len != 8) [[unlikely]] {
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_NEXT_NODUP) invalid "
                "key length {} != "
                "8",
                index,
                (uint64_t)key.iov_len
            );
            return buf;
        }

        if (index + 1 != *(nuraft::ulong *) key.iov_base) [[unlikely]] {
            BISQUE_WARN(
                logger_,
                "log_entry gap {}-{}: mdbx_cursor_get({}, "
                "MDBX_NEXT_NODUP) "
                "next "
                "key is {}",
                index,
                *(nuraft::ulong*)key.iov_base,
                index,
                *(nuraft::ulong*)key.iov_base
            );
        }
        ser.put_u32((uint32_t) value.iov_len);
        ser.put_bytes(value.iov_base, value.iov_len);
        index = *((nuraft::ulong *) key.iov_base);
        count++;
    } while (count < cnt);

    return buf;
}

/**
 * Apply the log pack to current log store, starting from index.
 *
 * @param index
 * The
 * start
 * log index number
 * (inclusive).
 * @param Packed logs.
 */
void Log_Store::apply_pack(nuraft::ulong index, nuraft::buffer &pack) {
    BISQUE_TRACE(logger_, "apply_pack(index={}, pack_size:{})", index, (uint64_t)pack.size());

    MDBX_txn *txn = nullptr;

    int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_RDONLY, &txn);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        BISQUE_CRITICAL(
            logger_,
            "mdbx_txn_begin(MDBX_TXN_READWRITE) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return;
    }

    MDBX_cursor *cur;
    err = mdbx_cursor_open(txn, logs_dbi_, &cur);
    if (err != MDBX_SUCCESS) [[unlikely]] {
        mdbx_txn_abort(txn);
        BISQUE_CRITICAL(
            logger_,
            "mdbx_cursor_open(logs_dbi) err: {} "
            "reason: {}",
            err,
            mdbx_strerror(err)
        );
        return;
    }
    DEFER(mdbx_cursor_close(cur));

    MDBX_val key;
    key.iov_base = (void *) &index;
    key.iov_len = 8;
    MDBX_val value;

    err = mdbx_cursor_get(cur, &key, &value, MDBX_SET_RANGE);
    if (err != MDBX_SUCCESS) [[likely]] {
        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get({}, MDBX_SET_RANGE) failed "
                "reason: {}",
                index,
                mdbx_strerror(err)
            );
            return;
        }
    } else {
        do {
            err = mdbx_cursor_del(cur, MDBX_CURRENT);
        } while (err == MDBX_SUCCESS);

        if (err != MDBX_NOTFOUND && err != MDBX_ENODATA) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_del(MDBX_CURRENT) err: {} reason: "
                "{}",
                err,
                mdbx_strerror(err)
            );
            return;
        }
    }

    nuraft::buffer_serializer bs(pack);
    if (pack.size() < 8) {
        mdbx_txn_abort(txn);
        BISQUE_CRITICAL(logger_, "log_store::apply_pack short pack buffer");
        return;
    }
    nuraft::ulong end_index = index + (nuraft::ulong) bs.get_u32();
    // uint32_t      count = 1;
    std::size_t size;
    void *entry;

    while (true) {
        if ((bs.size() - bs.pos()) < 4) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "log_store::apply_pack short buffer: {} "
                "remaining when "
                "expecting "
                "at least {}",
                (uint64_t)bs.size(),
                4
            );
            return;
        }
        size = (std::size_t) bs.get_u32();
        if ((bs.size() - bs.pos()) < size) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "log_store::apply_pack short buffer: {} "
                "remaining when "
                "expecting "
                "at least {}",
                (uint64_t)bs.size(),
                (uint64_t)size
            );
            return;
        }

        key.iov_base = (void *) &index;
        key.iov_len = 8;
        value.iov_len = size;
        err = mdbx_cursor_put(cur, &key, &value, MDBX_APPEND | MDBX_RESERVE);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_put({}, MDBX_APPEND|MDBX_RESERVE) "
                "failed reason: "
                "{}",
                index,
                mdbx_strerror(err)
            );
            return;
        }

        if (value.iov_len == size) [[likely]] {
            entry = bs.get_bytes(size);
            memcpy(key.iov_base, entry, size);
        } else {
            mdbx_txn_abort(txn);
            BISQUE_CRITICAL(
                logger_,
                "log_store::apply_pack invalid MDBX buffer size: "
                "{} != {}",
                (uint64_t)value.iov_len,
                (uint64_t)size
            );
            return;
        }

        index++;

        if (index >= end_index) {
            auto cloned = deserialize_entry(entry, size);

            MDBX_commit_latency commit_latency;
            err = mdbx_txn_commit_ex(txn, &commit_latency);

            if (err != MDBX_SUCCESS) [[unlikely]] {
                BISQUE_CRITICAL(
                    logger_, "mdbx_txn_commit err: {} reason: {}", err, mdbx_strerror(err)
                );
            } else {
                last_index_.store(index - 1, std::memory_order_relaxed);
                last_durable_index_.store(index - 1, std::memory_order_relaxed);
                std::lock_guard<std::mutex> guard(last_entry_mutex_);
                last_entry_ = cloned;
            }
            return;
        }
    }
}

/**
 * Compact the log store by purging all log entries,
 * including the given log index
 *
 *
 * number.
 *
 * If current
 * maximum log index is smaller than given `last_log_index`,

 * * set

 * * start log index to `last_log_index + 1`.
 *
 *
 * @param last_log_index Log index
 * number
 * that
 * will be purged up to (inclusive).
 * @return `true` on success.
 */
bool Log_Store::compact(nuraft::ulong last_log_index) {
    BISQUE_TRACE(logger_, "compact(last_log_index={})", last_log_index);

    auto start_index = this->start_index();

    while (start_index <= last_log_index) {
        MDBX_txn *tx = nullptr;

        int err = mdbx_txn_begin(env_, nullptr, MDBX_TXN_READWRITE, &tx);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            BISQUE_CRITICAL(
                logger_,
                "mdbx_txn_begin(MDBX_TXN_RDONLY) failed reason: "
                "{}",
                mdbx_strerror(err)
            );
            return false;
        }

        MDBX_cursor *cur = nullptr;
        err = mdbx_cursor_open(tx, logs_dbi_, &cur);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            mdbx_txn_abort(tx);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_txn_begin(MDBX_TXN_RDONLY) failed reason: "
                "{}",
                mdbx_strerror(err)
            );
            return false;
        }

        MDBX_val key;
        nuraft::ulong index = 0;
        MDBX_val value;
        err = mdbx_cursor_get(cur, &key, &value, MDBX_FIRST);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            if (err == MDBX_NOTFOUND || err == MDBX_ENODATA) [[likely]] {
                BISQUE_TRACE(
                    logger_, "mdbx_cursor_get(MDBX_FIRST) failed reason: {}", mdbx_strerror(err)
                );
                return true;
            }
            BISQUE_CRITICAL(
                logger_, "mdbx_cursor_get(MDBX_FIRST) failed reason: {}", mdbx_strerror(err)
            );
            return false;
        }

        if (key.iov_len != 8) [[unlikely]] {
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            BISQUE_CRITICAL(
                logger_,
                "mdbx_cursor_get(MDBX_FIRST) returned an invalid "
                "key_len: {} "
                "!= "
                "{}",
                (uint64_t)key.iov_len,
                8
            );
            return false;
        }

        index = *(nuraft::ulong *) key.iov_base;

        // Is the first key greater than last log index?
        if (index > last_log_index) {
            // Cancel and return
            mdbx_cursor_close(cur);
            mdbx_txn_abort(tx);
            return true;
        }

        nuraft::ulong last_index = index + compact_batch_size_ > last_log_index
                                       ? last_log_index
                                       : index + compact_batch_size_;

        // calculate remaining entries to delete with
        // last_log_index inclusive
        // (+1).
        nuraft::ulong remaining = last_index - index + 1;
        bool no_more = false;

        while (remaining > 0) {
            err = mdbx_cursor_del(cur, MDBX_CURRENT);
            if (err != MDBX_SUCCESS) [[unlikely]] {
                if (err != MDBX_ENODATA && err != MDBX_NOTFOUND) [[unlikely]] {
                    BISQUE_CRITICAL(
                        logger_,
                        "mdbx_cursor_del(MDBX_CURRENT) failed "
                        "reason: {}",
                        mdbx_strerror(err)
                    );
                    BISQUE_CRITICAL(
                        logger_,
                        "unexpected end: expected to delete {} more "
                        "entries",
                        remaining
                    );
                    remaining = 0;
                    no_more = true;
                } else {
                    BISQUE_TRACE(
                        logger_,
                        "mdbx_cursor_del(MDBX_CURRENT) failed "
                        "reason: {}",
                        mdbx_strerror(err)
                    );
                    BISQUE_TRACE(
                        logger_,
                        "unexpected end: expected to delete {} more "
                        "entries",
                        remaining
                    );
                    remaining = 0;
                    no_more = true;
                }
            } else {
                remaining--;
            }
        }

        mdbx_cursor_close(cur);
        MDBX_commit_latency commit_latency;
        err = mdbx_txn_commit_ex(tx, &commit_latency);
        if (err != MDBX_SUCCESS) [[unlikely]] {
            BISQUE_CRITICAL(logger_, "mdbx_txn_commit failed reason: {}", mdbx_strerror(err));
        }
        if (no_more) {
            start_index_.store(last_index + 1, std::memory_order_relaxed);
            return true;
        }
        start_index = last_index + 1;
        start_index_.store(start_index, std::memory_order_relaxed);
    }

    start_index_.store(start_index, std::memory_order_relaxed);
    return true;
}

/**
 * Compact the log store by purging all log entries,
 * including the given log index
 *
 *
 * number.
 *
 * Unlike
 * `compact`, this API allows to execute the log compaction in
 *
 *
 * background
 * asynchronously, aiming at reducing the
 * client-facing latency caused by
 * the

 * *
 * log compaction.
 *
 * This function call may return immediately, but after
 *
 * this
 * function
 *
 * call, following `start_index` should return `last_log_index + 1` even

 * * though
 * the log
 * compaction
 * is still in progress. In the meantime, the
 * actual job
 * incurring
 * disk IO can
 * run in background. Once the job is
 * done,
 * `when_done` should
 * be invoked.

 * *
 * @param
 * last_log_index Log index number that will be purged up to
 *
 * (inclusive).
 *
 * @param when_done
 * Callback function that will be called after
 * the log
 * compaction
 * is
 * done.
 */
void Log_Store::compact_async(
    nuraft::ulong last_log_index, const nuraft::async_result<bool>::handler_type &when_done
) {
    BISQUE_TRACE(logger_, "compact_async(last_log_index={})", last_log_index);
    // Run compact on a background thread.
    std::thread t([this, last_log_index, when_done]() {
        BISQUE_TRACE(logger_, "compact_async(last_log_index={}) thread started", last_log_index);
        bool result = false;
        nuraft::ptr<std::exception> exp(nullptr);
        try {
            result = this->compact(last_log_index);
        } catch (const std::exception &ex) { exp = nuraft::cs_new<std::exception>(ex); }
        when_done(result, exp);
    });
    t.detach();

    // bool result = false;

    // nuraft::ptr<std::exception> exp(nullptr);
    // try {
    //     result = this->compact(last_log_index);
    // } catch (const std::exception& ex) {
    //     exp = cs_new<std::exception>(ex);
    // }
    // when_done(result, exp);
}

/**
 * Synchronously flush all log entries in this log store to the backing storage
 * so
 * that
 *
 * all log entries are
 * guaranteed to be durable upon process crash.
 *
 * @return
 * `true` on
 *
 * success.
 */
bool Log_Store::flush() {
    return flush(true);
}

/**
 * Synchronously flush all log entries in this log store to the backing storage
 * so
 * that
 *
 * all log entries are
 * guaranteed to be durable upon process crash.
 *
 * @return
 * `true` on
 *
 * success.
 */
bool Log_Store::flush(bool sync) {
    BISQUE_TRACE(logger_, "flush()");
    nuraft::ulong begin = 0;
    nuraft::ulong cnt = 0; {
        std::lock_guard<std::mutex> lock(tail_mutex_);
        begin = append_start_index_;
        cnt = append_.size();
    }
    if (cnt > 0) {
        end_of_append_batch(begin, cnt);
    }
    if (sync) {
        mdbx_env_sync(env_);
    }
    return true;
}

/**
 * (Experimental)
 * This API is used only when `raft_params::parallel_log_appending_`
 * flag

 * * is set.
 * Please refer
 * to the comment of the flag.
 *
 * @return The last
 * durable log
 *
 * index.
 */
FORCE_INLINE nuraft::ulong Log_Store::last_durable_index() {
    // l_trace("last_durable_index()");
    return last_durable_index_.load(std::memory_order_relaxed);
}

extern "C" {
BOP_API bop_raft_state_mgr_ptr *bop_raft_mdbx_state_mgr_open(
    bop_raft_srv_config_ptr *my_srv_config,
    const char *dir,
    size_t dir_size,
    bop_raft_logger_ptr *logger,
    size_t size_lower,
    size_t size_now,
    size_t size_upper,
    size_t growth_step,
    size_t shrink_threshold,
    size_t pagesize,
    uint32_t flags, // MDBX_env_flags_t
    uint16_t mode, // mdbx_mode_t
    bop_raft_log_store_ptr *log_store
) {
    if (!log_store || !log_store->log_store) return nullptr;
    auto result = raft_mdbx_state_mgr_open(
        my_srv_config->config,
        std::string(dir, dir + dir_size),
        logger->logger,
        size_lower, size_now, size_upper,
        growth_step, shrink_threshold, pagesize,
        flags, mode, log_store->log_store
    );
    if (!result) {
        return nullptr;
    }
    return new bop_raft_state_mgr_ptr(result);
}

BOP_API bop_raft_log_store_ptr *bop_raft_mdbx_log_store_open(
    const char *path,
    size_t path_size,
    bop_raft_logger_ptr *logger,
    size_t size_lower,
    size_t size_now,
    size_t size_upper,
    size_t growth_step,
    size_t shrink_threshold,
    size_t pagesize,
    uint32_t flags,
    uint16_t mode,
    size_t compact_batch_size
) {
    if (!logger || !logger->logger) return nullptr;
    auto log_store = raft_mdbx_log_store_open(
        std::string(path, path_size),
        logger->logger,
        static_cast<intptr_t>(size_lower),
        static_cast<intptr_t>(size_now),
        static_cast<intptr_t>(size_upper),
        static_cast<intptr_t>(growth_step),
        static_cast<intptr_t>(shrink_threshold),
        static_cast<intptr_t>(pagesize),
        static_cast<MDBX_env_flags_t>(flags), mode, compact_batch_size
    );
    if (!log_store) return nullptr;
    return new bop_raft_log_store_ptr(log_store);
}
}
