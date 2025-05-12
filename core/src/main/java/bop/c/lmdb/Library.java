package bop.c.lmdb;

import bop.c.Loader;
import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;

interface Library {
  Arena ARENA = Loader.ARENA;
  Linker LINKER = Loader.LINKER;
  SymbolLookup LIB = Loader.LOOKUP;

  MethodHandle BOP_HELLO =
      LINKER.downcallHandle(LIB.find("bop_hello").orElseThrow(), FunctionDescriptor.ofVoid());

  // int mdb_env_create(MDB_env **env);
  MethodHandle MDB_ENV_CREATE = LINKER.downcallHandle(
      LIB.find("mdb_env_create").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS // MDB_env **env
          ));

  // int mdb_env_open(MDB_env *env, const char *path, unsigned int flags, mdb_mode_t mode);
  MethodHandle MDB_ENV_OPEN = LINKER.downcallHandle(
      LIB.find("mdb_env_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS, // const char *path
          ValueLayout.JAVA_INT, // unsigned int flags
          ValueLayout.JAVA_INT // mdb_mode_t mode
          ));

  // int mdb_env_stat(MDB_env *env, MDB_stat *stat);
  MethodHandle MDB_ENV_STAT = LINKER.downcallHandle(
      LIB.find("mdb_env_stat").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // MDB_stat *stat
          ));

  // int mdb_env_info(MDB_env *env, MDB_envinfo *stat);
  MethodHandle MDB_ENV_INFO = LINKER.downcallHandle(
      LIB.find("mdb_env_info").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // MDB_envinfo *stat
          ));

  // void mdb_env_sync(MDB_env *env, int force);
  MethodHandle MDB_ENV_SYNC = LINKER.downcallHandle(
      LIB.find("mdb_env_sync").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_INT // int force
          ));

  // void mdb_env_close(MDB_env *env);
  MethodHandle MDB_ENV_CLOSE = LINKER.downcallHandle(
      LIB.find("mdb_env_close").orElseThrow(),
      FunctionDescriptor.of(
              ValueLayout.ADDRESS, // void
              ValueLayout.ADDRESS // MDB_env *env
              )
          .dropReturnLayout() // void return
      );

  // int mdb_env_set_flags(MDB_env *env, unsigned int flags, int onoff);
  MethodHandle MDB_ENV_SET_FLAGS = LINKER.downcallHandle(
      LIB.find("mdb_env_set_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_INT, // unsigned int flags
          ValueLayout.JAVA_INT // int oneoff
          ));

  // int mdb_env_get_flags(MDB_env *env, unsigned int *flags);
  MethodHandle MDB_ENV_GET_FLAGS = LINKER.downcallHandle(
      LIB.find("mdb_env_get_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // unsigned int *flags
          ));

  // int mdb_env_get_path(MDB_env *env, const char **path);
  MethodHandle MDB_ENV_GET_PATH = LINKER.downcallHandle(
      LIB.find("mdb_env_get_path").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // const char **path
          ));

  // int mdb_env_get_fd(MDB_env *env, mdb_filehandle_t *fd);
  MethodHandle MDB_ENV_GET_FD = LINKER.downcallHandle(
      LIB.find("mdb_env_get_fd").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // mdb_filehandle_t *fd
          ));

  // int mdb_env_set_mapsize(MDB_env *env, mdb_size_t size);
  MethodHandle MDB_ENV_SET_MAPSIZE = LINKER.downcallHandle(
      LIB.find("mdb_env_set_mapsize").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_LONG // mdb_size_t size
          ));

  // int mdb_env_set_maxreaders(MDB_env *env, unsigned int readers);
  MethodHandle MDB_ENV_SET_MAXREADERS = LINKER.downcallHandle(
      LIB.find("mdb_env_set_maxreaders").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_INT // unsigned int readers
          ));

  // int mdb_env_get_maxreaders(MDB_env *env, unsigned int *readers);
  MethodHandle MDB_ENV_GET_MAXREADERS = LINKER.downcallHandle(
      LIB.find("mdb_env_get_maxreaders").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // unsigned int *readers
          ));

  // int mdb_env_set_maxdbs(MDB_env *env, MDB_dbi dbs);
  MethodHandle MDB_ENV_SET_MAXDBS = LINKER.downcallHandle(
      LIB.find("mdb_env_set_maxdbs").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_INT // MDB_dbi dbs
          ));

  // int mdb_env_get_maxkeysize(MDB_env *env);
  MethodHandle MDB_ENV_GET_MAXKEYSIZE = LINKER.downcallHandle(
      LIB.find("mdb_env_get_maxkeysize").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // int maxkeysize
          ValueLayout.ADDRESS // MDB_env *env
          ));

  // int mdb_env_set_userctx(MDB_env *env, void *ctx);
  MethodHandle MDB_ENV_SET_USERCTX = LINKER.downcallHandle(
      LIB.find("mdb_env_set_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.JAVA_LONG // MDB_dbi dbs
          ));

  // void *mdb_env_get_userctx(MDB_env *env);
  MethodHandle MDB_ENV_GET_USERCTX = LINKER.downcallHandle(
      LIB.find("mdb_env_get_userctx").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // void*
          ValueLayout.ADDRESS // MDB_env *env
          ));

  // int mdb_txn_begin(MDB_env *env, MDB_txn *parent, unsigned int flags, MDB_txn **txn);
  MethodHandle MDB_TXN_BEGIN = LINKER.downcallHandle(
      LIB.find("mdb_txn_begin").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS, // MDB_txn *parent
          ValueLayout.JAVA_INT, // unsigned int flags
          ValueLayout.ADDRESS // MDB_txn **txn
          ));

  // MDB_env *mdb_txn_env(MDB_txn *txn);
  MethodHandle MDB_TXN_ENV = LINKER.downcallHandle(
      LIB.find("mdb_txn_env").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // MDB_txn *txn
          ));

  // mdb_size_t mdb_txn_id(MDB_txn *txn);
  MethodHandle MDB_TXN_ID = LINKER.downcallHandle(
      LIB.find("mdb_txn_id").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_LONG, // mdb_size_t
          ValueLayout.ADDRESS // MDB_txn *txn
          ));

  // int mdb_txn_commit(MDB_txn *txn);
  MethodHandle MDB_TXN_COMMIT = LINKER.downcallHandle(
      LIB.find("mdb_txn_commit").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS // MDB_txn *txn
          ));

  // void mdb_txn_abort(MDB_txn *txn);
  MethodHandle MDB_TXN_ABORT = LINKER.downcallHandle(
      LIB.find("mdb_txn_abort").orElseThrow(),
      FunctionDescriptor.of(
              ValueLayout.JAVA_INT, // void
              ValueLayout.ADDRESS // MDB_txn *txn
              )
          .dropReturnLayout());

  // void mdb_txn_reset(MDB_txn *txn);
  MethodHandle MDB_TXN_RESET = LINKER.downcallHandle(
      LIB.find("mdb_txn_reset").orElseThrow(),
      FunctionDescriptor.of(
              ValueLayout.JAVA_INT, // void
              ValueLayout.ADDRESS // MDB_txn *txn
              )
          .dropReturnLayout());

  // int mdb_txn_renew(MDB_txn *txn);
  MethodHandle MDB_TXN_RENEW = LINKER.downcallHandle(
      LIB.find("mdb_txn_renew").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS // MDB_txn *txn
          ));

  // int mdb_dbi_open(MDB_txn *txn, const char *name, unsigned int flags, MDB_dbi *dbi);
  MethodHandle MDB_DBI_OPEN = LINKER.downcallHandle(
      LIB.find("mdb_dbi_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.ADDRESS, // const char *name
          ValueLayout.JAVA_INT, // unsigned int flags
          ValueLayout.ADDRESS // MDB_dbi *dbi
          ));

  // int mdb_stat(MDB_txn *txn, MDB_dbi dbi, MDB_stat *stat);
  MethodHandle MDB_STAT = LINKER.downcallHandle(
      LIB.find("mdb_stat").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS // MDB_stat *stat
          ));

  // int mdb_dbi_flags(MDB_txn *txn, MDB_dbi dbi, unsigned int *flags);
  MethodHandle MDB_DBI_FLAGS = LINKER.downcallHandle(
      LIB.find("mdb_dbi_flags").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS // unsigned int *flags
          ));

  // void mdb_dbi_close(MDB_env *env, MDB_dbi dbi);
  MethodHandle MDB_DBI_CLOSE = LINKER.downcallHandle(
      LIB.find("mdb_dbi_close").orElseThrow(),
      FunctionDescriptor.of(
              ValueLayout.JAVA_INT, // void - removed using "dropReturnLayout()"
              ValueLayout.ADDRESS, // MDB_env *env
              ValueLayout.JAVA_INT // MDB_dbi
              )
          .dropReturnLayout() // void
      );

  // int mdb_drop(MDB_txn *txn, MDB_dbi dbi, int del);
  MethodHandle MDB_DROP = LINKER.downcallHandle(
      LIB.find("mdb_drop").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.JAVA_INT // int del
          ));

  // int mdb_get(MDB_txn *txn, MDB_dbi dbi, MDB_val *key, MDB_val *data);
  MethodHandle MDB_GET = LINKER.downcallHandle(
      LIB.find("mdb_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS, // MDB_val *key
          ValueLayout.ADDRESS // MDB_val *data
          ));

  // int mdb_put(MDB_txn *txn, MDB_dbi dbi, MDB_val *key, MDB_val *data, unsigned int flags);
  MethodHandle MDB_PUT = LINKER.downcallHandle(
      LIB.find("mdb_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS, // MDB_val *key
          ValueLayout.ADDRESS, // MDB_val *data
          ValueLayout.JAVA_INT // unsigned int flags
          ));

  // int mdb_del(MDB_txn *txn, MDB_dbi dbi, MDB_val *key, MDB_val *data);
  MethodHandle MDB_DEL = LINKER.downcallHandle(
      LIB.find("mdb_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS, // MDB_val *key
          ValueLayout.ADDRESS // MDB_val *data
          ));

  // int mdb_cursor_open(MDB_txn *txn, MDB_dbi dbi, MDB_cursor **cursor);
  MethodHandle MDB_CURSOR_OPEN = LINKER.downcallHandle(
      LIB.find("mdb_cursor_open").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS // MDB_cursor **cursor
          ));

  // void mdb_cursor_close(MDB_cursor *cursor);
  MethodHandle MDB_CURSOR_CLOSE = LINKER.downcallHandle(
      LIB.find("mdb_cursor_close").orElseThrow(),
      FunctionDescriptor.ofVoid(
          ValueLayout.ADDRESS // MDB_cursor *cursor
          ));

  // int mdb_cursor_renew(MDB_txn *txn, MDB_cursor *cursor);
  MethodHandle MDB_CURSOR_RENEW = LINKER.downcallHandle(
      LIB.find("mdb_cursor_renew").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_txn *txn
          ValueLayout.ADDRESS // MDB_cursor *cursor
          ));

  // MDB_txn *mdb_cursor_txn(MDB_cursor *cursor);
  MethodHandle MDB_CURSOR_TXN = LINKER.downcallHandle(
      LIB.find("mdb_cursor_txn").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.ADDRESS, // MDB_txn*
          ValueLayout.ADDRESS // MDB_cursor *cursor
          ));

  MethodHandle MDB_CURSOR_DBI = LINKER.downcallHandle(
      LIB.find("mdb_cursor_dbi").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_dbi
          ValueLayout.ADDRESS // MDB_cursor *cursor
          ));

  // int mdb_cursor_get(MDB_cursor *cursor, MDB_val *key, MDB_val *data, MDB_cursor_op op);
  MethodHandle MDB_CURSOR_GET = LINKER.downcallHandle(
      LIB.find("mdb_cursor_get").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_cursor *cursor
          ValueLayout.ADDRESS, // MDB_val *key
          ValueLayout.ADDRESS, // MDB_val *data
          ValueLayout.JAVA_INT // MDB_cursor_op op
          ));

  // int mdb_cursor_put(MDB_cursor *cursor, MDB_val *key, MDB_val *data, unsigned int flags);
  MethodHandle MDB_CURSOR_PUT = LINKER.downcallHandle(
      LIB.find("mdb_cursor_put").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_cursor *cursor
          ValueLayout.ADDRESS, // MDB_val *key
          ValueLayout.ADDRESS, // MDB_val *data
          ValueLayout.JAVA_INT // unsigned int flags
          ));

  // int mdb_cursor_del(MDB_cursor *cursor, unsigned int flags);
  MethodHandle MDB_CURSOR_DEL = LINKER.downcallHandle(
      LIB.find("mdb_cursor_del").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_cursor *cursor
          ValueLayout.JAVA_INT // unsigned int flags
          ));

  // int mdb_cursor_count(MDB_cursor *cursor, mdb_size_t *countp);
  MethodHandle MDB_CURSOR_COUNT = LINKER.downcallHandle(
      LIB.find("mdb_cursor_count").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_cursor *cursor
          ValueLayout.ADDRESS // mdb_size_t *countp
          ));

  // int mdb_reader_check(MDB_env *env, int *dead);
  MethodHandle MDB_READER_CHECK = LINKER.downcallHandle(
      LIB.find("mdb_reader_check").orElseThrow(),
      FunctionDescriptor.of(
          ValueLayout.JAVA_INT, // MDB_return_code
          ValueLayout.ADDRESS, // MDB_env *env
          ValueLayout.ADDRESS // int *dead
          ));
}
