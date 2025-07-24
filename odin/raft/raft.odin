package raft

import bop "../libbop"

import "base:runtime"
import c "core:c/libc"

Buffer :: bop.Raft_Buffer

buffer_new_from_io_vec :: proc "c" (
	io_vec: bop.IO_Vec,
) -> (
	b: ^Buffer,
	err: runtime.Allocator_Error,
) {
	if io_vec.base == nil || io_vec.len == 0 {
		return nil, .Invalid_Argument
	}
	b = buffer_new(c.size_t(io_vec.len))
	if b == nil {
		return nil, .Out_Of_Memory
	}
	data := buffer_data(b)
	ensure_contextless(data != nil)
	ensure_contextless(buffer_size(b) == c.size_t(io_vec.len))
	copy(data[0:io_vec.len], io_vec.base[0:io_vec.len])
	return b, nil
}

Buffer_Ptr :: bop.Raft_Buffer_Ptr

Async_U64_Ptr :: bop.Raft_Async_U64_Ptr
Async_U64_Done :: bop.Raft_Async_U64_Done

Async_Buffer_Ptr :: bop.Raft_Async_Buffer_Ptr
Async_Buffer_Done :: bop.Raft_Async_Buffer_Done

Cluster_Config :: bop.Raft_Cluster_Config
Cluster_Config_Ptr :: bop.Raft_Cluster_Config_Ptr
Srv_Config :: bop.Raft_Srv_Config
Srv_Config_Ptr :: bop.Raft_Srv_Config_Ptr
Srv_Config_Vec :: bop.Raft_Srv_Config_Vec
Srv_State :: bop.Raft_Srv_State

Snapshot :: bop.Raft_Snapshot

Log_Level :: bop.Raft_Log_Level

Log_Level_Names := bop.Raft_Log_Level_Names

Log_Level_To_Odin_Level := bop.Raft_Log_Level_To_Odin_Level


Logger_Write_Func :: bop.Raft_Logger_Write_Func

Logger_Ptr :: bop.Raft_Logger_Ptr

Locking_Method_Type :: bop.Raft_Locking_Method_Type

Return_Method_Type :: bop.Raft_Return_Method_Type

SSL_CTX :: bop.SSL_CTX

Asio_Ssl_Ctx_Provider :: bop.Raft_Asio_Ssl_Ctx_Provider
Asio_Worker_Start :: bop.Raft_Asio_Worker_Start
Asio_Worker_Stop :: bop.Raft_Asio_Worker_Stop
Asio_Verify_Sn :: bop.Raft_Asio_Verify_Sn
Asio_Custom_Resolver_Response :: bop.Raft_Asio_Custom_Resolver_Response
Asio_Custom_Resolver :: bop.Raft_Asio_Custom_Resolver
Asio_Corrupted_Msg_Handler :: bop.Raft_Asio_Corrupted_Msg_Handler

Delayed_Task :: bop.Raft_Delayed_Task
Asio_RPC_Listener_Ptr :: bop.Raft_Asio_RPC_Listener_Ptr
Asio_RPC_Client_Ptr :: bop.Raft_Asio_RPC_Client_Ptr

Asio_Options :: bop.Raft_Asio_Options

Asio_Service_Ptr :: bop.Raft_Asio_Service_Ptr

Params :: bop.Raft_Params

FSM_Commit_Func :: bop.Raft_FSM_Commit_Func

FSM_Cluster_Config_Func :: bop.Raft_FSM_Cluster_Config_Func

FSM_Rollback_Func :: bop.Raft_FSM_Rollback_Func

FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func :: bop.Raft_FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func

FSM_Save_Snapshot_Func :: bop.Raft_FSM_Save_Snapshot_Func

FSM_Apply_Snapshot_Func :: bop.Raft_FSM_Apply_Snapshot_Func

FSM_Read_Snapshot_Func :: bop.Raft_FSM_Read_Snapshot_Func

FSM_Free_User_Snapshot_Ctx_Func :: bop.Raft_FSM_Free_User_Snapshot_Ctx_Func

FSM_Last_Snapshot_Func :: bop.Raft_FSM_Last_Snapshot_Func

FSM_Last_Commit_Index_Func :: bop.Raft_FSM_Last_Commit_Index_Func

FSM_Create_Snapshot_Func :: bop.Raft_FSM_Create_Snapshot_Func

FSM_Chk_Create_Snapshot_Func :: bop.Raft_FSM_Chk_Create_Snapshot_Func

FSM_Allow_Leadership_Transfer_Func :: bop.Raft_FSM_Allow_Leadership_Transfer_Func

FSM_Adjust_Commit_Index_Params :: bop.Raft_FSM_Adjust_Commit_Index_Params

FSM_Adjust_Commit_Index_Func :: bop.Raft_FSM_Adjust_Commit_Index_Func

FSM_Funcs :: bop.Raft_FSM_Funcs

FSM_Ptr :: bop.Raft_FSM_Ptr

Log_Entry :: bop.Raft_Log_Entry

Log_Entry_Ptr :: bop.Raft_Log_Entry_Ptr

Log_Entry_Vec :: bop.Raft_Log_Entry_Vec

Log_Store_Next_Slot_Func :: bop.Raft_Log_Store_Next_Slot_Func

Log_Store_Start_Index_Func :: bop.Raft_Log_Store_Start_Index_Func

Log_Store_Last_Entry_Func :: bop.Raft_Log_Store_Last_Entry_Func

Log_Store_Append_Func :: bop.Raft_Log_Store_Append_Func

Log_Store_Write_At_Func :: bop.Raft_Log_Store_Write_At_Func

Log_Store_End_Of_Append_Batch_Func :: bop.Raft_Log_Store_End_Of_Append_Batch_Func

Log_Store_Log_Entries_Func :: bop.Raft_Log_Store_Log_Entries_Func

Log_Store_Entry_At_Func :: bop.Raft_Log_Store_Entry_At_Func

Log_Store_Term_At_Func :: bop.Raft_Log_Store_Term_At_Func

Log_Store_Pack_Func :: bop.Raft_Log_Store_Pack_Func

Log_Store_Apply_Pack_Func :: bop.Raft_Log_Store_Apply_Pack_Func

Log_Store_Compact_Func :: bop.Raft_Log_Store_Compact_Func

Log_Store_Compact_Async_Func :: bop.Raft_Log_Store_Compact_Async_Func

Log_Store_Flush_Func :: bop.Raft_Log_Store_Flush_Func

Log_Store_Last_Durable_Index_Func :: bop.Raft_Log_Store_Last_Durable_Index_Func

Log_Store_Ptr :: bop.Raft_Log_Store_Ptr


State_Mgr_Load_Config_Func :: bop.Raft_State_Mgr_Load_Config_Func

State_Mgr_Save_Config_Func :: bop.Raft_State_Mgr_Save_Config_Func

State_Mgr_Read_State_Func :: bop.Raft_State_Mgr_Read_State_Func

State_Mgr_Save_State_Func :: bop.Raft_State_Mgr_Save_State_Func

State_Mgr_Load_Log_Store_Func :: bop.Raft_State_Mgr_Load_Log_Store_Func

State_Mgr_Server_ID_Func :: bop.Raft_State_Mgr_Server_ID_Func

State_Mgr_System_Exit_Func :: bop.Raft_State_Mgr_System_Exit_Func

State_Mgr_Ptr :: bop.Raft_State_Mgr_Ptr

Counter :: bop.Raft_Counter
Gauge :: bop.Raft_Gauge
Histogram :: bop.Raft_Histogram

Append_Entries_Ptr :: bop.Raft_Append_Entries_Ptr

Server_Peer_Info :: bop.Raft_Server_Peer_Info

Server_Peer_Info_Vec :: bop.Raft_Server_Peer_Info_Vec

Server :: bop.Raft_Server

Server_Ptr :: bop.Raft_Server_Ptr

Server_Priority_Set_Result :: bop.Raft_Server_Priority_Set_Result

CB_Req :: bop.Raft_CB_Req

CB_Resp :: bop.Raft_CB_Resp

CB_Ctx :: bop.Raft_CB_Ctx

CB_Type :: bop.Raft_CB_Type

Inc_Term_Handler :: bop.Raft_Inc_Term_Handler


buffer_new :: bop.raft_buffer_new
buffer_free :: bop.raft_buffer_free
buffer_container_size :: bop.raft_buffer_container_size
buffer_data :: bop.raft_buffer_data
buffer_size :: bop.raft_buffer_size
buffer_pos :: bop.raft_buffer_pos
buffer_set_pos :: bop.raft_buffer_set_pos

async_u64_make :: bop.raft_async_u64_make
async_u64_delete :: bop.raft_async_u64_delete
async_u64_get_user_data :: bop.raft_async_u64_get_user_data
async_u64_set_user_data :: bop.raft_async_u64_set_user_data
async_u64_get_when_ready :: bop.raft_async_u64_get_when_ready
async_u64_set_when_ready :: bop.raft_async_u64_set_when_ready

async_buffer_make :: bop.raft_async_buffer_make
async_buffer_delete :: bop.raft_async_buffer_delete
async_buffer_get_user_data :: bop.raft_async_buffer_get_user_data
async_buffer_set_user_data :: bop.raft_async_buffer_set_user_data
async_buffer_get_when_ready :: bop.raft_async_buffer_get_when_ready
async_buffer_set_when_ready :: bop.raft_async_buffer_set_when_ready

snapshot_serialize :: bop.raft_snapshot_serialize
snapshot_deserialize :: bop.raft_snapshot_deserialize

cluster_config_new :: bop.raft_cluster_config_new
cluster_config_free :: bop.raft_cluster_config_free
cluster_config_ptr_create :: bop.raft_cluster_config_ptr_create

cluster_config_ptr_delete :: bop.raft_cluster_config_ptr_delete
cluster_config_serialize :: bop.raft_cluster_config_serialize
cluster_config_deserialize :: bop.raft_cluster_config_deserialize
cluster_config_log_idx :: bop.raft_cluster_config_log_idx
cluster_config_prev_log_idx :: bop.raft_cluster_config_prev_log_idx

cluster_config_is_async_replication :: bop.raft_cluster_config_is_async_replication

cluster_config_user_ctx :: bop.raft_cluster_config_user_ctx

cluster_config_user_ctx_size :: bop.raft_cluster_config_user_ctx_size

cluster_config_servers_size :: bop.raft_cluster_config_servers_size

cluster_config_server :: bop.raft_cluster_config_server

srv_config_vec_create :: bop.raft_srv_config_vec_create

srv_config_vec_delete :: bop.raft_srv_config_vec_delete

srv_config_vec_size :: bop.raft_srv_config_vec_size

srv_config_vec_get :: bop.raft_srv_config_vec_get

srv_config_ptr_make :: bop.raft_srv_config_ptr_make

srv_config_ptr_delete :: bop.raft_srv_config_ptr_delete

srv_config_make :: bop.raft_srv_config_make

srv_config_delete :: bop.raft_srv_config_delete

srv_config_id :: bop.raft_srv_config_id

srv_config_dc_id :: bop.raft_srv_config_dc_id

srv_config_endpoint :: bop.raft_srv_config_endpoint

srv_config_endpoint_size :: bop.raft_srv_config_endpoint_size

srv_config_aux :: bop.raft_srv_config_aux

srv_config_aux_size :: bop.raft_srv_config_aux_size

srv_config_is_learner :: bop.raft_srv_config_is_learner

srv_config_is_new_joiner :: bop.raft_srv_config_is_new_joiner

srv_config_priority :: bop.raft_srv_config_priority

srv_state_serialize :: bop.raft_srv_state_serialize

srv_state_deserialize :: bop.raft_srv_state_deserialize

srv_state_delete :: bop.raft_srv_state_delete

srv_state_term :: bop.raft_srv_state_term

srv_state_voted_for :: bop.raft_srv_state_voted_for

srv_state_is_election_timer_allowed :: bop.raft_srv_state_is_election_timer_allowed

srv_state_is_catching_up :: bop.raft_srv_state_is_catching_up

srv_state_is_receiving_snapshot :: bop.raft_srv_state_is_receiving_snapshot

logger_make :: bop.raft_logger_make

logger_delete :: bop.raft_logger_delete

delayed_task_make :: bop.raft_delayed_task_make

delayed_task_delete :: bop.raft_delayed_task_delete

delayed_task_cancel :: bop.raft_delayed_task_cancel

delayed_task_reset :: bop.raft_delayed_task_reset

delayed_task_type :: bop.raft_delayed_task_type

delayed_task_user_data :: bop.raft_delayed_task_user_data

asio_service_make :: bop.raft_asio_service_make

asio_service_stop :: bop.raft_asio_service_stop

asio_service_get_active_workers :: bop.raft_asio_service_get_active_workers

asio_service_schedule :: bop.raft_asio_service_schedule

asio_rpc_listener_make :: bop.raft_asio_rpc_listener_make

asio_rpc_client_make :: bop.raft_asio_rpc_client_make

asio_rpc_client_delete :: bop.raft_asio_rpc_client_delete

fsm_make :: bop.raft_fsm_make

fsm_delete :: bop.raft_fsm_delete

log_entry_make :: bop.raft_log_entry_make

log_entry_ptr_ref_count :: bop.raft_log_entry_ptr_ref_count

log_entry_ptr_retain :: bop.raft_log_entry_ptr_retain

log_entry_ptr_release :: bop.raft_log_entry_ptr_release

log_entry_delete :: bop.raft_log_entry_delete

log_entry_vec_push :: bop.raft_log_entry_vec_push

log_store_make :: bop.raft_log_store_make

log_store_delete :: bop.raft_log_store_delete

state_mgr_make :: bop.raft_state_mgr_make

state_mgr_delete :: bop.raft_state_mgr_delete

server_peer_info_vec_make :: bop.raft_server_peer_info_vec_make

server_peer_info_vec_delete :: bop.raft_server_peer_info_vec_delete

server_peer_info_vec_size :: bop.raft_server_peer_info_vec_size

server_peer_info_vec_get :: bop.raft_server_peer_info_vec_get

params_make :: bop.raft_params_make

params_delete :: bop.raft_params_delete

server_launch :: bop.raft_server_launch

server_stop :: bop.raft_server_stop

server_get :: bop.raft_server_get

server_is_initialized :: bop.raft_server_is_initialized

server_is_catching_up :: bop.raft_server_is_catching_up

server_is_receiving_snapshot :: bop.raft_server_is_receiving_snapshot

server_add_srv :: bop.raft_server_add_srv

server_remove_srv :: bop.raft_server_remove_srv

server_flip_learner_flag :: bop.raft_server_flip_learner_flag

server_append_entries :: bop.raft_server_append_entries

server_set_priority :: bop.raft_server_set_priority

server_broadcast_priority_change :: bop.raft_server_broadcast_priority_change

server_yield_leadership :: bop.raft_server_yield_leadership

server_request_leadership :: bop.raft_server_request_leadership

server_restart_election_timer :: bop.raft_server_restart_election_timer

server_set_user_ctx :: bop.raft_server_set_user_ctx

server_get_user_ctx :: bop.raft_server_get_user_ctx

server_get_snapshot_sync_ctx_timeout :: bop.raft_server_get_snapshot_sync_ctx_timeout

server_get_id :: bop.raft_server_get_id

server_get_term :: bop.raft_server_get_term

server_get_log_term :: bop.raft_server_get_log_term

server_get_last_log_term :: bop.raft_server_get_last_log_term

server_get_last_log_idx :: bop.raft_server_get_last_log_idx

server_get_committed_log_idx :: bop.raft_server_get_committed_log_idx

server_get_target_committed_log_idx :: bop.raft_server_get_target_committed_log_idx

server_get_leader_committed_log_idx :: bop.raft_server_get_leader_committed_log_idx

server_get_log_idx_at_becoming_leader :: bop.raft_server_get_log_idx_at_becoming_leader

server_get_expected_committed_log_idx :: bop.raft_server_get_expected_committed_log_idx

server_get_config :: bop.raft_server_get_config

server_get_dc_id :: bop.raft_server_get_dc_id

server_get_aux :: bop.raft_server_get_aux

server_get_leader :: bop.raft_server_get_leader

server_is_leader :: bop.raft_server_is_leader

server_is_leader_alive :: bop.raft_server_is_leader_alive

server_get_srv_config :: bop.raft_server_get_srv_config

server_get_srv_config_all :: bop.raft_server_get_srv_config_all

server_update_srv_config :: bop.raft_server_update_srv_config

server_get_peer_info :: bop.raft_server_get_peer_info

server_get_peer_info_all :: bop.raft_server_get_peer_info_all

server_shutdown :: bop.raft_server_shutdown

server_start_server :: bop.raft_server_start_server

server_stop_server :: bop.raft_server_stop_server

server_send_reconnect_request :: bop.raft_server_send_reconnect_request

server_update_params :: bop.raft_server_update_params

server_get_current_params :: bop.raft_server_get_current_params

server_get_stat_counter :: bop.raft_server_get_stat_counter

server_get_stat_gauge :: bop.raft_server_get_stat_gauge

server_get_stat_histogram :: bop.raft_server_get_stat_histogram

server_reset_counter :: bop.raft_server_reset_counter

server_reset_gauge :: bop.raft_server_reset_gauge

server_reset_histogram :: bop.raft_server_reset_histogram

server_reset_all_stats :: bop.raft_server_reset_all_stats

server_set_inc_term_func :: bop.raft_server_set_inc_term_func

server_pause_state_machine_execution :: bop.raft_server_pause_state_machine_execution

server_resume_state_machine_execution :: bop.raft_server_resume_state_machine_execution

server_is_state_machine_execution_paused :: bop.raft_server_is_state_machine_execution_paused

server_wait_for_state_machine_pause :: bop.raft_server_wait_for_state_machine_pause

server_notify_log_append_completion :: bop.raft_server_notify_log_append_completion

server_create_snapshot :: bop.raft_server_create_snapshot

server_schedule_snapshot_creation :: bop.raft_server_schedule_snapshot_creation

server_get_last_snapshot_idx :: bop.raft_server_get_last_snapshot_idx

mdbx_state_mgr_open :: bop.raft_mdbx_state_mgr_open

mdbx_log_store_open :: bop.raft_mdbx_log_store_open
