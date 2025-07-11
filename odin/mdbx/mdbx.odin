package mdbx

import bop "../libbop"

Max_DBI :: bop.MDBX_Max_DBI
Max_Data_Size :: bop.MDBX_Max_Data_Size
Min_Page_Size :: bop.MDBX_Min_Page_Size
Max_Page_Size :: bop.MDBX_Max_Page_Size

Env :: bop.MDBX_Env
DBI :: bop.MDBX_DBI
Cursor :: bop.MDBX_Cursor
Txn :: bop.MDBX_Txn

Val :: bop.MDBX_Val

val_i32 :: #force_inline proc "c" (v: ^i32) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 4,
    }
}

val_u32 :: #force_inline proc "c" (v: ^u32) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 4,
    }
}

val_f32 :: #force_inline proc "c" (v: ^f32) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 4,
    }
}

val_i64 :: #force_inline proc "c" (v: ^i64) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 8,
    }
}

val_u64 :: #force_inline proc "c" (v: ^u64) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 8,
    }
}

val_f64 :: #force_inline proc "c" (v: ^f64) -> Val {
    return Val{
        base = cast(^byte)v,
        len = 8,
    }
}

DBI_State :: bop.MDBX_DBI_State

Assert_Func :: bop.MDBX_Assert_Func

Table_Enum_Func :: bop.MDBX_Table_Enum_Func

Preserve_Func :: bop.MDBX_Preserve_Func

Predicate_Func :: bop.MDBX_Predicate_Func

Reader_List_Func :: bop.MDBX_Reader_List_Func

Hsr_Func :: bop.MDBX_Hsr_Func

Canary :: bop.MDBX_Canary

Env_Flags :: bop.MDBX_Env_Flags

Txn_Flags :: bop.MDBX_Txn_Flags

DB_Flags :: bop.MDBX_DB_Flags

Put_Flags :: bop.MDBX_Put_Flags

Copy_Flags :: bop.MDBX_Copy_Flags

Cursor_Op :: bop.MDBX_Cursor_Op

Error :: bop.MDBX_Error

Option :: bop.MDBX_Option

Env_Delete_Mode :: bop.MDBX_Env_Delete_Mode

Stat :: bop.MDBX_Stat

Env_Info :: bop.MDBX_Env_Info

Warmup_Flags :: bop.MDBX_Warmup_Flags

Txn_Info :: bop.MDBX_Txn_Info

Commit_Latency :: bop.MDBX_Commit_Latency

env_set_assert :: bop.mdbx_env_set_assert

strerror :: bop.mdbx_strerror

liberr2str :: bop.mdbx_liberr2str

env_create :: bop.mdbx_env_create

env_set_option :: bop.mdbx_env_set_option

env_get_option :: bop.mdbx_env_get_option

env_open :: bop.mdbx_env_open

env_delete :: bop.mdbx_env_delete

env_copy :: bop.mdbx_env_copy

txn_copy2pathname :: bop.mdbx_txn_copy2pathname

env_stat_ex :: bop.mdbx_env_stat_ex

env_info_ex :: bop.mdbx_env_info_ex

env_sync_ex :: bop.mdbx_env_sync_ex

env_close_ex :: bop.mdbx_env_close_ex

nv_warmup :: bop.mdbx_env_warmup

env_set_flags :: bop.mdbx_env_set_flags

env_get_flags :: bop.mdbx_env_get_flags

env_get_path :: bop.mdbx_env_get_path

env_get_fd :: bop.mdbx_env_get_fd

env_set_geometry :: bop.mdbx_env_set_geometry

is_readahead_reasonable :: bop.mdbx_is_readahead_reasonable

limits_dbsize_min :: bop.mdbx_limits_dbsize_min

limits_dbsize_max :: bop.mdbx_limits_dbsize_max

get_sysraminfo :: bop.mdbx_get_sysraminfo

env_set_userctx :: bop.mdbx_env_set_userctx

env_get_userctx :: bop.mdbx_env_get_userctx

txn_begin_ex :: bop.mdbx_txn_begin_ex

txn_set_userctx :: bop.mdbx_txn_set_userctx

txn_get_userctx :: bop.mdbx_txn_get_userctx

txn_info :: bop.mdbx_txn_info

txn_env :: bop.mdbx_txn_env

txn_flags :: bop.mdbx_txn_flags

txn_id :: bop.mdbx_txn_id

txn_commit_ex :: bop.mdbx_txn_commit_ex

txn_commit :: bop.mdbx_txn_commit

txn_abort :: bop.mdbx_txn_abort

txn_break :: bop.mdbx_txn_break

txn_reset :: bop.mdbx_txn_reset

txn_park :: bop.mdbx_txn_park

txn_unpark :: bop.mdbx_txn_unpark

txn_renew :: bop.mdbx_txn_renew

canary_put :: bop.mdbx_canary_put

canary_get :: bop.mdbx_canary_get

dbi_open :: bop.mdbx_dbi_open

dbi_open2 :: bop.mdbx_dbi_open2

dbi_rename :: bop.mdbx_dbi_rename
dbi_rename2 :: bop.mdbx_dbi_rename2

enumerate_tables :: bop.mdbx_enumerate_tables

dbi_stat :: bop.mdbx_dbi_stat

dbi_dupsort_depthmask :: bop.mdbx_dbi_dupsort_depthmask

dbi_flags_ex :: bop.mdbx_dbi_flags_ex

dbi_close :: bop.mdbx_dbi_close

drop :: bop.mdbx_drop

get :: bop.mdbx_get

get_ex :: bop.mdbx_get_ex

get_equal_or_great :: bop.mdbx_get_equal_or_great

put :: bop.mdbx_put

replace :: bop.mdbx_replace

replace_ex :: bop.mdbx_replace_ex

del :: bop.mdbx_del

cursor_create :: bop.mdbx_cursor_create

cursor_set_userctx :: bop.mdbx_cursor_set_userctx

cursor_get_userctx :: bop.mdbx_cursor_get_userctx

cursor_bind :: bop.mdbx_cursor_bind

cursor_unbind :: bop.mdbx_cursor_unbind

cursor_reset :: bop.mdbx_cursor_reset

cursor_open :: bop.mdbx_cursor_open

cursor_close :: bop.mdbx_cursor_close

txn_release_all_cursors :: bop.mdbx_txn_release_all_cursors

cursor_renew :: bop.mdbx_cursor_renew

cursor_txn :: bop.mdbx_cursor_txn

cursor_dbi :: bop.mdbx_cursor_dbi

cursor_copy :: bop.mdbx_cursor_copy

cursor_compare :: bop.mdbx_cursor_compare

cursor_get :: bop.mdbx_cursor_get

cursor_ignord :: bop.mdbx_cursor_ignord

cursor_scan :: bop.mdbx_cursor_scan

cursor_scan_from :: bop.mdbx_cursor_scan_from

cursor_get_batch :: bop.mdbx_cursor_get_batch

cursor_put :: bop.mdbx_cursor_put

cursor_del :: bop.mdbx_cursor_del

cursor_count :: bop.mdbx_cursor_count

cursor_eof :: bop.mdbx_cursor_eof

cursor_on_first :: bop.mdbx_cursor_on_first

cursor_on_first_dup :: bop.mdbx_cursor_on_first_dup

cursor_on_last :: bop.mdbx_cursor_on_last

cursor_on_last_dup :: bop.mdbx_cursor_on_last_dup

estimate_distance :: bop.mdbx_estimate_distance

estimate_move :: bop.mdbx_estimate_move

estimate_range :: bop.mdbx_estimate_range

is_dirty :: bop.mdbx_is_dirty

dbi_sequence :: bop.mdbx_dbi_sequence

reader_list :: bop.mdbx_reader_list

reader_check :: bop.mdbx_reader_check

txn_straggler :: bop.mdbx_txn_straggler

thread_register :: bop.mdbx_thread_register

thread_unregister :: bop.mdbx_thread_unregister

env_set_hsr :: bop.mdbx_env_set_hsr

env_get_hsr :: bop.mdbx_env_get_hsr

txn_lock :: bop.mdbx_txn_lock

txn_unlock :: bop.mdbx_txn_unlock

env_open_for_recovery :: bop.mdbx_env_open_for_recovery

