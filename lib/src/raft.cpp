#include <boost/mpl/integral_c_tag.hpp>

#include "./lib.h"
#include "asio_service_options.hxx"

extern "C" {
//    #define LIBMDBX_API BOP_API
//    #define LIBMDBX_EXPORTS BOP_API
//    #define SQLITE_API BOP_API
#include <mdbx.h>
#include <sqlite3.h>
#include "./hash/rapidhash.h"
#include "./hash/xxh3.h"
#include "libnuraft/nuraft.hxx"

////////////////////////////////////////////////////////////////////////////////////
/// NuRaft C API
////////////////////////////////////////////////////////////////////////////////////

typedef void (*worker_start_fn)(void *user_data, uint_t value);

typedef void (*worker_stop_fn)(void *user_data, uint_t value);

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

BOP_API std::shared_ptr<nuraft::asio_service> *bop_raft_asio_service_create(
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
    bool streaming_mode
) {
    nuraft::asio_service_options opts{};
    opts.thread_pool_size_ = thread_pool_size;
    if (worker_start != nullptr) {
        opts.worker_start_ = [worker_start, worker_start_user_data](uint_t value) {
            worker_start(worker_start_user_data, value);
        };
    }
    if (worker_stop != nullptr) {
        opts.worker_stop_ = [worker_stop_user_data, worker_stop](uint_t value) {
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
    return new std::shared_ptr(std::make_shared<nuraft::asio_service>(opts));
}

BOP_API void bop_raft_asio_service_retain(std::shared_ptr<nuraft::asio_service> *asio_service) {
    // auto* sp = static_cast<std::shared_ptr<nuraft::asio_service>*>(asio_service);
    *asio_service = *asio_service;
}

BOP_API nuraft::asio_service* bop_raft_asio_service_get(const std::shared_ptr<nuraft::asio_service> *asio_service) {
    return asio_service->get();
}

BOP_API void bop_raft_asio_service_release(const std::shared_ptr<nuraft::asio_service> *asio_service) {
    delete asio_service;
}


BOP_API std::shared_ptr<nuraft::raft_params> *bop_raft_params_create(
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

    return new std::shared_ptr(std::make_shared<nuraft::raft_params>(params));
}

BOP_API nuraft::raft_params* bop_raft_params_get(const std::shared_ptr<nuraft::raft_params> *params) {
    return params->get();
}

BOP_API void bop_raft_params_release(const std::shared_ptr<nuraft::raft_params> *params) {
    delete params;
}

typedef void(*raft_logger_put_details)(
    int level,
    const char* source_file,
    const char* func_name,
    size_t line_number,
    const char* log_line,
    size_t log_line_size
);

struct Raft_Logger final : nuraft::logger {
    raft_logger_put_details cb{};

    Raft_Logger(const raft_logger_put_details cb) : cb(cb) {}

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
        const char* source_file,
        const char* func_name,
        size_t line_number,
        const std::string& log_line
    ) override {
        cb(level, source_file, func_name, line_number, log_line.data(), log_line.size());
    }
};

BOP_API std::shared_ptr<Raft_Logger>* bop_raft_logger_create(raft_logger_put_details callback) {
    return new std::shared_ptr(std::make_shared<Raft_Logger>(callback));
}

BOP_API Raft_Logger* bop_raft_logger_get(const std::shared_ptr<Raft_Logger> *logger) {
    return logger->get();
}

BOP_API void bop_raft_logger_release(const std::shared_ptr<Raft_Logger> *logger) {
    delete logger;
}

typedef void(*bop_raft_fsm_commit)(
    const nuraft::ulong log_idx,
    const unsigned char* data,
    const size_t size,
    nuraft::buffer* result
);

struct Raft_State_Machine final : nuraft::state_machine {
    bop_raft_fsm_commit commit_;

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
        nuraft::buffer& data
    ) {
        nuraft::buffer *result{nullptr};
        commit_(log_idx, data.data_begin(), data.size(), )
        return nullptr;
    }
};
}
