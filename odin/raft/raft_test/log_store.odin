package raft_test

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:thread"

import bop "../"

Log_Store :: struct {}

/*
The first available slot of the store, starts with 1

@return Last log index number + 1
*/
log_store_next_slot :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

/*
The start index of the log store, at the very beginning, it must be 1.
However, after some compact actions, this could be anything equal to or
greater than one
*/
log_store_start_index :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

/*
The last log entry in store.

@return If no log entry exists: a dummy constant entry with
        value set to null and term set to zero.
*/
log_store_last_entry :: proc "c" (user_data: rawptr) -> ^bop.Raft_Log_Entry {
	return nil
}

/*
Append a log entry to store.

@param entry Log entry
@return Log index number.
*/
log_store_append :: proc "c" (
	user_data: rawptr,
	term: u64,
	data: [^]byte,
	data_size: uintptr,
	log_timestamp: u64,
	has_crc32: bool,
	crc32: u32,
) -> u64 {
	return 0
}

/*
Overwrite a log entry at the given `index`.
This API should make sure that all log entries
after the given `index` should be truncated (if exist),
as a result of this function call.

@param index Log index number to overwrite.
@param entry New log entry to overwrite.
*/
log_store_write_at :: proc "c" (
	user_data: rawptr,
	index: u64,
	term: u64,
	data: [^]byte,
	data_size: uintptr,
	log_timestamp: u64,
	has_crc32: bool,
	crc32: u32,
) {

}

/*
Invoked after a batch of logs is written as a part of
a single append_entries request.

@param start The start log index number (inclusive)
@param cnt The number of log entries written.
*/
log_store_end_of_append_batch :: proc "c" (user_data: rawptr, start: u64, count: u64) {

}

/*
Get log entries with index [start, end).

Return nullptr to indicate error if any log entry within the requested range
could not be retrieved (e.g. due to external log truncation).

@param start The start log index number (inclusive).
@param end The end log index number (exclusive).
@return The log entries between [start, end).
*/
log_store_log_entries :: proc "c" (
	user_data: rawptr,
	entries: ^bop.Raft_Log_Entry_Vec,
	start: u64,
	end: u64,
) {

}

/*
Get the log entry at the specified log index number.

@param index Should be equal to or greater than 1.
@return The log entry or null if index >= this->next_slot().
*/
log_store_entry_at :: proc "c" (user_data: rawptr, index: u64) -> ^bop.Raft_Log_Entry {
	return nil
}

/*
Get the term for the log entry at the specified index.
Suggest to stop the system if the index >= this->next_slot()

@param index Should be equal to or greater than 1.
@return The term for the specified log entry, or
        0 if index < this->start_index().
*/
log_store_term_at :: proc "c" (user_data: rawptr, index: u64) -> u64 {
	return 0
}

/*
Pack the given number of log items starting from the given index.

@param index The start log index number (inclusive).
@param cnt The number of logs to pack.
@return Packed (encoded) logs.
*/
log_store_pack :: proc "c" (user_data: rawptr, index: u64, count: i32) -> ^bop.Raft_Buffer {
	return nil
}

/*
Apply the log pack to current log store, starting from index.

@param index The start log index number (inclusive).
@param Packed logs.
*/
log_store_apply_pack :: proc "c" (user_data: rawptr, index: u64, pack: ^bop.Raft_Buffer) {

}

/*
Compact the log store by purging all log entries,
including the given log index number.

If current maximum log index is smaller than given `last_log_index`,
set start log index to `last_log_index + 1`.

@param last_log_index Log index number that will be purged up to (inclusive).
@return `true` on success.
*/
log_store_compact :: proc "c" (user_data: rawptr, last_log_index: u64) -> bool {
	return true
}

/*
Compact the log store by purging all log entries,
including the given log index number.

Unlike `compact`, this API allows to execute the log compaction in background
asynchronously, aiming at reducing the client-facing latency caused by the
log compaction.

This function call may return immediately, but after this function
call, following `start_index` should return `last_log_index + 1` even
though the log compaction is still in progress. In the meantime, the
actual job incurring disk IO can run in background. Once the job is done,
`when_done` should be invoked.

@param last_log_index Log index number that will be purged up to (inclusive).
@param when_done Callback function that will be called after
                 the log compaction is done.
*/
log_store_compact_async :: proc "c" (user_data: rawptr, last_log_index: u64) -> bool {
	return true
}

/*
Synchronously flush all log entries in this log store to the backing storage
so that all log entries are guaranteed to be durable upon process crash.

@return `true` on success.
*/
log_store_flush :: proc "c" (user_data: rawptr) -> bool {
	return true
}

/*
(Experimental)
This API is used only when `raft_params::parallel_log_appending_` flag is set.
Please refer to the comment of the flag.

@return The last durable log index.
*/
log_store_last_durable_index :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

wire_log_store :: proc() {
	log_store := bop.raft_log_store_make(
		nil,
		log_store_next_slot,
		log_store_start_index,
		log_store_last_entry,
		log_store_append,
		log_store_write_at,
		log_store_end_of_append_batch,
		log_store_log_entries,
		log_store_entry_at,
		log_store_term_at,
		log_store_pack,
		log_store_apply_pack,
		log_store_compact,
		log_store_compact_async,
		log_store_flush,
		log_store_last_durable_index,
	)
	ensure(log_store != nil, "log_store is nil")
	bop.raft_log_store_delete(log_store)
}

