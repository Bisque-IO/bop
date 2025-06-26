
#ifndef BOP_RAFT_H
#define BOP_RAFT_H

#include "./lib.h"

#ifdef __cplusplus
extern "C" {
#endif

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::buffer
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_buffer_ptr;
struct bop_raft_buffer;

BOP_API bop_raft_buffer *bop_raft_buffer_alloc(const size_t size);

// Do not call this when passing to a function that takes ownership.
BOP_API void bop_raft_buffer_free(bop_raft_buffer *buf);

BOP_API unsigned char *bop_raft_buffer_data(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_container_size(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_size(bop_raft_buffer *buf);

BOP_API size_t bop_raft_buffer_pos(bop_raft_buffer *buf);

BOP_API void bop_raft_buffer_set_pos(bop_raft_buffer *buf, size_t pos);


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<uint64_t>
///////////////////////////////////////////////////////////////////////////////////////////////////

typedef void (*bop_raft_cmd_result_uint64_when_ready_func)(
    void *user_data,
    uint64_t result,
    const char *error
);

struct bop_raft_cmd_result_uint64_t;

BOP_API bop_raft_cmd_result_uint64_t *bop_raft_cmd_result_uint64_create(
    void *user_data,
    bop_raft_cmd_result_uint64_when_ready_func when_ready
);

BOP_API void bop_raft_cmd_result_uint64_delete(const bop_raft_cmd_result_uint64_t *self);

BOP_API void *bop_raft_cmd_result_uint64_get_user_data(
    const bop_raft_cmd_result_uint64_t *self
);

BOP_API bop_raft_cmd_result_uint64_when_ready_func bop_raft_cmd_result_uint64_get_when_ready(
    const bop_raft_cmd_result_uint64_t *self
);


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::cmd_result<nuraft::ptr<nuraft::buffer>>
///////////////////////////////////////////////////////////////////////////////////////////////////

typedef void (*bop_raft_cmd_result_buffer_when_ready_func)(
    void *user_data,
    nuraft::buffer *result,
    const char *error
);

struct bop_raft_cmd_result_buffer_t;

BOP_API bop_raft_cmd_result_buffer_t *bop_raft_cmd_result_buffer_create(
    void *user_data,
    bop_raft_cmd_result_buffer_when_ready_func when_ready
);

BOP_API void bop_raft_cmd_result_buffer_delete(const bop_raft_cmd_result_buffer_t *self);

BOP_API void *bop_raft_cmd_result_buffer_get_user_data(
    bop_raft_cmd_result_buffer_t *self
);

BOP_API bop_raft_cmd_result_buffer_when_ready_func bop_raft_cmd_result_buffer_get_when_ready(
    bop_raft_cmd_result_buffer_t *self
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

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_alloc();

BOP_API bop_raft_cluster_config_ptr *bop_raft_cluster_config_alloc_ptr(bop_raft_cluster_config *config);

BOP_API void bop_raft_cluster_config_delete(const bop_raft_cluster_config *config);

BOP_API void bop_raft_cluster_config_ptr_delete(const bop_raft_cluster_config_ptr *config);

BOP_API bop_raft_buffer *bop_raft_cluster_config_serialize(bop_raft_cluster_config *conf);

BOP_API bop_raft_cluster_config *bop_raft_cluster_config_deserialize(bop_raft_buffer *buf);

// Log index number of current config.
BOP_API uint64_t bop_raft_cluster_config_log_idx(bop_raft_cluster_config *cfg);

// Log index number of previous config.
BOP_API uint64_t bop_raft_cluster_config_prev_log_idx(bop_raft_cluster_config *cfg);

// `true` if asynchronous replication mode is on.
BOP_API bool bop_raft_cluster_config_is_async_replication(bop_raft_cluster_config *cfg);

// Custom config data given by user.
BOP_API const char *bop_raft_cluster_config_user_ctx(bop_raft_cluster_config *cfg);

BOP_API size_t bop_raft_cluster_config_user_ctx_size(bop_raft_cluster_config *cfg);

// Number of servers.
BOP_API size_t bop_raft_cluster_config_servers_size(bop_raft_cluster_config *cfg);

// Server config at index
BOP_API bop_raft_srv_config *bop_raft_cluster_config_server(bop_raft_cluster_config *cfg, int idx);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::srv_config
///////////////////////////////////////////////////////////////////////////////////////////////////


struct bop_raft_srv_config_ptr;

struct bop_raft_srv_config_vec;

BOP_API bop_raft_srv_config_vec *bop_raft_srv_config_vec_create();

BOP_API void bop_raft_srv_config_vec_delete(const bop_raft_srv_config_vec *vec);

BOP_API size_t bop_raft_srv_config_vec_size(bop_raft_srv_config_vec *vec);

BOP_API nuraft::srv_config *bop_raft_srv_config_vec_get(bop_raft_srv_config_vec *vec, size_t idx);

BOP_API bop_raft_srv_config_ptr *bop_raft_srv_config_ptr_alloc(bop_raft_srv_config *config);

BOP_API void bop_raft_srv_config_ptr_delete(const bop_raft_srv_config_ptr *config);

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
 *          as it will be stored as a C-style string.
 */
BOP_API const char *bop_raft_srv_config_aux(bop_raft_srv_config *cfg);

BOP_API size_t bop_raft_srv_config_aux_size(bop_raft_srv_config *cfg);

/**
 * `true` if this node is learner.
 * Learner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_learner(bop_raft_srv_config *cfg);

/**
 * `true` if this node is a new joiner, but not yet fully synced.
 * New joiner will not initiate or participate in leader election.
 */
BOP_API bool bop_raft_srv_config_is_new_joiner(bop_raft_srv_config *cfg);

/**
 * Priority of this node.
 * 0 will never be a leader.
 */
BOP_API int32_t bop_raft_srv_config_priority(bop_raft_srv_config *cfg);


///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::svr_state
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_srv_state;

BOP_API bop_raft_srv_state *bop_raft_svr_state_deserialize(bop_raft_buffer *buf);

BOP_API bop_raft_buffer *bop_raft_svr_state_serialize(bop_raft_srv_state *state);

BOP_API void bop_raft_svr_state_delete(const bop_raft_srv_state *state);

/**
 * Term
 */
BOP_API uint64_t bop_raft_svr_state_term(const bop_raft_srv_state *state);

/**
 * Server ID that this server voted for.
 * `-1` if not voted.
 */
BOP_API int bop_raft_svr_state_voted_for(const bop_raft_srv_state *state);

/**
 * `true` if election timer is allowed.
 */
BOP_API int bop_raft_svr_state_is_election_timer_allowed(const bop_raft_srv_state *state);

/**
 * true if this server has joined the cluster but has not yet
 * fully caught up with the latest log. While in the catch-up status,
 * this server will not receive normal append_entries requests.
 */
BOP_API int bop_raft_svr_state_is_catching_up(const bop_raft_srv_state *state);

/**
 * `true` if this server is receiving a snapshot.
 * Same as `catching_up_`, it must be a durable flag so as not to be
 * reset after restart. While this flag is set, this server will neither
 * receive normal append_entries requests nor initiate election.
 */
BOP_API int bop_raft_svr_state_is_receiving_snapshot(const bop_raft_srv_state *state);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::logger
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_logger_ptr;

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
typedef void (*bop_raft_logger_put_details_func)(
    void *user_data,
    int level,
    const char *source_file,
    const char *func_name,
    size_t line_number,
    const char *log_line,
    size_t log_line_size
);

BOP_API bop_raft_logger_ptr *bop_raft_logger_create(void *user_data, bop_raft_logger_put_details_func callback);

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
// typedef const char* (*write_req_meta_fn)(void* user_data, const asio_service_meta_cb_params_t* params);

typedef bool (*bop_raft_asio_service_verify_sn_fn)(void *user_data, const char *data, size_t size);

typedef SSL_CTX * (*bop_raft_asio_service_ssl_ctx_provider_fn)(void *user_data);

typedef void (*bop_raft_asio_service_customer_resolver_response_fn)(
    void *response_impl,
    const char *v1, size_t v1_size,
    const char *v2, size_t v2_size,
    int error_code
);

typedef void (*bop_raft_asio_service_custom_resolver_fn)(
    void *user_data,
    void *response_impl,
    const char *v1, size_t v1_size,
    const char *v2, size_t v2_size,
    bop_raft_asio_service_customer_resolver_response_fn response
);

typedef void (*bop_raft_asio_service_corrupted_msg_handler_fn)(
    void *user_data,
    unsigned char *header, size_t header_size,
    unsigned char *payload, size_t payload_size
);

struct bop_raft_asio_service_ptr;

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
);

BOP_API void bop_raft_asio_service_delete(const bop_raft_asio_service_ptr *asio_service);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::raft_params
///////////////////////////////////////////////////////////////////////////////////////////////////

enum bop_raft_params_locking_method_type {
    bop_raft_params_locking_method_single_mutex = 0,
    bop_raft_params_locking_method_dual_mutex = 1,
    bop_raft_params_locking_method_dual_rw_mutex = 2,
};

enum bop_raft_params_return_method_type {
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
     * for catch-up of joining an empty node.
     */
    int32_t log_sync_batch_size;

    /**
     * Log gap (the number of logs) to stop catch-up of
     * joining a new node. Once this condition meets,
     * that newly joined node is added to peer list
     * and starts to receive heartbeat from leader.
     *
     * If zero, the new node will be added to the peer list
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
     * exceeds this, it will be considered as timeout and ctx will be released.
     * 0 means it will be set to the default value
     * `heart_beat_interval_ * response_limit_`.
     */
    int32_t snapshot_sync_ctx_timeout;

    /**
     * Enable randomized snapshot creation which will avoid
     * simultaneous snapshot creation among cluster members.
     * It is achieved by randomizing the distance of the
     * first snapshot. From the second snapshot, the fixed
     * distance given by snapshot_distance_ will be used.
     */
    bool enable_randomized_snapshot_creation;

    /**
     * Max number of logs that can be packed in a RPC
     * for append entry request.
     */
    int32_t max_append_size;

    /**
     * Minimum number of logs that will be preserved
     * (i.e., protected from log compaction) since the
     * last Raft snapshot.
     */
    int32_t reserved_log_items;

    /**
     * Client request timeout in millisecond.
     */
    int32_t client_req_timeout;

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as fresh.
     */
    int32_t fresh_log_gap;

    /**
     * Log gap (compared to the leader's latest log)
     * for treating this node as stale.
     */
    int32_t stale_log_gap;

    /**
     * Custom quorum size for commit.
     * If set to zero, the default quorum size will be used.
     */
    int32_t custom_commit_quorum_size;

    /**
     * Custom quorum size for leader election.
     * If set to zero, the default quorum size will be used.
     */
    int32_t custom_election_quorum_size;

    /**
     * Expiration time of leadership in millisecond.
     * If more than quorum nodes do not respond within
     * this time, the current leader will immediately
     * yield its leadership and become follower.
     * If 0, it is automatically set to `heartbeat * 20`.
     * If negative number, leadership will never be expired
     * (the same as the original Raft logic).
     */
    int32_t leadership_expiry;

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
    int32_t leadership_transfer_min_wait_time;

    /**
     * If true, zero-priority member can initiate vote
     * when leader is not elected long time (that can happen
     * only the zero-priority member has the latest log).
     * Once the zero-priority member becomes a leader,
     * it will immediately yield leadership so that other
     * higher priority node can takeover.
     */
    bool allow_temporary_zero_priority_leader;

    /**
     * If true, follower node will forward client request
     * to the current leader.
     * Otherwise, it will return error to client immediately.
     */
    bool auto_forwarding;

    /**
     * The maximum number of connections for auto forwarding (if enabled).
     */
    int32_t auto_forwarding_max_connections;

    /**
     * If true, creating replication (append_entries) requests will be
     * done by a background thread, instead of doing it in user threads.
     * There can be some delay a little bit, but it improves reducing
     * the lock contention.
     */
    bool use_bg_thread_for_urgent_commit;

    /**
     * If true, a server who is currently receiving snapshot will not be
     * counted in quorum. It is useful when there are only two servers
     * in the cluster. Once the follower is receiving snapshot, the
     * leader cannot make any progress.
     */
    bool exclude_snp_receiver_from_quorum;

    /**
     * If `true` and the size of the cluster is 2, the quorum size
     * will be adjusted to 1 automatically, once one of two nodes
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
     * must be larger than client_req_timeout_.
     * If 0, there will be no timeout for auto forwarding.
     */
    int32_t auto_forwarding_req_timeout;

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
    int32_t grace_period_of_lagging_state_machine;

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
    bool use_new_joiner_type;

    /**
     * (Experimental)
     * If `true`, reading snapshot objects will be done by a background thread
     * asynchronously instead of synchronous read by Raft worker threads.
     * Asynchronous IO will reduce the overall latency of the leader's operations.
     */
    bool use_bg_thread_for_snapshot_io;

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
    bool use_full_consensus_among_healthy_members;

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
    bool parallel_log_appending;

    /**
     * If non-zero, streaming mode is enabled and `append_entries` requests are
     * dispatched instantly without awaiting the response from the prior request.
     *,
     * The count of logs in-flight will be capped by this value, allowing it
     * to function as a throttling mechanism, in conjunction with
     * `max_bytes_in_flight_in_stream_`.
     */
    int32_t max_log_gap_in_stream;

    /**
     * If non-zero, the volume of data in-flight will be restricted to this
     * specified byte limit. This limitation is effective only in streaming mode.
     */
    int64_t max_bytes_in_flight_in_stream;
};

struct bop_raft_params_ptr;

BOP_API bop_raft_params *bop_raft_params_default_alloc();

BOP_API void bop_raft_params_delete(const bop_raft_params *params);

///////////////////////////////////////////////////////////////////////////////////////////////////
/// nuraft::state_machine
///////////////////////////////////////////////////////////////////////////////////////////////////

struct bop_raft_fsm_ptr;

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
    bop_raft_cluster_config *last_config,
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
    bop_raft_cluster_config *last_config,
    uint64_t size,
    uint8_t type
);

typedef int (*bop_raft_fsm_snapshot_read)(
    void *user_data,
    void **user_snapshot_ctx,
    uint64_t obj_id,
    bop_raft_buffer **data_out,
    bool *is_last_obj
);

typedef void (*bop_raft_fsm_free_user_snapshot_ctx)(
    void *user_data,
    void **user_snapshot_ctx
);

typedef bop_raft_snapshot * (*bop_raft_fsm_last_snapshot)(
    void *user_data
);

typedef uint64_t (*bop_raft_fsm_last_commit_index)(void *user_data);

typedef void (*bop_raft_fsm_create_snapshot)(
    void *user_data,
    bop_raft_snapshot *snapshot,
    bop_raft_buffer *snapshot_data,
    void *snp_data,
    size_t snp_data_size
);

typedef bool (*bop_raft_fsm_chk_create_snapshot)(void *user_data);

typedef bool (*bop_raft_fsm_allow_leadership_transfer)(void *user_data);

struct bop_raft_fsm_adjust_commit_index_params;

typedef uint64_t (*bop_raft_fsm_adjust_commit_index)(
    void *user_data,
    uint64_t current_commit_index,
    uint64_t expected_commit_index,
    const bop_raft_fsm_adjust_commit_index_params *params
);

BOP_API uint64_t bop_raft_fsm_adjust_commit_index_peer_index(
    bop_raft_fsm_adjust_commit_index_params *params, const int peerID
);

BOP_API void bop_raft_fsm_adjust_commit_index_peer_indexes(
    bop_raft_fsm_adjust_commit_index_params *params, uint64_t *peers[16]
);

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
);

BOP_API void bop_raft_fsm_delete(const bop_raft_fsm_ptr *fsm);

#ifdef __cplusplus
}
#endif

#endif //BOP_RAFT_H
