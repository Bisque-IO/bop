#include <iostream>

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


////////////////////////////////////////////////////////////////////////////////////
/// NuRaft C API
////////////////////////////////////////////////////////////////////////////////////

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::buffer
///////////////////////////////////////////////////////////////////////////////////////////////////

BOP_API nuraft::buffer *bop_raft_buffer_alloc(const size_t size) {
    return nuraft::buffer::alloc_unique(size);
}

// Do not call this when passing to a function that takes ownership.
BOP_API void bop_raft_buffer_free(nuraft::buffer *buf) {
    delete[] reinterpret_cast<char *>(buf);
}

BOP_API unsigned char *bop_raft_buffer_data(nuraft::buffer *buf) {
    return buf->data_begin();
}

BOP_API size_t bop_raft_buffer_container_size(nuraft::buffer *buf) {
    return buf->container_size();
}

BOP_API size_t bop_raft_buffer_size(nuraft::buffer *buf) {
    return buf->size();
}

BOP_API size_t bop_raft_buffer_pos(nuraft::buffer *buf) {
    return buf->pos();
}

BOP_API void bop_raft_buffer_set_pos(nuraft::buffer *buf, size_t pos) {
    buf->pos(pos);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::snapshot
///////////////////////////////////////////////////////////////////////////////////////////////////

static nuraft::snapshot *bop_raft_snapshot_deserialize0(nuraft::buffer_serializer &bs) {
    nuraft::snapshot::type snp_type = static_cast<nuraft::snapshot::type>(bs.get_u8());
    uint64_t last_log_idx = bs.get_u64();
    uint64_t last_log_term = bs.get_u64();
    uint64_t size = bs.get_u64();
    auto last_config = nuraft::cluster_config::deserialize(bs);
    return new nuraft::snapshot(last_log_idx, last_log_term, last_config, size, snp_type);
}

BOP_API nuraft::buffer *bop_raft_snapshot_serialize(nuraft::snapshot *snapshot) {
    auto conf_buf = snapshot->get_last_config()->serialize();
    nuraft::buffer *buf = nuraft::buffer::alloc_unique(conf_buf->size() + 8 * 3 + 1);
    buf->put(snapshot->get_type());
    buf->put(snapshot->get_last_log_idx());
    buf->put(snapshot->get_last_log_term());
    buf->put(snapshot->size());
    buf->put(*conf_buf);
    buf->pos(0);
    return buf;
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

BOP_API nuraft::snapshot *bop_raft_snapshot_deserialize(nuraft::buffer *buf) {
    nuraft::buffer_serializer bs(*buf);
    return bop_raft_snapshot_deserialize0(bs);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cluster_config
///////////////////////////////////////////////////////////////////////////////////////////////////

BOP_API nuraft::buffer *bop_raft_cluster_config_serialize(nuraft::cluster_config *conf) {
    size_t sz = 2 * 8 + 4 + 1;
    std::vector<nuraft::ptr<nuraft::buffer> > srv_buffs;
    for (auto it = conf->get_servers().cbegin(); it != conf->get_servers().cend(); ++it) {
        nuraft::ptr<nuraft::buffer> buf = (*it)->serialize();
        srv_buffs.push_back(buf);
        sz += buf->size();
    }
    // For aux string.
    sz += 4;
    sz += conf->get_user_ctx().size();

    nuraft::buffer *result = nuraft::buffer::alloc_unique(sz);
    result->put(conf->get_log_idx());
    result->put(conf->get_prev_log_idx());
    result->put((nuraft::byte) (conf->is_async_replication() ? 1 : 0));
    result->put((nuraft::byte *) conf->get_user_ctx().data(), conf->get_user_ctx().size());
    result->put((nuraft::int32) conf->get_servers().size());
    for (size_t i = 0; i < srv_buffs.size(); ++i) {
        result->put(*srv_buffs[i]);
    }

    result->pos(0);
    return result;
}

nuraft::cluster_config *bop_raft_cluster_config_deserialize0(nuraft::buffer_serializer &bs) {
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

nuraft::cluster_config *bop_raft_cluster_config_deserialize(nuraft::buffer *buf) {
    nuraft::buffer_serializer bs(*buf);
    return bop_raft_cluster_config_deserialize0(bs);
}

// Log index number of current config.
BOP_API uint64_t bop_raft_cluster_config_log_idx(nuraft::cluster_config *cfg) {
    return cfg->get_log_idx();
}

// Log index number of previous config.
BOP_API uint64_t bop_raft_cluster_config_prev_log_idx(nuraft::cluster_config *cfg) {
    return cfg->get_prev_log_idx();
}

// `true` if asynchronous replication mode is on.
BOP_API bool bop_raft_cluster_config_is_async_replication(nuraft::cluster_config *cfg) {
    return cfg->is_async_replication();
}

// Custom config data given by user.
BOP_API const char *bop_raft_cluster_config_user_ctx(nuraft::cluster_config *cfg) {
    return cfg->get_user_ctx().data();
}

BOP_API size_t bop_raft_cluster_config_user_ctx_size(nuraft::cluster_config *cfg) {
    return cfg->get_user_ctx().size();
}

// Number of servers.
BOP_API size_t bop_raft_cluster_config_servers_size(nuraft::cluster_config *cfg) {
    return cfg->get_servers().size();
}

// Server config at index
BOP_API nuraft::srv_config *bop_raft_cluster_config_server(nuraft::cluster_config *cfg, int idx) {
    auto svr_cfg = cfg->get_server(idx);
    return (svr_cfg) ? svr_cfg.get() : nullptr;
}

// ID of this server, should be positive number.
BOP_API int32_t bop_raft_srv_config_id(nuraft::srv_config *cfg) {
    return cfg->get_id();
}

// ID of datacenter where this server is located. 0 if not used.
BOP_API int32_t bop_raft_srv_config_dc_id(nuraft::srv_config *cfg) {
    return cfg->get_dc_id();
}

// Endpoint (address + port).
BOP_API const char *bop_raft_srv_config_endpoint(nuraft::srv_config *cfg) {
    return cfg->get_endpoint().data();
}

// Size of Endpoint (address + port).
BOP_API size_t bop_raft_srv_config_endpoint_size(nuraft::srv_config *cfg) {
    return cfg->get_endpoint().size();
}

/**
 * Custom string given by user.
 * WARNING: It SHOULD NOT contain NULL character,
 *          as it will be stored as a C-style string.
 */
BOP_API const char *bop_raft_srv_config_aux(nuraft::srv_config *cfg) {
    return cfg->get_aux().data();
}

BOP_API size_t bop_raft_srv_config_aux_size(nuraft::srv_config *cfg) {
    return cfg->get_aux().size();
}

/**
 * `true` if this node is learner.
 * Learner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_learner(nuraft::srv_config *cfg) {
    return cfg->is_learner();
}

/**
 * `true` if this node is a new joiner, but not yet fully synced.
 * New joiner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_new_joiner(nuraft::srv_config *cfg) {
    return cfg->is_new_joiner();
}

/**
 * Priority of this node.
 * 0 will never be a leader.
 */
BOP_API int32_t bop_raft_srv_config_priority(nuraft::srv_config *cfg) {
    return cfg->get_priority();
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::logger
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_logger_t {
    nuraft::ptr<nuraft::logger> logger;

    bop_raft_logger_t(const nuraft::ptr<nuraft::logger> &logger) : logger(logger) {
    }
};

typedef void (*bop_raft_logger_put_details)(
    void *user_data,
    int level,
    const char *source_file,
    const char *func_name,
    size_t line_number,
    const char *log_line,
    size_t log_line_size
);

struct bop_raft_logger final : nuraft::logger {
    void *user_data;
    bop_raft_logger_put_details put_details_{};

    bop_raft_logger(void *user_data, bop_raft_logger_put_details put_details) : user_data(user_data),
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

BOP_API bop_raft_logger_t *bop_raft_logger_create(void *user_data, bop_raft_logger_put_details callback) {
    return new bop_raft_logger_t(std::make_shared<bop_raft_logger>(user_data, callback));
}

BOP_API void bop_raft_logger_delete(const bop_raft_logger_t *logger) {
    delete logger;
}

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::asio_service
///////////////////////////////////////////////////////////////////////////////////////////////////

typedef void (*worker_start_fn)(void *user_data, uint32_t value);

typedef void (*worker_stop_fn)(void *user_data, uint32_t value);

// typedef struct {
//     const char* key;
//     int_t     id;
//     // Add other fields as needed...
// } asio_service_meta_cb_params_t;
//
// typedef const char* (*write_req_meta_fn)(void* user_data, const asio_service_meta_cb_params_t* params);

typedef bool (*verify_sn_fn)(void *user_data, const char *data, size_t size);

typedef SSL_CTX * (*SSL_CTX_provider_fn)(void *user_data);

typedef void (*customer_resolver_response_fn)(
    void *response_impl,
    const char *v1, size_t v1_size,
    const char *v2, size_t v2_size,
    int error_code
);

typedef void (*custom_resolver_fn)(
    void *user_data,
    void *response_impl,
    const char *v1, size_t v1_size,
    const char *v2, size_t v2_size,
    customer_resolver_response_fn response
);

typedef void (*corrupted_msg_handler_fn)(
    void *user_data,
    unsigned char *header, size_t header_size,
    unsigned char *payload, size_t payload_size
);

struct bop_raft_asio_service_t {
    nuraft::ptr<nuraft::asio_service> asio_service;

    bop_raft_asio_service_t(const nuraft::ptr<nuraft::asio_service> &asio_service) : asio_service(asio_service) {
    }
};

BOP_API bop_raft_asio_service_t *bop_raft_asio_service_create(
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
    worker_start_fn worker_start,
    /**
     * Lifecycle callback function on worker thread start.
     */
    void *worker_stop_user_data,
    /**
     * Lifecycle callback function on worker thread start.
     */
    worker_stop_fn worker_stop,
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
    verify_sn_fn verify_sn,

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
    SSL_CTX_provider_fn ssl_context_provider_server,

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
    SSL_CTX_provider_fn ssl_context_provider_client,

    void *custom_resolver_user_data,

    /**
     * Custom IP address resolver. If given, it will be invoked
     * before the connection is established.
     *
     * If you want to selectively bypass some hosts, just pass the given
     * host and port to the response function as they are.
     */
    custom_resolver_fn custom_resolver,

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
    corrupted_msg_handler_fn corrupted_msg_handler,

    /**
     * If `true`,  NuRaft will use streaming mode, which allows it to send
     * subsequent requests without waiting for the response to previous requests.
     * The order of responses will be identical to the order of requests.
     */
    bool streaming_mode,

    bop_raft_logger_t *logger
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
    return new bop_raft_asio_service_t(std::make_shared<nuraft::asio_service>(opts, logger->logger));
}

BOP_API void bop_raft_asio_service_delete(const bop_raft_asio_service_t *asio_service) {
    delete asio_service;
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_params
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_params_t {
    nuraft::ptr<nuraft::raft_params> params;

    bop_raft_params_t(const nuraft::ptr<nuraft::raft_params> &params) : params(params) {
    }
};

BOP_API bop_raft_params_t *bop_raft_params_create(
    /**
     * Upper bound of election timer, in millisecond.
     */
    int election_timeout_upper_bound,

    /**
     * Lower bound of election timer, in millisecond.
     */
    int election_timeout_lower_bound,

    /**
     * Heartbeat interval, in millisecond.
     */
    int heart_beat_interval,

    /**
     * Backoff time when RPC failure happens, in millisecond.
     */
    int rpc_failure_backoff,

    /**
     * Max number of logs that can be packed in a RPC
     * for catch-up of joining an empty node.
     */
    int log_sync_batch_size,

    /**
     * Log gap (the number of logs) to stop catch-up of
     * joining a new node. Once this condition meets,
     * that newly joined node is added to peer list
     * and starts to receive heartbeat from leader.
     *
     * If zero, the new node will be added to the peer list
     * immediately.
     */
    int log_sync_stop_gap,

    /**
     * Log gap (the number of logs) to create a Raft snapshot.
     */
    int snapshot_distance,

    /**
     * (Deprecated).
     */
    int snapshot_block_size,

    /**
     * Timeout(ms) for snapshot_sync_ctx, if a single snapshot syncing request
     * exceeds this, it will be considered as timeout and ctx will be released.
     * 0 means it will be set to the default value
     * `heart_beat_interval_ * response_limit_`.
     */
    int snapshot_sync_ctx_timeout,

    /**
     * Enable randomized snapshot creation which will avoid
     * simultaneous snapshot creation among cluster members.
     * It is achieved by randomizing the distance of the
     * first snapshot. From the second snapshot, the fixed
     * distance given by snapshot_distance_ will be used.
     */
    bool enable_randomized_snapshot_creation,

    /**
     * Max number of logs that can be packed in a RPC
     * for append entry request.
     */
    int max_append_size,

    /**
     * Minimum number of logs that will be preserved
     * (i.e., protected from log compaction) since the
     * last Raft snapshot.
     */
    int reserved_log_items,

    /**
     * Client request timeout in millisecond.
     */
    int client_req_timeout,

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as fresh.
     */
    int fresh_log_gap,

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as stale.
     */
    int stale_log_gap,

    /**
     * Custom quorum size for commit.
     * If set to zero, the default quorum size will be used.
     */
    int custom_commit_quorum_size,

    /**
     * Custom quorum size for leader election.
     * If set to zero, the default quorum size will be used.
     */
    int custom_election_quorum_size,

    /**
     * Expiration time of leadership in millisecond.
     * If more than quorum nodes do not respond within
     * this time, the current leader will immediately
     * yield its leadership and become follower.
     * If 0, it is automatically set to `heartbeat * 20`.
     * If negative number, leadership will never be expired
     * (the same as the original Raft logic).
     */
    int leadership_expiry,

    /**
     * Minimum wait time required for transferring the leadership
     * in millisecond. If this value is non-zero, and the below
     * conditions are met together,
     *   - the elapsed time since this server became a leader
     *     is longer than this number, and
     *   - the current leader's priority is not the highest one, and
     *   - all peers are responding, and
     *   - the log gaps of all peers are smaller than `stale_log_gap_`, and
     *   - `allow_leadership_transfer` of the state machine returns true,
     * then the current leader will transfer its leadership to the peer
     * with the highest priority.
     */
    int leadership_transfer_min_wait_time,

    /**
     * If true, zero-priority member can initiate vote
     * when leader is not elected long time (that can happen
     * only the zero-priority member has the latest log).
     * Once the zero-priority member becomes a leader,
     * it will immediately yield leadership so that other
     * higher priority node can takeover.
     */
    bool allow_temporary_zero_priority_leader,

    /**
     * If true, follower node will forward client request
     * to the current leader.
     * Otherwise, it will return error to client immediately.
     */
    bool auto_forwarding,

    /**
     * The maximum number of connections for auto forwarding (if enabled).
     */
    int auto_forwarding_max_connections,

    /**
     * If true, creating replication (append_entries) requests will be
     * done by a background thread, instead of doing it in user threads.
     * There can be some delay a little bit, but it improves reducing
     * the lock contention.
     */
    bool use_bg_thread_for_urgent_commit,

    /**
     * If true, a server who is currently receiving snapshot will not be
     * counted in quorum. It is useful when there are only two servers
     * in the cluster. Once the follower is receiving snapshot, the
     * leader cannot make any progress.
     */
    bool exclude_snp_receiver_from_quorum,

    /**
     * If `true` and the size of the cluster is 2, the quorum size
     * will be adjusted to 1 automatically, once one of two nodes
     * becomes offline.
     */
    bool auto_adjust_quorum_for_small_cluster,

    /**
     * Choose the type of lock that will be used by user threads.
     */
    int locking_method_type,

    /**
     * To choose blocking call or asynchronous call.
     */
    int return_method,

    /**
     * Wait ms for response after forwarding request to leader.
     * must be larger than client_req_timeout_.
     * If 0, there will be no timeout for auto forwarding.
     */
    int auto_forwarding_req_timeout,

    /**
     * If non-zero, any server whose state machine's commit index is
     * lagging behind the last committed log index will not
     * initiate vote requests for the given amount of time
     * in milliseconds.
     *
     * The purpose of this option is to avoid a server (whose state
     * machine is still catching up with the committed logs and does
     * not contain the latest data yet) being a leader.
     */
    int grace_period_of_lagging_state_machine,

    /**
     * If `true`, the new joiner will be added to cluster config as a `new_joiner`
     * even before syncing all data. The new joiner will not initiate a vote or
     * participate in leader election.
     *
     * Once the log gap becomes smaller than `log_sync_stop_gap_`, the new joiner
     * will be a regular member.
     *
     * The purpose of this featuer is to preserve the new joiner information
     * even after leader re-election, in order to let the new leader continue
     * the sync process without calling `add_srv` again.
     */
    bool use_new_joiner_type,

    /**
     * (Experimental)
     * If `true`, reading snapshot objects will be done by a background thread
     * asynchronously instead of synchronous read by Raft worker threads.
     * Asynchronous IO will reduce the overall latency of the leader's operations.
     */
    bool use_bg_thread_for_snapshot_io,

    /**
     * (Experimental)
     * If `true`, it will commit a log upon the agreement of all healthy members.
     * In other words, with this option, all healthy members have the log at the
     * moment the leader commits the log. If the number of healthy members is
     * smaller than the regular (or configured custom) quorum size, the leader
     * cannot commit the log.
     *
     * A member becomes "unhealthy" if it does not respond to the leader's
     * request for a configured time (`response_limit_`).
     */
    bool use_full_consensus_among_healthy_members,

    /**
     * (Experimental)
     * If `true`, users can let the leader append logs parallel with their
     * replication. To implement parallel log appending, users need to make
     * `log_store::append`, `log_store::write_at`, or
     * `log_store::end_of_append_batch` API triggers asynchronous disk writes
     * without blocking the thread. Even while the disk write is in progress,
     * the other read APIs of log store should be able to read the log.
     *
     * The replication and the disk write will be executed in parallel,
     * and users need to call `raft_server::notify_log_append_completion`
     * when the asynchronous disk write is done. Also, users need to properly
     * implement `log_store::last_durable_index` API to return the most recent
     * durable log index. The leader will commit the log based on the
     * result of this API.
     *
     *   - If the disk write is done earlier than the replication,
     *     the commit behavior is the same as the original protocol.
     *
     *   - If the replication is done earlier than the disk write,
     *     the leader will commit the log based on the quorum except
     *     for the leader itself. The leader can apply the log to
     *     the state machine even before completing the disk write
     *     of the log.
     *
     * Note that parallel log appending is available for the leader only,
     * and followers will wait for `notify_log_append_completion` call
     * before returning the response.
     */
    bool parallel_log_appending,

    /**
     * If non-zero, streaming mode is enabled and `append_entries` requests are
     * dispatched instantly without awaiting the response from the prior request.
     *,
     * The count of logs in-flight will be capped by this value, allowing it
     * to function as a throttling mechanism, in conjunction with
     * `max_bytes_in_flight_in_stream_`.
     */
    int max_log_gap_in_stream,

    /**
     * If non-zero, the volume of data in-flight will be restricted to this
     * specified byte limit. This limitation is effective only in streaming mode.
     */
    long long max_bytes_in_flight_in_stream
) {
    nuraft::raft_params params{};
    params.election_timeout_upper_bound_ = election_timeout_upper_bound;
    params.election_timeout_lower_bound_ = election_timeout_lower_bound;
    params.heart_beat_interval_ = heart_beat_interval;
    params.rpc_failure_backoff_ = rpc_failure_backoff;
    params.log_sync_batch_size_ = log_sync_batch_size;
    params.log_sync_stop_gap_ = log_sync_stop_gap;
    params.snapshot_distance_ = snapshot_distance;
    params.snapshot_block_size_ = snapshot_block_size;
    params.snapshot_sync_ctx_timeout_ = snapshot_sync_ctx_timeout;
    params.enable_randomized_snapshot_creation_ = enable_randomized_snapshot_creation;
    params.max_append_size_ = max_append_size;
    params.reserved_log_items_ = reserved_log_items;
    params.client_req_timeout_ = client_req_timeout;
    params.fresh_log_gap_ = fresh_log_gap;
    params.stale_log_gap_ = stale_log_gap;
    params.custom_commit_quorum_size_ = custom_commit_quorum_size;
    params.custom_election_quorum_size_ = custom_election_quorum_size;
    params.leadership_expiry_ = leadership_expiry;
    params.leadership_transfer_min_wait_time_ = leadership_transfer_min_wait_time;
    params.allow_temporary_zero_priority_leader_ = allow_temporary_zero_priority_leader;
    params.auto_forwarding_ = auto_forwarding;
    params.auto_forwarding_max_connections_ = auto_forwarding_max_connections;
    params.use_bg_thread_for_urgent_commit_ = use_bg_thread_for_urgent_commit;
    params.exclude_snp_receiver_from_quorum_ = exclude_snp_receiver_from_quorum;
    params.auto_adjust_quorum_for_small_cluster_ = auto_adjust_quorum_for_small_cluster;

    switch (locking_method_type) {
        case 1:
            /**
             * `append_entries()` and background worker threads will
             * use separate mutexes.
             */
            params.locking_method_type_ = nuraft::raft_params::locking_method_type::dual_mutex;
            break;
        case 2:
            /**
             * (Not supported yet)
             * `append_entries()` will use RW-lock, which is separate to
             * the mutex used by background worker threads.
             */
            params.locking_method_type_ = nuraft::raft_params::locking_method_type::dual_rw_lock;
            break;
        default:
            /**
             * `append_entries()` will share the same mutex with
             * background worker threads.
             */
            params.locking_method_type_ = nuraft::raft_params::locking_method_type::single_mutex;
            break;
    }

    switch (return_method) {
        case 1:
            /**
             * `append_entries()` will return immediately,
             * and callback function (i.e., handler) will be
             * invoked after it is committed in leader node.
             */
            params.return_method_ = nuraft::raft_params::return_method_type::async_handler;
            break;
        default:
            /**
             * `append_entries()` will be a blocking call,
             * and will return after it is committed in leader node.
             */
            params.return_method_ = nuraft::raft_params::return_method_type::blocking;
            break;
    }

    params.auto_forwarding_req_timeout_ = auto_forwarding_req_timeout;
    params.grace_period_of_lagging_state_machine_ = grace_period_of_lagging_state_machine;
    params.use_new_joiner_type_ = use_new_joiner_type;
    params.use_bg_thread_for_snapshot_io_ = use_bg_thread_for_snapshot_io;
    params.use_full_consensus_among_healthy_members_ = use_full_consensus_among_healthy_members;
    params.parallel_log_appending_ = parallel_log_appending;
    params.max_log_gap_in_stream_ = max_log_gap_in_stream;
    params.max_bytes_in_flight_in_stream_ = max_bytes_in_flight_in_stream;

    return reinterpret_cast<bop_raft_params_t *>(new nuraft::raft_params(params));
}

BOP_API void bop_raft_params_delete(bop_raft_params_t *params) {
    delete reinterpret_cast<nuraft::raft_params *>(params);
}


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_machine
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_fsm_t {
    nuraft::ptr<nuraft::state_machine> state_machine;

    bop_raft_fsm_t(const nuraft::ptr<nuraft::state_machine> &state_machine) : state_machine(state_machine) {
    }
};

typedef void (*bop_raft_fsm_commit)(
    void *user_data,
    uint64_t log_idx,
    const unsigned char *data,
    size_t size,
    nuraft::buffer **result
);

typedef void (*bop_raft_fsm_cluster_config)(
    void *user_data,
    uint64_t log_idx,
    void *new_conf
);

typedef void (*bop_raft_fsm_rollback)(
    void *user_data,
    uint64_t log_idx,
    const unsigned char *data,
    size_t size
);

typedef int64_t (*bop_raft_fsm_get_next_batch_size_hint_in_bytes)(
    void *user_data
);

typedef void (*bop_raft_fsm_snapshot_save)(
    void *user_data,
    uint64_t last_log_idx,
    uint64_t last_log_term,
    nuraft::cluster_config *last_config,
    uint64_t size,
    uint8_t type,
    bool is_first_obj,
    bool is_last_obj,
    const unsigned char *data,
    size_t data_size
);

typedef bool (*bop_raft_fsm_snapshot_apply)(
    void *user_data,
    uint64_t last_log_idx,
    uint64_t last_log_term,
    nuraft::cluster_config *last_config,
    uint64_t size,
    uint8_t type
);

typedef int (*bop_raft_fsm_snapshot_read)(
    void *user_data,
    void **user_snapshot_ctx,
    uint64_t obj_id,
    nuraft::buffer **data_out,
    bool *is_last_obj
);

typedef void (*bop_raft_fsm_free_user_snapshot_ctx)(
    void *user_data,
    void **user_snapshot_ctx
);

typedef nuraft::snapshot * (*bop_raft_fsm_last_snapshot)(
    void *user_data
);

typedef uint64_t (*bop_raft_fsm_last_commit_index)(void *user_data);

typedef void (*bop_raft_fsm_create_snapshot)(
    void *user_data,
    nuraft::snapshot *snapshot,
    nuraft::buffer *snapshot_data,
    void *snp_data,
    size_t snp_data_size
);

typedef bool (*bop_raft_fsm_chk_create_snapshot)(void *user_data);

typedef bool (*bop_raft_fsm_allow_leadership_transfer)(void *user_data);

BOP_API uint64_t bop_raft_fsm_adjust_commit_index_peer_index(
    nuraft::state_machine::adjust_commit_index_params *params, const int peerID
) {
    return params->peer_index_map_[peerID];
}

BOP_API void bop_raft_fsm_adjust_commit_index_peer_indexes(
    nuraft::state_machine::adjust_commit_index_params *params, uint64_t *peers[16]
) {
    for (const auto &[peer_id, index]: params->peer_index_map_) {
        if (peer_id >= 0 && peer_id < 16) {
            *peers[peer_id] = index;
        }
    }
}

typedef uint64_t (*bop_raft_fsm_adjust_commit_index)(
    void *user_data,
    uint64_t current_commit_index,
    uint64_t expected_commit_index,
    nuraft::state_machine::adjust_commit_index_params *params
);

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
        const auto cb = commit_;
        if (!cb) return nullptr;
        nuraft::buffer *result{nullptr};
        cb(user_data_, log_idx, data.data_begin(), data.size(), &result);
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
        const auto cb = commit_config_;
        current_conf = new_conf;
        if (!cb) return;
        cb(user_data_, log_idx, new_conf.get());
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
        const auto cb = rollback_;
        if (!cb) return;
        cb(user_data_, log_idx, data.data_begin(), data.size());
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
        const auto cb = rollback_config_;
        rollback_conf = conf;
        if (!cb) return;
        cb(user_data_, log_idx, conf.get());
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
        const auto cb = get_next_batch_size_hint_in_bytes_;
        if (!cb) return 0;
        return cb(user_data_);
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
        const auto cb = save_snapshot_;
        if (!cb) return;
        cb(
            user_data_,
            s.get_last_log_idx(),
            s.get_last_log_term(),
            s.get_last_config().get(),
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
        const auto cb = apply_snapshot_;
        if (!cb) return true;
        return cb(
            user_data_,
            s.get_last_log_idx(),
            s.get_last_log_term(),
            s.get_last_config().get(),
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
        const auto cb = read_snapshot_;
        if (!cb) return 0;
        nuraft::buffer *data{nullptr};
        int result = cb(
            user_data_,
            static_cast<void **>(user_snp_ctx),
            obj_id,
            &data,
            reinterpret_cast<bool *>(is_last_obj)
        );
        if (data == nullptr) {
            is_last_obj = true;
        } else {
            data_out = nuraft::buffer::make_shared(data);
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
        const auto cb = free_snapshot_user_ctx_;
        if (cb) cb(user_data_, static_cast<void **>(user_snp_ctx));
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
        const auto cb = last_snapshot_;
        if (!cb) return nullptr;
        return nuraft::ptr<nuraft::snapshot>(cb(user_data_));
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
        const auto cb = last_commit_index_;
        if (!cb) return 0;
        return cb(user_data_);
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
        const auto cb = create_snapshot_;
        if (cb == nullptr) {
            bool done = true;
            nuraft::ptr<std::exception> err{nullptr};
            when_done(done, err);
            return;
        }

        // Clone snapshot from `s`.
        nuraft::ptr<nuraft::buffer> snp_buf = s.serialize();
        nuraft::ptr<nuraft::snapshot> snp = nuraft::snapshot::deserialize(*snp_buf);

        if (async_snapshot_) {
            std::thread snp_thread([this, snp, snp_buf, when_done, cb] {
                try {
                    cb(user_data_, snp.get(), snp_buf.get(), snp_buf->data(), snp_buf->size());
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
                cb(user_data_, snp.get(), snp_buf.get(), snp_buf->data(), snp_buf->size());
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
        const auto cb = chk_create_snapshot_;
        if (!cb) return true;
        return cb(user_data_);
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
        const auto cb = allow_leadership_transfer_;
        if (!cb) return true;
        return cb(user_data_);
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
        return params.expected_commit_index_;
    }
};

BOP_API bop_raft_fsm_t *bop_raft_fsm_create(
    void *user_data,
    nuraft::ptr<nuraft::cluster_config> current_conf,
    nuraft::ptr<nuraft::cluster_config> rollback_conf,
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
    return new bop_raft_fsm_t(std::make_shared<bop_raft_state_machine>(
        user_data,
        current_conf,
        rollback_conf,
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

BOP_API void bop_raft_fsm_delete(const bop_raft_fsm_t *fsm) {
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

struct bop_raft_state_mgr : nuraft::state_mgr {
};

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
    return new bop_raft_log_store_t(std::make_shared<bop_raft_log_store>(
        user_data, next_slot, start_index, last_entry, append, write_at, end_of_append_batch, log_entries,
        entry_at, term_at, pack, apply_pack, compact, compact_async, flush, last_durable_index
    ));
}

BOP_API void bop_raft_log_store_delete(const bop_raft_log_store_t *log_store) {
    if (log_store) delete log_store;
}
}
