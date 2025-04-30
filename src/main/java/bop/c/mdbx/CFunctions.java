package bop.c.mdbx;

import static bop.c.Loader.LINKER;
import static bop.c.Loader.LOOKUP;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

interface CFunctions {
  MethodHandle MDBX_VAL_IOV_BASE_OFFSET = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_iov_base_offset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ));

  MethodHandle MDBX_VAL_IOV_LEN_OFFSET = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_iov_len_offset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ));

  MethodHandle MDBX_VAL_SIZE = LINKER.downcallHandle(
      LOOKUP.find("bop_MDBX_val_size").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG // size_t
          ));

  // int mdbx_get_sysraminfo(intptr_t *page_size, intptr_t *total_pages, intptr_t *avail_pages);
  MethodHandle MDBX_GET_SYSRAMINFO = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get_sysraminfo").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // intptr_t *page_size
          ValueLayout.JAVA_LONG, // intptr_t *total_pages
          ValueLayout.JAVA_LONG // intptr_t *avail_pages
          ));

  // int mdbx_env_set_userctx(MDBX_env *env, void *ctx);
  MethodHandle MDBX_ENV_SET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG // void* ctx
          ));

  // void* mdbx_env_get_userctx(MDBX_env *env);
  MethodHandle MDBX_ENV_GET_USERCTX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // void*
          ValueLayout.JAVA_LONG // MDBX_env *env
          ));

  // int mdb_env_create(MDB_env **env);
  MethodHandle MDBX_ENV_CREATE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_create").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_env **penv
          ));

  MethodHandle MDBX_ENV_SET_OPTION = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_option").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_option_t option
          ValueLayout.JAVA_LONG // uint64_t value
          ));

  MethodHandle MDBX_ENV_GET_OPTION = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_option").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_option_t option
          ValueLayout.JAVA_LONG // uint64_t* value
          ));

  MethodHandle MDBX_ENV_OPEN = LINKER.downcallHandle(
      LOOKUP.find("bop_mdbx_env_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // const char *pathname
          ValueLayout.JAVA_INT, // MDBX_env_flags_t flags
          ValueLayout.JAVA_INT // mdbx_mode_t mode
          ));

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
          ));

  // int mdbx_env_set_flags(MDBX_env *env, MDBX_env_flags_t flags, bool onoff);
  MethodHandle MDBX_ENV_SET_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_set_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_INT, // MDBX_env_flags_t flags
          ValueLayout.JAVA_BOOLEAN // bool onoff
          ));

  // int mdbx_env_get_flags(const MDBX_env *env, unsigned *flags);
  MethodHandle MDBX_ENV_GET_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_get_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG // MDBX_env_flags_t* flags
          ));

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
          ));

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
          ));

  MethodHandle MDBX_ENV_DELETE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_delete").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.ADDRESS, // const char *pathname
          ValueLayout.JAVA_INT // MDBX_env_delete_mode_t mode
          ));

  MethodHandle MDBX_ENV_CLOSE_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_close_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_BOOLEAN // bool dont_sync
          ));

  MethodHandle MDBX_ENV_SYNC_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_sync_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_BOOLEAN, // bool force
          ValueLayout.JAVA_BOOLEAN // bool nonblock
          ));

  // int mdbx_env_stat_ex(const MDBX_env *env, const MDBX_txn *txn, MDBX_stat *stat, size_t bytes);
  MethodHandle MDBX_ENV_STAT_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_env_stat_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_env *env
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_stat *stat
          ValueLayout.JAVA_LONG // size_t bytes
          ));

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
          ));

  // int mdbx_txn_info(const MDBX_txn *txn, MDBX_txn_info *info, bool scan_rlt);
  MethodHandle MDBX_TXN_INFO = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_info").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // MDBX_txn_info *parent
          ValueLayout.JAVA_BOOLEAN // bool scan_rlt
          ));

  // MDBX_txn_flags_t mdbx_txn_flags(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_ENV = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_env").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // MDBX_env*
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // MDBX_txn_flags_t mdbx_txn_flags(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_FLAGS = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // uint64_t mdbx_txn_id(const MDBX_txn *txn);
  MethodHandle MDBX_TXN_ID = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_id").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // uint64_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // int mdbx_txn_commit_ex(MDBX_txn *txn, MDBX_commit_latency *latency);
  MethodHandle MDBX_TXN_COMMIT_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_commit_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG // MDBX_commit_latency *latency
          ));

  // int mdbx_txn_abort(MDBX_txn *txn);
  MethodHandle MDBX_TXN_ABORT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_abort").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // int mdbx_txn_break(MDBX_txn *txn);
  MethodHandle MDBX_TXN_BREAK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_break").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // int mdbx_txn_reset(MDBX_txn *txn);
  MethodHandle MDBX_TXN_RESET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_reset").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // int mdbx_txn_park(MDBX_txn *txn, bool autounpark);
  MethodHandle MDBX_TXN_PARK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_park").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_BOOLEAN // bool autounpark
          ));

  // int mdbx_txn_unpark(MDBX_txn *txn, bool restart_if_ousted);
  MethodHandle MDBX_TXN_UNPARK = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_unpark").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_BOOLEAN // bool restart_if_ousted
          ));

  // int mdbx_txn_renew(MDBX_txn *txn);
  MethodHandle MDBX_TXN_RENEW = LINKER.downcallHandle(
      LOOKUP.find("mdbx_txn_renew").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG // MDBX_txn *txn
          ));

  // int mdbx_dbi_open(MDBX_txn *txn, const char *name, MDBX_db_flags_t flags, MDBX_dbi *dbi);
  MethodHandle MDBX_DBI_OPEN = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_LONG, // const char *name,
          ValueLayout.JAVA_INT, // MDBX_db_flags_t flags
          ValueLayout.JAVA_LONG // MDBX_dbi *dbi
          ));

  // int mdbx_dbi_rename(MDBX_txn *txn, MDBX_dbi dbi, const char *name);
  MethodHandle MDBX_DBI_RENAME = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_rename").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG // const char *name
          ));

  // int mdbx_dbi_stat(const MDBX_txn *txn, MDBX_dbi dbi, MDBX_stat *stat, size_t bytes);
  MethodHandle MDBX_DBI_STAT = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_stat").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_stat *stat
          ValueLayout.JAVA_LONG // size_t bytes
          ));

  // int mdbx_dbi_flags_ex(const MDBX_txn *txn, MDBX_dbi dbi, unsigned *flags, unsigned *state);
  MethodHandle MDBX_DBI_FLAGS_EX = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_flags_ex").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // unsigned *flags
          ValueLayout.JAVA_LONG // unsigned *state
          ));

  // int mdbx_dbi_close(MDBX_env *env, MDBX_dbi dbi);
  MethodHandle MDBX_DBI_CLOSE = LINKER.downcallHandle(
      LOOKUP.find("mdbx_dbi_close").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT // MDBX_dbi dbi
          ));

  // int mdbx_drop(MDBX_txn *txn, MDBX_dbi dbi, bool del);
  MethodHandle MDBX_DROP = LINKER.downcallHandle(
      LOOKUP.find("mdbx_drop").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_BOOLEAN // bool del
          ));

  // int mdbx_get(const MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, MDBX_val *data);
  MethodHandle MDBX_GET = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ));

  // int mdbx_get_equal_or_great(const MDBX_txn *txn, MDBX_dbi dbi, MDBX_val *key, MDBX_val *data);
  MethodHandle MDBX_GET_EQUAL_OR_GREATER = LINKER.downcallHandle(
      LOOKUP.find("mdbx_get_equal_or_great").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ));

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
          ));

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
          ));

  // int mdbx_del(MDBX_txn *txn, MDBX_dbi dbi, const MDBX_val *key, const MDBX_val *data);
  MethodHandle MDBX_DEL = LINKER.downcallHandle(
      LOOKUP.find("mdbx_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDBX_error_t
          ValueLayout.JAVA_LONG, // MDBX_txn *txn
          ValueLayout.JAVA_INT, // MDBX_dbi dbi
          ValueLayout.JAVA_LONG, // MDBX_val *key
          ValueLayout.JAVA_LONG // MDBX_val *data
          ));
}
