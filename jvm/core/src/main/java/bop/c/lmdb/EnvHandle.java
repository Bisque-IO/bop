package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;

public class EnvHandle {
  final Env env;
  final long envAddress;
  final Arena arena;
  final MemorySegment envAddressSegment;
  final EnvInfo.Segment info;
  final Stat.Struct stat;

  public EnvHandle(Env env) {
    this.env = env;
    this.envAddress = env.address;
    this.envAddressSegment = env.envSegment;
    this.arena = env.arena;
    this.info = EnvInfo.create(arena);
    this.stat = Stat.create(arena);
  }

  public Env env() {
    return env;
  }

  public void close() {
    arena.close();
  }

  /// Return information about the LMDB environment.
  ///
  /// @return MDB_envinfo struct
  public EnvInfo info() {
    try {
      final int result = (int) Library.MDB_ENV_INFO.invokeExact(envAddressSegment, info.segment());
      if (result != Code.MDB_SUCCESS) {
        throw new LMDBError(result);
      }
      return info;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Return statistics about the LMDB environment.
  ///
  /// @return MDB_stat struct
  public Stat stat() {
    try {
      final int result = (int) Library.MDB_ENV_STAT.invokeExact(envAddressSegment, stat.segment());
      if (result != Code.MDB_SUCCESS) {
        throw new LMDBError(result);
      }
      return stat;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Flush the data buffers to disk.
  ///
  /// Data is always written to disk when #mdb_txn_commit() is called, but the operating system
  /// may keep it buffered. LMDB always flushes the OS buffers upon commit as well, unless the
  /// environment was opened with #MDB_NOSYNC or in part #MDB_NOMETASYNC. This call is not valid
  /// if the environment was opened with #MDB_RDONLY.
  ///
  /// @param force If non-zero, force a synchronous flush. Otherwise, if the environment has the
  ///     #MDB_NOSYNC flag set the flushes will be omitted, and with #MDB_MAPASYNC they will be
  ///     asynchronous.
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - EACCES - the environment is read-only.
  ///   - EINVAL - an invalid parameter was specified.
  ///   - EIO - an error occurred during synchronization.
  ///
  public int sync(int force) {
    return env.sync(force);
  }
}
