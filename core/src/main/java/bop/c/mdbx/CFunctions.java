package bop.c.mdbx;

import static bop.c.Loader.LINKER;
import static bop.c.Loader.LOOKUP;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

interface CFunctions {
  MethodHandle BOP_MDBX_BUILD_INFO = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_build_info").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG // MDBX_build_info* info
          ),
      Linker.Option.critical(true));
  MethodHandle BOP_MDBX_VERSION = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_version").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG // MDBX_version_info* info
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_VAL_IOV_BASE_OFFSET = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_iov_base_offset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_VAL_IOV_LEN_OFFSET = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_iov_len_offset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_VAL_SIZE = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_size").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ),
      Linker.Option.critical(true));

  // int mdbx_get_sysraminfo(intptr_t *page_size, intptr_t *total_pages, intptr_t *avail_pages);
  MethodHandle MDBX_GET_SYSRAMINFO = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get_sysraminfo").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // intptr_t *page_size
          ValueLayout.JAVA_LONG, // intptr_t *total_pages
          ValueLayout.JAVA_LONG // intptr_t *avail_pages
          ),
      Linker.Option.critical(true));

  // int mdbx_env_copy(MDBX_env *env, const char *dest, MDBX_copy_flags_t flags);
  MethodHandle MDBX_ENV_COPY = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_copy").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // const char *dest
          ValueLayout.JAVA_INT // MDBX_copy_flags_t flags
          ),
      Linker.Option.critical(false));

  // int mdbx_env_set_userctx(MDBX_env *env, void *ctx);
  MethodHandle MDBX_ENV_SET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG // void* ctx
          ),
      Linker.Option.critical(true));

  // void* mdbx_env_get_userctx(MDBX_env *env);
  MethodHandle MDBX_ENV_GET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // void*
          ValueLayout.JAVA_LONG // MDBX_env *env
          ),
      Linker.Option.critical(true));

  // int mdb_env_create(MDB_env **env);
  MethodHandle MDBX_ENV_CREATE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_create").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_env **penv
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_SET_OPTION = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_option").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_option_t option
          ValueLayout.JAVA_LONG // uint64_t value
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_GET_OPTION = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_option").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_option_t option
          ValueLayout.JAVA_LONG // uint64_t* value
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_OPEN = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_env_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // const char *pathname
          ValueLayout.JAVA_INT, // MDBX_env_flags_t flags
          ValueLayout.JAVA_INT // mdbx_mode_t mode
          ),
      Linker.Option.critical(false));

  // int mdbx_env_open_for_recovery(MDBX_env *env, const char *pathname, unsigned target_meta, bool
  // writeable);
  MethodHandle MDBX_ENV_OPEN_FOR_RECOVERY = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_open_for_recovery").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.ADDRESS, // const char *pathname
          ValueLayout.JAVA_INT, // unsigned target_meta
          ValueLayout.JAVA_BOOLEAN // bool writeable
          ),
      Linker.Option.critical(false));

  // int mdbx_env_set_flags(MDBX_env *env, MDBX_env_flags_t flags, bool onoff);
  MethodHandle MDBX_ENV_SET_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_env_flags_t flags
          ValueLayout.JAVA_BOOLEAN // bool onoff
          ),
      Linker.Option.critical(true));

  // int mdbx_env_get_flags(const MDBX_env *env, unsigned *flags);
  MethodHandle MDBX_ENV_GET_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG // MDBX_env_flags_t* flags
          ),
      Linker.Option.critical(true));

  /// int mdbx_env_set_geometry(MDBX_env *env, intptr_t size_lower, intptr_t size_now, intptr_t
  /// size_upper, intptr_t growth_step, intptr_t shrink_threshold, intptr_t pagesize);
  MethodHandle MDBX_ENV_SET_GEOMETRY = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_geometry").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // intptr_t size_lower
          ValueLayout.JAVA_LONG, // intptr_t size_now
          ValueLayout.JAVA_LONG, // intptr_t size_upper
          ValueLayout.JAVA_LONG, // intptr_t growth_step
          ValueLayout.JAVA_LONG, // intptr_t shrink_threshold
          ValueLayout.JAVA_LONG // intptr_t pagesize
          ),
      Linker.Option.critical(true));

  // int mdbx_env_info_ex(const MDBX_env *env, const MDBX_txn *txn, MDBX_envinfo *info, size_t
  // bytes);
  MethodHandle MDBX_ENV_INFO_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_info_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_envinfo *info
          ValueLayout.JAVA_LONG // size_t bytes
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_DELETE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_delete").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.ADDRESS, // const char *pathname
          ValueLayout.JAVA_INT // MDBX_env_delete_mode_t mode
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_CLOSE_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_close_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_BOOLEAN // bool dont_sync
          ),
      Linker.Option.critical(true));

  MethodHandle MDBX_ENV_SYNC_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_sync_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_BOOLEAN, // bool force
          ValueLayout.JAVA_BOOLEAN // bool nonblock
          ),
      Linker.Option.critical(true));

  // int mdbx_env_stat_ex(const MDBX_env *env, const MDBX_txn *txn, MDBX_stat *stat, size_t bytes);
  MethodHandle MDBX_ENV_STAT_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_stat_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_stat *stat
          ValueLayout.JAVA_LONG // size_t bytes
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_begin_ex(MDBX_env *env, MDBX_txn *parent,
  //    MDBX_txn_flags_t flags, MDBX_txn **txn, void *context);
  MethodHandle MDBX_TXN_BEGIN_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_begin_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // MDBX_txn *parent
          ValueLayout.JAVA_INT, // MDBX_txn_flags_t flags
          ValueLayout.JAVA_LONG, // MDBX_txn **txn
          ValueLayout.JAVA_LONG // void *context
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_info(const MDBX_txn *txn, MDBX_txn_info *info, bool scan_rlt);
  MethodHandle MDBX_TXN_INFO = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_info").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_txn_info *parent
          ValueLayout.JAVA_BOOLEAN // bool scan_rlt
          ),
      Linker.Option.critical(true));

  // MDBX_txn_flags_t mdbx_txn_flags(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_ENV = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_env").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // MDBX_env*
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // MDBX_txn_flags_t mdbx_txn_flags(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // uint64_t mdbx_txn_id(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_ID = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_id").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_commit_ex(MDBX_txn *txn, MDBX_commit_latency *latency);
  MethodHandle MDBX_TXN_COMMIT_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_commit_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG // MDBX_commit_latency *latency
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_abort(MDBX_txn *txn);
  MethodHandle MDBX_TXN_ABORT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_abort").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_break(MDBX_txn *txn);
  MethodHandle MDBX_TXN_BREAK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_break").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_reset(MDBX_txn *txn);
  MethodHandle MDBX_TXN_RESET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_reset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_park(MDBX_txn *txn, bool autounpark);
  MethodHandle MDBX_TXN_PARK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_park").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_BOOLEAN // bool autounpark
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_unpark(MDBX_txn *txn, bool restart_if_ousted);
  MethodHandle MDBX_TXN_UNPARK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_unpark").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_BOOLEAN // bool restart_if_ousted
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_renew(MDBX_txn *txn);
  MethodHandle MDBX_TXN_RENEW = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_renew").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ),
      Linker.Option.critical(true));

  // int mdbx_dbi_open(MDBX_txn *txn, const char *name, MDBX_db_flags_t flags, MDBX_dbi *dbi);
  MethodHandle MDBX_DBI_OPEN = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // const char *name,
          ValueLayout.JAVA_INT, // MDBX_db_flags_t flags
          ValueLayout.JAVA_LONG // MDBX_dbi *dbi
          ),
      Linker.Option.critical(true));

  // int mdbx_dbi_rename(MDBX_txn *txn, MDBX_dbi dbi, const char *name);
  MethodHandle MDBX_DBI_RENAME = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_rename").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG // const char *name
          ),
      Linker.Option.critical(true));

  // int mdbx_dbi_stat(const MDBX_txn *txn, MDBX_dbi dbi, MDBX_stat *stat, size_t bytes);
  MethodHandle MDBX_DBI_STAT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_stat").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_stat *stat
          ValueLayout.JAVA_LONG // size_t bytes
          ),
      Linker.Option.critical(true));

  // int mdbx_dbi_flags_ex(const MDBX_txn *txn, MDBX_dbi dbi, unsigned *flags, unsigned *state);
  MethodHandle MDBX_DBI_FLAGS_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_flags_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // unsigned *flags
          ValueLayout.JAVA_LONG // unsigned *state
          ),
      Linker.Option.critical(true));

  // int mdbx_dbi_close(MDBX_env *env, MDBX_dbi dbi);
  MethodHandle MDBX_DBI_CLOSE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_close").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT // MDBX_dbi dbi
          ),
      Linker.Option.critical(true));

  // int mdbx_drop(MDBX_txn *txn, MDBX_dbi dbi, bool del);
  MethodHandle MDBX_DROP = LINKER.downcallHandle(
      LOOKUP.find("mdbx_drop").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_BOOLEAN // bool del
          ),
      Linker.Option.critical(true));

  // int mdbx_get(const MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, MDBX_val *data);
  MethodHandle MDBX_GET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     void* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* found_key,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_GET = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // void *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG, // MDBX_val* found_key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get_int(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_GET_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_get_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  // int mdbx_get_equal_or_great(const MDBX_txn *txn, MDBX_dbi dbi, MDBX_val *key, MDBX_val *data);
  MethodHandle MDBX_GET_EQUAL_OR_GREATER = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get_equal_or_great").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get_greater_or_equal(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     void* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* found_key,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_GET_EQUAL_OR_GREATER = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_get_greater_or_equal").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // void *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG, // MDBX_val* found_key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get_greater_or_equal_int(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     uint64_t* found_key,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_GET_EQUAL_OR_GREATER_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_get_greater_or_equal_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG, // uint64_t* found_key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  // int mdbx_put(MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, MDBX_val *data,
  // MDBX_put_flags_t flags);
  MethodHandle MDBX_PUT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_put(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_PUT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // uint8_t *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.ADDRESS, // uint8_t *data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val* prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_put2(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_PUT2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_put2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // uint8_t *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_put_flags_t
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_put3(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     MDBX_val* key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_PUT3 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_put3").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val *prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_put_int(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_PUT_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_put_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val *prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_put_int2(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     MDBX_val* data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_PUT_INT2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_put_int2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG, // MDBX_val* data
          ValueLayout.JAVA_INT // MDBX_put_flags_t
          ),
      Linker.Option.critical(true));

  // int mdbx_replace(MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, MDBX_val *new_data, MDBX_val
  // *old_data, MDBX_put_flags_t flags);
  MethodHandle MDBX_REPLACE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_replace").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG, // MDBX_val *new_data
          ValueLayout.JAVA_LONG, // MDBX_val *old_data
          ValueLayout.JAVA_INT // MDBX_val *data
          ),
      Linker.Option.critical(true));

  // int mdbx_del(MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, const MDBX_val *data);
  MethodHandle MDBX_DEL = LINKER.downcallHandle(
      LOOKUP.find("mdbx_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_del(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size
  /// )
  MethodHandle BOP_MDBX_DEL = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // uint8_t *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.ADDRESS, // uint8_t *data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG // size_t data_size
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_del2(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_DEL2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_del2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.ADDRESS, // uint8_t *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG // MDBX_val *data
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_del3(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     MDBX_val* key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size
  /// )
  MethodHandle BOP_MDBX_DEL3 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_del3").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG // size_t data_size
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_del_int(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size
  /// )
  MethodHandle BOP_MDBX_DEL_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_del_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG // size_t data_size
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_del_int2(
  ///     MDBX_txn* txn,
  ///     MDBX_dbi dbi,
  ///     uint64_t key,
  ///     MDBX_val* data
  /// )
  MethodHandle BOP_MDBX_DEL_INT2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_del_int2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG // MDBX_val* data
          ),
      Linker.Option.critical(true));

  // MDBX_cursor *mdbx_cursor_create(void *context);
  MethodHandle MDBX_CURSOR_CREATE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_create").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // MDBX_cursor*
          ValueLayout.JAVA_LONG // void* context
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_set_userctx(MDBX_cursor *cursor, void *ctx);
  MethodHandle MDBX_CURSOR_SET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_set_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor*
          ValueLayout.JAVA_LONG // void* context
          ),
      Linker.Option.critical(true));

  // void *mdbx_cursor_get_userctx(const MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_GET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_get_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // void*
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_bind(MDBX_txn *txn, MDBX_cursor *cursor, MDBX_dbi dbi);
  MethodHandle MDBX_CURSOR_BIND = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_bind").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_INT // MDBX_dbi dbi
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_unbind(MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_UNBIND = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_unbind").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_reset(MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_RESET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_reset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_open(MDBX_txn *txn, MDBX_dbi dbi, MDBX_cursor **cursor);
  MethodHandle MDBX_CURSOR_OPEN = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG // MDBX_cursor **cursor
          ),
      Linker.Option.critical(true));

  // void mdbx_cursor_close(MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_CLOSE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_close").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_txn_release_all_cursors_ex(const MDBX_txn *txn, bool unbind, uint64_t *count);
  MethodHandle MDBX_TXN_RELEASE_ALL_CURSORS_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_release_all_cursors_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // bool unbind
          ValueLayout.JAVA_BOOLEAN // bool unbind
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_renew(MDBX_txn *txn, MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_RENEW = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_renew").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_copy(const MDBX_cursor *src, MDBX_cursor *dest);
  MethodHandle MDBX_CURSOR_COPY = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_copy").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *src
          ValueLayout.JAVA_LONG // MDBX_cursor *dest
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_compare(const MDBX_cursor *left, const MDBX_cursor *right,
  // bool ignore_multival);
  MethodHandle MDBX_CURSOR_COMPARE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_compare").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // <=>
          ValueLayout.JAVA_LONG, // MDBX_cursor *left
          ValueLayout.JAVA_LONG, // MDBX_cursor *right
          ValueLayout.JAVA_BOOLEAN // bool ignore_multival
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_get(MDBX_cursor *cursor, MDBX_val *key, MDBX_val *data, MDBX_cursor_op op);
  MethodHandle MDBX_CURSOR_GET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_cursor_op op
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get_int(
  ///     MDBX_cursor* cur,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* found_key,
  ///     MDBX_val* data,
  ///     MDBX_cursor_op op
  /// )
  MethodHandle BOP_MDBX_CURSOR_GET = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cur
          ValueLayout.ADDRESS, // uint8_t* key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG, // MDBX_val* found_key
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_cursor_op
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_get_int(
  ///     MDBX_cursor* cur,
  ///     uint64_t key,
  ///     MDBX_val* found_key,
  ///     MDBX_val* data,
  ///     MDBX_cursor_op op
  /// )
  MethodHandle BOP_MDBX_CURSOR_GET_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_get_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cur
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG, // MDBX_val* found_key
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_cursor_op
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_ignord(MDBX_cursor *cursor);
  MethodHandle MDBX_CURSOR_IGNORED = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_ignord").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_cursor *cursor
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_put(MDBX_cursor *cursor, const MDBX_val *key, MDBX_val *data,
  // MDBX_put_flags_t flags);
  MethodHandle MDBX_CURSOR_PUT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG, // MDBX_val *data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_cursor_put(
  ///     MDBX_cursor* cur,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_CURSOR_PUT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.ADDRESS, // uint8_t *key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.ADDRESS, // uint8_t *data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val* prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_cursor_put2(
  ///     MDBX_cursor* cur,
  ///     uint8_t* key,
  ///     size_t key_offset,
  ///     size_t key_size,
  ///     MDBX_val* data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_CURSOR_PUT2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_put2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.ADDRESS, // uint8_t* key
          ValueLayout.JAVA_LONG, // size_t key_offset
          ValueLayout.JAVA_LONG, // size_t key_size
          ValueLayout.JAVA_LONG, // MDBX_val* data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_cursor_put3(
  ///     MDBX_cursor* cur,
  ///     MDBX_val* key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_CURSOR_PUT3 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_put3").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_LONG, // MDBX_val* key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val* prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_cursor_put_int(
  ///     MDBX_cursor* cur,
  ///     uint64_t key,
  ///     uint8_t* data,
  ///     size_t data_offset,
  ///     size_t data_size,
  ///     MDBX_val* prev_data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_CURSOR_PUT_INT = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_put_int").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.ADDRESS, // uint8_t* data
          ValueLayout.JAVA_LONG, // size_t data_offset
          ValueLayout.JAVA_LONG, // size_t data_size
          ValueLayout.JAVA_LONG, // MDBX_val* prev_data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  /// int bop_mdbx_cursor_put_int2(
  ///     MDBX_cursor* cur,
  ///     uint64_t key,
  ///     MDBX_val* data,
  ///     MDBX_put_flags_t flags
  /// )
  MethodHandle BOP_MDBX_CURSOR_PUT_INT2 = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_cursor_put_int2").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_LONG, // uint64_t key
          ValueLayout.JAVA_LONG, // MDBX_val* data
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));

  // int mdbx_cursor_del(MDBX_cursor *cursor, MDBX_put_flags_t flags);
  MethodHandle MDBX_CURSOR_DEL = LINKER.downcallHandle(
      LOOKUP.find("mdbx_cursor_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_cursor *cursor
          ValueLayout.JAVA_INT // MDBX_put_flags_t flags
          ),
      Linker.Option.critical(true));
}
