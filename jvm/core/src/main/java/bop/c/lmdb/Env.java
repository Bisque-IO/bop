package bop.c.lmdb;

import bop.c.PointerRef;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.util.Objects;
import java.util.concurrent.locks.ReentrantLock;

public class Env {
  final String path;
  final Config config;
  final Arena arena;
  final long address;
  final MemorySegment envSegment;
  final ReentrantLock lock = new ReentrantLock();

  public Env(String path, Config config, Arena arena, MemorySegment address) {
    this.path = path;
    this.config = config;
    this.arena = arena;
    this.envSegment = address;
    this.address = address.address();
  }

  /// Create an LMDB environment.
  ///
  /// This function allocates memory for a #MDB_env structure. To release the allocated memory
  /// and discard the handle, call #mdb_env_close(). Before the handle may be used, it must be
  /// opened using #mdb_env_open(). Various other options may also need to be set before opening
  /// the handle, e.g. #mdb_env_set_mapsize(), #mdb_env_set_maxreaders(), #mdb_env_set_maxdbs(),
  /// depending on usage requirements.
  ///
  ///
  /// Open an environment handle.
  ///
  /// If this function fails, #mdb_env_close() must be called to discard the #MDB_env handle.
  ///
  ///   - #MDB_VERSION_MISMATCH - the version of the LMDB library doesn't match the version that
  ///     created the database environment.
  ///   - #MDB_INVALID - the environment file headers are corrupted.
  ///   - ENOENT - the directory specified by the path parameter doesn't exist.
  ///   - EACCES - the user didn't have permission to access the environment files.
  ///   - EAGAIN - the environment was locked by another process.
  ///
  ///
  /// Flags set by mdb_env_set_flags() are also used.
  ///
  ///   - #MDB_FIXEDMAP use a fixed address for the mmap region. This flag must be specified when
  ///     creating the environment, and is stored persistently in the environment. If successful,
  ///     the memory map will always reside at the same virtual address and pointers used to
  ///     reference data items in the database will be constant across multiple invocations. This
  ///     option may not always work, depending on how the operating system has allocated memory
  ///     to shared libraries and other uses. The feature is highly experimental.
  ///   - #MDB_NOSUBDIR By default, LMDB creates its environment in a directory whose pathname is
  ///     given in \b path, and creates its data and lock files under that directory. With this
  ///     option, \b path is used as-is for the database main data file. The database lock file
  ///     is the \b path with "-lock" appended.
  ///   - #MDB_RDONLY Open the environment in read-only mode. No write operations will be
  ///     allowed. LMDB will still modify the lock file - except on read-only filesystems, where
  ///     LMDB does not use locks.
  ///   - #MDB_WRITEMAP Use a writeable memory map unless MDB_RDONLY is set. This uses fewer
  ///     mallocs but loses protection from application bugs like wild pointer writes and other
  ///     bad updates into the database. This may be slightly faster for DBs that fit entirely in
  ///     RAM, but is slower for DBs larger than RAM. Incompatible with nested transactions. Do
  ///     not mix processes with and without MDB_WRITEMAP on the same environment. This can
  ///     defeat durability (#mdb_env_sync etc).
  ///   - #MDB_NOMETASYNC Flush system buffers to disk only once per transaction, omit the
  ///     metadata flush. Defer that until the system flushes files to disk, or next
  ///     non-MDB_RDONLY commit or #mdb_env_sync(). This optimization maintains database
  ///     integrity, but a system crash may undo the last committed transaction. I.e. it
  ///     preserves the ACI (atomicity, consistency, isolation) but not D (durability) database
  ///     property. This flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_NOSYNC Don't flush system buffers to disk when committing a transaction. This
  ///     optimization means a system crash can corrupt the database or lose the last
  ///     transactions if buffers are not yet flushed to disk. The risk is governed by how often
  ///     the system flushes dirty buffers to disk and how often #mdb_env_sync() is called.
  ///     However, if the filesystem preserves write order and the #MDB_WRITEMAP flag is not
  ///     used, transactions exhibit ACI (atomicity, consistency, isolation) properties and only
  ///     lose D (durability). I.e. database integrity is maintained, but a system crash may undo
  ///     the final transactions. Note that (#MDB_NOSYNC | #MDB_WRITEMAP) leaves the system with
  ///     no hint for when to write transactions to disk, unless #mdb_env_sync() is called.
  ///     (#MDB_MAPASYNC | #MDB_WRITEMAP) may be preferable. This flag may be changed at any time
  ///     using #mdb_env_set_flags().
  ///   - #MDB_MAPASYNC When using #MDB_WRITEMAP, use asynchronous flushes to disk. As with
  ///     #MDB_NOSYNC, a system crash can then corrupt the database or lose the last
  ///     transactions. Calling #mdb_env_sync() ensures on-disk database integrity until next
  ///     commit. This flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_NOTLS Don't use Thread-Local Storage. Tie reader locktable slots to #MDB_txn
  ///     objects instead of to threads. I.e. #mdb_txn_reset() keeps the slot reserved for the
  ///     #MDB_txn object. A thread may use parallel read-only transactions. A read-only
  ///     transaction may span threads if the user synchronizes its use. Applications that
  ///     multiplex many user threads over individual OS threads need this option. Such an
  ///     application must also serialize the write transactions in an OS thread, since LMDB's
  ///     write locking is unaware of the user threads.
  ///   - #MDB_NOLOCK Don't do any locking. If concurrent access is anticipated, the caller must
  ///     manage all concurrency itself. For proper operation the caller must enforce
  ///     single-writer semantics, and must ensure that no readers are using old transactions
  ///     while a writer is active. The simplest approach is to use an exclusive lock so that no
  ///     readers may be active at all when a writer begins.
  ///   - #MDB_NORDAHEAD Turn off readahead. Most operating systems perform readahead on read
  ///     requests by default. This option turns it off if the OS supports it. Turning it off may
  ///     help random read performance when the DB is larger than RAM and system RAM is full. The
  ///     option is not implemented on Windows.
  ///   - #MDB_NOMEMINIT Don't initialize malloc'd memory before writing to unused spaces in the
  ///     data file. By default, memory for pages written to the data file is obtained using
  ///     malloc. While these pages may be reused in subsequent transactions, freshly malloc'd
  ///     pages will be initialized to zeroes before use. This avoids persisting leftover data
  ///     from other code (that used the heap and subsequently freed the memory) into the data
  ///     file. Note that many other system libraries may allocate and free memory from the heap
  ///     for arbitrary uses. E.g., stdio may use the heap for file I/O buffers. This
  ///     initialization step has a modest performance cost so some applications may want to
  ///     disable it using this flag. This option can be a problem for applications which handle
  ///     sensitive data like passwords, and it makes memory checkers like Valgrind noisy. This
  ///     flag is not needed with #MDB_WRITEMAP, which writes directly to the mmap instead of
  ///     using malloc for pages. The initialization is also skipped if #MDB_RESERVE is used; the
  ///     caller is expected to overwrite all of the memory that was reserved in that case. This
  ///     flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_PREVSNAPSHOT Open the environment with the previous snapshot rather than the
  ///     latest one. This loses the latest transaction, but may help work around some types of
  ///     corruption. If opened with write access, this must be the only process using the
  ///     environment. This flag is automatically reset after a write transaction is successfully
  ///     committed.
  public static Env open(String path, Config config) {
    Objects.requireNonNull(path);
    final Arena arena = Arena.ofShared();
    final Env env;

    try {
      // MDB_env **env
      final var refEnvPointer = PointerRef.allocate(arena);

      final int result = (int) Library.MDB_ENV_CREATE.invokeExact(refEnvPointer.segment());
      if (result != Code.MDB_SUCCESS) {
        throw new LMDBError(result);
      }

      final var envPointer = refEnvPointer.value();
      if (envPointer.address() == 0L) {
        throw new LMDBError(result, "mdb_env_create");
      }

      env = new Env(path, config, arena, envPointer);
    } catch (Throwable e) {
      arena.close();
      throw new RuntimeException(e);
    }

    if (config == null) {
      config = MDB.config();
    }

    if (config.mapSize > 0) {
      try {
        final int result =
            (int) Library.MDB_ENV_SET_MAPSIZE.invokeExact(env.envSegment, config.mapSize);
        if (result != Code.MDB_SUCCESS) {
          throw new LMDBError(result, "mdb_env_set_mapsize");
        }
      } catch (Throwable e) {
        env.close();
        throw new RuntimeException(e);
      }
    }

    if (config.maxReaders > 0) {
      try {
        final int result =
            (int) Library.MDB_ENV_SET_MAXREADERS.invokeExact(env.envSegment, config.maxReaders);
        if (result != Code.MDB_SUCCESS) {
          throw new LMDBError(result, "mdb_env_set_maxreaders");
        }
      } catch (Throwable e) {
        env.close();
        throw new RuntimeException(e);
      }
    }

    if (config.maxDbs > 0) {
      try {
        final int result =
            (int) Library.MDB_ENV_SET_MAXDBS.invokeExact(env.envSegment, config.maxDbs);
        if (result != Code.MDB_SUCCESS) {
          throw new LMDBError(result, "mdb_env_set_maxdbs");
        }
      } catch (Throwable e) {
        env.close();
        throw new RuntimeException(e);
      }
    }

    try (final var tempArena = Arena.ofConfined()) {
      final int result = (int) Library.MDB_ENV_OPEN.invokeExact(
          env.envSegment, tempArena.allocateFrom(path), config.flags, config.mode);
      if (result != Code.MDB_SUCCESS) {
        throw new LMDBError(result, "mdb_env_open");
      }
    } catch (Throwable e) {
      env.close();
      throw new RuntimeException(e);
    }

    return env;
  }

  public EnvHandle newHandle() {
    return new EnvHandle(this);
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
  ///     - EACCES - the environment is read-only.
  ///   - EINVAL - an invalid parameter was specified.
  ///   - EIO - an error occurred during synchronization.
  ///
  public int sync(int force) {
    lock.lock();
    try {
      return (int) Library.MDB_ENV_SYNC.invokeExact(envSegment, force);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    } finally {
      lock.unlock();
    }
  }

  /// Close the environment and release the memory map.
  ///
  /// Only a single thread may call this function. All transactions, databases, and cursors must
  /// already be closed before calling this function. Attempts to use any such handles after
  /// calling this function will cause a SIGSEGV. The environment handle will be freed and must
  /// not be used again after this call.
  public void close() {
    lock.lock();
    try {
      Library.MDB_ENV_CLOSE.invokeExact(envSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    } finally {
      try {
        arena.close();
      } catch (Throwable e) {
        // Ignore.
      }
      lock.unlock();
    }
  }
}
