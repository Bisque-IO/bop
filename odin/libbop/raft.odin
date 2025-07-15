package libbop

import "base:runtime"


/*
Raft Buffer Layout and Design
-----------------------------

This buffer structure is used to store and manipulate Raft log data
efficiently. It consists of a contiguous memory block with a fixed-size
header followed by the actual data payload. The layout is designed to
allow fast access to metadata (size and position) without needing an
external structure.

Buffer Memory Layout (in bytes):

+------------+------------+----------------------+
|   Size     |   Pos      |      Data[...]       |
|  (u32)     |  (u32)     |   (size bytes)       |
+------------+------------+----------------------+
  [0..3]       [4..7]          [8..8+size-1]

Fields:
  - Size (4 bytes, little-endian): total number of valid bytes in the data section.
  - Pos  (4 bytes, little-endian): read/write cursor position relative to the data start.
  - Data (variable): actual byte content of the buffer.

Use Cases:
  - Efficient log entry reads and writes in Raft protocol.
  - Simple pointer arithmetic for slicing and copying data.
  - Encapsulation of metadata within the buffer avoids auxiliary structs.

Accessing Data Example:
  raft_buffer_data(buf)

Buffer Allocation:
  raft_buffer_alloc(size_in_bytes)

Safety Note:
  The buffer must always be accessed via the appropriate casts and offsets.
  Misaligned or overflow access can lead to undefined behavior.
*/
Raft_Buffer :: struct {}

/*
Raft_Buffer_Ptr — C-Compatible Smart Pointer Wrapper for Raft_Buffer
---------------------------------------------------------------------

This struct provides a safe and reference-counted interface to a
`Raft_Buffer` object managed internally by a C++ `std::shared_ptr`.

The goal is to expose `std::shared_ptr<raft_buffer>` to C consumers
in a way that allows lifetime tracking, shared ownership, and safe
deletion through `raft_buffer_ptr_delete(buf)`.

Design:
-------
C code only deals with opaque handles:

    typedef struct Raft_Buffer_Ptr Raft_Buffer_Ptr;

Internally, `Raft_Buffer_Ptr` wraps:

    std::shared_ptr<Raft_Buffer>

Ownership is shared: cloning the pointer in C++ increments the count;
calling `raft_buffer_ptr_delete()` decrements the reference count.

Once the count reaches zero, the `Raft_Buffer` is destroyed.

C Interface:
------------
  Raft_Buffer_Ptr* raft_buffer_ptr_new(Raft_Buffer* raw_buffer);
    - Wraps a raw `Raft_Buffer*` into a shared_ptr and returns a C handle.

  void raft_buffer_ptr_delete(Raft_Buffer_Ptr* ptr);
    - Decrements the shared_ptr ref count and deletes the wrapper struct.
    - If this was the last reference, the `Raft_Buffer` is destroyed.

  Raft_Buffer* raft_buffer_ptr_get(Raft_Buffer_Ptr* ptr);
    - Returns the raw pointer to the internal `Raft_Buffer`.
    - Caller must not free or retain this pointer beyond the wrapper's lifetime.

Thread Safety:
--------------
The underlying `std::shared_ptr` is thread-safe for ref counting,
but access to the actual `Raft_Buffer` contents must be externally synchronized.

Example Usage (C):
------------------
    Raft_Buffer_Ptr* buf = raft_buffer_ptr_make(create_buffer(...));
    use_buffer(raft_buffer_ptr_get(buf));
    raft_buffer_ptr_delete(buf); // safe, decrements ref count

Notes:
------
- Never `free()` a `Raft_Buffer_Ptr` directly.
- Do not call `delete` on the raw C++ pointer from C code.
- Always pair each `*_new` with a `*_delete`.
 */
Raft_Buffer_Ptr :: struct {}

Raft_Async_U64_Ptr :: struct {}
Raft_Async_U64_Done :: #type proc "c" (user_data: rawptr, result: u64, err: cstring)

Raft_Async_Buffer_Ptr :: struct {}
Raft_Async_Buffer_Done :: #type proc "c" (user_data: rawptr, result: ^Raft_Buffer, err: cstring)

Raft_Cluster_Config :: struct {}
Raft_Cluster_Config_Ptr :: struct {}
Raft_Srv_Config :: struct {}
Raft_Srv_Config_Ptr :: struct {}
Raft_Srv_Config_Vec :: struct {}
Raft_Srv_State :: struct {}

Raft_Snapshot :: struct {}

Raft_Log_Level :: enum i32 {
	Fatal   = 1,
	Error   = 2,
	Warning = 3,
	Info    = 4,
	Debug   = 5,
	Trace   = 6,
}

Raft_Log_Level_Names: [Raft_Log_Level]string = {
	.Fatal   = "FATAL",
	.Error   = "ERROR",
	.Warning = "WARN",
	.Info    = "INFO",
	.Debug   = "DEBUG",
	.Trace   = "TRACE",
}

Raft_Log_Level_To_Odin_Level: [Raft_Log_Level]runtime.Logger_Level = {
	.Fatal   = .Fatal,
	.Error   = .Error,
	.Warning = .Warning,
	.Info    = .Info,
	.Debug   = .Debug,
	.Trace   = .Debug,
}


/*
Put a log with level, line number, function name,
and file name.

Log level info:

Trace:    6
Debug:    5
Info:     4
Warning:  3
Error:    2
Fatal:    1

@param level Level of given log.
@param source_file Name of file where the log is
located.
@param func_name Name of function where the log is located.
@param line_number
Line number of the log.
@param log_line Contents of the log.
*/
Raft_Logger_Write_Func :: #type proc "c" (
	user_data: rawptr,
	level: Raft_Log_Level,
	source_file: cstring,
	func_name: cstring,
	line_number: uintptr,
	log_line: [^]byte,
	log_line_size: uintptr,
)
Raft_Logger_Ptr :: struct {}

Raft_Locking_Method_Type :: enum i32 {
	Single_Mutex  = 0,
	Dual_Mutex    = 1,
	Dual_RW_Mutex = 2,
}

Raft_Return_Method_Type :: enum i32 {
	Blocking = 0x0,
	Async    = 0x1,
}

SSL_CTX :: struct {}
Raft_Asio_Ssl_Ctx_Provider :: #type proc "c" (user_data: rawptr) -> ^SSL_CTX
Raft_Asio_Worker_Start :: #type proc "c" (user_data: rawptr, value: u32)
Raft_Asio_Worker_Stop :: #type proc "c" (user_data: rawptr, value: u32)
Raft_Asio_Verify_Sn :: #type proc "c" (user_data: rawptr, data: [^]byte, size: uintptr) -> bool
Raft_Asio_Custom_Resolver_Response :: #type proc "c" (
	response_impl: rawptr,
	v1: [^]byte,
	v1_size: uintptr,
	v2: [^]byte,
	v2_size: uintptr,
	error_code: i32,
)
Raft_Asio_Custom_Resolver :: #type proc "c" (
	user_data: rawptr,
	response_impl: rawptr,
	v1: [^]byte,
	v1_size: uintptr,
	v2: [^]byte,
	v2_size: uintptr,
)
Raft_Asio_Corrupted_Msg_Handler :: #type proc "c" (
	user_data: rawptr,
	header: [^]byte,
	header_size: uintptr,
	payload: [^]byte,
	payload_size: uintptr,
)

Raft_Delayed_Task :: struct{}
Raft_Asio_RPC_Listener_Ptr :: struct{}
Raft_Asio_RPC_Client_Ptr :: struct{}

Raft_Asio_Options :: struct {
	/*
    Number of ASIO worker threads.
    If zero, it will be automatically set to number of cores.
    */
	thread_pool_size:                      uintptr,

	/*
    Lifecycle callback function on worker thread start.
    */
	worker_start_user_data:                rawptr,

	/*
    Lifecycle callback function on worker thread start.
    */
	worker_start:                          Raft_Asio_Worker_Start,

	/*
    Lifecycle callback function on worker thread start.
    */
	worker_stop_user_data:                 rawptr,

	/*
    Lifecycle callback function on worker thread start.
    */
	worker_stop:                           Raft_Asio_Worker_Stop,

	/*
    If `true`, enable SSL/TLS secure connection.
    */
	enable_ssl:                            bool,

	/*
    If `true`, skip certificate verification.
    */
	skip_verification:                     bool,

	/*
    Path to server certificate file.
    */
	server_cert_file:                      cstring,

	/*
    Path to server key file.
    */
	server_key_file:                       cstring,

	/*
    Path to root certificate file.
    */
	root_cert_file:                        cstring,

	/*
    If `true`, it will invoke `read_req_meta_` even though the received meta is empty.
    */
	invoke_req_cb_on_empty_meta:           bool,

	/*
    If `true`, it will invoke `read_resp_meta_` even though the received meta is empty.
    */
	invoke_resp_cb_on_empty_meta:          bool,
	verify_sn_user_data:                   rawptr,
	verify_sn:                             Raft_Asio_Verify_Sn,
	ssl_context_provider_server_user_data: rawptr,

	/*
    Callback function that provides pre-configured SSL_CTX.
    Asio takes ownership of the provided object and disposes
    it later with SSL_CTX_free.

    No configuration changes are applied to the provided context,
    so callback must return properly configured and operational SSL_CTX.

    Note that it might be unsafe to share SSL_CTX with other threads,
    consult with your OpenSSL library documentation/guidelines.
    */
	ssl_context_provider_server:           Raft_Asio_Ssl_Ctx_Provider,
	ssl_context_provider_client_user_data: rawptr,

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
	ssl_context_provider_client:           Raft_Asio_Ssl_Ctx_Provider,
	custom_resolver_user_data:             rawptr,

	/*
    Custom IP address resolver. If given, it will be invoked
    before the connection is established.

    If you want to selectively bypass some hosts, just pass the given
    host and port to the response function as they are.
    */
	custom_resolver:                       Raft_Asio_Ssl_Ctx_Provider,

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
	replicate_log_timestamp:               bool,

	/*
    If `true`, NuRaft will validate the entire message with CRC. Otherwise, it
    validates the header part only.
    */
	crc_on_entire_message:                 bool,

	/*
    If `true`, each log entry will contain a CRC checksum of the entry's
    payload.

    To support this feature, the log store implementation should be able to
    store and retrieve the CRC checksum when it reads log entries.

    This feature is not backward compatible. To enable this feature, there
    should not be any member running with the old version before supporting
    this flag.
    */
	crc_on_payload:                        bool,
	corrupted_msg_handler_user_data:       rawptr,
	/*
    Callback function that will be invoked when the received message is corrupted.

    The first `buffer` contains the raw binary of message header, and the second `buffer`
    contains the user payload including metadata, if it is not null.
    */
	corrupted_msg_handler:                 Raft_Asio_Corrupted_Msg_Handler,

	/*
    If `true`,  NuRaft will use streaming mode, which allows it to send subsequent
    requests without waiting for the response to previous requests. The order of responses
    will be identical to the order of requests.
    */
	streaming_mode:                        bool,
}

Raft_Asio_Service_Ptr :: struct {}

Raft_Params :: struct {
	/*
	Upper bound of election timer, in millisecond.
	*/
	election_timeout_upper_bound:             i32,

	/*
	Lower bound of election timer, in millisecond.
	*/
	election_timeout_lower_bound:             i32,

	/*
	Heartbeat interval, in millisecond.
	*/
	heart_beat_interval:                      i32,

	/*
	Backoff time when RPC failure happens, in millisecond.
	*/
	rpc_failure_backoff:                      i32,

	/*
	Max number of logs that can be packed in a RPC
	for catch-up of joining an
	empty node.
	*/
	log_sync_batch_size:                      i32,

	/*
	Log gap (the number of logs) to stop catch-up of
	joining a new node. Once this condition meets,
	that newly joined node is added to peer list
	and starts to receive heartbeat from leader.

	If zero, the new node will be added to the peer list immediately.
	*/
	log_sync_stop_gap:                        i32,

	/*
	Log gap (the number of logs) to create a Raft snapshot.
	*/
	snapshot_distance:                        i32,

	/*
	(Deprecated).
	*/
	snapshot_block_size:                      i32,

	/*
	Timeout(ms) for snapshot_sync_ctx, if a single snapshot syncing request
	exceeds this, it will be considered as timeout and ctx will be released.
	0 means it will be set to the default value `heart_beat_interval_ * response_limit_`.
	*/
	snapshot_sync_ctx_timeout:                i32,

	/*
	Enable randomized snapshot creation which will avoid simultaneous snapshot
	creation among cluster members. It is achieved by randomizing the distance of the
	first snapshot. From the second snapshot, the fixed distance given by
	snapshot_distance_ will be used.
	*/
	enable_randomized_snapshot_creation:      bool,

	/*
	Max number of logs that can be packed in a RPC for append entry request.
	*/
	max_append_size:                          i32,

	/*
	Minimum number of logs that will be preserved
	(i.e., protected from log compaction) since the last Raft snapshot.
	*/
	reserved_log_items:                       i32,

	/*
	Client request timeout in millisecond.
	*/
	client_req_timeout:                       i32,

	/*
	Log gap (compared to the leader's latest log) for treating this node as fresh.
	*/
	fresh_log_gap:                            i32,

	/*
	Log gap (compared to the leader's latest log) for treating this node as stale.
	*/
	stale_log_gap:                            i32,

	/*
	Custom quorum size for commit. If set to zero, the default quorum size will be used.
	*/
	custom_commit_quorum_size:                i32,


	/*
	Custom quorum size for leader election. If set to zero, the default quorum size will be used.
	*/
	custom_election_quorum_size:              i32,

	/*
	Expiration time of leadership in millisecond. If more than quorum nodes do not
	respond within this time, the current leader will immediately yield its leadership
	and become follower.

	If 0, it is automatically set to `heartbeat * 20`.

	If negative number, leadership will never be expired

	(the same as the original Raft logic).
	*/
	leadership_expiry:                        i32,

	/*
	Minimum wait time required for transferring the leadership in millisecond. If
	this value is non-zero, and the below conditions are met together,
	  - the elapsed time since this server became a leader
	    is longer than this number, and
	  - the current leader's priority is not the highest one, and
	  - all peers are responding, and
	  - the log gaps of all peers are smaller than `stale_log_gap_`, and
	  - `allow_leadership_transfer` of the state machine returns true, then the current
	    leader will transfer its leadership to the peer with the highest priority.
	*/
	leadership_transfer_min_wait_time:        i32,

	/*
	If true, zero-priority member can initiate vote when leader is not elected
	long time (that can happen only the zero-priority member has the latest log).

	Once the zero-priority member becomes a leader, it will immediately yield leadership
	so that other higher priority node can takeover.
	*/
	allow_temporary_zero_priority_leader:     bool,

	/*
	If true, follower node will forward client request to the current leader.
	Otherwise, it will return error to client immediately.
	*/
	auto_forwarding:                          bool,

	/*
	The maximum number of connections for auto forwarding (if enabled).
	*/
	auto_forwarding_max_connections:          i32,

	/*
	If true, creating replication (append_entries) requests will be done by a
	background thread, instead of doing it in user threads. There can be some
	delay a little bit, but it improves reducing the lock contention.
	*/
	use_bg_thread_for_urgent_commit:          bool,

	/*
	If true, a server who is currently receiving snapshot will not be counted in
	quorum. It is useful when there are only two servers in the cluster. Once the
	follower is receiving snapshot, the leader cannot make any progress.
	*/
	exclude_snp_receiver_from_quorum:         bool,

	/*
	If `true` and the size of the cluster is 2, the quorum size will be adjusted
	to 1 automatically, once one of two nodes becomes offline.
	*/
	auto_adjust_quorum_for_small_cluster:     bool,

	/*
	Choose the type of lock that will be used by user threads.
	*/
	locking_method_type:                      Raft_Locking_Method_Type,

	/*
	To choose blocking call or asynchronous call.
	*/
	return_method:                            Raft_Return_Method_Type,

	/*
	Wait ms for response after forwarding request to leader. must be larger than client_req_timeout_.

	If 0, there will be no timeout for auto forwarding.
	*/
	auto_forwarding_req_timeout:              i32,

	/*
	If non-zero, any server whose state machine's commit index is lagging behind the last
	committed log index will not initiate vote requests for the given amount of time
	in milliseconds.

	The purpose of this option is to avoid a server (whose state machine is still catching
	up with the committed logs and does not contain the latest data yet) being a leader.
	*/
	grace_period_of_lagging_state_machine:    i32,

	/*
	If `true`, the new joiner will be added to cluster config as a `new_joiner`

	even before syncing all data. The new joiner will not initiate a vote or participate
	in leader election.

	Once the log gap becomes smaller than `log_sync_stop_gap_`, the new joiner will be
	a regular member.

	The purpose of this featuer is to preserve the new joiner information even after
	leader re-election, in order to let the new leader continue the sync process
	without calling `add_srv` again.
	*/
	use_new_joiner_type:                      bool,

	/*
	(Experimental)

	If `true`, reading snapshot objects will be done by a background thread
	asynchronously instead of synchronous read by Raft worker threads.

	Asynchronous IO will reduce the overall latency of the leader's operations.
	*/
	use_bg_thread_for_snapshot_io:            bool,

	/*
	(Experimental)

	If `true`, it will commit a log upon the agreement of all healthy members.
	In other words, with this option, all healthy members have the log at
	the moment the leader commits the log. If the number of healthy members is
	smaller than the regular (or configured custom) quorum size, the leader
	cannot commit the log.

	A member becomes "unhealthy" if it does not respond to the leader's request
	for a configured time (`response_limit`).
	*/
	use_full_consensus_among_healthy_members: bool,

	/*
	(Experimental)

	If `true`, users can let the leader append logs parallel with their replication.
	To implement parallel log appending, users need to make `log_store::append`,
	`log_store::write_at`, or `log_store::end_of_append_batch` API triggers asynchronous
	disk writes without blocking the thread. Even while the disk write is in progress,
	the other read APIs of log store should be able to read the log.

	The replication and the disk write will be executed in parallel, and users need to
	call `raft_server::notify_log_append_completion` when the asynchronous disk write is
	done. Also, users need to properly implement `log_store::last_durable_index` API to
	return the most recent durable log index. The leader will commit the log based on the
	result of this API.

	  - If the disk write is done earlier than the replication,
	    the commit behavior is the sameas the original protocol.

	  - If the replication is done earlier than the disk write, the leader will commit the
	    log based on the quorum except for the leader itself. The leader can apply the log to
	    the state machine even before completing the disk write of the log.

	Note that parallel log appending is available for the leader only, and followers will
	wait for `notify_log_append_completion` call before returning the response.
	*/
	parallel_log_appending:                   bool,

	/*
	If non-zero, streaming mode is enabled and `append_entries` requests are dispatched
	instantly without awaiting the response from the prior request. The count of logs
	in-flight will be capped by this value, allowing it to function as a throttling
	mechanism, in conjunction with `max_bytes_in_flight_in_stream`.
	*/
	max_log_gap_in_stream:                    i32,


	/*
	If non-zero, the volume of data in-flight will be restricted to this
	specified byte limit. This limitation is effective only in streaming mode.
	*/
	max_bytes_in_flight_in_stream:            i64,
}

Raft_FSM_Commit_Func :: #type proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
	result: ^^Raft_Buffer,
)

Raft_FSM_Cluster_Config_Func :: #type proc "c" (
	user_data: rawptr,
	log_idx: u64,
	new_conf: ^Raft_Cluster_Config,
)

Raft_FSM_Rollback_Func :: #type proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
)

Raft_FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func :: #type proc "c" (
	user_data: rawptr,
) -> i64

Raft_FSM_Save_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
	last_log_idx: u64,
	last_log_term: u64,
	last_config: ^Raft_Cluster_Config,
	size: uintptr,
	type: u8,
	is_first_obj: bool,
	is_last_obj: bool,
	data: [^]byte,
	data_size: uintptr,
)

Raft_FSM_Apply_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
	last_log_idx: u64,
	last_log_term: u64,
	last_config: ^Raft_Cluster_Config,
	size: uintptr,
	type: u8,
) -> bool

Raft_FSM_Read_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr,
	obj_id: u64,
	data_out: ^^Raft_Buffer,
	is_last_obj: ^bool,
) -> i32

Raft_FSM_Free_User_Snapshot_Ctx_Func :: #type proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr,
)

Raft_FSM_Last_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
) -> ^Raft_Snapshot

Raft_FSM_Last_Commit_Index_Func :: #type proc "c" (
	user_data: rawptr,
) -> u64

Raft_FSM_Create_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
	snapshot: ^Raft_Snapshot,
	snapshot_data: ^Raft_Buffer,
	snp_data: [^]byte,
	snp_data_size: uintptr,
)

Raft_FSM_Chk_Create_Snapshot_Func :: #type proc "c" (
	user_data: rawptr,
) -> bool

Raft_FSM_Allow_Leadership_Transfer_Func :: #type proc "c" (
	user_data: rawptr,
) -> bool

Raft_FSM_Adjust_Commit_Index_Params :: struct {}

Raft_FSM_Adjust_Commit_Index_Func :: #type proc "c" (
	user_data: rawptr,
	current_commit_index: u64,
	expected_commit_index: u64,
	params: ^Raft_FSM_Adjust_Commit_Index_Params,
)

Raft_FSM_Funcs :: struct {
	user_data: rawptr,
	current_conf: ^Raft_Cluster_Config,
	rollback_conf: ^Raft_Cluster_Config,
	commit: Raft_FSM_Commit_Func,
	commit_config: Raft_FSM_Cluster_Config_Func,
	pre_commit: Raft_FSM_Commit_Func,
	rollback: Raft_FSM_Rollback_Func,
	rollback_config: Raft_FSM_Cluster_Config_Func,
	get_next_batch_size_hint_in_bytes: Raft_FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func,
	save_snapshot: Raft_FSM_Save_Snapshot_Func,
	apply_snapshot: Raft_FSM_Apply_Snapshot_Func,
	read_snapshot: Raft_FSM_Read_Snapshot_Func,
	free_snapshot_user_ctx: Raft_FSM_Free_User_Snapshot_Ctx_Func,
	last_snapshot: Raft_FSM_Last_Snapshot_Func,
	last_commit_index: Raft_FSM_Last_Commit_Index_Func,
	create_snapshot: Raft_FSM_Create_Snapshot_Func,
	chk_create_snapshot: Raft_FSM_Chk_Create_Snapshot_Func,
	allow_leadership_transfer: Raft_FSM_Allow_Leadership_Transfer_Func,
	adjust_commit_index: Raft_FSM_Adjust_Commit_Index_Func,
}

Raft_FSM_Ptr :: struct {}

Raft_Log_Entry :: struct {}

Raft_Log_Entry_Ptr :: struct {
	data: u128,
}

Raft_Log_Entry_Vec :: struct {}

Raft_Log_Store_Next_Slot_Func :: #type proc "c" (user_data: rawptr) -> u64

Raft_Log_Store_Start_Index_Func :: #type proc "c" (user_data: rawptr) -> u64

Raft_Log_Store_Last_Entry_Func :: #type proc "c" (user_data: rawptr) -> ^Raft_Log_Entry

Raft_Log_Store_Append_Func :: #type proc "c" (
	user_data: rawptr,
	term: u64,
	data: [^]byte,
	data_size: uintptr,
	log_timestamp: u64,
	has_crc32: bool,
	crc32: u32
) -> u64

Raft_Log_Store_Write_At_Func :: #type proc "c" (
	user_data: rawptr,
	index: u64,
	term: u64,
	data: [^]byte,
	data_size: uintptr,
	log_timestamp: u64,
	has_crc32: bool,
	crc32: u32
)

Raft_Log_Store_End_Of_Append_Batch_Func :: #type proc "c" (
	user_data: rawptr,
	start: u64,
	count: u64
)

Raft_Log_Store_Log_Entries_Func :: #type proc "c" (
	user_data: rawptr,
	entries: ^Raft_Log_Entry_Vec,
	start: u64,
	end: u64
)

Raft_Log_Store_Entry_At_Func :: #type proc "c" (
	user_data: rawptr,
	index: u64
) -> ^Raft_Log_Entry

Raft_Log_Store_Term_At_Func :: #type proc "c" (
	user_data: rawptr,
	index: u64
) -> u64

Raft_Log_Store_Pack_Func :: #type proc "c" (
	user_data: rawptr,
	index: u64,
	count: i32
) -> ^Raft_Buffer

Raft_Log_Store_Apply_Pack_Func :: #type proc "c" (
	user_data: rawptr,
	index: u64,
	pack: ^Raft_Buffer
)

Raft_Log_Store_Compact_Func :: #type proc "c" (
	user_data: rawptr,
	last_log_index: u64
) -> bool

Raft_Log_Store_Compact_Async_Func :: #type proc "c" (
	user_data: rawptr,
	last_log_index: u64
) -> bool

Raft_Log_Store_Flush_Func :: #type proc "c" (
	user_data: rawptr
) -> bool

Raft_Log_Store_Last_Durable_Index_Func :: #type proc "c" (
	user_data: rawptr
) -> u64

Raft_Log_Store_Ptr :: struct {}


Raft_State_Mgr_Load_Config_Func :: #type proc "c" (user_data: rawptr) -> ^Raft_Cluster_Config

Raft_State_Mgr_Save_Config_Func :: #type proc "c" (
	user_data: rawptr,
	config: ^Raft_Cluster_Config
)

Raft_State_Mgr_Read_State_Func :: #type proc "c" (user_data: rawptr) -> ^Raft_Srv_State

Raft_State_Mgr_Save_State_Func :: #type proc "c" (user_data: rawptr, state: ^Raft_Srv_State)

Raft_State_Mgr_Load_Log_Store_Func :: #type proc "c" (user_data: rawptr) -> ^Raft_Log_Store_Ptr

Raft_State_Mgr_Server_ID_Func :: #type proc "c" (user_data: rawptr) -> i32

Raft_State_Mgr_System_Exit_Func :: #type proc "c" (user_data: rawptr, exit_code: i32)

Raft_State_Mgr_Ptr :: struct {}

Raft_Counter :: struct {}
Raft_Gauge :: struct {}
Raft_Histogram :: struct {}

Raft_Append_Entries_Ptr :: struct {}

Raft_Server_Peer_Info :: struct {
	// Peer ID
	id: i32,
	// The last log index that the peer has, from this server's point of view.
	last_log_idx: u64,
	// The elapsed time since the last successful response from this peer, in microseconds.
	last_succ_resp_us: u64,
}

Raft_Server_Peer_Info_Vec :: struct {}

Raft_Server :: struct {}

Raft_Server_Ptr :: struct {}

Raft_Server_Priority_Set_Result :: enum i32 {
	Set       = 0,
	Broadcast = 1,
	Ignored   = 2,
}

Raft_CB_Req :: struct {}
Raft_CB_Resp :: struct {}

Raft_CB_Ctx :: union {
	Raft_CB_Req,
	Raft_CB_Resp,
}

Raft_CB_Type :: enum i32 {
	/*
	Got request from peer or client.

	ctx: pointer to request
	*/
	Process_Req = 1,

	Got_Append_Entry_Resp_From_Peer = 2,

	Append_Logs = 3,

	Heartbeat = 4,

	Joined_Cluster = 5,

	Become_Leader = 6,

	Request_Append_Entries = 7,

	Save_Snapshot = 8,

	New_Config = 9,

	Removed_From_Cluster = 10,

	Become_Follower = 11,

	Become_Fresh = 12,

	Become_Stale = 13,

	Got_Append_Entry_Req_From_Leader = 14,

	Out_Of_Log_Range_Warning = 15,

	Connection_Opened = 16,

	Connection_Closed = 17,

	New_Session_From_Leader = 18,

	State_Machine_Execution = 19,

	Sent_Append_Entries_Req = 20,

	Received_Append_Entries_Req = 21,

	Sent_Append_Entries_Resp = 22,

	Received_Append_Entries_Resp = 23,

	Auto_Adjust_Quorum = 24,

	Server_Join_Failed = 25,

	Snapshot_Creation_Begin = 26,

	Resignation_From_Leader = 27,

	Follower_Lost = 28,

	Received_Misbehaving_Message = 29,
}

Raft_Inc_Term_Handler :: #type proc "c" (user_data: rawptr) -> u64