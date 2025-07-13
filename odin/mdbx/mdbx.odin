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

/*
Set all size-related parameters of environment, including page size
and the min/max size of the memory map.

In contrast to LMDB, the MDBX provide automatic size management of an
database according the given parameters, including shrinking and resizing
on the fly. From user point of view all of these just working. Nevertheless,
it is reasonable to know some details in order to make optimal decisions
when choosing parameters.

@see mdbx_env_info_ex()

Both @ref mdbx_env_set_geometry() and legacy @ref mdbx_env_set_mapsize() are
inapplicable to read-only opened environment.

Both @ref mdbx_env_set_geometry() and legacy @ref mdbx_env_set_mapsize()
could be called either before or after @ref mdbx_env_open(), either within
the write transaction running by current thread or not:

 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
   was called BEFORE @ref mdbx_env_open(), i.e. for closed environment, then
   the specified parameters will be used for new database creation,
   or will be applied during opening if database exists and no other process
   using it.

   If the database is already exist, opened with @ref MDBX_EXCLUSIVE or not
   used by any other process, and parameters specified by
   @ref mdbx_env_set_geometry() are incompatible (i.e. for instance,
   different page size) then @ref mdbx_env_open() will return
   @ref MDBX_INCOMPATIBLE error.

   In another way, if database will opened read-only or will used by other
   process during calling @ref mdbx_env_open() that specified parameters will
   silently discarded (open the database with @ref MDBX_EXCLUSIVE flag
   to avoid this).

 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
   was called after @ref mdbx_env_open() WITHIN the write transaction running
   by current thread, then specified parameters will be applied as a part of
   write transaction, i.e. will not be completely visible to any others
   processes until the current write transaction has been committed by the
   current process. However, if transaction will be aborted, then the
   database file will be reverted to the previous size not immediately, but
   when a next transaction will be committed or when the database will be
   opened next time.

 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
   was called after @ref mdbx_env_open() but OUTSIDE a write transaction,
   then MDBX will execute internal pseudo-transaction to apply new parameters
   (but only if anything has been changed), and changes be visible to any
   others processes immediately after successful completion of function.

Essentially a concept of "automatic size management" is simple and useful:
 - There are the lower and upper bounds of the database file size;
 - There is the growth step by which the database file will be increased,
   in case of lack of space;
 - There is the threshold for unused space, beyond which the database file
   will be shrunk;
 - The size of the memory map is also the maximum size of the database;
 - MDBX will automatically manage both the size of the database and the size
   of memory map, according to the given parameters.

So, there some considerations about choosing these parameters:
 - The lower bound allows you to prevent database shrinking below certain
   reasonable size to avoid unnecessary resizing costs.
 - The upper bound allows you to prevent database growth above certain
   reasonable size. Besides, the upper bound defines the linear address space
   reservation in each process that opens the database. Therefore changing
   the upper bound is costly and may be required reopening environment in
   case of @ref MDBX_UNABLE_EXTEND_MAPSIZE errors, and so on. Therefore, this
   value should be chosen reasonable large, to accommodate future growth of
   the database.
 - The growth step must be greater than zero to allow the database to grow,
   but also reasonable not too small, since increasing the size by little
   steps will result a large overhead.
 - The shrink threshold must be greater than zero to allow the database
   to shrink but also reasonable not too small (to avoid extra overhead) and
   not less than growth step to avoid up-and-down flouncing.
 - The current size (i.e. `size_now` argument) is an auxiliary parameter for
   simulation legacy @ref mdbx_env_set_mapsize() and as workaround Windows
   issues (see below).

Unfortunately, Windows has is a several issue
with resizing of memory-mapped file:
 - Windows unable shrinking a memory-mapped file (i.e memory-mapped section)
   in any way except unmapping file entirely and then map again. Moreover,
   it is impossible in any way when a memory-mapped file is used more than
   one process.
 - Windows does not provide the usual API to augment a memory-mapped file
   (i.e. a memory-mapped partition), but only by using "Native API"
   in an undocumented way.

MDBX bypasses all Windows issues, but at a cost:
 - Ability to resize database on the fly requires an additional lock
   and release `SlimReadWriteLock` during each read-only transaction.
 - During resize all in-process threads should be paused and then resumed.
 - Shrinking of database file is performed only when it used by single
   process, i.e. when a database closes by the last process or opened
   by the first.
 = Therefore, the size_now argument may be useful to set database size
   by the first process which open a database, and thus avoid expensive
   remapping further.

For create a new database with particular parameters, including the page
size, @ref mdbx_env_set_geometry() should be called after
@ref mdbx_env_create() and before @ref mdbx_env_open(). Once the database is
created, the page size cannot be changed. If you do not specify all or some
of the parameters, the corresponding default values will be used. For
instance, the default for database size is 10485760 bytes.

If the mapsize is increased by another process, MDBX silently and
transparently adopt these changes at next transaction start. However,
@ref mdbx_txn_begin() will return @ref MDBX_UNABLE_EXTEND_MAPSIZE if new
mapping size could not be applied for current process (for instance if
address space is busy).  Therefore, in the case of
@ref MDBX_UNABLE_EXTEND_MAPSIZE error you need close and reopen the
environment to resolve error.

@note Actual values may be different than your have specified because of
rounding to specified database page size, the system page size and/or the
size of the system virtual memory management unit. You can get actual values
by @ref mdbx_env_info_ex() or see by using the tool `mdbx_chk` with the `-v`
option.

Legacy @ref mdbx_env_set_mapsize() correspond to calling
@ref mdbx_env_set_geometry() with the arguments `size_lower`, `size_now`,
`size_upper` equal to the `size` and `-1` (i.e. default) for all other
parameters.

@param [in] env         An environment handle returned
                        by @ref mdbx_env_create()

@param [in] size_lower  The lower bound of database size in bytes.
                        Zero value means "minimal acceptable",
                        and negative means "keep current or use default".

@param [in] size_now    The size in bytes to setup the database size for
                        now. Zero value means "minimal acceptable", and
                        negative means "keep current or use default". So,
                        it is recommended always pass -1 in this argument
                        except some special cases.

@param [in] size_upper The upper bound of database size in bytes.
                       Zero value means "minimal acceptable",
                       and negative means "keep current or use default".
                       It is recommended to avoid change upper bound while
                       database is used by other processes or threaded
                       (i.e. just pass -1 in this argument except absolutely
                       necessary). Otherwise you must be ready for
                       @ref MDBX_UNABLE_EXTEND_MAPSIZE error(s), unexpected
                       pauses during remapping and/or system errors like
                       "address busy", and so on. In other words, there
                       is no way to handle a growth of the upper bound
                       robustly because there may be a lack of appropriate
                       system resources (which are extremely volatile in
                       a multi-process multi-threaded environment).

@param [in] growth_step  The growth step in bytes, must be greater than
                         zero to allow the database to grow. Negative value
                         means "keep current or use default".

@param [in] shrink_threshold  The shrink threshold in bytes, must be greater
                              than zero to allow the database to shrink and
                              greater than growth_step to avoid shrinking
                              right after grow.
                              Negative value means "keep current
                              or use default". Default is 2*growth_step.

@param [in] pagesize          The database page size for new database
                              creation or -1 otherwise. Once the database
                              is created, the page size cannot be changed.
                              Must be power of 2 in the range between
                              @ref MDBX_MIN_PAGESIZE and
                              @ref MDBX_MAX_PAGESIZE. Zero value means
                              "minimal acceptable", and negative means
                              "keep current or use default".

@returns A non-zero error value on failure and 0 on success,
         some possible errors are:
@retval MDBX_EINVAL    An invalid parameter was specified,
                       or the environment has an active write transaction.
@retval MDBX_EPERM     Two specific cases for Windows:
                       1) Shrinking was disabled before via geometry settings
                       and now it enabled, but there are reading threads that
                       don't use the additional `SRWL` (which is required to
                       avoid Windows issues).
                       2) Temporary close memory mapped is required to change
                       geometry, but there read transaction(s) is running
                       and no corresponding thread(s) could be suspended
                       since the @ref MDBX_NOSTICKYTHREADS mode is used.
@retval MDBX_EACCESS   The environment opened in read-only.
@retval MDBX_MAP_FULL  Specified size smaller than the space already
                       consumed by the environment.
@retval MDBX_TOO_LARGE Specified size is too large, i.e. too many pages for
                       given size, or a 32-bit process requests too much
                       bytes for the 32-bit address space.
*/
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
