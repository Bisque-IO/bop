
#ifndef BOP_RAFT_H
#define BOP_RAFT_H

#include "./lib.h"

#ifdef __cplusplus
extern "C" {
#endif

#include <inttypes.h>
#include <stddef.h>

////////////////////////////////////////////////////////////////
//
// nuraft::buffer
//
//

struct bop_raft_buffer_ptr;
struct bop_raft_buffer;

BOP_API bop_raft_buffer *bop_raft_buffer_new(const size_t size);

// Do not call this when passing to a function that takes ownership.
BOP_API void bop_raft_buffer_free(bop_raft_buffer *buf);

BOP_API unsigned char *bop_raft_buffer_data(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_container_size(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_size(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_pos(bop_raft_buffer *buf);

BOP_API void bop_raft_buffer_set_pos(bop_raft_buffer *buf, size_t pos);

////////////////////////////////////////////////////////////////
//
// nuraft::cmd_result<uint64_t>
//
//

typedef void (*bop_raft_async_uint64_when_ready)(
    void *user_data, uint64_t result, const char *error
);

struct bop_raft_async_uint64_ptr;

BOP_API bop_raft_async_uint64_ptr *bop_raft_async_u64_make(
    void *user_data, bop_raft_async_uint64_when_ready when_ready
);

BOP_API void bop_raft_async_u64_delete(const bop_raft_async_uint64_ptr *self);

BOP_API void *bop_raft_async_u64_get_user_data(const bop_raft_async_uint64_ptr *self);

BOP_API void bop_raft_async_u64_set_user_data(bop_raft_async_uint64_ptr *self, void *user_data);

BOP_API bop_raft_async_uint64_when_ready
bop_raft_async_u64_get_when_ready(const bop_raft_async_uint64_ptr *self);

BOP_API void bop_raft_async_u64_set_when_ready(
    bop_raft_async_uint64_ptr *self, void *user_data,
    bop_raft_async_uint64_when_ready when_ready
);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<nuraft::ptr<nuraft::buffer>>
///////////////////////////////////////////////////////////////////////////////////////////////////

typedef void (*bop_raft_async_buffer_when_ready)(
    void *user_data, bop_raft_buffer *result, const char *error
);

struct bop_raft_async_buffer_ptr;

BOP_API bop_raft_async_buffer_ptr *bop_raft_async_buffer_make(
    void *user_data, bop_raft_async_buffer_when_ready when_ready
);

BOP_API void bop_raft_async_buffer_delete(const bop_raft_async_buffer_ptr *self);

BOP_API void *bop_raft_async_buffer_get_user_data(bop_raft_async_buffer_ptr *self);

BOP_API void bop_raft_async_buffer_set_user_data(bop_raft_async_buffer_ptr *self, void *user_data);

BOP_API bop_raft_async_buffer_when_ready
bop_raft_async_buffer_get_when_ready(bop_raft_async_buffer_ptr *self);

BOP_API void bop_raft_async_buffer_set_when_ready(
    bop_raft_async_buffer_ptr *self, void *user_data,
    bop_raft_async_buffer_when_ready when_ready
);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::snapshot
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_snapshot;

BOP_API bop_raft_buffer *bop_raft_snapshot_serialize(bop_raft_snapshot *snapshot);

BOP_API bop_raft_snapshot *bop_raft_snapshot_deserialize(bop_raft_buffer *buf);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cluster_config
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_cluster_config;

struct bop_raft_cluster_config_ptr;

struct bop_raft_srv_config;

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_new();

BOP_API bop_raft_cluster_config_ptr *
bop_raft_cluster_config_ptr_make(bop_raft_cluster_config *config);

BOP_API void bop_raft_cluster_config_free(const bop_raft_cluster_config *config);

BOP_API void bop_raft_cluster_config_ptr_delete(const bop_raft_cluster_config_ptr *config);

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_ptr_get(bop_raft_cluster_config_ptr *conf);

BOP_API bop_raft_buffer *bop_raft_cluster_config_serialize(bop_raft_cluster_config *conf);

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_deserialize(bop_raft_buffer *buf);

// Log index number of current config.
BOP_API uint64_t bop_raft_cluster_config_log_idx(bop_raft_cluster_config *cfg);

// Log index number of previous config.
BOP_API uint64_t bop_raft_cluster_config_prev_log_idx(bop_raft_cluster_config *cfg);

// `true` if asynchronous replication mode is on.
BOP_API bool bop_raft_cluster_config_is_async_replication(bop_raft_cluster_config *cfg);

// Custom config data given by user.
BOP_API void bop_raft_cluster_config_user_ctx(bop_raft_cluster_config *cfg, char *out_data, size_t out_data_size);

BOP_API size_t bop_raft_cluster_config_user_ctx_size(bop_raft_cluster_config *cfg);

// Number of servers.
BOP_API size_t bop_raft_cluster_config_servers_size(bop_raft_cluster_config *cfg);

// Server config at index
BOP_API bop_raft_srv_config *bop_raft_cluster_config_server(bop_raft_cluster_config *cfg, int32_t idx);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_srv_config
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_srv_config_ptr;

struct bop_raft_srv_config_vec;

BOP_API bop_raft_srv_config_vec *bop_raft_srv_config_vec_make();

BOP_API void bop_raft_srv_config_vec_delete(const bop_raft_srv_config_vec *vec);

BOP_API size_t bop_raft_srv_config_vec_size(bop_raft_srv_config_vec *vec);

BOP_API bop_raft_srv_config *bop_raft_srv_config_vec_get(bop_raft_srv_config_vec *vec, size_t idx);

BOP_API bop_raft_srv_config_ptr *bop_raft_srv_config_ptr_make(bop_raft_srv_config *config);

BOP_API void bop_raft_srv_config_ptr_delete(const bop_raft_srv_config_ptr *config);

BOP_API bop_raft_srv_config* bop_raft_srv_config_make(
    int32_t id,
    int32_t dc_id,
    const char *endpoint,
    size_t endpoint_size,
    const char *aux,
    size_t aux_size,
    bool learner,
    int32_t priority
);

BOP_API void bop_raft_srv_config_delete(const bop_raft_srv_config *config);

// ID of this server, should be positive number.
BOP_API int32_t bop_raft_srv_config_id(bop_raft_srv_config *cfg);

// ID of datacenter where this server is located. 0 if not used.
BOP_API int32_t bop_raft_srv_config_dc_id(bop_raft_srv_config *cfg);

// Endpoint (address + port).
BOP_API const char *bop_raft_srv_config_endpoint(bop_raft_srv_config *cfg);

// Size of Endpoint (address + port).
BOP_API size_t bop_raft_srv_config_endpoint_size(bop_raft_srv_config *cfg);

/**
 * Custom string given by user.
 * WARNING: It SHOULD NOT contain NULL character,
 *          as
 * it will be stored as a C-style string.
 */
BOP_API const char *bop_raft_srv_config_aux(bop_raft_srv_config *cfg);

BOP_API size_t bop_raft_srv_config_aux_size(bop_raft_srv_config *cfg);

/**
 * `true` if this node is learner.
 * Learner will not initiate or participate in leader
 * election.
 */
BOP_API bool bop_raft_srv_config_is_learner(bop_raft_srv_config *cfg);

BOP_API void bop_raft_srv_config_set_is_learner(bop_raft_srv_config *cfg, bool learner);

/**
 * `true` if this node is a new joiner, but not yet fully synced.
 * New joiner will not
 * initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_new_joiner(bop_raft_srv_config *cfg);
BOP_API void bop_raft_srv_config_set_new_joiner(bop_raft_srv_config *cfg, bool new_joiner);

/**
 * Priority of this node.
 * 0 will never be a leader.
 */
BOP_API int32_t bop_raft_srv_config_priority(bop_raft_srv_config *cfg);
BOP_API void bop_raft_srv_config_set_priority(bop_raft_srv_config *cfg, int32_t priority);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::svr_state
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_srv_state;

BOP_API bop_raft_buffer *bop_raft_svr_state_serialize(bop_raft_srv_state *state);

BOP_API bop_raft_srv_state *bop_raft_svr_state_deserialize(bop_raft_buffer *buf);

BOP_API void bop_raft_svr_state_delete(const bop_raft_srv_state *state);

/**
 * Term
 */
BOP_API uint64_t bop_raft_svr_state_term(const bop_raft_srv_state *state);

/**
 * Server ID that this server voted for.
 * `-1` if not voted.
 */
BOP_API int32_t bop_raft_svr_state_voted_for(const bop_raft_srv_state *state);

/**
 * `true` if election timer is allowed.
 */
BOP_API bool bop_raft_svr_state_is_election_timer_allowed(const bop_raft_srv_state *state);

/**
 * true if this server has joined the cluster but has not yet
 * fully caught up with the latest
 * log. While in the catch-up status,
 * this server will not receive normal append_entries
 * requests.
 */
BOP_API bool bop_raft_svr_state_is_catching_up(const bop_raft_srv_state *state);

/**
 * `true` if this server is receiving a snapshot.
 * Same as `catching_up_`, it must be a
 * durable flag so as not to be
 * reset after restart. While this flag is set, this server will
 * neither
 * receive normal append_entries requests nor initiate election.
 */
BOP_API bool bop_raft_svr_state_is_receiving_snapshot(const bop_raft_srv_state *state);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::logger
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_logger_ptr;

/**
 * Put a log with level, line number, function name,
 * and file name.
 *
 * Log level info:
 *
 * Trace:    6
 *    Debug:    5
 *    Info:     4
 *    Warning:  3
 *    Error:    2
 *    Fatal:
 * 1
 *
 * @param level Level of given log.
 * @param source_file Name of file where the log is
 * located.
 * @param func_name Name of function where the log is located.
 * @param line_number
 * Line number of the log.
 * @param log_line Contents of the log.
 */
typedef void (*bop_raft_logger_put_details_func)(
    void *user_data, int32_t level, const char *source_file, const char *func_name, size_t line_number,
    const char *log_line, size_t log_line_size
);

BOP_API bop_raft_logger_ptr *
bop_raft_logger_make(void *user_data, bop_raft_logger_put_details_func callback);

BOP_API void bop_raft_logger_delete(const bop_raft_logger_ptr *logger);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::asio_service
///////////////////////////////////////////////////////////////////////////////////////////////////

typedef void (*bop_raft_asio_service_worker_start_fn)(void *user_data, uint32_t value);

typedef void (*bop_raft_asio_service_worker_stop_fn)(void *user_data, uint32_t value);

// typedef struct {
//     const char* key;
//     int_t     id;
//     // Add other fields as needed...
// } asio_service_meta_cb_params_t;
//
// typedef const char* (*write_req_meta_fn)(void* user_data, const asio_service_meta_cb_params_t*
// params);

typedef bool (*bop_raft_asio_service_verify_sn_fn)(void *user_data, const char *data, size_t size);

typedef SSL_CTX * (*bop_raft_asio_service_ssl_ctx_provider_fn)(void *user_data);

typedef void (*bop_raft_asio_service_custom_resolver_response_fn)(
    void *response_impl, const char *v1, size_t v1_size, const char *v2, size_t v2_size,
    int32_t error_code
);

typedef void (*bop_raft_asio_service_custom_resolver_fn)(
    void *user_data, void *response_impl, const char *v1, size_t v1_size, const char *v2,
    size_t v2_size, bop_raft_asio_service_custom_resolver_response_fn response
);

typedef void (*bop_raft_asio_service_corrupted_msg_handler_fn)(
    void *user_data, unsigned char *header, size_t header_size, unsigned char *payload,
    size_t payload_size
);

struct bop_raft_asio_service_ptr;

struct bop_raft_asio_options {
    /*
    Number of ASIO worker threads.
    If zero, it will be automatically set to number of cores.
    */
    size_t thread_pool_size;

    /*
    Lifecycle callback function on worker thread start.
    */
    void *worker_start_user_data;

    /*
    Lifecycle callback function on worker thread start.
    */
    bop_raft_asio_service_worker_start_fn worker_start;

    /*
    Lifecycle callback function on worker thread start.
    */
    void *worker_stop_user_data;

    /*
    Lifecycle callback function on worker thread start.
    */
    bop_raft_asio_service_worker_stop_fn worker_stop;

    /*
    If `true`, enable SSL/TLS secure connection.
    */
    bool enable_ssl;

    /*
    If `true`, skip certificate verification.
    */
    bool skip_verification;

    /*
    Path to server certificate file.
    */
    char *server_cert_file;

    /*
    Path to server key file.
    */
    char *server_key_file;

    /*
    Path to root certificate file.
    */
    char *root_cert_file;

    /*
    If `true`, it will invoke `read_req_meta_` even though the received meta is empty.
    */
    bool invoke_req_cb_on_empty_meta;

    /*
    If `true`, it will invoke `read_resp_meta_` even though the received meta is empty.
    */
    bool invoke_resp_cb_on_empty_meta;

    void *verify_sn_user_data;
    bop_raft_asio_service_verify_sn_fn verify_sn;

    void *ssl_context_provider_server_user_data;

    /*
    Callback function that provides pre-configured SSL_CTX.
    Asio takes ownership of the provided object and disposes
    it later with SSL_CTX_free.

    No configuration changes are applied to the provided context,
    so callback must return properly configured and operational SSL_CTX.

    Note that it might be unsafe to share SSL_CTX with other threads,
    consult with your OpenSSL library documentation/guidelines.
    */
    bop_raft_asio_service_ssl_ctx_provider_fn ssl_context_provider_server;

    void *ssl_context_provider_client_user_data;

    /*
    Callback function that provides pre-configured SSL_CTX.

    Asio takes ownership of the provided object and disposes it
    later with SSL_CTX_free. No configuration changes are applied
    to the provided context, so callback must return properly
    configured and operational SSL_CTX.

    Note that it might be unsafe to
    share SSL_CTX with other threads, consult with your OpenSSL library
    documentation/guidelines.
    */
    bop_raft_asio_service_ssl_ctx_provider_fn ssl_context_provider_client;

    void *custom_resolver_user_data;

    /*
    Custom IP address resolver. If given, it will be invoked
    before the connection is established.

    If you want to selectively bypass some hosts, just pass the given
    host and port to the response function as they are.
    */
    bop_raft_asio_service_custom_resolver_fn custom_resolver;

    /*
    If `true`, each log entry will contain timestamp when it was generated
    by the leader, and those timestamps will be replicated to all followers
    so that they will see the same timestamp for the same log entry.

    To support this feature, the log store implementation should be able to
    restore the timestamp when it reads log entries.

    This feature is not backward compatible. To enable this feature, there
    should not be any member running with old version before supporting this
    flag.
    */
    bool replicate_log_timestamp;

    /*
    If `true`, NuRaft will validate the entire message with CRC. Otherwise, it
    validates the header part only.
    */
    bool crc_on_entire_message;

    /*
    If `true`, each log entry will contain a CRC checksum of the entry's
    payload.

    To support this feature, the log store implementation should be able to
    store and retrieve the CRC checksum when it reads log entries.

    This feature is not backward compatible. To enable this feature, there
    should not be any member running with the old version before supporting
    this flag.
    */
    bool crc_on_payload;

    void *corrupted_msg_handler_user_data;
    /*
    Callback function that will be invoked when the received message is corrupted.

    The first `buffer` contains the raw binary of message header, and the second `buffer`
    contains the user payload including metadata, if it is not null.
    */
    bop_raft_asio_service_corrupted_msg_handler_fn corrupted_msg_handler;

    /*
    If `true`,  NuRaft will use streaming mode, which allows it to send subsequent
    requests without waiting for the response to previous requests. The order of responses
    will be identical to the order of requests.
    */
    bool streaming_mode;
};

BOP_API bop_raft_asio_service_ptr *bop_raft_asio_service_make(
    bop_raft_asio_options *options,
    bop_raft_logger_ptr *logger
);

BOP_API void bop_raft_asio_service_delete(const bop_raft_asio_service_ptr *asio_service);

BOP_API void bop_raft_asio_service_stop(bop_raft_asio_service_ptr *asio_service);

BOP_API uint32_t bop_raft_asio_service_get_active_workers(bop_raft_asio_service_ptr *asio_service);

struct bop_raft_delayed_task_ptr;

typedef void (*bop_raft_delayed_task_func)(void *user_data);

BOP_API bop_raft_delayed_task_ptr *bop_raft_delayed_task_make(
    void *user_data,
    int32_t type,
    bop_raft_delayed_task_func exec_func,
    bop_raft_delayed_task_func deleter_func
);

BOP_API void bop_raft_delayed_task_delete(const bop_raft_delayed_task_ptr *task);

BOP_API void bop_raft_delayed_task_cancel(bop_raft_delayed_task_ptr *task);

BOP_API void bop_raft_delayed_task_reset(bop_raft_delayed_task_ptr *task);

BOP_API int32_t bop_raft_delayed_task_type(bop_raft_delayed_task_ptr *task);

BOP_API void *bop_raft_delayed_task_user_data(bop_raft_delayed_task_ptr *task);

BOP_API void bop_raft_asio_service_schedule(
    bop_raft_asio_service_ptr *asio_service,
    bop_raft_delayed_task_ptr *delayed_task,
    int32_t milliseconds
);

struct bop_raft_rpc_listener_ptr;

BOP_API bop_raft_rpc_listener_ptr *bop_raft_asio_rpc_listener_make(
    bop_raft_asio_service_ptr *asio_service,
    uint16_t listening_port,
    bop_raft_logger_ptr *logger
);

BOP_API void bop_raft_asio_rpc_listener_delete(const bop_raft_rpc_listener_ptr *rpc_listener);

struct bop_raft_rpc_client_ptr;

BOP_API bop_raft_rpc_client_ptr *bop_raft_asio_rpc_client_make(
    bop_raft_asio_service_ptr *asio_service,
    const char *endpoint,
    size_t endpoint_size
);

BOP_API void bop_raft_asio_rpc_client_delete(const bop_raft_rpc_client_ptr *rpc_client);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_params
///////////////////////////////////////////////////////////////////////////////////////////////////

enum bop_raft_params_locking_method_type : int32_t {
    bop_raft_params_locking_method_single_mutex = 0,
    bop_raft_params_locking_method_dual_mutex = 1,
    bop_raft_params_locking_method_dual_rw_mutex = 2,
};

enum bop_raft_params_return_method_type : int32_t {
    bop_raft_params_return_method_blocking = 0x0,
    bop_raft_params_return_method_async_handler = 0x1,
};

struct bop_raft_params {
    /**
     * Upper bound of election timer, in millisecond.
     */
    int32_t election_timeout_upper_bound;

    /**
     * Lower bound of election timer, in millisecond.
     */
    int32_t election_timeout_lower_bound;

    /**
     * Heartbeat interval, in millisecond.
     */
    int32_t heart_beat_interval;

    /**
     * Backoff time when RPC failure happens, in millisecond.
     */
    int32_t rpc_failure_backoff;

    /**
     * Max number of logs that can be packed in a RPC
     * for catch-up of joining an
     * empty node.
     */
    int32_t log_sync_batch_size;

    /**
     * Log gap (the number of logs) to stop catch-up of
     * joining a new node. Once this
     * condition meets,
     * that newly joined node is added to peer list
     * and starts to
     * receive heartbeat from leader.
     *
     * If zero, the new node will be added to the peer
     * list
     * immediately.
     */
    int32_t log_sync_stop_gap;

    /**
     * Log gap (the number of logs) to create a Raft snapshot.
     */
    int32_t snapshot_distance;

    /**
     * (Deprecated).
     */
    int32_t snapshot_block_size;

    /**
     * Timeout(ms) for snapshot_sync_ctx, if a single snapshot syncing request
     *
     * exceeds this, it will be considered as timeout and ctx will be released.
     * 0 means it
     * will be set to the default value
     * `heart_beat_interval_ * response_limit_`.
     */
    int32_t snapshot_sync_ctx_timeout;

    /**
     * Enable randomized snapshot creation which will avoid
     * simultaneous snapshot
     * creation among cluster members.
     * It is achieved by randomizing the distance of the

     * * first snapshot. From the second snapshot, the fixed
     * distance given by
     * snapshot_distance_ will be used.
     */
    bool enable_randomized_snapshot_creation;

    /**
     * Max number of logs that can be packed in a RPC
     * for append entry request.
 */
    int32_t max_append_size;

    /**
     * Minimum number of logs that will be preserved
     * (i.e., protected from log
     * compaction) since the
     * last Raft snapshot.
     */
    int32_t reserved_log_items;

    /**
     * Client request timeout in millisecond.
     */
    int32_t client_req_timeout;

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as
     * fresh.
     */
    int32_t fresh_log_gap;

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as
     * stale.
     */
    int32_t stale_log_gap;

    /**
     * Custom quorum size for commit.
     * If set to zero, the default quorum size will be
     * used.
     */
    int32_t custom_commit_quorum_size;

    /**
     * Custom quorum size for leader election.
     * If set to zero, the default quorum
     * size will be used.
     */
    int32_t custom_election_quorum_size;

    /**
     * Expiration time of leadership in millisecond.
     * If more than quorum nodes do not
     * respond within
     * this time, the current leader will immediately
     * yield its
     * leadership and become follower.
     * If 0, it is automatically set to `heartbeat * 20`.

     * * If negative number, leadership will never be expired
     * (the same as the original Raft
     * logic).
     */
    int32_t leadership_expiry;

    /**
     * Minimum wait time required for transferring the leadership
     * in millisecond. If
     * this value is non-zero, and the below
     * conditions are met together,
     *   - the
     * elapsed time since this server became a leader
     *     is longer than this number, and
 *
     * - the current leader's priority is not the highest one, and
     *   - all peers are
     * responding, and
     *   - the log gaps of all peers are smaller than `stale_log_gap_`, and

     * *   - `allow_leadership_transfer` of the state machine returns true,
     * then the current
     * leader will transfer its leadership to the peer
     * with the highest priority.
     */
    int32_t leadership_transfer_min_wait_time;

    /**
     * If true, zero-priority member can initiate vote
     * when leader is not elected
     * long time (that can happen
     * only the zero-priority member has the latest log).
     *
     * Once the zero-priority member becomes a leader,
     * it will immediately yield leadership
     * so that other
     * higher priority node can takeover.
     */
    bool allow_temporary_zero_priority_leader;

    /**
     * If true, follower node will forward client request
     * to the current leader.

     * * Otherwise, it will return error to client immediately.
     */
    bool auto_forwarding;

    /**
     * The maximum number of connections for auto forwarding (if enabled).
     */
    int32_t auto_forwarding_max_connections;

    /**
     * If true, creating replication (append_entries) requests will be
     * done by a
     * background thread, instead of doing it in user threads.
     * There can be some delay a
     * little bit, but it improves reducing
     * the lock contention.
     */
    bool use_bg_thread_for_urgent_commit;

    /**
     * If true, a server who is currently receiving snapshot will not be
     * counted in
     * quorum. It is useful when there are only two servers
     * in the cluster. Once the follower
     * is receiving snapshot, the
     * leader cannot make any progress.
     */
    bool exclude_snp_receiver_from_quorum;

    /**
     * If `true` and the size of the cluster is 2, the quorum size
     * will be adjusted
     * to 1 automatically, once one of two nodes
     * becomes offline.
     */
    bool auto_adjust_quorum_for_small_cluster;

    /**
     * Choose the type of lock that will be used by user threads.
     */
    bop_raft_params_locking_method_type locking_method_type;

    /**
     * To choose blocking call or asynchronous call.
     */
    bop_raft_params_return_method_type return_method;

    /**
     * Wait ms for response after forwarding request to leader.
     * must be larger than
     * client_req_timeout_.
     * If 0, there will be no timeout for auto forwarding.
     */
    int32_t auto_forwarding_req_timeout;

    /**
     * If non-zero, any server whose state machine's commit index is
     * lagging behind
     * the last committed log index will not
     * initiate vote requests for the given amount of
     * time
     * in milliseconds.
     *
     * The purpose of this option is to avoid a server
     * (whose state
     * machine is still catching up with the committed logs and does
     * not
     * contain the latest data yet) being a leader.
     */
    int32_t grace_period_of_lagging_state_machine;

    /**
     * If `true`, the new joiner will be added to cluster config as a `new_joiner`
     *
     * even before syncing all data. The new joiner will not initiate a vote or
     * participate
     * in leader election.
     *
     * Once the log gap becomes smaller than `log_sync_stop_gap_`,
     * the new joiner
     * will be a regular member.
     *
     * The purpose of this featuer is
     * to preserve the new joiner information
     * even after leader re-election, in order to let
     * the new leader continue
     * the sync process without calling `add_srv` again.
     */
    bool use_new_joiner_type;

    /**
     * (Experimental)
     * If `true`, reading snapshot objects will be done by a
     * background thread
     * asynchronously instead of synchronous read by Raft worker threads.

     * * Asynchronous IO will reduce the overall latency of the leader's operations.
     */
    bool use_bg_thread_for_snapshot_io;

    /**
     * (Experimental)
     * If `true`, it will commit a log upon the agreement of all
     * healthy members.
     * In other words, with this option, all healthy members have the log at
     * the
     * moment the leader commits the log. If the number of healthy members is
     *
     * smaller than the regular (or configured custom) quorum size, the leader
     * cannot commit
     * the log.
     *
     * A member becomes "unhealthy" if it does not respond to the leader's

     * * request for a configured time (`response_limit_`).
     */
    bool use_full_consensus_among_healthy_members;

    /**
     * (Experimental)
     * If `true`, users can let the leader append logs parallel with
     * their
     * replication. To implement parallel log appending, users need to make
     *
     * `log_store::append`, `log_store::write_at`, or
     * `log_store::end_of_append_batch` API
     * triggers asynchronous disk writes
     * without blocking the thread. Even while the disk
     * write is in progress,
     * the other read APIs of log store should be able to read the
     * log.
     *
     * The replication and the disk write will be executed in parallel,
     *
     * and users need to call `raft_server::notify_log_append_completion`
     * when the
     * asynchronous disk write is done. Also, users need to properly
     * implement
     * `log_store::last_durable_index` API to return the most recent
     * durable log index. The
     * leader will commit the log based on the
     * result of this API.
     *
     *   - If the
     * disk write is done earlier than the replication,
     *     the commit behavior is the same
     * as the original protocol.
     *
     *   - If the replication is done earlier than the disk
     * write,
     *     the leader will commit the log based on the quorum except
     *     for
     * the leader itself. The leader can apply the log to
     *     the state machine even before
     * completing the disk write
     *     of the log.
     *
     * Note that parallel log
     * appending is available for the leader only,
     * and followers will wait for
     * `notify_log_append_completion` call
     * before returning the response.
     */
    bool parallel_log_appending;

    /**
     * If non-zero, streaming mode is enabled and `append_entries` requests are
     *
     * dispatched instantly without awaiting the response from the prior request.
     *,
     * The
     * count of logs in-flight will be capped by this value, allowing it
     * to function as a
     * throttling mechanism, in conjunction with
     * `max_bytes_in_flight_in_stream_`.
     */
    int32_t max_log_gap_in_stream;

    /**
     * If non-zero, the volume of data in-flight will be restricted to this
     * specified
     * byte limit. This limitation is effective only in streaming mode.
     */
    int64_t max_bytes_in_flight_in_stream;
};

struct bop_raft_params_ptr;

BOP_API bop_raft_params *bop_raft_params_make();

BOP_API void bop_raft_params_delete(const bop_raft_params *params);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_machine
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_fsm_ptr;

typedef void (*bop_raft_fsm_commit)(
    void *user_data, uint64_t log_idx, const unsigned char *data, size_t size,
    nuraft::buffer **result
);

typedef void (*bop_raft_fsm_cluster_config)(void *user_data, uint64_t log_idx, void *new_conf);

typedef void (*bop_raft_fsm_rollback)(
    void *user_data, uint64_t log_idx, const unsigned char *data, size_t size
);

typedef int64_t (*bop_raft_fsm_get_next_batch_size_hint_in_bytes)(void *user_data);

typedef void (*bop_raft_fsm_snapshot_save)(
    void *user_data, uint64_t last_log_idx, uint64_t last_log_term,
    bop_raft_cluster_config *last_config, uint64_t size, uint8_t type, bool is_first_obj,
    bool is_last_obj, const unsigned char *data, size_t data_size
);

typedef bool (*bop_raft_fsm_snapshot_apply)(
    void *user_data, uint64_t last_log_idx, uint64_t last_log_term,
    bop_raft_cluster_config *last_config, uint64_t size, uint8_t type
);

typedef int32_t (*bop_raft_fsm_snapshot_read)(
    void *user_data, void **user_snapshot_ctx, uint64_t obj_id, bop_raft_buffer **data_out,
    bool *is_last_obj
);

typedef void (*bop_raft_fsm_free_user_snapshot_ctx)(void *user_data, void **user_snapshot_ctx);

typedef bop_raft_snapshot * (*bop_raft_fsm_last_snapshot)(void *user_data);

typedef uint64_t (*bop_raft_fsm_last_commit_index)(void *user_data);

typedef void (*bop_raft_fsm_create_snapshot)(
    void *user_data, bop_raft_snapshot *snapshot, bop_raft_buffer *snapshot_data, void *snp_data,
    size_t snp_data_size
);

typedef bool (*bop_raft_fsm_chk_create_snapshot)(void *user_data);

typedef bool (*bop_raft_fsm_allow_leadership_transfer)(void *user_data);

struct bop_raft_fsm_adjust_commit_index_params;

typedef uint64_t (*bop_raft_fsm_adjust_commit_index)(
    void *user_data, uint64_t current_commit_index, uint64_t expected_commit_index,
    const bop_raft_fsm_adjust_commit_index_params *params
);

BOP_API uint64_t bop_raft_fsm_adjust_commit_index_peer_index(
    bop_raft_fsm_adjust_commit_index_params *params, const int32_t peerID
);

BOP_API void bop_raft_fsm_adjust_commit_index_peer_indexes(
    bop_raft_fsm_adjust_commit_index_params *params, uint64_t *peers[16]
);

BOP_API bop_raft_fsm_ptr *bop_raft_fsm_make(
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
);

BOP_API void bop_raft_fsm_delete(const bop_raft_fsm_ptr *fsm);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_mgr
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_log_store_ptr;
struct bop_raft_state_mgr_ptr;

typedef bop_raft_cluster_config * (*bop_raft_state_mgr_load_config)(void *user_data);

typedef void (*bop_raft_state_mgr_save_config)(
    void *user_data, const bop_raft_cluster_config *config
);

typedef void (*bop_raft_state_mgr_save_state)(void *user_data, const bop_raft_srv_state *state);

typedef bop_raft_srv_state * (*bop_raft_state_mgr_read_state)(void *user_data);

typedef bop_raft_log_store_ptr * (*bop_raft_state_mgr_load_log_store)(void *user_data);

typedef int32_t (*bop_raft_state_mgr_server_id)(void *user_data);

typedef void (*bop_raft_state_mgr_system_exit)(void *user_data, const int32_t exit_code);

BOP_API bop_raft_state_mgr_ptr *bop_raft_state_mgr_make(
    void *user_data,
    bop_raft_state_mgr_load_config load_config,
    bop_raft_state_mgr_save_config save_config,
    bop_raft_state_mgr_read_state read_state,
    bop_raft_state_mgr_save_state save_state,
    bop_raft_state_mgr_load_log_store load_log_store,
    bop_raft_state_mgr_server_id server_id,
    bop_raft_state_mgr_system_exit system_exit
);

BOP_API void bop_raft_state_mgr_delete(const bop_raft_state_mgr_ptr *sm);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::log_store
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_log_entry;

BOP_API bop_raft_log_entry *bop_raft_log_entry_make(
    uint64_t term, nuraft::buffer *data, uint64_t timestamp, bool has_crc32, uint32_t crc32
);

BOP_API void bop_raft_log_entry_delete(const bop_raft_log_entry *entry);

struct bop_raft_log_entry_vector;

BOP_API void bop_raft_log_entry_vec_push(
    const bop_raft_log_entry_vector *vec, uint64_t term, nuraft::buffer *data, uint64_t timestamp,
    bool has_crc32, uint32_t crc32
);

typedef uint64_t (*bop_raft_log_store_next_slot)(void *user_data);

typedef uint64_t (*bop_raft_log_store_start_index)(void *user_data);

typedef bop_raft_log_entry * (*bop_raft_log_store_last_entry)(void *user_data);

typedef uint64_t (*bop_raft_log_store_append)(
    void *user_data, uint64_t term, uint8_t *data, size_t data_size, uint64_t log_timestamp,
    bool has_crc32, uint32_t crc32
);

typedef void (*bop_raft_log_store_write_at)(
    void *user_data, uint64_t index, uint64_t term, uint8_t *data, size_t data_size,
    uint64_t log_timestamp, bool has_crc32, uint32_t crc32
);

typedef void (*bop_raft_log_store_end_of_append_batch)(
    void *user_data, uint64_t start, uint64_t cnt
);

typedef bool (*bop_raft_log_store_log_entries)(
    void *user_data, bop_raft_log_entry_vector *vec, uint64_t start, uint64_t end
);

typedef bop_raft_log_entry * (*bop_raft_log_store_entry_at)(void *user_data, uint64_t index);

typedef uint64_t (*bop_raft_log_store_term_at)(void *user_data, uint64_t index);

typedef bop_raft_buffer * (*bop_raft_log_store_pack)(void *user_data, uint64_t index, int32_t cnt);

typedef void (*bop_raft_log_store_apply_pack)(
    void *user_data, uint64_t index, bop_raft_buffer *pack
);

typedef bool (*bop_raft_log_store_compact)(void *user_data, uint64_t last_log_index);

typedef bool (*bop_raft_log_store_compact_async)(void *user_data, uint64_t last_log_index);

typedef bool (*bop_raft_log_store_flush)(void *user_data);

typedef uint64_t (*bop_raft_log_store_last_durable_index)(void *user_data);

BOP_API bop_raft_log_store_ptr *bop_raft_log_store_make(
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
);

BOP_API void bop_raft_log_store_delete(const bop_raft_log_store_ptr *log_store);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::counter
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_counter;

BOP_API bop_raft_counter *bop_raft_counter_make(const char *name, size_t name_size);

BOP_API void bop_raft_counter_delete(const bop_raft_counter *counter);

BOP_API const char *bop_raft_counter_name(const bop_raft_counter *counter);

BOP_API uint64_t bop_raft_counter_value(const bop_raft_counter *counter);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::gauge
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_gauge;

BOP_API bop_raft_gauge *bop_raft_gauge_make(const char *name, size_t name_size);

BOP_API void bop_raft_gauge_delete(const bop_raft_gauge *gauge);

BOP_API const char *bop_raft_gauge_name(const bop_raft_gauge *gauge);

BOP_API int64_t bop_raft_gauge_value(const bop_raft_gauge *gauge);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::histogram
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_histogram;

BOP_API bop_raft_histogram *bop_raft_histogram_make(const char *name, size_t name_size);

BOP_API void bop_raft_histogram_delete(const bop_raft_histogram *histogram);

BOP_API const char *bop_raft_histogram_name(const bop_raft_histogram *histogram);

BOP_API size_t bop_raft_histogram_size(const bop_raft_histogram *histogram);

BOP_API uint64_t bop_raft_histogram_get(const bop_raft_histogram *histogram, double key);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_append_entries_ptr
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_append_entries_ptr;

BOP_API bop_raft_append_entries_ptr *bop_raft_append_entries_create();

BOP_API void bop_raft_append_entries_delete(const bop_raft_append_entries_ptr *self);

BOP_API size_t bop_raft_append_entries_size(bop_raft_append_entries_ptr *self);

BOP_API size_t
bop_raft_append_entries_push(bop_raft_append_entries_ptr *self, bop_raft_buffer *buf);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_server_peer_info
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_server_peer_info {
    /**
     * Peer ID.
     */
    int32_t id;

    /**
     * The last log index that the peer has, from this server's point of view.
     */
    uint64_t last_log_idx;

    /**
     * The elapsed time since the last successful response from this peer, in microseconds.
     */
    uint64_t last_succ_resp_us;
};

struct bop_raft_server_peer_info_vec;

BOP_API bop_raft_server_peer_info_vec *bop_raft_server_peer_info_vec_make();

BOP_API void bop_raft_server_peer_info_vec_delete(const bop_raft_server_peer_info_vec *vec);

BOP_API size_t bop_raft_server_peer_info_vec_size(bop_raft_server_peer_info_vec *vec);

BOP_API bop_raft_server_peer_info *
bop_raft_server_peer_info_vec_get(bop_raft_server_peer_info_vec *vec, size_t idx);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// bop_raft_server
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_server;

struct bop_raft_server_ptr;

enum bop_raft_server_priority_set_result : int32_t {
    bop_raft_server_priority_set_result_set = 0,
    bop_raft_server_priority_set_result_broadcast = 1,
    bop_raft_server_priority_set_result_ignored = 2,
};

enum bop_raft_cb_type : int32_t {
    /**
     * Got request from peer or client.
     * ctx: pointer to request.
     */
    bop_raft_cb_type_process_req = 1,

    /**
     * Got append entry response from peer.
     * ctx: pointer to new matched index
     * number.
     */
    bop_raft_cb_type_got_append_entry_resp_from_peer = 2,

    /**
     * Appended logs and executed pre-commit locally.
     * Happens on leader only.
     *
     * ctx: pointer to last log number.
     */
    bop_raft_cb_type_append_logs = 3,

    /**
     * Heartbeat timer wakes up.
     * Happens on leader only.
     * ctx: pointer to last
     * log number.
     */
    bop_raft_cb_type_heart_beat = 4,

    /**
     * Joined a cluster.
     * Happens on follower only.
     * ctx: pointer to cluster
     * config.
     */
    bop_raft_cb_type_joined_cluster = 5,

    /**
     * Became a leader.
     * ctx: pointer to term number.
     */
    bop_raft_cb_type_become_leader = 6,

    /**
     * Request append entries to followers.
     * Happens on leader only.
     */
    bop_raft_cb_type_request_append_entries = 7,

    /**
     * Save snapshot chunk or object, in receiver's side.
     * ctx: snapshot_sync_req.
 */
    bop_raft_cb_type_save_snapshot = 8,

    /**
     * Committed a new config.
     * ctx: pointer to log index of new config.
     */
    bop_raft_cb_type_new_config = 9,

    /**
     * Removed from a cluster.
     * Happens on follower only.
     * ctx: null.
     */
    bop_raft_cb_type_removed_from_cluster = 10,

    /**
     * Became a follower.
     * ctx: pointer to term number.
     */
    bop_raft_cb_type_become_follower = 11,

    /**
     * The difference of committed log index between the follower and the
     * master
     * became smaller than a user-specified threshold.
     * Happens on follower only.
     * ctx:
     * null.
     */
    bop_raft_cb_type_become_fresh = 12,

    /**
     * The difference of committed log index between the follower and the
     * master
     * became larger than a user-specified threshold.
     * Happens on follwer only.
     * ctx:
     * null
     */
    bop_raft_cb_type_become_stale = 13,

    /**
     * Got append entry request from leader.
     * It will be invoked only for acceptable
     * logs.
     * ctx: pointer to request.
     */
    bop_raft_cb_type_got_append_entry_req_from_leader = 14,

    /**
     * This node is out of log range, which means that
     * leader has no valid log or
     * snapshot to send for this node.
     * ctx: pointer to `OutOfLogRangeWarningArgs`.
     */
    bop_raft_cb_type_out_of_log_range_warning = 15,

    /**
     * New connection is established.
     * Mostly this event happens in below cases:
 * 1)
     * Leader sends message to follower, then follower will fire
     *      this event.
     *   2)
     * Candidate sends vote request to peer, then the peer (receiver)
     *      will fire this
     * event.
     * ctx: pointer to `ConnectionArgs`.
     */
    bop_raft_cb_type_connection_opened = 16,

    /**
     * Connection is closed.
     * ctx: pointer to `ConnectionArgs`.
     */
    bop_raft_cb_type_connection_closed = 17,

    /**
     * Invoked when a session receives a message from the valid leader
     * first time.
     * This callback is preceded by `ConnectionOpened`
     * event.
     * ctx: pointer to
     * `ConnectionArgs`.
     */
    bop_raft_cb_type_new_session_from_leader = 18,

    /**
     * Executed a log in the state machine.
     * ctx: pointer to the log index.
     */
    bop_raft_cb_type_state_machine_execution = 19,

    /**
     * Just sent an append entries request.
     * ctx: pointer to `req_msg` instance.
 */
    bop_raft_cb_type_sent_append_entries_req = 20,

    /**
     * Just received an append entries request.
     * ctx: pointer to `req_msg` instance.

     */
    bop_raft_cb_type_received_append_entries_req = 21,

    /**
     * Just sent an append entries response.
     * ctx: pointer to `resp_msg` instance.
 */
    bop_raft_cb_type_sent_append_entries_resp = 22,

    /**
     * Just received an append entries response.
     * ctx: pointer to `resp_msg`
     * instance.
     */
    bop_raft_cb_type_received_append_entries_resp = 23,

    /**
     * When cluster size is 2 and `auto_adjust_quorum_for_small_cluster_` is on,
     * this
     * server attempts to adjust the quorum size to 1.
     * ctx: null
     */
    bop_raft_cb_type_auto_adjust_quorum = 24,

    /**
     * Adding a server failed due to RPC errors and timeout expiry.
     * ctx: null
     */
    bop_raft_cb_type_server_join_failed = 25,

    /**
     * Snapshot creation begins.
     * ctx: pointer to `uint64_t` (committed_idx).
     */
    bop_raft_cb_type_snapshot_creation_begin = 26,

    /**
     * Got a resgination request either automatically or manually.
     * ctx: null.
     */
    bop_raft_cb_type_resignation_from_leader = 27,

    /**
     * When a peer RPC errors count exceeds raft_server::limits.warning_limit_, or
     * a
     * peer doesn't respond for a long time (raft_params::leadership_expiry_),
     * the peer is
     * considered lost.
     * ctx: null.
     */
    bop_raft_cb_type_follower_lost = 28,

    /**
     * When the server receives a misbehaving message from a peer,
     * the callback has
     * the ability to either ignore the message
     * or respond normally by adjusting ReqResp.resp
     * as indicated by ctx.
     *
     * Furthermore, the callback can opt to terminate
     * if
     * the situation is deemed critical.
     *
     * ctx: pointer to `ReqResp` instance.
     */
    bop_raft_cb_type_received_misbehaving_message = 29,
};

struct bop_raft_cb_param {
    int32_t my_id;
    int32_t leader_id;
    int32_t peer_id;
    void *ctx;
};

struct bop_raft_req_msg;

struct bop_raft_resp_msg;

enum bop_raft_cb_return_code : int32_t {
    bop_raft_cb_return_code_ok = 0,
    bop_raft_cb_return_code_return_null = -1,
};

struct bop_raft_cb_out_of_log_range_warning_args {
    uint64_t start_idx_of_leader;
};

struct bop_raft_cb_connection_args {
    /**
     * ID of session.
     */
    uint64_t session_id;

    /**
     * Endpoint address.
     */
    const char *address;
    size_t address_len;

    /**
     * Endpoint port.
     */
    uint32_t port;

    /**
     * Endpoint server ID if given.
     */
    int32_t srv_id;

    /**
     * `true` if the endpoint server is leader.
     */
    bool is_leader;
};

enum bop_raft_msg_type : int32_t {
    bop_raft_msg_type_request_vote_request = 1,
    bop_raft_msg_type_request_vote_response = 2,
    bop_raft_msg_type_append_entries_request = 3,
    bop_raft_msg_type_append_entries_response = 4,
    bop_raft_msg_type_client_request = 5,
    bop_raft_msg_type_add_server_request = 6,
    bop_raft_msg_type_add_server_response = 7,
    bop_raft_msg_type_remove_server_request = 8,
    bop_raft_msg_type_remove_server_response = 9,
    bop_raft_msg_type_sync_log_request = 10,
    bop_raft_msg_type_sync_log_response = 11,
    bop_raft_msg_type_join_cluster_request = 12,
    bop_raft_msg_type_join_cluster_response = 13,
    bop_raft_msg_type_leave_cluster_request = 14,
    bop_raft_msg_type_leave_cluster_response = 15,
    bop_raft_msg_type_install_snapshot_request = 16,
    bop_raft_msg_type_install_snapshot_response = 17,
    bop_raft_msg_type_ping_request = 18,
    bop_raft_msg_type_ping_response = 19,
    bop_raft_msg_type_pre_vote_request = 20,
    bop_raft_msg_type_pre_vote_response = 21,
    bop_raft_msg_type_other_request = 22,
    bop_raft_msg_type_other_response = 23,
    bop_raft_msg_type_priority_change_request = 24,
    bop_raft_msg_type_priority_change_response = 25,
    bop_raft_msg_type_reconnect_request = 26,
    bop_raft_msg_type_reconnect_response = 27,
    bop_raft_msg_type_custom_notification_request = 28,
    bop_raft_msg_type_custom_notification_response = 29,
};

enum bop_raft_cmd_result_code : int32_t {
    BOP_RAFT_CMD_RESULT_OK = 0,
    BOP_RAFT_CMD_RESULT_CANCELLED = -1,
    BOP_RAFT_CMD_RESULT_TIMEOUT = -2,
    BOP_RAFT_CMD_RESULT_NOT_LEADER = -3,
    BOP_RAFT_CMD_RESULT_BAD_REQUEST = -4,
    BOP_RAFT_CMD_RESULT_SERVER_ALREADY_EXISTS = -5,
    BOP_RAFT_CMD_RESULT_CONFIG_CHANGING = -6,
    BOP_RAFT_CMD_RESULT_SERVER_IS_JOINING = -7,
    BOP_RAFT_CMD_RESULT_SERVER_NOT_FOUND = -8,
    BOP_RAFT_CMD_RESULT_CANNOT_REMOVE_LEADER = -9,
    BOP_RAFT_CMD_RESULT_SERVER_IS_LEAVING = -10,
    BOP_RAFT_CMD_RESULT_TERM_MISMATCH = -11,

    BOP_RAFT_CMD_RESULT_RESULT_NOT_EXIST_YET = -10000,

    BOP_RAFT_CMD_RESULT_FAILED = -32768,
};

struct bop_raft_cb_req_resp;

struct bop_raft_cb_req_msg {
    uint64_t term;
    bop_raft_msg_type type;
    int32_t src;
    int32_t dst;
    // Term of last log below.
    uint64_t last_log_term;
    // Last log index that the destination (i.e., follower) node
    // currently has. If below `log_entries_` contains logs,
    // the starting index will be `last_log_idx_ + 1`.
    uint64_t last_log_idx;
    // Source (i.e., leader) node's current committed log index.
    // As a pipelining, follower will do commit on this index number
    // after appending given logs.
    uint64_t commit_idx;
};

struct bop_raft_cb_resp_peer;

struct bop_raft_cb_resp_msg {
    uint64_t term;
    bop_raft_msg_type type;
    int32_t src;
    int32_t dst;
    uint64_t next_idx;
    int64_t next_batch_size_hint_in_bytes;
    bool accepted;
    bop_raft_buffer *ctx;
    bop_raft_cb_resp_peer *peer;
    bop_raft_cmd_result_code result_code;
};

BOP_API void bop_raft_cb_get_req_msg(bop_raft_cb_req_resp *req_resp, bop_raft_cb_req_msg *req_msg);

BOP_API size_t bop_raft_cb_get_req_msg_entries_size(bop_raft_cb_req_resp *req_resp);

BOP_API bop_raft_log_entry *
bop_raft_cb_get_req_msg_get_entry(bop_raft_cb_req_resp *req_resp, size_t idx);

BOP_API void
bop_raft_cb_get_resp_msg(bop_raft_cb_req_resp *req_resp, bop_raft_cb_resp_msg *resp_msg);

typedef bop_raft_cb_return_code (*bop_raft_cb_func)(
    void *user_data, bop_raft_cb_type type, bop_raft_cb_param *param
);

typedef uint64_t (*bop_raft_inc_term_func)(void *user_data, uint64_t current_term);

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
);

BOP_API bool bop_raft_server_stop(bop_raft_server_ptr *server, size_t time_limit_sec);

BOP_API bop_raft_server *bop_raft_server_get(bop_raft_server_ptr *s);

/**
 * Check if this server is ready to serve operation.
 *
 * @return `true` if it is ready.
 */
BOP_API bool bop_raft_server_is_initialized(const bop_raft_server *rs);

/**
 * Check if this server is catching up the current leader
 * to join the cluster.
 *
 * @return
 * `true` if it is in catch-up mode.
 */
BOP_API bool bop_raft_server_is_catching_up(const bop_raft_server *rs);

/**
 * Check if this server is receiving snapshot from leader.
 *
 * @return `true` if it is
 * receiving snapshot.
 */
BOP_API bool bop_raft_server_is_receiving_snapshot(const bop_raft_server *rs);

/**
 * Add a new server to the current cluster. Only leader will accept this operation.
 * Note that this is an asynchronous task so that needs more network communications.
 * Returning this function does not guarantee adding the server.
 *
 * @param srv Configuration of server to add.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_add_srv(
    bop_raft_server *rs, const bop_raft_srv_config_ptr *srv, bop_raft_async_buffer_ptr *handler
);

/**
 * Remove a server from the current cluster. Only leader will accept this operation.
 * The same as `add_srv`, this is also an asynchronous task.
 *
 * @param srv_id ID of server to remove.
 * @return `get_accepted()` will be true on success.
 */
BOP_API bool bop_raft_server_remove_srv(
    bop_raft_server *rs,
    int32_t srv_id,
    bop_raft_async_buffer_ptr *handler
);

/**
 * Flip learner flag of given server. Learner will be excluded from the quorum. Only
 * leader will accept this operation. This is also an asynchronous task.
 *
 * @param srv_id ID of the server to set as a learner.
 * @param to If `true`, set the server as a learner, otherwise, clear learner flag.
 * @return `ret->get_result_code()` will be OK on success.
 */
BOP_API bool bop_raft_server_flip_learner_flag(
    bop_raft_server *rs,
    const int32_t srv_id,
    const bool to,
    bop_raft_async_buffer_ptr *handler
);

/**
 * Append and replicate the given logs. Only leader will accept this operation.
 *
 * @param entries Set of logs to replicate.
 * @return
 *     In blocking mode, it will be blocked during replication, and
 *     return `cmd_result` instance which contains the commit results from
 *     the state machine.
 *
 *     In async mode, this function will return immediately, and the commit
 *     results will be set to returned `cmd_result` instance later.
 */
BOP_API bool bop_raft_server_append_entries(
    bop_raft_server *rs,
    bop_raft_append_entries_ptr *entries,
    bop_raft_async_buffer_ptr *handler
);

/**
 * Update the priority of given server.
 *
 * @param rs local bop_raft_server instance
 * @param srv_id ID of server to update priority.
 * @param new_priority Priority value, greater than or equal to 0.
 *        If priority is set to 0, this server will never be a leader.
 * @param broadcast_when_leader_exists If we're not a leader and a
 *        leader exists, broadcast priority change to other peers.
 *        If false, set_priority does nothing. Please note that
 *        setting this option to true may possibly cause cluster config
 *        to diverge.
 * @return SET If we're a leader and we have committed priority change.
 * @return BROADCAST
 *     If either there's no
 *         live leader now, or we're a leader, and we want to set our priority to 0,
 *
 *         or we're not a leader and broadcast_when_leader_exists = true.
 *         We have sent messages to other peers about priority change but haven't
 *         committed this change.
 * @return IGNORED If we're not a leader and broadcast_when_leader_exists = false.
 *
 */
BOP_API bop_raft_server_priority_set_result bop_raft_server_set_priority(
    bop_raft_server *rs,
    const int32_t srv_id,
    const int32_t new_priority,
    bool broadcast_when_leader_exists = false
);

/**
 * Broadcast the priority change of given server to all peers. This function should be used
 * only when there is no live leader and leader election is blocked by priorities of live
 * followers. In that case, we are not able to change priority by using normal `set_priority`
 * operation.
 *
 * @param rs local bop_raft_server instance
 * @param srv_id ID of server to update
 * priority.
 * @param new_priority New priority.
 */
BOP_API void bop_raft_server_broadcast_priority_change(
    bop_raft_server *rs,
    const int32_t srv_id,
    const int32_t new_priority
);

/**
 * Yield current leadership and becomes a follower. Only a leader will accept this
 *
 * If given `immediate_yield` flag is `true`, it will become a follower immediately.
 * The subsequent leader election will be totally random so that there is always a
 * chance that this server becomes the next leader again.
 *
 * Otherwise, this server will pause write operations first, wait until the successor
 * (except for this server) finishes the catch-up of the latest log, and then resign.
 * In such a case, the next leader will be much more predictable.
 *
 * Users can designate the successor. If not given, this API will automatically choose
 * the highest priority server as a successor.
 *
 * @param rs local bop_raft_server instance
 * @param immediate_yield If `true`, yield immediately.
 * @param successor_id The server ID of the successor.
 *                     If `-1`, the successor will be chosen automatically.
 */
BOP_API void
bop_raft_server_yield_leadership(
    bop_raft_server *rs,
    bool immediate_yield,
    int32_t successor_id
);

/**
 * Send a request to the current leader to yield its leadership,
 * and become the next leader.

 * *
 * @return `true` on success. But it does not guarantee to become
 *         the next leader due to various failures.
 */
BOP_API bool bop_raft_server_request_leadership(bop_raft_server *rs);

/**
 * Start the election timer on this server, if this server is a follower. It will
 * allow the election timer permanently, if it was disabled by state manager.
 */
BOP_API void bop_raft_server_restart_election_timer(bop_raft_server *rs);

/**
 * Set custom context to Raft cluster config. It will create a new configuration log and
 * replicate it.
 *
 * @param ctx Custom context.
 */
BOP_API void bop_raft_server_set_user_ctx(bop_raft_server *rs, const char *data, size_t size);

/**
 * Get custom context from the current cluster config.
 *
 * @return Custom context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_user_ctx(bop_raft_server *rs);

/**
* Get timeout for snapshot_sync_ctx
*
* @return snapshot_sync_ctx_timeout.
*/
BOP_API int32_t bop_raft_server_get_snapshot_sync_ctx_timeout(const bop_raft_server *rs);

/**
 * Get ID of this server.
 *
 * @return Server ID.
 */
BOP_API int32_t bop_raft_server_get_id(const bop_raft_server *rs);

/**
 * Get the current term of this server.
 *
 * @return Term.
 */
BOP_API uint64_t bop_raft_server_get_term(const bop_raft_server *rs);

/**
 * Get the term of given log index number.
 *
 * @param log_idx Log index number
 * @return Term of given log.
 */
BOP_API uint64_t bop_raft_server_get_log_term(const bop_raft_server *rs, uint64_t log_idx);

/**
 * Get the term of the last log.
 *
 * @return Term of the last log.
 */
BOP_API uint64_t bop_raft_server_get_last_log_term(const bop_raft_server *rs);

/**
 * Get the last log index number.
 *
 * @return Last log index number.
 */
BOP_API uint64_t bop_raft_server_get_last_log_idx(const bop_raft_server *rs);

/**
 * Get the last committed log index number of state machine.
 *
 * @return Last committed log index number of state machine.
 */
BOP_API uint64_t bop_raft_server_get_committed_log_idx(const bop_raft_server *rs);

/**
 * Get the target log index number we are required to commit.
 *
 * @return Target committed log index number.
 */
BOP_API uint64_t bop_raft_server_get_target_committed_log_idx(const bop_raft_server *rs);

/**
 * Get the leader's last committed log index number.
 *
 * @return The leader's last committed log index number.
 */
BOP_API uint64_t bop_raft_server_get_leader_committed_log_idx(const bop_raft_server *rs);

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
BOP_API uint64_t bop_raft_server_get_log_idx_at_becoming_leader(const bop_raft_server *rs);

/**
 * Calculate the log index to be committed from current peers' matched indexes.
 *
 * @return Expected committed log index.
 */
BOP_API uint64_t bop_raft_server_get_expected_committed_log_idx(bop_raft_server *rs);

/**
 * Get the current Raft cluster config.
 *
 * @param rs raft server instance
 * @param cluster_config Wrapper for holding nuraft::ptr<nuraft::cluster_config>
 */
BOP_API void
bop_raft_server_get_config(const bop_raft_server *rs, bop_raft_cluster_config_ptr *cluster_config);

/**
 * Get data center ID of the given server.
 *
 * @param srv_id Server ID.
 * @return -1 if given server ID does not exist.
 *          0 if data center ID was not assigned.
 */
BOP_API int32_t bop_raft_server_get_dc_id(const bop_raft_server *rs, int32_t srv_id);

/**
 * Get auxiliary context stored in the server config.
 *
 * @param srv_id Server ID.
 * @return
 * Auxiliary context.
 */
BOP_API bop_raft_buffer *bop_raft_server_get_aux(const bop_raft_server *rs, int32_t srv_id);

/**
 * Get the ID of current leader.
 *
 * @return Leader ID
 *         -1 if there is no live leader.
 */
BOP_API int32_t bop_raft_server_get_leader(const bop_raft_server *rs);

/**
 * Check if this server is leader.
 *
 * @return `true` if it is leader.
 */
BOP_API bool bop_raft_server_is_leader(const bop_raft_server *rs);

/**
 * Check if there is live leader in the current cluster.
 *
 * @return `true` if live leader exists.
 */
BOP_API bool bop_raft_server_is_leader_alive(const bop_raft_server *rs);

/**
 * Get the configuration of given server.
 *
 * @param srv_id Server ID.
 * @return Server configuration.
 */
BOP_API void bop_raft_server_get_srv_config(
    const bop_raft_server *rs, bop_raft_srv_config_ptr *svr_config, int32_t srv_id
);

/**
 * Get the configuration of all servers.
 *
 * @param[out] configs_out Set of server configurations.
 */
BOP_API void
bop_raft_server_get_srv_config_all(const bop_raft_server *rs, bop_raft_srv_config_vec *configs_out);

/**
 * Update the server configuration, only leader will accept this operation.
 * This function will update the current cluster config and replicate it to all peers.
 *
 * We don't allow changing multiple server configurations at once, due to safety reason.
 *
 * Change on endpoint will not be accepted (should be removed and then re-added).
 * If the server is in new joiner state, it will be rejected.
 * If the server ID does not exist, it will also be rejected.
 *
 * @param new_config Server configuration to update.
 * @return `true` on success, `false` if rejected.
 */
BOP_API void
bop_raft_server_update_srv_config(bop_raft_server *rs, bop_raft_srv_config_ptr *new_config);

/**
 * Get the peer info of the given ID. Only leader will return peer info.
 *
 * @param srv_id Server ID
 * @return Peer info
 */
BOP_API bool
bop_raft_server_get_peer_info(bop_raft_server *rs, int32_t srv_id, bop_raft_server_peer_info *peer);

/**
 * Get the info of all peers. Only leader will return peer info.
 *
 * @return Vector of peer
 * info.
 */
BOP_API void bop_raft_server_get_peer_info_all(
    const bop_raft_server *rs, bop_raft_server_peer_info_vec *peers_out
);

/**
 * Shut down server instance.
 */
BOP_API void bop_raft_server_shutdown(bop_raft_server *rs);

/**
 *  Start internal background threads, initialize election
 */
BOP_API void bop_raft_server_start_server(bop_raft_server *rs, bool skip_initial_election_timeout);

/**
 * Stop background commit thread.
 */
BOP_API void bop_raft_server_stop_server(bop_raft_server *rs);

/**
 * Send reconnect request to leader. Leader will re-establish the connection to this server
 * in a few seconds. Only follower will accept this operation.
 */
BOP_API void bop_raft_server_send_reconnect_request(bop_raft_server *rs);

/**
 * Update Raft parameters.
 *
 * @param new_params Parameters to set.
 */
BOP_API void bop_raft_server_update_params(bop_raft_server *rs, bop_raft_params *params);

/**
 * Get the current Raft parameters. Returned instance is the clone of the original one,
 * so that user can modify its contents.
 *
 * @return Clone of Raft parameters.
 */
BOP_API void bop_raft_server_get_current_params(const bop_raft_server *rs, bop_raft_params *params);

/**
 * Get the counter number of given stat name.
 *
 * @param name Stat name to retrieve.
 *
 * @return Counter value.
 */
BOP_API uint64_t bop_raft_server_get_stat_counter(bop_raft_server *rs, bop_raft_counter *counter);

/**
 * Get the gauge number of given stat name.
 *
 * @param name Stat name to retrieve.
 * @return Gauge value.
 */
BOP_API int64_t bop_raft_server_get_stat_gauge(bop_raft_server *rs, bop_raft_gauge *gauge);

/**
 * Get the histogram of given stat name.
 *
 * @param name Stat name to retrieve.
 * @param[out] histogram_out Histogram as a map. Key is the upper bound of a bucket, and
 *             value is the counter of that bucket.
 * @return `true` on success.
 *         `false` if stat does not exist, or is not histogram type.
 */
BOP_API bool bop_raft_server_get_stat_histogram(bop_raft_server *rs, bop_raft_histogram *histogram);

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_counter(bop_raft_server *rs, bop_raft_counter *counter);

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_gauge(bop_raft_server *rs, bop_raft_gauge *gauge);

/**
 * Reset given stat to zero.
 *
 * @param name Stat name to reset.
 */
BOP_API void bop_raft_server_reset_histogram(bop_raft_server *rs, bop_raft_histogram *histogram);

/**
 * Reset all existing stats to zero.
 */
BOP_API void bop_raft_server_reset_all_stats(bop_raft_server *rs);

/**
 * Set a custom callback function for increasing term.
 */
BOP_API void bop_raft_server_set_inc_term_func(
    bop_raft_server *rs, void *user_data, bop_raft_inc_term_func func
);

/**
 * Pause the background execution of the state machine. If an operation execution is
 * currently happening, the state machine may not be paused immediately.
 *
 * @param timeout_ms If non-zero, this function will be blocked until
 *                   either it completely pauses the state machine execution
 *                   or reaches the given time limit in milliseconds.
 *                   Otherwise, this function will return immediately, and there
 *                   is a possibility that the state machine execution
 *                   is still happening.
 */
BOP_API void bop_raft_server_pause_state_machine_execution(bop_raft_server *rs, size_t timeout_ms);

/**
 * Resume the background execution of state machine.
 */
BOP_API void bop_raft_server_resume_state_machine_execution(bop_raft_server *rs);

/**
 * Check if the state machine execution is paused.
 *
 * @return `true` if paused.
 */
BOP_API bool bop_raft_server_is_state_machine_execution_paused(const bop_raft_server *rs);

/**
 * Block the current thread and wake it up when the state machine execution is paused.
 *
 * @param timeout_ms If non-zero, wake up after the given amount of time
 *                   even though the state machine is not paused yet.
 * @return `true` if the state machine is paused.
 */
BOP_API bool bop_raft_server_wait_for_state_machine_pause(bop_raft_server *rs, size_t timeout_ms);

/**
 * (Experimental)
 * This API is used when `raft_params::parallel_log_appending_` is set.
 *
 * Everytime an asynchronous log appending job is done, users should call this API to notify
 * Raft server to handle the log. Note that calling this API once for multiple logs is acceptable
 * and recommended.
 *
 * @param ok `true` if appending succeeded.
 */
BOP_API void bop_raft_server_notify_log_append_completion(bop_raft_server *rs, bool ok);

/**
 * Manually create a snapshot based on the latest committed log index of the state machine.
 *
 * Note that snapshot creation will fail immediately if the previous snapshot task is still
 * running.
 *
 * @param serialize_commit
 *        If `true`, the background commit will be blocked until `create_snapshot`
 *        returns. However, it will not block the commit for the entire duration
 *        of the snapshot creation process, as long as your state machine creates
 *        the snapshot asynchronously. The purpose of this flag is to ensure that
 *        the log index used for the snapshot creation is the most recent one.
 *
 * @return Log index number of the created snapshot or`0` if failed.
 */
BOP_API uint64_t bop_raft_server_create_snapshot(
    bop_raft_server *rs,
    bool serialize_commit
);

/**
 * Manually and asynchronously create a snapshot on the next earliest available commited
 * log index.
 *
 * Unlike `create_snapshot`, if the previous snapshot task is running, it will wait
 * until the previous task is done. Once the snapshot creation is finished, it will be
 * notified via the returned `cmd_result` with the log index number of the snapshot.
 *
 * @param `cmd_result` instance.
 *        `nullptr` if there is already a scheduled snapshot creation.
 */
BOP_API void bop_raft_server_schedule_snapshot_creation(
    bop_raft_server *rs, bop_raft_async_uint64_ptr *result_handler
);

/**
 * Get the log index number of the last snapshot.
 *
 * @return Log index number of the last snapshot. `0` if snapshot does not exist.
 */
BOP_API uint64_t bop_raft_server_get_last_snapshot_idx(const bop_raft_server *rs);

BOP_API bop_raft_state_mgr_ptr* bop_raft_mdbx_state_mgr_open(
    bop_raft_srv_config_ptr* my_srv_config,
    const char* dir,
    size_t dir_size,
    bop_raft_logger_ptr* logger,
    size_t size_lower,
    size_t size_now,
    size_t size_upper,
    size_t growth_step,
    size_t shrink_threshold,
    size_t pagesize,
    uint32_t flags, // MDBX_env_flags_t
    uint16_t mode, // mdbx_mode_t
    bop_raft_log_store_ptr* log_store
);

BOP_API bop_raft_log_store_ptr* bop_raft_mdbx_log_store_open(
    const char* path,
    size_t path_size,
    bop_raft_logger_ptr* logger,
    size_t size_lower,
    size_t size_now,
    size_t size_upper,
    size_t growth_step,
    size_t shrink_threshold,
    size_t pagesize,
    uint32_t flags,
    uint16_t mode,
    size_t compact_batch_size
);

#ifdef __cplusplus
}
#endif

#endif // BOP_RAFT_H
