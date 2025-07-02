package raft_test

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:thread"

import bop "../"

log_store_next_slot :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

log_store_start_index :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

log_store_last_entry :: proc "c" (user_data: rawptr) -> ^bop.Raft_Log_Entry {
	return nil
}

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

log_store_end_of_append_batch :: proc "c" (user_data: rawptr, start: u64, count: u64) {

}

log_store_log_entries :: proc "c" (
	user_data: rawptr,
	entries: ^bop.Raft_Log_Entry_Vec,
	start: u64,
	end: u64,
) {

}

log_store_entry_at :: proc "c" (user_data: rawptr, index: u64) -> ^bop.Raft_Log_Entry {
	return nil
}

log_store_term_at :: proc "c" (user_data: rawptr, index: u64) -> u64 {
	return 0
}

log_store_pack :: proc "c" (user_data: rawptr, index: u64, count: i32) -> ^bop.Raft_Buffer {
	return nil
}

log_store_apply_pack :: proc "c" (user_data: rawptr, index: u64, pack: ^bop.Raft_Buffer) {

}

log_store_compact :: proc "c" (user_data: rawptr, last_log_index: u64) -> bool {
	return true
}

log_store_compact_async :: proc "c" (user_data: rawptr, last_log_index: u64) -> bool {
	return true
}

log_store_flush :: proc "c" (user_data: rawptr) -> bool {
	return true
}

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

