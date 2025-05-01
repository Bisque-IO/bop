package bop.c.mdbx;

import bop.c.LongRef;
import bop.c.Memory;
import bop.concurrent.SpinLock;
import bop.unsafe.Danger;
import java.io.File;
import java.lang.foreign.Arena;

public class Env {
  File path;
  long pathPointer;
  long envPointer;
  long scratchPointer;
  long scratchPointerLength;
  Stat stat;
  Info info;
  SpinLock lock = new SpinLock();

  public Env(File path, long envPointer) {
    this.path = path;
    this.pathPointer = Memory.allocCString(path.getAbsolutePath());
    this.envPointer = envPointer;
    this.scratchPointer = Memory.zalloc(1024);
    this.scratchPointerLength = 1024;
  }

  public static Env create(String path) {
    return create(new File(path));
  }

  public static Env create(File path) {
    final var refEnvPointer = LongRef.allocate();
    try {
      final int result = (int) CFunctions.MDBX_ENV_CREATE.invokeExact(refEnvPointer.address());
      if (result != Error.SUCCESS) {
        throw new MDBXError(result);
      }
      final var envPointer = refEnvPointer.value();
      return new Env(path, envPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    } finally {
      refEnvPointer.close();
    }
  }

  /// Delete the environment's files in a proper and multiprocess-safe way.
  /// @param path  The pathname for the database or the directory in which
  ///              the database files reside.
  ///
  /// @param mode  Specifies deletion mode for the environment.
  ///              @see DeleteMode
  ///
  /// @apiNote The \ref MDBX_ENV_JUST_DELETE don't supported on Windows since system
  /// unable to delete a memory-mapped files.
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_RESULT_TRUE   No corresponding files or directories were found,
  ///                            so no deletion was performed.
  public static int delete(String path, int mode) {
    try (final var tempArena = Arena.ofConfined()) {
      return (int) CFunctions.MDBX_ENV_DELETE.invokeExact(tempArena.allocateFrom(path), mode);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Copy an MDBX environment to the specified path, with options.
  ///
  /// This function may be used to make a backup of an existing environment.
  /// No lockfile is created, since it gets recreated at need.
  /// \note This call can trigger significant file size growth if run in
  /// parallel with write transactions, because it employs a read-only
  /// transaction. See long-lived transactions under \ref restrictions section.
  /// \note On Windows the \ref mdbx_env_copyW() is recommended to use.
  ///
  /// \see mdbx_env_copy2fd()
  /// \see mdbx_txn_copy2pathname()
  ///
  /// @param dst    The pathname of a file in which the copy will reside.
  ///               This file must not be already exist, but parent directory
  ///               must be writable.
  /// @param flags  Specifies options for this operation. This parameter
  ///               must be bitwise OR'ing together any of the constants
  ///               described here:
  ///
  ///  - \ref MDBX_CP_DEFAULTS
  ///      Perform copy as-is without compaction, etc.
  ///  - \ref MDBX_CP_COMPACT
  ///      Perform compaction while copying: omit free pages and sequentially
  ///      renumber all pages in output. This option consumes little bit more
  ///      CPU for processing, but may running quickly than the default, on
  ///      account skipping free pages.
  ///  - \ref MDBX_CP_FORCE_DYNAMIC_SIZE
  ///      Force to make resizable copy, i.e. dynamic size instead of fixed.
  ///  - \ref MDBX_CP_DONT_FLUSH
  ///      Don't explicitly flush the written data to an output media to reduce
  ///      the time of the operation and the duration of the transaction.
  ///  - \ref MDBX_CP_THROTTLE_MVCC
  ///      Use read transaction parking during copying MVCC-snapshot
  ///      to avoid stopping recycling and overflowing the database.
  ///      This allows the writing transaction to oust the read
  ///      transaction used to copy the database if copying takes so long
  ///      that it will interfere with the recycling old MVCC snapshots
  ///      and may lead to an overflow of the database.
  ///      However, if the reading transaction is ousted the copy will
  ///      be aborted until successful completion. Thus, this option
  ///      allows copy the database without interfering with write
  ///      transactions and a threat of database overflow, but at the cost
  ///      that copying will be aborted to prevent such conditions.
  ///      \see mdbx_txn_park()
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int copy(String dst, int flags) {
    final var dstPtr = Memory.allocCString(dst);
    try {
      return (int) CFunctions.MDBX_ENV_COPY.invokeExact(envPointer, dstPtr, flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    } finally {
      Memory.dealloc(dstPtr);
    }
  }

  /// Return statistics about the MDBX environment.
  ///
  /// At least one of `env` or `txn` argument must be non-null. If txn is passed
  /// non-null then stat will be filled accordingly to the given transaction.
  /// Otherwise, if txn is null, then stat will be populated by a snapshot from
  /// the last committed write transaction, and at next time, other information
  /// can be returned.
  /// Legacy mdbx_env_stat() correspond to calling \ref mdbx_env_stat_ex() with the
  /// null `txn` argument.
  /// \param stat   The address of an \ref MDBX_stat structure where
  ///                     the statistics will be copied
  /// \param bytes   The size of \ref MDBX_stat.
  /// \returns A non-zero error value on failure and 0 on success.
  public Stat stat() {
    if (stat == null) {
      stat = new Stat();
    }
    int err;
    try {
      err = (int) CFunctions.MDBX_ENV_STAT_EX.invokeExact(envPointer, 0L, scratchPointer, 48L);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    stat.update(scratchPointer);
    return stat;
  }

  /// Return statistics about the MDBX environment.
  ///
  /// At least one of `env` or `txn` argument must be non-null. If txn is passed
  /// non-null then stat will be filled accordingly to the given transaction.
  /// Otherwise, if txn is null, then stat will be populated by a snapshot from
  /// the last committed write transaction, and at next time, other information
  /// can be returned.
  /// Legacy mdbx_env_stat() correspond to calling \ref mdbx_env_stat_ex() with the
  /// null `txn` argument.
  /// \param stat   The address of an \ref MDBX_stat structure where
  ///                     the statistics will be copied
  /// \param bytes   The size of \ref MDBX_stat.
  /// \returns A non-zero error value on failure and 0 on success.
  public Info info() {
    final int err;
    try {
      err =
          (int) CFunctions.MDBX_ENV_INFO_EX.invokeExact(envPointer, 0L, scratchPointer, Info.SIZE);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    if (info == null) {
      info = new Info();
    }
    info.update(scratchPointer);
    return info;
  }

  /// Get environment flags.
  ///
  /// returns flags
  ///         Integer.MIN_VALUE on error
  public int getFlags() {
    try {
      final var err = (int) CFunctions.MDBX_ENV_GET_FLAGS.invokeExact(envPointer, scratchPointer);
      if (err != Error.SUCCESS) {
        return Integer.MIN_VALUE;
      }
      return Danger.UNSAFE.getInt(scratchPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// \brief Set environment flags.
  ///
  /// This may be used to set some flags in addition to those from
  /// mdbx_env_open(), or to unset these flags.
  ///
  /// @apiNote  In contrast to LMDB, the MDBX serialize threads via mutex while
  /// changing the flags. Therefore this function will be blocked while a write
  /// transaction running by other thread, or \ref MDBX_BUSY will be returned if
  /// function called within a write transaction.
  ///
  /// @param  flags    The \ref env_flags to change, bitwise OR'ed together.
  /// @param  onoff    A non-zero value sets the flags, zero clears them.
  ///
  /// returns A non-zero error value on failure and 0 on success,
  ///         some possible errors are:
  /// retval MDBX_EINVAL  An invalid parameter was specified.
  public int setFlags(final int flags, final boolean onoff) {
    try {
      return (int) CFunctions.MDBX_ENV_SET_FLAGS.invokeExact(envPointer, flags, onoff);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Gets the value of extra runtime options from an environment.
  ///
  /// @param option  The option from \ref MDBX_option_t to get value of it.
  ///
  /// @see Option
  /// returns Long.MIN_VALUE on error; otherwise the value
  public long getOption(int option) {
    try {
      final var err =
          (int) CFunctions.MDBX_ENV_GET_OPTION.invokeExact(envPointer, option, scratchPointer);
      if (err != Error.SUCCESS) {
        return Long.MIN_VALUE;
      }
      return Danger.UNSAFE.getLong(scratchPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Sets the value of a extra runtime options for an environment.
  ///
  /// @param option  The option from @see Options to set value of it.
  /// @param value   The value of option to be set.
  ///
  /// @see Option
  /// returns A non-zero error value on failure and 0 on success.
  public int setOption(int option, long value) {
    try {
      return (int) CFunctions.MDBX_ENV_SET_OPTION.invokeExact(envPointer, option, value);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Set all size-related parameters of environment, including page size
  /// and the min/max size of the memory map.
  ///
  /// In contrast to LMDB, the MDBX provide automatic size management of an
  /// database according the given parameters, including shrinking and resizing
  /// on the fly. From user point of view all of these just working. Nevertheless,
  /// it is reasonable to know some details in order to make optimal decisions
  /// when choosing parameters.
  ///
  /// \see mdbx_env_info_ex()
  /// Both \ref mdbx_env_set_geometry() and legacy \ref mdbx_env_set_mapsize() are
  /// inapplicable to read-only opened environment.
  ///
  /// Both \ref mdbx_env_set_geometry() and legacy \ref mdbx_env_set_mapsize()
  /// could be called either before or after \ref mdbx_env_open(), either within
  /// the write transaction running by current thread or not:
  ///
  ///  - In case \ref mdbx_env_set_geometry() or legacy \ref mdbx_env_set_mapsize()
  ///    was called BEFORE \ref mdbx_env_open(), i.e. for closed environment, then
  ///    the specified parameters will be used for new database creation,
  ///    or will be applied during opening if database exists and no other process
  ///    using it.
  ///
  ///    If the database is already exist, opened with \ref MDBX_EXCLUSIVE or not
  ///    used by any other process, and parameters specified by
  ///    \ref mdbx_env_set_geometry() are incompatible (i.e. for instance,
  ///    different page size) then \ref mdbx_env_open() will return
  ///    \ref MDBX_INCOMPATIBLE error.
  ///
  ///    In another way, if database will opened read-only or will used by other
  ///    process during calling \ref mdbx_env_open() that specified parameters will
  ///    silently discard (open the database with \ref MDBX_EXCLUSIVE flag
  ///    to avoid this).
  ///
  ///  - In case \ref mdbx_env_set_geometry() or legacy \ref mdbx_env_set_mapsize()
  ///    was called after \ref mdbx_env_open() WITHIN the write transaction running
  ///    by current thread, then specified parameters will be applied as a part of
  ///    write transaction, i.e. will not be completely visible to any others
  ///    processes until the current write transaction has been committed by the
  ///    current process. However, if transaction will be aborted, then the
  ///    database file will be reverted to the previous size not immediately, but
  ///    when a next transaction will be committed or when the database will be
  ///    opened next time.
  ///
  ///  - In case \ref mdbx_env_set_geometry() or legacy \ref mdbx_env_set_mapsize()
  ///    was called after \ref mdbx_env_open() but OUTSIDE a write transaction,
  ///    then MDBX will execute internal pseudo-transaction to apply new parameters
  ///    (but only if anything has been changed), and changes be visible to any
  ///    others processes immediately after successful completion of function.
  ///
  /// Essentially a concept of "automatic size management" is simple and useful:
  ///
  ///  - There are the lower and upper bounds of the database file size;
  ///
  ///  - There is the growth step by which the database file will be increased,
  ///    in case of lack of space;
  ///
  ///  - There is the threshold for unused space, beyond which the database file
  ///    will be shrunk;
  ///
  ///  - The size of the memory map is also the maximum size of the database;
  ///
  ///  - MDBX will automatically manage both the size of the database and the size
  ///    of memory map, according to the given parameters.
  ///
  /// So, there some considerations about choosing these parameters:
  ///
  ///  - The lower bound allows you to prevent database shrinking below certain
  ///    reasonable size to avoid unnecessary resizing costs.
  ///
  ///  - The upper bound allows you to prevent database growth above certain
  ///    reasonable size. Besides, the upper bound defines the linear address space
  ///    reservation in each process that opens the database. Therefore changing
  ///    the upper bound is costly and may be required reopening environment in
  ///    case of \ref MDBX_UNABLE_EXTEND_MAPSIZE errors, and so on. Therefore, this
  ///    value should be chosen reasonable large, to accommodate future growth of
  ///    the database.
  ///
  ///  - The growth step must be greater than zero to allow the database to grow,
  ///    but also reasonable not too small, since increasing the size by little
  ///    steps will result a large overhead.
  ///
  ///  - The shrink threshold must be greater than zero to allow the database
  ///    to shrink but also reasonable not too small (to avoid extra overhead) and
  ///    not less than growth step to avoid up-and-down flouncing.
  ///
  ///  - The current size (i.e. `size_now` argument) is an auxiliary parameter for
  ///    simulation legacy \ref mdbx_env_set_mapsize() and as workaround Windows
  ///    issues (see below).
  ///
  /// Unfortunately, Windows has is a several issue with resizing of memory-mapped file:
  ///
  ///  - Windows unable shrinking a memory-mapped file (i.e memory-mapped section)
  ///    in any way except unmapping file entirely and then map again. Moreover,
  ///    it is impossible in any way when a memory-mapped file is used more than
  ///    one process.
  ///
  ///  - Windows does not provide the usual API to augment a memory-mapped file
  ///    (i.e. a memory-mapped partition), but only by using "Native API"
  ///    in an undocumented way.
  ///
  /// MDBX bypasses all Windows issues, but at a cost:
  ///
  ///  - Ability to resize database on the fly requires an additional lock
  ///    and release `SlimReadWriteLock` during each read-only transaction.
  ///
  ///  - During resize all in-process threads should be paused and then resumed.
  ///
  ///  - Shrinking of database file is performed only when it used by single
  ///    process, i.e. when a database closes by the last process or opened
  ///    by the first.
  ///
  ///  = Therefore, the size_now argument may be useful to set database size
  ///    by the first process which open a database, and thus avoid expensive
  ///    remapping further.
  ///
  /// For create a new database with particular parameters, including the page
  /// size, \ref mdbx_env_set_geometry() should be called after
  /// \ref mdbx_env_create() and before \ref mdbx_env_open(). Once the database is
  /// created, the page size cannot be changed. If you do not specify all or some
  /// of the parameters, the corresponding default values will be used. For
  /// instance, the default for database size is 10485760 bytes.
  ///
  /// If the mapsize is increased by another process, MDBX silently and
  /// transparently adopt these changes at next transaction start. However,
  /// \ref mdbx_txn_begin() will return \ref MDBX_UNABLE_EXTEND_MAPSIZE if new
  /// mapping size could not be applied for current process (for instance if
  /// address space is busy).  Therefore, in the case of
  /// \ref MDBX_UNABLE_EXTEND_MAPSIZE error you need close and reopen the
  /// environment to resolve error.
  ///
  /// \note Actual values may be different than your have specified because of
  /// rounding to specified database page size, the system page size and/or the
  /// size of the system virtual memory management unit. You can get actual values
  /// by \ref mdbx_env_info_ex() or see by using the tool `mdbx_chk` with the `-v`
  /// option.
  ///
  /// Legacy \ref mdbx_env_set_mapsize() correspond to calling
  /// \ref mdbx_env_set_geometry() with the arguments `size_lower`, `size_now`,
  /// `size_upper` equal to the `size` and `-1` (i.e. default) for all other
  /// parameters.
  ///
  /// @param sizeLower         The lower bound of database size in bytes.
  ///                          Zero value means "minimal acceptable",
  ///                          and negative means "keep current or use default".
  ///
  /// @param sizeNow           The size in bytes to setup the database size for
  ///                          now. Zero value means "minimal acceptable", and
  ///                          negative means "keep current or use default". So,
  ///                          it is recommended always pass -1 in this argument
  ///                          except some special cases.
  ///
  /// @param sizeUpper         The upper bound of database size in bytes.
  ///                          Zero value means "minimal acceptable",
  ///                          and negative means "keep current or use default".
  ///                          It is recommended to avoid change upper bound while
  ///                          database is used by other processes or threaded
  ///                          (i.e. just pass -1 in this argument except absolutely
  ///                          necessary). Otherwise you must be ready for
  ///                          \ref MDBX_UNABLE_EXTEND_MAPSIZE error(s), unexpected
  ///                          pauses during remapping and/or system errors like
  ///                          "address busy", and so on. In other words, there
  ///                          is no way to handle a growth of the upper bound
  ///                          robustly because there may be a lack of appropriate
  ///                          system resources (which are extremely volatile in
  ///                          a multi-process multi-threaded environment).
  ///
  /// @param growthStep        The growth step in bytes, must be greater than
  ///                          zero to allow the database to grow. Negative value
  ///                          means "keep current or use default".
  ///
  /// @param shrinkThreshold   The shrink threshold in bytes, must be greater
  ///                          than zero to allow the database to shrink and
  ///                          greater than growth_step to avoid shrinking
  ///                          right after grow.
  ///                          Negative value means "keep current
  ///                          or use default". Default is 2*growth_step.
  ///
  /// @param pageSize          The database page size for new database
  ///                          creation or -1 otherwise. Once the database
  ///                          is created, the page size cannot be changed.
  ///                          Must be power of 2 in the range between
  ///                          \ref MDBX_MIN_PAGESIZE and
  ///                          \ref MDBX_MAX_PAGESIZE. Zero value means
  ///                          "minimal acceptable", and negative means
  ///                          "keep current or use default".
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_EINVAL    An invalid parameter was specified,
  ///                        or the environment has an active write transaction.
  /// \retval MDBX_EPERM     Two specific cases for Windows:
  ///                        1) Shrinking was disabled before via geometry settings
  ///                        and now it enabled, but there are reading threads that
  ///                        don't use the additional `SRWL` (which is required to
  ///                        avoid Windows issues).
  ///                        2) Temporary close memory mapped is required to change
  ///                        geometry, but there read transaction(s) is running
  ///                        and no corresponding thread(s) could be suspended
  ///                        since the \ref MDBX_NOSTICKYTHREADS mode is used.
  /// \retval MDBX_EACCESS   The environment opened in read-only.
  /// \retval MDBX_MAP_FULL  Specified size smaller than the space already
  ///                        consumed by the environment.
  /// \retval MDBX_TOO_LARGE Specified size is too large, i.e. too many pages for
  ///                        given size, or a 32-bit process requests too much
  ///                        bytes for the 32-bit address space.
  public int setGeometry(
      long sizeLower,
      long sizeNow,
      long sizeUpper,
      long growthStep,
      long shrinkThreshold,
      long pageSize) {
    try {
      return (int) CFunctions.MDBX_ENV_SET_GEOMETRY.invokeExact(
          envPointer, sizeLower, sizeNow, sizeUpper, growthStep, shrinkThreshold, pageSize);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Returns an application information (a context pointer) associated
  /// with the environment.
  ///
  /// \returns The pointer set by \ref mdbx_env_set_userctx()
  ///          or `NULL` if something wrong.
  public long userCtx() {
    try {
      return (long) CFunctions.MDBX_ENV_GET_USERCTX.invokeExact(envPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// \brief Sets application information (a context pointer) associated with
  /// the environment.
  ///
  /// @param ctx  An arbitrary pointer for whatever the application needs.
  /// \returns A non-zero error value on failure and 0 on success.
  public int userCtx(long ctx) {
    try {
      return (int) CFunctions.MDBX_ENV_SET_USERCTX.invokeExact(envPointer, ctx);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// \brief Open an environment instance using specific meta-page
  /// for checking and recovery.
  ///
  /// This function mostly of internal API for `mdbx_chk` utility and subject to
  /// change at any time. Do not use this function to avoid shooting your own
  /// leg(s).
  public int openForRecovery(int targetMeta, boolean writeable) {
    try {
      return (int)
          CFunctions.MDBX_ENV_OPEN_FOR_RECOVERY.invokeExact(envPointer, targetMeta, writeable);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// \brief Open an environment instance.
  /// \ingroup c_opening
  ///
  /// Indifferently this function will fail or not, the \ref mdbx_env_close() must
  /// be called later to discard the \ref MDBX_env handle and release associated
  /// resources.
  ///
  /// @apiNote On Windows the \ref mdbx_env_openW() is recommended to use.
  ///
  /// @param flags          Specifies options for this environment.
  ///                       This parameter must be bitwise OR'ing together
  ///                       any constants described above in the \ref env_flags
  ///                       and \ref sync_modes sections.
  ///
  /// Flags set by mdbx_env_set_flags() are also used:
  ///  - \ref MDBX_ENV_DEFAULTS, \ref MDBX_NOSUBDIR, \ref MDBX_RDONLY,
  ///    \ref MDBX_EXCLUSIVE, \ref MDBX_WRITEMAP, \ref MDBX_NOSTICKYTHREADS,
  ///    \ref MDBX_NORDAHEAD, \ref MDBX_NOMEMINIT, \ref MDBX_COALESCE,
  ///    \ref MDBX_LIFORECLAIM. See \ref env_flags section.
  ///
  ///  - \ref MDBX_SYNC_DURABLE, \ref MDBX_NOMETASYNC, \ref MDBX_SAFE_NOSYNC,
  ///    \ref MDBX_UTTERLY_NOSYNC. See \ref sync_modes section.
  ///
  /// @apiNote `MDB_NOLOCK` flag don't supported by MDBX,
  ///       try use \ref MDBX_EXCLUSIVE as a replacement.
  ///
  /// @apiNote MDBX don't allow to mix processes with different \ref MDBX_SAFE_NOSYNC
  ///       flags on the same environment.
  ///       In such case \ref MDBX_INCOMPATIBLE will be returned.
  ///
  /// If the database is already exist and parameters specified early by
  /// \ref mdbx_env_set_geometry() are incompatible (i.e. for instance, different
  /// page size) then \ref mdbx_env_open() will return \ref MDBX_INCOMPATIBLE
  /// error.
  ///
  /// @param mode   The UNIX permissions to set on created files.
  ///               Zero value means to open existing, but do not create.
  ///
  /// \return A non-zero error value on failure and 0 on success,
  ///         some possible errors are:
  /// \retval MDBX_VERSION_MISMATCH The version of the MDBX library doesn't match
  ///                            the version that created the database environment.
  /// \retval MDBX_INVALID       The environment file headers are corrupted.
  /// \retval MDBX_ENOENT        The directory specified by the path parameter
  ///                            doesn't exist.
  /// \retval MDBX_EACCES        The user didn't have permission to access
  ///                            the environment files.
  /// \retval MDBX_BUSY          The \ref MDBX_EXCLUSIVE flag was specified and the
  ///                            environment is in use by another process,
  ///                            or the current process tries to open environment
  ///                            more than once.
  /// \retval MDBX_INCOMPATIBLE  Environment is already opened by another process,
  ///                            but with different set of \ref MDBX_SAFE_NOSYNC,
  ///                            \ref MDBX_UTTERLY_NOSYNC flags.
  ///                            Or if the database is already exist and parameters
  ///                            specified early by \ref mdbx_env_set_geometry()
  ///                            are incompatible (i.e. different pagesize, etc).
  /// \retval MDBX_WANNA_RECOVERY The \ref MDBX_RDONLY flag was specified but
  ///                             read-write access is required to rollback
  ///                             inconsistent state after a system crash.
  /// \retval MDBX_TOO_LARGE      Database is too large for this process,
  ///                             i.e. 32-bit process tries to open >4Gb database.
  public int open(int flags, int mode) {
    try {
      return (int) CFunctions.MDBX_ENV_OPEN.invokeExact(envPointer, pathPointer, flags, mode);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Close the environment and release the memory map.
  ///
  /// Only a single thread may call this function. All transactions, tables,
  /// and cursors must already be closed before calling this function. Attempts
  /// to use any such handles after calling this function is UB and would cause
  /// a `SIGSEGV`. The environment handle will be freed and must not be used again
  /// after this call.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_BUSY   The write transaction is running by other thread,
  ///                     in such case \ref MDBX_env instance has NOT be destroyed
  ///                     not released!
  ///                     \note If any OTHER error code was returned then
  ///                     given MDBX_env instance has been destroyed and released.
  ///
  /// \retval MDBX_EBADSIGN  Environment handle already closed or not valid,
  ///                        i.e. \ref mdbx_env_close() was already called for the
  ///                        `env` or was not created by \ref mdbx_env_create().
  ///
  /// \retval MDBX_PANIC  If \ref mdbx_env_close_ex() was called in the child
  ///                     process after `fork()`. In this case \ref MDBX_PANIC
  ///                     is expected, i.e. \ref MDBX_env instance was freed in
  ///                     proper manner.
  ///
  /// \retval MDBX_EIO    An error occurred during the flushing/writing data
  ///                     to a storage medium/disk.
  public int close() {
    return close(false);
  }

  /// Close the environment and release the memory map.
  ///
  /// Only a single thread may call this function. All transactions, tables,
  /// and cursors must already be closed before calling this function. Attempts
  /// to use any such handles after calling this function is UB and would cause
  /// a `SIGSEGV`. The environment handle will be freed and must not be used again
  /// after this call.
  ///
  /// @param dontSync        A dont'sync flag, if non-zero the last checkpoint
  ///                        will be kept "as is" and may be still "weak" in the
  ///                        \ref MDBX_SAFE_NOSYNC or \ref MDBX_UTTERLY_NOSYNC
  ///                        modes. Such "weak" checkpoint will be ignored on
  ///                        opening next time, and transactions since the last
  ///                        non-weak checkpoint (meta-page update) will rolledback
  ///                        for consistency guarantee.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_BUSY   The write transaction is running by other thread,
  ///                     in such case \ref MDBX_env instance has NOT be destroyed
  ///                     not released!
  ///                     \note If any OTHER error code was returned then
  ///                     given MDBX_env instance has been destroyed and released.
  ///
  /// \retval MDBX_EBADSIGN  Environment handle already closed or not valid,
  ///                        i.e. \ref mdbx_env_close() was already called for the
  ///                        `env` or was not created by \ref mdbx_env_create().
  ///
  /// \retval MDBX_PANIC  If \ref mdbx_env_close_ex() was called in the child
  ///                     process after `fork()`. In this case \ref MDBX_PANIC
  ///                     is expected, i.e. \ref MDBX_env instance was freed in
  ///                     proper manner.
  ///
  /// \retval MDBX_EIO    An error occurred during the flushing/writing data
  ///                     to a storage medium/disk.
  public int close(boolean dontSync) {
    try {
      return (int) CFunctions.MDBX_ENV_CLOSE_EX.invokeExact(envPointer, dontSync);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    } finally {
      freeMemory();
    }
  }

  private void freeMemory() {
    final var scratchPointer = this.scratchPointer;
    if (scratchPointer != 0L) {
      Memory.dealloc(scratchPointer);
      this.scratchPointer = 0L;
    }
    final var pathPointer = this.pathPointer;
    if (pathPointer != 0L) {
      Memory.dealloc(pathPointer);
      this.pathPointer = 0L;
    }
  }

  public int sync() {
    return sync(true, false);
  }

  public int syncPoll() {
    return sync(false, true);
  }

  /// \brief Flush the environment data buffers to disk.
  /// \ingroup c_extra
  ///
  /// Unless the environment was opened with no-sync flags (\ref MDBX_NOMETASYNC,
  /// \ref MDBX_SAFE_NOSYNC and \ref MDBX_UTTERLY_NOSYNC), then
  /// data is always written an flushed to disk when \ref mdbx_txn_commit() is
  /// called. Otherwise \ref mdbx_env_sync() may be called to manually write and
  /// flush unsynced data to disk.
  ///
  /// Besides, \ref mdbx_env_sync_ex() with argument `force=false` may be used to
  /// provide polling mode for lazy/asynchronous sync in conjunction with
  /// \ref mdbx_env_set_syncbytes() and/or \ref mdbx_env_set_syncperiod().
  /// \note This call is not valid if the environment was opened with MDBX_RDONLY.
  ///
  /// @param force         If non-zero, force a flush. Otherwise, If force is
  ///                      zero, then will run in polling mode,
  ///                      i.e. it will check the thresholds that were
  ///                      set \ref mdbx_env_set_syncbytes()
  ///                      and/or \ref mdbx_env_set_syncperiod() and perform flush
  ///                      if at least one of the thresholds is reached.
  ///
  /// @param nonblock      Don't wait if write transaction
  ///                      is running by other thread.
  ///
  /// \returns A non-zero error value on failure and \ref MDBX_RESULT_TRUE or 0 on
  ///     success. The \ref MDBX_RESULT_TRUE means no data pending for flush
  ///     to disk, and 0 otherwise. Some possible errors are:
  /// \retval MDBX_EACCES   The environment is read-only.
  ///
  /// \retval MDBX_BUSY     The environment is used by other thread
  ///                       and `nonblock=true`.
  ///
  /// \retval MDBX_EINVAL   An invalid parameter was specified.
  ///
  /// \retval MDBX_EIO      An error occurred during the flushing/writing data
  ///                       to a storage medium/disk.
  public int sync(boolean force, boolean nonblock) {
    try {
      return (int) CFunctions.MDBX_ENV_SYNC_EX.invokeExact(envPointer, force, nonblock);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// \brief Create a transaction with a user provided context pointer
  /// for use with the environment.
  /// \ingroup c_transactions
  /// The transaction handle may be discarded using \ref mdbx_txn_abort()
  /// or \ref mdbx_txn_commit().
  /// \see mdbx_txn_begin()
  /// \note A transaction and its cursors must only be used by a single thread,
  /// and a thread may only have a single transaction at a time unless
  /// the \ref MDBX_NOSTICKYTHREADS is used.
  /// \note Cursors may not span transactions.
  ///
  /// @param parent  If this parameter is non-NULL, the new transaction will
  ///                     be a nested transaction, with the transaction indicated
  ///                     by parent as its parent. Transactions may be nested
  ///                     to any level. A parent transaction and its cursors may
  ///                     not issue any other operations than mdbx_txn_commit and
  ///                     \ref mdbx_txn_abort() while it has active child
  ///                     transactions.
  /// @param flags   Special options for this transaction. This parameter
  ///                     must be set to 0 or by bitwise OR'ing together one
  ///                     or more of the values described here:
  ///                      - \ref MDBX_RDONLY   This transaction will not perform
  ///                                           any write operations.
  ///                      - \ref MDBX_TXN_TRY  Do not block when starting
  ///                                           a write transaction.
  ///                      - \ref MDBX_SAFE_NOSYNC, \ref MDBX_NOMETASYNC.
  ///                        Do not sync data to disk corresponding
  ///                        to \ref MDBX_NOMETASYNC or \ref MDBX_SAFE_NOSYNC
  ///                        description. \see sync_modes
  /// @param context A pointer to application context to be associated with
  ///                     created transaction and could be retrieved by
  ///                     \ref mdbx_txn_get_userctx() until transaction finished.
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_PANIC         A fatal error occurred earlier and the
  ///                            environment must be shut down.
  /// \retval MDBX_UNABLE_EXTEND_MAPSIZE  Another process wrote data beyond
  ///                                     this MDBX_env's mapsize and this
  ///                                     environment map must be resized as well.
  ///                                     See \ref mdbx_env_set_mapsize().
  /// \retval MDBX_READERS_FULL  A read-only transaction was requested and
  ///                            the reader lock table is full.
  ///                            See \ref mdbx_env_set_maxreaders().
  /// \retval MDBX_ENOMEM        Out of memory.
  /// \retval MDBX_BUSY          The write transaction is already started by the
  ///                            current thread.
  public Txn begin(Txn parent, int flags, long context) {
    final var txnPointer = Memory.alloc(8L);
    try {
      final var parentPointer = parent == null ? 0 : parent.txnPointer;
      final int err;
      try {
        err = (int) CFunctions.MDBX_TXN_BEGIN_EX.invokeExact(
            envPointer, parentPointer, flags, txnPointer, context);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
      if (err != Error.SUCCESS) {
        throw new MDBXError(err);
      }
      final var txn = new Txn();
      txn.init(this, Danger.UNSAFE.getLong(txnPointer), flags);
      return txn;
    } finally {
      Memory.dealloc(txnPointer);
    }
  }

  /// Environment flags
  public interface Flags {
    int DEFAULTS = 0;

    /// Extra validation of DB structure and pages content.
    ///
    /// The `MDBX_VALIDATION` enabled the simple safe/careful mode for working
    /// with damaged or untrusted DB. However, a notable performance
    /// degradation should be expected.
    int VALIDATION = 0x00002000;

    /// No environment directory.
    ///
    /// By default, MDBX creates its environment in a directory whose pathname is
    /// given in path, and creates its data and lock files under that directory.
    /// With this option, path is used as-is for the database main data file.
    /// The database lock file is the path with "-lck" appended.
    ///
    /// - with `MDBX_NOSUBDIR` = in a filesystem we have the pair of MDBX-files
    ///   which names derived from given pathname by appending predefined suffixes.
    ///
    /// - without `MDBX_NOSUBDIR` = in a filesystem we have the MDBX-directory with
    ///   given pathname, within that a pair of MDBX-files with predefined names.
    ///
    /// This flag affects only at new environment creating by \ref mdbx_env_open(),
    /// otherwise at opening an existing environment libmdbx will choice this
    /// automatically.
    int NOSUBDIR = 0x4000;

    /// Read only mode.
    ///
    /// Open the environment in read-only mode. No write operations will be
    /// allowed. MDBX will still modify the lock file - except on read-only
    /// filesystems, where MDBX does not use locks.
    ///
    /// - with `MDBX_RDONLY` = open environment in read-only mode.
    ///   MDBX supports pure read-only mode (i.e. without opening LCK-file) only
    ///   when environment directory and/or both files are not writable (and the
    ///   LCK-file may be missing). In such case allowing file(s) to be placed
    ///   on a network read-only share.
    ///
    /// - without `MDBX_RDONLY` = open environment in read-write mode.
    ///
    /// This flag affects only at environment opening but can't be changed after.
    int RDONLY = 0x20000;

    /// Open environment in exclusive/monopolistic mode.
    ///
    /// `MDBX_EXCLUSIVE` flag can be used as a replacement for `MDB_NOLOCK`,
    /// which don't supported by MDBX.
    /// In this way, you can get the minimal overhead, but with the correct
    /// multi-process and multi-thread locking.
    ///
    /// - with `MDBX_EXCLUSIVE` = open environment in exclusive/monopolistic mode
    ///   or return \ref MDBX_BUSY if environment already used by other process.
    ///   The main feature of the exclusive mode is the ability to open the
    ///   environment placed on a network share.
    ///
    /// - without `MDBX_EXCLUSIVE` = open environment in cooperative mode,
    ///   i.e. for multi-process access/interaction/cooperation.
    ///   The main requirements of the cooperative mode are:
    ///
    ///   1. data files MUST be placed in the LOCAL file system,
    ///      but NOT on a network share.
    ///   2. environment MUST be opened only by LOCAL processes,
    ///      but NOT over a network.
    ///   3. OS kernel (i.e. file system and memory mapping implementation) and
    ///      all processes that open the given environment MUST be running
    ///      in the physically single RAM with cache-coherency. The only
    ///      exception for cache-consistency requirement is Linux on MIPS
    ///      architecture, but this case has not been tested for a long time).
    ///
    /// This flag affects only at environment opening but can't be changed after.
    int EXCLUSIVE = 0x400000;

    /// Using database/environment which already opened by another process(es).
    ///
    /// The `MDBX_ACCEDE` flag is useful to avoid \ref MDBX_INCOMPATIBLE error
    /// while opening the database/environment which is already used by another
    /// process(es) with unknown mode/flags. In such cases, if there is a
    /// difference in the specified flags (\ref MDBX_NOMETASYNC,
    /// \ref MDBX_SAFE_NOSYNC, \ref MDBX_UTTERLY_NOSYNC, \ref MDBX_LIFORECLAIM
    /// and \ref MDBX_NORDAHEAD), instead of returning an error,
    /// the database will be opened in a compatibility with the already used mode.
    ///
    /// `MDBX_ACCEDE` has no effect if the current process is the only one either
    /// opening the DB in read-only mode or other process(es) uses the DB in
    /// read-only mode.
    int ACCEDE = 0x40000000;

    /// Map data into memory with write permission.
    ///
    /// Use a writeable memory map unless \ref MDBX_RDONLY is set. This uses fewer
    /// mallocs and requires much less work for tracking database pages, but
    /// loses protection from application bugs like wild pointer writes and other
    /// bad updates into the database. This may be slightly faster for DBs that
    /// fit entirely in RAM, but is slower for DBs larger than RAM. Also adds the
    /// possibility for stray application writes thru pointers to silently
    /// corrupt the database.
    ///
    /// - with `MDBX_WRITEMAP` = all data will be mapped into memory in the
    ///   read-write mode. This offers a significant performance benefit, since the
    ///   data will be modified directly in mapped memory and then flushed to disk
    ///   by single system call, without any memory management nor copying.
    ///
    /// - without `MDBX_WRITEMAP` = data will be mapped into memory in the
    ///   read-only mode. This requires stocking all modified database pages in
    ///   memory and then writing them to disk through file operations.
    ///
    /// \warning On the other hand, `MDBX_WRITEMAP` adds the possibility for stray
    /// application writes thru pointers to silently corrupt the database.
    ///
    /// \note The `MDBX_WRITEMAP` mode is incompatible with nested transactions,
    /// since this is unreasonable. I.e. nested transactions requires mallocation
    /// of database pages and more work for tracking ones, which neuters a
    /// performance boost caused by the `MDBX_WRITEMAP` mode.
    ///
    /// This flag affects only at environment opening but can't be changed after.
    int WRITEMAP = 0x80000;

    int NOSTICKYTHREADS = 0x200000;

    /// Don't do readahead.
    ///
    /// Turn off readahead. Most operating systems perform readahead on read
    /// requests by default. This option turns it off if the OS supports it.
    /// Turning it off may help random read performance when the DB is larger
    /// than RAM and system RAM is full.
    ///
    /// By default libmdbx dynamically enables/disables readahead depending on
    /// the actual database size and currently available memory. On the other
    /// hand, such automation has some limitation, i.e. could be performed only
    /// when DB size changing but can't tracks and reacts changing a free RAM
    /// availability, since it changes independently and asynchronously.
    ///
    /// \note The mdbx_is_readahead_reasonable() function allows to quickly find
    /// out whether to use readahead or not based on the size of the data and the
    /// amount of available memory.
    ///
    /// This flag affects only at environment opening and can't be changed after.
    int NORDAHEAD = 0x800000;

    /// Don't initialize malloc'ed memory before writing to datafile.
    ///
    /// Don't initialize malloc'ed memory before writing to unused spaces in the
    /// data file. By default, memory for pages written to the data file is
    /// obtained using malloc. While these pages may be reused in subsequent
    /// transactions, freshly malloc'ed pages will be initialized to zeroes before
    /// use. This avoids persisting leftover data from other code (that used the
    /// heap and subsequently freed the memory) into the data file.
    ///
    /// Note that many other system libraries may allocate and free memory from
    /// the heap for arbitrary uses. E.g., stdio may use the heap for file I/O
    /// buffers. This initialization step has a modest performance cost so some
    /// applications may want to disable it using this flag. This option can be a
    /// problem for applications which handle sensitive data like passwords, and
    /// it makes memory checkers like Valgrind noisy. This flag is not needed
    /// with \ref MDBX_WRITEMAP, which writes directly to the mmap instead of using
    /// malloc for pages. The initialization is also skipped if \ref MDBX_RESERVE
    /// is used; the caller is expected to overwrite all of the memory that was
    /// reserved in that case.
    ///
    /// This flag may be changed at any time using `mdbx_env_set_flags()`.
    int NOMEMINIT = 0x1000000;

    /// LIFO policy for recycling a Garbage Collection items.
    ///
    /// `MDBX_LIFORECLAIM` flag turns on LIFO policy for recycling a Garbage
    /// Collection items, instead of FIFO by default. On systems with a disk
    /// write-back cache, this can significantly increase write performance, up
    /// to several times in a best case scenario.
    ///
    /// LIFO recycling policy means that for reuse pages will be taken which became
    /// unused the lastest (i.e. just now or most recently). Therefore the loop of
    /// database pages circulation becomes as short as possible. In other words,
    /// the number of pages, that are overwritten in memory and on disk during a
    /// series of write transactions, will be as small as possible. Thus creates
    /// ideal conditions for the efficient operation of the disk write-back cache.
    /// \ref MDBX_LIFORECLAIM is compatible with all no-sync flags, but gives NO
    /// noticeable impact in combination with \ref MDBX_SAFE_NOSYNC or
    /// \ref MDBX_UTTERLY_NOSYNC. Because MDBX will reused pages only before the
    /// last "steady" MVCC-snapshot, i.e. the loop length of database pages
    /// circulation will be mostly defined by frequency of calling
    /// \ref mdbx_env_sync() rather than LIFO and FIFO difference.
    ///
    /// This flag may be changed at any time using mdbx_env_set_flags().
    int LIFORECLAIM = 0x4000000;

    /// Debugging option, fill/perturb released pages.
    int PAGPERTURB = 0x8000000;

    /* SYNC MODES****************************************************************/
    /// \defgroup sync_modes SYNC MODES
    /// \attention Using any combination of \ref MDBX_SAFE_NOSYNC, \ref
    /// MDBX_NOMETASYNC and especially \ref MDBX_UTTERLY_NOSYNC is always a deal to
    /// reduce durability for gain write performance. You must know exactly what
    /// you are doing and what risks you are taking!
    /// \note for LMDB users: \ref MDBX_SAFE_NOSYNC is NOT similar to LMDB_NOSYNC,
    /// but \ref MDBX_UTTERLY_NOSYNC is exactly match LMDB_NOSYNC. See details
    /// below.
    ///
    /// THE SCENE:
    /// - The DAT-file contains several MVCC-snapshots of B-tree at same time,
    ///   each of those B-tree has its own root page.
    /// - Each of meta pages at the beginning of the DAT file contains a
    ///   pointer to the root page of B-tree which is the result of the particular
    ///   transaction, and a number of this transaction.
    /// - For data durability, MDBX must first write all MVCC-snapshot data
    ///   pages and ensure that are written to the disk, then update a meta page
    ///   with the new transaction number and a pointer to the corresponding new
    ///   root page, and flush any buffers yet again.
    /// - Thus during commit a I/O buffers should be flushed to the disk twice;
    ///   i.e. fdatasync(), FlushFileBuffers() or similar syscall should be
    ///   called twice for each commit. This is very expensive for performance,
    ///   but guaranteed durability even on unexpected system failure or power
    ///   outage. Of course, provided that the operating system and the
    ///   underlying hardware (e.g. disk) work correctly.
    ///
    /// TRADE-OFF:
    /// By skipping some stages described above, you can significantly benefit in
    /// speed, while partially or completely losing in the guarantee of data
    /// durability and/or consistency in the event of system or power failure.
    /// Moreover, if for any reason disk write order is not preserved, then at
    /// moment of a system crash, a meta-page with a pointer to the new B-tree may
    /// be written to disk, while the itself B-tree not yet. In that case, the
    /// database will be corrupted!
    /// \see MDBX_SYNC_DURABLE \see MDBX_NOMETASYNC \see MDBX_SAFE_NOSYNC
    /// \see MDBX_UTTERLY_NOSYNC

    /// Default robust and durable sync mode.
    ///
    /// Metadata is written and flushed to disk after a data is written and
    /// flushed, which guarantees the integrity of the database in the event
    /// of a crash at any time.
    ///
    /// \attention Please do not use other modes until you have studied all the
    /// details and are sure. Otherwise, you may lose your users' data, as happens
    /// in [Miranda NG](https://www.miranda-ng.org/) messenger.
    int SYNC_DURABLE = 0;

    /// Don't sync the meta-page after commit.
    ///
    /// Flush system buffers to disk only once per transaction commit, omit the
    /// metadata flush. Defer that until the system flushes files to disk,
    /// or next non-\ref MDBX_RDONLY commit or \ref mdbx_env_sync(). Depending on
    /// the platform and hardware, with \ref MDBX_NOMETASYNC you may get a doubling
    /// of write performance.
    ///
    /// This trade-off maintains database integrity, but a system crash may
    /// undo the last committed transaction. I.e. it preserves the ACI
    /// (atomicity, consistency, isolation) but not D (durability) database
    /// property.
    ///
    /// `MDBX_NOMETASYNC` flag may be changed at any time using
    /// \ref mdbx_env_set_flags() or by passing to \ref mdbx_txn_begin() for
    /// particular write transaction. \see sync_modes
    int NOMETASYNC = 0x40000;

    /// Don't sync anything but keep previous steady commits.
    /// Like \ref MDBX_UTTERLY_NOSYNC the `MDBX_SAFE_NOSYNC` flag disable similarly
    /// flush system buffers to disk when committing a transaction. But there is a
    /// huge difference in how are recycled the MVCC snapshots corresponding to
    /// previous "steady" transactions (see below).
    /// With \ref MDBX_WRITEMAP the `MDBX_SAFE_NOSYNC` instructs MDBX to use
    /// asynchronous mmap-flushes to disk. Asynchronous mmap-flushes means that
    /// actually all writes will scheduled and performed by operation system on it
    /// own manner, i.e. unordered. MDBX itself just notify operating system that
    /// it would be nice to write data to disk, but no more.
    /// Depending on the platform and hardware, with `MDBX_SAFE_NOSYNC` you may get
    /// a multiple increase of write performance, even 10 times or more.
    /// In contrast to \ref MDBX_UTTERLY_NOSYNC mode, with `MDBX_SAFE_NOSYNC` flag
    /// MDBX will keeps untouched pages within B-tree of the last transaction
    /// "steady" which was synced to disk completely. This has big implications for
    /// both data durability and (unfortunately) performance:
    ///  - a system crash can't corrupt the database, but you will lose the last
    ///    transactions; because MDBX will rollback to last steady commit since it
    ///    kept explicitly.
    ///  - the last steady transaction makes an effect similar to "long-lived" read
    ///    transaction (see above in the \ref restrictions section) since prevents
    ///    reuse of pages freed by newer write transactions, thus the any data
    ///    changes will be placed in newly allocated pages.
    ///  - to avoid rapid database growth, the system will sync data and issue
    ///    a steady commit-point to resume reuse pages, each time there is
    ///    insufficient space and before increasing the size of the file on disk.
    /// In other words, with `MDBX_SAFE_NOSYNC` flag MDBX insures you from the
    /// whole database corruption, at the cost increasing database size and/or
    /// number of disk IOPs. So, `MDBX_SAFE_NOSYNC` flag could be used with
    /// \ref mdbx_env_sync() as alternatively for batch committing or nested
    /// transaction (in some cases). As well, auto-sync feature exposed by
    /// \ref mdbx_env_set_syncbytes() and \ref mdbx_env_set_syncperiod() functions
    /// could be very useful with `MDBX_SAFE_NOSYNC` flag.
    /// The number and volume of disk IOPs with MDBX_SAFE_NOSYNC flag will
    /// exactly the as without any no-sync flags. However, you should expect a
    /// larger process's [work set](https://bit.ly/2kA2tFX) and significantly worse
    /// a [locality of reference](https://bit.ly/2mbYq2J), due to the more
    /// intensive allocation of previously unused pages and increase the size of
    /// the database.
    /// `MDBX_SAFE_NOSYNC` flag may be changed at any time using
    /// \ref mdbx_env_set_flags() or by passing to \ref mdbx_txn_begin() for
    /// particular write transaction.
    int SAFE_NOSYNC = 0x10000;

    /// \deprecated Please use \ref MDBX_SAFE_NOSYNC instead of `MDBX_MAPASYNC`.
    /// Since version 0.9.x the `MDBX_MAPASYNC` is deprecated and has the same
    /// effect as \ref MDBX_SAFE_NOSYNC with \ref MDBX_WRITEMAP. This just API
    /// simplification is for convenience and clarity.
    int MAPASYNC = 0x10000;

    /// Don't sync anything and wipe previous steady commits.
    /// Don't flush system buffers to disk when committing a transaction. This
    /// optimization means a system crash can corrupt the database, if buffers are
    /// not yet flushed to disk. Depending on the platform and hardware, with
    /// `MDBX_UTTERLY_NOSYNC` you may get a multiple increase of write performance,
    /// even 100 times or more.
    /// If the filesystem preserves write order (which is rare and never provided
    /// unless explicitly noted) and the \ref MDBX_WRITEMAP and \ref
    /// MDBX_LIFORECLAIM flags are not used, then a system crash can't corrupt the
    /// database, but you can lose the last transactions, if at least one buffer is
    /// not yet flushed to disk. The risk is governed by how often the system
    /// flushes dirty buffers to disk and how often \ref mdbx_env_sync() is called.
    /// So, transactions exhibit ACI (atomicity, consistency, isolation) properties
    /// and only lose `D` (durability). I.e. database integrity is maintained, but
    /// a system crash may undo the final transactions.
    /// Otherwise, if the filesystem not preserves write order (which is
    /// typically) or \ref MDBX_WRITEMAP or \ref MDBX_LIFORECLAIM flags are used,
    /// you should expect the corrupted database after a system crash.
    /// So, most important thing about `MDBX_UTTERLY_NOSYNC`:
    ///  - a system crash immediately after commit the write transaction
    ///    high likely lead to database corruption.
    ///  - successful completion of mdbx_env_sync(force = true) after one or
    ///    more committed transactions guarantees consistency and durability.
    ///  - BUT by committing two or more transactions you back database into
    ///    a weak state, in which a system crash may lead to database corruption!
    ///    In case single transaction after mdbx_env_sync, you may lose transaction
    ///    itself, but not a whole database.
    /// Nevertheless, `MDBX_UTTERLY_NOSYNC` provides "weak" durability in case
    /// of an application crash (but no durability on system failure), and
    /// therefore may be very useful in scenarios where data durability is
    /// not required over a system failure (e.g for short-lived data), or if you
    /// can take such risk.
    /// `MDBX_UTTERLY_NOSYNC` flag may be changed at any time using
    /// \ref mdbx_env_set_flags(), but don't has effect if passed to
    /// \ref mdbx_txn_begin() for particular write transaction. \see sync_modes
    int UTTERLY_NOSYNC = 0x100000;
  }

  /// \brief MDBX environment extra runtime options.
  /// \ingroup c_settings
  /// \see mdbx_env_set_option() \see mdbx_env_get_option()
  public interface Option {
    /// \brief Controls the maximum number of named tables for the environment.
    ///
    /// \details By default only unnamed key-value table could used and
    /// appropriate value should set by `MDBX_opt_max_db` to using any more named
    /// table(s). To reduce overhead, use the minimum sufficient value. This option
    /// may only set after \ref mdbx_env_create() and before \ref mdbx_env_open().
    ///
    /// \see mdbx_env_set_maxdbs() \see mdbx_env_get_maxdbs()
    int MAX_DB = 0;

    /// \brief Defines the maximum number of threads/reader slots
    /// for all processes interacting with the database.
    ///
    /// \details This defines the number of slots in the lock table that is used to
    /// track readers in the environment. The default is about 100 for 4K
    /// system page size. Starting a read-only transaction normally ties a lock
    /// table slot to the current thread until the environment closes or the thread
    /// exits. If \ref MDBX_NOSTICKYTHREADS is in use, \ref mdbx_txn_begin()
    /// instead ties the slot to the \ref MDBX_txn object until it or the \ref
    /// MDBX_env object is destroyed. This option may only set after \ref
    /// mdbx_env_create() and before \ref mdbx_env_open(), and has an effect only
    /// when the database is opened by the first process interacts with the
    /// database.
    ///
    /// \see mdbx_env_set_maxreaders() \see mdbx_env_get_maxreaders()
    int MAX_READERS = 1;

    /// \brief Controls interprocess/shared threshold to force flush the data
    /// buffers to disk, if \ref MDBX_SAFE_NOSYNC is used.
    ///
    /// \see mdbx_env_set_syncbytes() \see mdbx_env_get_syncbytes()
    int SYNC_BYTES = 2;

    /// \brief Controls interprocess/shared relative period since the last
    /// unsteady commit to force flush the data buffers to disk,
    /// if \ref MDBX_SAFE_NOSYNC is used.
    /// \see mdbx_env_set_syncperiod() \see mdbx_env_get_syncperiod()
    int SYNC_PERIOD = 3;

    /// \brief Controls the in-process limit to grow a list of reclaimed/recycled
    /// page's numbers for finding a sequence of contiguous pages for large data
    /// items.
    /// \see MDBX_opt_gc_time_limit
    ///
    /// \details A long values requires allocation of contiguous database pages.
    /// To find such sequences, it may be necessary to accumulate very large lists,
    /// especially when placing very long values (more than a megabyte) in a large
    /// databases (several tens of gigabytes), which is much expensive in extreme
    /// cases. This threshold allows you to avoid such costs by allocating new
    /// pages at the end of the database (with its possible growth on disk),
    /// instead of further accumulating/reclaiming Garbage Collection records.
    ///
    /// On the other hand, too small threshold will lead to unreasonable database
    /// growth, or/and to the inability of put long values.
    /// The `MDBX_opt_rp_augment_limit` controls described limit for the current
    /// process. By default, this limit adjusted dynamically to 1/3 of current
    /// quantity of DB pages, which is usually enough for most cases.
    int RP_AUGMENT_LIMIT = 4;

    /// \brief Controls the in-process limit to grow a cache of dirty
    /// pages for reuse in the current transaction.
    ///
    /// \details A 'dirty page' refers to a page that has been updated in memory
    /// only, the changes to a dirty page are not yet stored on disk.
    /// To reduce overhead, it is reasonable to release not all such pages
    /// immediately, but to leave some ones in cache for reuse in the current
    /// transaction.
    ///
    /// The `MDBX_opt_loose_limit` allows you to set a limit for such cache inside
    /// the current process. Should be in the range 0..255, default is 64.
    int LOOSE_LIMIT = 5;

    /// \brief Controls the in-process limit of a pre-allocated memory items
    /// for dirty pages.
    ///
    /// \details A 'dirty page' refers to a page that has been updated in memory
    /// only, the changes to a dirty page are not yet stored on disk.
    /// Without \ref MDBX_WRITEMAP dirty pages are allocated from memory and
    /// released when a transaction is committed. To reduce overhead, it is
    /// reasonable to release not all ones, but to leave some allocations in
    /// reserve for reuse in the next transaction(s).
    ///
    /// The `MDBX_opt_dp_reserve_limit` allows you to set a limit for such reserve
    /// inside the current process. Default is 1024.
    int DP_RESERVE_LIMIT = 6;

    /// \brief Controls the in-process limit of dirty pages
    /// for a write transaction.
    ///
    /// \details A 'dirty page' refers to a page that has been updated in memory
    /// only, the changes to a dirty page are not yet stored on disk.
    /// Without \ref MDBX_WRITEMAP dirty pages are allocated from memory and will
    /// be busy until are written to disk. Therefore for a large transactions is
    /// reasonable to limit dirty pages collecting above an some threshold but
    /// spill to disk instead.
    ///
    /// The `MDBX_opt_txn_dp_limit` controls described threshold for the current
    /// process. Default is 1/42 of the sum of whole and currently available RAM
    /// size, which the same ones are reported by \ref mdbx_get_sysraminfo().
    int TXN_DP_LIMIT = 7;

    /// \brief Controls the in-process initial allocation size for dirty pages
    /// list of a write transaction. Default is 1024.
    int TXN_DP_INITIAL = 8;

    /// \brief Controls the in-process how maximal part of the dirty pages may be
    /// spilled when necessary.
    ///
    /// \details The `MDBX_opt_spill_max_denominator` defines the denominator for
    /// limiting from the top for part of the current dirty pages may be spilled
    /// when the free room for a new dirty pages (i.e. distance to the
    /// `MDBX_opt_txn_dp_limit` threshold) is not enough to perform requested
    /// operation.
    ///
    /// Exactly `max_pages_to_spill = dirty_pages - dirty_pages / N`,
    /// where `N` is the value set by `MDBX_opt_spill_max_denominator`.
    ///
    /// Should be in the range 0..255, where zero means no limit, i.e. all dirty
    /// pages could be spilled. Default is 8, i.e. no more than 7/8 of the current
    /// dirty pages may be spilled when reached the condition described above.
    int SPILL_MAX_DENOMINATOR = 9;

    /// \brief Controls the in-process how minimal part of the dirty pages should
    /// be spilled when necessary.
    ///
    /// \details The `MDBX_opt_spill_min_denominator` defines the denominator for
    /// limiting from the bottom for part of the current dirty pages should be
    /// spilled when the free room for a new dirty pages (i.e. distance to the
    /// `MDBX_opt_txn_dp_limit` threshold) is not enough to perform requested
    /// operation.
    ///
    /// Exactly `min_pages_to_spill = dirty_pages / N`,
    /// where `N` is the value set by `MDBX_opt_spill_min_denominator`.
    ///
    /// Should be in the range 0..255, where zero means no restriction at the
    /// bottom. Default is 8, i.e. at least the 1/8 of the current dirty pages
    /// should be spilled when reached the condition described above.
    int SPILL_MIN_DENOMINATOR = 10;

    /// \brief Controls the in-process how much of the parent transaction dirty
    /// pages will be spilled while start each child transaction.
    ///
    /// \details The `MDBX_opt_spill_parent4child_denominator` defines the
    /// denominator to determine how much of parent transaction dirty pages will be
    /// spilled explicitly while start each child transaction.
    /// Exactly `pages_to_spill = dirty_pages / N`,
    /// where `N` is the value set by `MDBX_opt_spill_parent4child_denominator`.
    ///
    /// For a stack of nested transactions each dirty page could be spilled only
    /// once, and parent's dirty pages couldn't be spilled while child
    /// transaction(s) are running. Therefore a child transaction could reach
    /// \ref MDBX_TXN_FULL when parent(s) transaction has  spilled too less (and
    /// child reach the limit of dirty pages), either when parent(s) has spilled
    /// too more (since child can't spill already spilled pages). So there is no
    /// universal golden ratio.
    ///
    /// Should be in the range 0..255, where zero means no explicit spilling will
    /// be performed during starting nested transactions.
    /// Default is 0, i.e. by default no spilling performed during starting nested
    /// transactions, that correspond historically behaviour.
    int SPILL_PARENT4CHILD_DENOMINATOR = 11;

    /// \brief Controls the in-process threshold of semi-empty pages merge.
    /// \details This option controls the in-process threshold of minimum page
    /// fill, as used space of percentage of a page. Neighbour pages emptier than
    /// this value are candidates for merging. The threshold value is specified
    /// in 1/65536 points of a whole page, which is equivalent to the 16-dot-16
    /// fixed point format.
    ///
    /// The specified value must be in the range from 12.5% (almost empty page)
    /// to 50% (half empty page) which corresponds to the range from 8192 and
    /// to 32768 in units respectively.
    ///
    /// \see MDBX_opt_prefer_waf_insteadof_balance
    int MERGE_THRESHOLD_16DOT16_PERCENT = 12;

    /// \brief Controls the choosing between use write-through disk writes and
    /// usual ones with followed flush by the `fdatasync()` syscall.
    /// \details Depending on the operating system, storage subsystem
    /// characteristics and the use case, higher performance can be achieved by
    /// either using write-through or a serie of usual/lazy writes followed by
    /// the flush-to-disk.
    ///
    /// Basically for N chunks the latency/cost of write-through is:
    ///  latency = N * (emit + round-trip-to-storage + storage-execution);
    /// And for serie of lazy writes with flush is:
    ///  latency = N * (emit + storage-execution) + flush + round-trip-to-storage.
    ///
    /// So, for large N and/or noteable round-trip-to-storage the write+flush
    /// approach is win. But for small N and/or near-zero NVMe-like latency
    /// the write-through is better.
    ///
    /// To solve this issue libmdbx provide `MDBX_opt_writethrough_threshold`:
    ///  - when N described above less or equal specified threshold,
    ///    a write-through approach will be used;
    ///  - otherwise, when N great than specified threshold,
    ///    a write-and-flush approach will be used.
    ///
    /// \note MDBX_opt_writethrough_threshold affects only \ref MDBX_SYNC_DURABLE
    /// mode without \ref MDBX_WRITEMAP, and not supported on Windows.
    /// On Windows a write-through is used always but \ref MDBX_NOMETASYNC could
    /// be used for switching to write-and-flush.
    int WRITETHROUGH_THRESHOLD = 13;

    /// \brief Controls prevention of page-faults of reclaimed and allocated pages
    /// in the \ref MDBX_WRITEMAP mode by clearing ones through file handle before
    /// touching.
    int PREFAULT_WRITE_ENABLE = 14;

    /// \brief Controls the in-process spending time limit of searching
    /// consecutive pages inside GC.
    /// \see MDBX_opt_rp_augment_limit
    ///
    /// Specifies a time limit in 1/65536 of a second that may be spent during a write
    // transaction
    // searching for sequences of
    /// pages within the GC/freelist after reaching the limit specified by the option
    /// \ref MDBX_opt_rp_augment_limit. Time control is not performed when
    /// searching/allocating single pages and allocating pages for GC needs (when
    /// updating the GC during a transaction commit).
    ///
    /// The specified time limit is calculated by the "wall clock" and is controlled
    /// within the transaction, inherited for nested transactions and with
    /// accumulation in the parent when they commit. Time control
    /// is performed only when reaching the limit specified by the option \ref
    /// MDBX_opt_rp_augment_limit. This allows flexible control of the behavior
    /// using both options.
    ///
    /// By default, the limit is set to 0, which causes GC searches to stop immediately when the
    // \ref
    /// MDBX_opt_rp_augment_limit is reached in the internal transaction state, and
    /// corresponds to the behavior before the `MDBX_opt_gc_time_limit` option was introduced.
    /// On the other hand, with the minimum value (including 0)
    /// `MDBX_opt_rp_augment_limit`, GC processing will be limited
    /// primarily by the elapsed time.
    int GC_TIME_LIMIT = 15;

    /// Controls the choice between striving for uniformity of
    /// page filling, or reducing the number of modified and written pages.
    ///
    /// \details After deletion operations, pages containing less than the minimum
    /// keys, or emptied to \ref MDBX_opt_merge_threshold_16dot16_percent
    /// are subject to merging with one of the neighboring ones. If the pages to the right and
    // left of
    /// the current one are both "dirty" (were modified during the transaction and must be
    /// written to disk), or both are "clean" (were not modified in the current transaction),
    /// then the less filled page is always chosen as the target for merging.
    ///
    /// When only one of the neighboring ones is "dirty" and the other one is "clean", then two
    /// tactics of choosing a target for merging are possible:
    ///
    /// - If `MDBX_opt_prefer_waf_insteadof_balance = True`, then the already modified page will
    // be
    /// chosen, which will NOT INCREASE the number of modified pages and the amount of disk
    // writes
    /// when committing the current transaction, but on average will INCREASE the unevenness
    /// of page filling.
    ///
    /// - If `MDBX_opt_prefer_waf_insteadof_balance = False`, then the less filled page will be
    /// chosen, which will INCREASE the number of modified pages and the amount of disk writes
    /// when committing the current transaction, but on average will DECREASE the unevenness
    /// of page filling.
    ///
    /// \see MDBX_opt_merge_threshold_16dot16_percent
    int PREFER_WAF_INSTEADOF_BALANCE = 16;

    /// Maximum size of nested pages used to
    /// accommodate a small number of multi-values associated with one key.
    ///
    /// Using nested pages, instead of moving values to separate
    /// pages of the nested tree, allows to reduce the amount of unused space
    /// and thus increase the density of data placement.
    ///
    /// However, as the size of nested pages increases, more leaf
    /// pages of the main tree are required, which also increases the height of the main tree.
    /// In addition, changing data on nested pages requires additional
    /// copies, so the cost can be higher in many scenarios.
    ///
    /// min 12.5% (8192), max 100% (65535), default = 100%
    int SUBPAGE_LIMIT = 17;

    /// the minimum amount of free space on the main page, in the absence
    /// of which the nested pages are moved to a separate tree.
    ///
    /// min 0, max 100% (65535), default = 0
    int SUBPAGE_ROOM_THRESHOLD = 18;

    /// the minimum amount of free space on the main page,
    /// if available, a reservation of space is made in the nested page.
    ///
    /// If there is not enough free space on the main page, the nested
    /// page will be of the minimum size. In turn, if there is no reserve
    /// in the nested page, each addition of elements to it will require
    /// reforming the main page with the transfer of all data nodes.
    ///
    /// Therefore, reserving space is usually beneficial in scenarios with
    /// intensive addition of short multi-values, for example, when
    /// indexing. But it reduces the density of data placement, and accordingly
    /// increases the volume of the database and input-output operations.
    ///
    /// min 0, max 100% (65535), default = 42% (27525)
    int SUBPAGE_RESERVE_PREREQ = 19;

    /// Limitation of space reservation on nested pages.
    ///
    /// min 0, max 100% (65535), default = 4.2% (2753)
    int SUBPAGE_RESERVE_LIMIT = 20;
  }

  public interface DeleteMode {
    /// Just delete the environment's files and directory if any.
    ///
    /// @apiNote On POSIX systems, processes already working with the database will
    /// continue to work without interference until it close the environment.
    ///
    /// @apiNote On Windows, the behavior of `MDBX_ENV_JUST_DELETE` is different
    /// because the system does not support deleting files that are currently
    /// memory mapped.
    int JUST_DELETE = 0;

    /// Make sure that the environment is not being used by other
    /// processes, or return an error otherwise.
    int ENSURE_UNUSED = 1;

    /// Wait until other processes closes the environment before deletion.
    int WAIT_FOR_UNUSED = 2;
  }

  /// \brief Warming up options
  /// \ingroup c_settings
  /// \anchor warmup_flags
  /// \see mdbx_env_warmup()
  public interface WarmupFlags {
    /// By default \ref mdbx_env_warmup() just ask OS kernel to asynchronously
    /// prefetch database pages.
    int DEFAULT = 0;

    /// Peeking all pages of allocated portion of the database
    /// to force ones to be loaded into memory. However, the pages are just peeks
    /// sequentially, so unused pages that are in GC will be loaded in the same
    /// way as those that contain payload.
    int FORCE = 1;

    /// Using system calls to peeks pages instead of directly accessing ones,
    /// which at the cost of additional overhead avoids killing the current
    /// process by OOM-killer in a lack of memory condition.
    /// \note Has effect only on POSIX (non-Windows) systems with conjunction
    /// to \ref MDBX_warmup_force option.
    int OOM_SAFE = 2;

    /// Try to lock database pages in memory by `mlock()` on POSIX-systems
    /// or `VirtualLock()` on Windows. Please refer to description of these
    /// functions for reasonability of such locking and the information of
    /// effects, including the system as a whole.
    ///
    /// Such locking in memory requires that the corresponding resource limits
    /// (e.g. `RLIMIT_RSS`, `RLIMIT_MEMLOCK` or process working set size)
    /// and the availability of system RAM are sufficiently high.
    ///
    /// On successful, all currently allocated pages, both unused in GC and
    /// containing payload, will be locked in memory until the environment closes,
    /// or explicitly unblocked by using \ref MDBX_warmup_release, or the
    /// database geometry will changed, including its auto-shrinking.
    int LOCK = 4;

    /// Alters corresponding current resource limits to be enough for lock pages
    /// by \ref MDBX_warmup_lock. However, this option should be used in simpler
    /// applications since takes into account only current size of this environment
    /// disregarding all other factors. For real-world database application you
    /// will need full-fledged management of resources and their limits with
    /// respective engineering.
    int TOUCH_LIMIT = 8;

    /// Release the lock that was performed before by \ref MDBX_warmup_lock.
    int RELEASE = 16;
  }

  public interface CopyFlags {
    int DEFAULTS = 0;

    /// Copy with compactification: Omit free space from copy and renumber all
    /// pages sequentially
    int COMPACT = 1;

    /// Force to make resizable copy, i.e. dynamic size instead of fixed
    int FORCE_DYNAMIC_SIZE = 2;

    /// Don't explicitly flush the written data to an output media
    int DONT_FLUSH = 4;

    /// Use read transaction parking during copying MVCC-snapshot
    /// \see mdbx_txn_park()
    int THROTTLE_MVCC = 8;

    /// Abort/dispose passed transaction after copy
    /// \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname()
    int DISPOSE_TXN = 16;

    /// Enable renew/restart read transaction in case it use outdated
    /// MVCC shapshot, otherwise the \ref MDBX_MVCC_RETARDED will be returned
    /// \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname()
    int RENEW_TXN = 32;
  }

  /// /** \brief Information about the environment
  ///  * \ingroup c_statinfo
  ///  * \see mdbx_env_info_ex() */
  /// struct MDBX_envinfo {
  ///   struct {
  ///     uint64_t lower;   /**< Lower limit for datafile size */
  ///     uint64_t upper;   /**< Upper limit for datafile size */
  ///     uint64_t current; /**< Current datafile size */
  ///     uint64_t shrink;  /**< Shrink threshold for datafile */
  ///     uint64_t grow;    /**< Growth step for datafile */
  ///   } mi_geo;
  ///   uint64_t mi_mapsize;                  /**< Size of the data memory map */
  ///   uint64_t mi_last_pgno;                /**< Number of the last used page */
  ///   uint64_t mi_recent_txnid;             /**< ID of the last committed transaction */
  ///   uint64_t mi_latter_reader_txnid;      /**< ID of the last reader transaction */
  ///   uint64_t mi_self_latter_reader_txnid; /**< ID of the last reader transaction
  ///                                            of caller process */
  ///   uint64_t mi_meta_txnid[3], mi_meta_sign[3];
  ///   uint32_t mi_maxreaders;   /**< Total reader slots in the environment */
  ///   uint32_t mi_numreaders;   /**< Max reader slots used in the environment */
  ///   uint32_t mi_dxb_pagesize; /**< Database pagesize */
  ///   uint32_t mi_sys_pagesize; /**< System pagesize */
  ///
  ///   /** \brief A mostly unique ID that is regenerated on each boot.
  ///
  ///    As such it can be used to identify the local machine's current boot. MDBX
  ///    uses such when open the database to determine whether rollback required to
  ///    the last steady sync point or not. I.e. if current bootid is differ from the
  ///    value within a database then the system was rebooted and all changes since
  ///    last steady sync must be reverted for data integrity. Zeros mean that no
  ///    relevant information is available from the system. */
  ///   struct {
  ///     struct {
  ///       uint64_t x, y;
  ///     } current, meta[3];
  ///   } mi_bootid;
  ///
  ///   /** Bytes not explicitly synchronized to disk */
  ///   uint64_t mi_unsync_volume;
  ///   /** Current auto-sync threshold, see \ref mdbx_env_set_syncbytes(). */
  ///   uint64_t mi_autosync_threshold;
  ///   /** Time since entering to a "dirty" out-of-sync state in units of 1/65536 of
  ///    * second. In other words, this is the time since the last non-steady commit
  ///    * or zero if it was steady. */
  ///   uint32_t mi_since_sync_seconds16dot16;
  ///   /** Current auto-sync period in 1/65536 of second,
  ///    * see \ref mdbx_env_set_syncperiod(). */
  ///   uint32_t mi_autosync_period_seconds16dot16;
  ///   /** Time since the last readers check in 1/65536 of second,
  ///    * see \ref mdbx_reader_check(). */
  ///   uint32_t mi_since_reader_check_seconds16dot16;
  ///   /** Current environment mode.
  ///    * The same as \ref mdbx_env_get_flags() returns. */
  ///   uint32_t mi_mode;
  ///
  ///   /** Statistics of page operations.
  ///    * \details Overall statistics of page operations of all (running, completed
  ///    * and aborted) transactions in the current multi-process session (since the
  ///    * first process opened the database after everyone had previously closed it).
  ///    */
  ///   struct {
  ///     uint64_t newly;    /**< Quantity of a new pages added */
  ///     uint64_t cow;      /**< Quantity of pages copied for update */
  ///     uint64_t clone;    /**< Quantity of parent's dirty pages clones
  ///                             for nested transactions */
  ///     uint64_t split;    /**< Page splits */
  ///     uint64_t merge;    /**< Page merges */
  ///     uint64_t spill;    /**< Quantity of spilled dirty pages */
  ///     uint64_t unspill;  /**< Quantity of unspilled/reloaded pages */
  ///     uint64_t wops;     /**< Number of explicit write operations (not a pages)
  ///                             to a disk */
  ///     uint64_t prefault; /**< Number of prefault write operations (not a pages) */
  ///     uint64_t mincore;  /**< Number of mincore() calls */
  ///     uint64_t msync;    /**< Number of explicit msync-to-disk operations (not a pages) */
  ///     uint64_t fsync;    /**< Number of explicit fsync-to-disk operations (not a pages) */
  ///   } mi_pgop_stat;
  ///
  ///   /* GUID of the database DXB file. */
  ///   struct {
  ///     uint64_t x, y;
  ///   } mi_dxbid;
  /// };

  public static final class Info {
    public static final long SIZE = 352L;

    public final Geo geo = new Geo();
    /// \brief A mostly unique ID that is regenerated on each boot.
    /// As such it can be used to identify the local machine's current boot. MDBX
    /// uses such when open the database to determine whether rollback required to
    /// the last steady sync point or not. I.e. if current bootid is differ from the
    /// value within a database then the system was rebooted and all changes since
    /// last steady sync must be reverted for data integrity. Zeros mean that no
    /// relevant information is available from the system.
    public final BootID bootId = new BootID();
    /// Statistics of page operations.
    /// \details Overall statistics of page operations of all (running, completed
    /// and aborted) transactions in the current multi-process session (since the
    /// first process opened the database after everyone had previously closed it).
    public final PageOpStat pageOpStat = new PageOpStat();
    /// GUID of the database DXB file.
    public final XY dxbId = new XY();
    /// Size of the data memory map
    public long mapSize;
    /// Number of the last used page
    public long lastPageNo;
    /// ID of the last committed transaction
    public long recentTxnId;
    /// ID of the last reader transaction
    public long latterReaderTxnId;
    /// ID of the last reader transaction
    public long selfLatterReaderTxnId;
    public long metaTxnID0;
    public long metaTxnID1;
    public long metaTxnID2;
    public long metaSign0;
    public long metaSign1;
    public long metaSign2;
    /// Total reader slots in the environment
    public int maxReaders;
    /// Max reader slots used in the environment
    public int numReaders;
    /// Database pagesize
    public int dxbPageSize;
    /// System pagesize
    public int sysPageSize;
    /// Bytes not explicitly synchronized to disk
    public long unsyncVolume;
    /// Current auto-sync threshold, see \ref mdbx_env_set_syncbytes().
    public long autoSyncThreshold;
    /// Time since entering to a "dirty" out-of-sync state in units of 1/65536 of
    /// second. In other words, this is the time since the last non-steady commit
    /// or zero if it was steady.
    public int sinceSyncSeconds16dot16;
    /// Current auto-sync period in 1/65536 of second,
    /// see \ref mdbx_env_set_syncperiod().
    public int autoSyncPeriodSeconds16dot16;
    /// Time since the last readers check in 1/65536 of second,
    /// see \ref mdbx_reader_check().
    public int sinceReaderCheckSeconds16dot16;
    /// Current environment mode.
    /// The same as \ref mdbx_env_get_flags() returns.
    public int mode;

    void update(long ptr) {
      geo.lower = Danger.UNSAFE.getLong(ptr);
      geo.upper = Danger.UNSAFE.getLong(ptr + 8L);
      geo.current = Danger.UNSAFE.getLong(ptr + 16L);
      geo.shrink = Danger.UNSAFE.getLong(ptr + 24L);
      geo.grow = Danger.UNSAFE.getLong(ptr + 32L);
      mapSize = Danger.UNSAFE.getLong(ptr + 40L);
      lastPageNo = Danger.UNSAFE.getLong(ptr + 48L);
      recentTxnId = Danger.UNSAFE.getLong(ptr + 56L);
      latterReaderTxnId = Danger.UNSAFE.getLong(ptr + 64L);
      selfLatterReaderTxnId = Danger.UNSAFE.getLong(ptr + 72L);
      metaTxnID0 = Danger.UNSAFE.getLong(ptr + 80L);
      metaTxnID1 = Danger.UNSAFE.getLong(ptr + 88L);
      metaTxnID2 = Danger.UNSAFE.getLong(ptr + 96L);
      metaSign0 = Danger.UNSAFE.getLong(ptr + 104L);
      metaSign1 = Danger.UNSAFE.getLong(ptr + 112L);
      metaSign2 = Danger.UNSAFE.getLong(ptr + 120L);
      maxReaders = Danger.UNSAFE.getInt(ptr + 128L);
      numReaders = Danger.UNSAFE.getInt(ptr + 132L);
      dxbPageSize = Danger.UNSAFE.getInt(ptr + 136L);
      sysPageSize = Danger.UNSAFE.getInt(ptr + 140L);
      bootId.current.x = Danger.UNSAFE.getLong(ptr + 144L);
      bootId.current.y = Danger.UNSAFE.getLong(ptr + 152L);
      bootId.meta0.x = Danger.UNSAFE.getLong(ptr + 160L);
      bootId.meta0.y = Danger.UNSAFE.getLong(ptr + 168L);
      bootId.meta1.x = Danger.UNSAFE.getLong(ptr + 176L);
      bootId.meta1.y = Danger.UNSAFE.getLong(ptr + 184L);
      bootId.meta2.x = Danger.UNSAFE.getLong(ptr + 192L);
      bootId.meta2.y = Danger.UNSAFE.getLong(ptr + 200L);
      unsyncVolume = Danger.UNSAFE.getLong(ptr + 208L);
      autoSyncThreshold = Danger.UNSAFE.getLong(ptr + 216L);
      sinceSyncSeconds16dot16 = Danger.UNSAFE.getInt(ptr + 224L);
      autoSyncPeriodSeconds16dot16 = Danger.UNSAFE.getInt(ptr + 228L);
      sinceReaderCheckSeconds16dot16 = Danger.UNSAFE.getInt(ptr + 232L);
      mode = Danger.UNSAFE.getInt(ptr + 236L);
      pageOpStat.newly = Danger.UNSAFE.getLong(ptr + 240L);
      pageOpStat.cow = Danger.UNSAFE.getLong(ptr + 248L);
      pageOpStat.cloned = Danger.UNSAFE.getLong(ptr + 256L);
      pageOpStat.split = Danger.UNSAFE.getLong(ptr + 264L);
      pageOpStat.merge = Danger.UNSAFE.getLong(ptr + 272L);
      pageOpStat.spill = Danger.UNSAFE.getLong(ptr + 280L);
      pageOpStat.unspill = Danger.UNSAFE.getLong(ptr + 288L);
      pageOpStat.wops = Danger.UNSAFE.getLong(ptr + 296L);
      pageOpStat.prefault = Danger.UNSAFE.getLong(ptr + 304L);
      pageOpStat.mincore = Danger.UNSAFE.getLong(ptr + 312L);
      pageOpStat.msync = Danger.UNSAFE.getLong(ptr + 320L);
      pageOpStat.fsync = Danger.UNSAFE.getLong(ptr + 328L);
      dxbId.x = Danger.UNSAFE.getLong(ptr + 336L);
      dxbId.y = Danger.UNSAFE.getLong(ptr + 344L);
    }

    @Override
    public String toString() {
      return "Record{"
          + "geo="
          + geo
          + ", mapSize="
          + mapSize
          + ", lastPageNo="
          + lastPageNo
          + ", recentTxnId="
          + recentTxnId
          + ", latterReaderTxnId="
          + latterReaderTxnId
          + ", selfLatterReaderTxnId="
          + selfLatterReaderTxnId
          + ", metaTxnID0="
          + metaTxnID0
          + ", metaTxnID1="
          + metaTxnID1
          + ", metaTxnID2="
          + metaTxnID2
          + ", metaSign0="
          + metaSign0
          + ", metaSign1="
          + metaSign1
          + ", metaSign2="
          + metaSign2
          + ", maxReaders="
          + maxReaders
          + ", numReaders="
          + numReaders
          + ", dxbPageSize="
          + dxbPageSize
          + ", sysPageSize="
          + sysPageSize
          + ", bootId="
          + bootId
          + ", unsyncVolume="
          + unsyncVolume
          + ", autoSyncThreshold="
          + autoSyncThreshold
          + ", sinceSyncSeconds16dot16="
          + sinceSyncSeconds16dot16
          + ", autoSyncPeriodSeconds16dot16="
          + autoSyncPeriodSeconds16dot16
          + ", sinceReaderCheckSeconds16dot16="
          + sinceReaderCheckSeconds16dot16
          + ", mode="
          + mode
          + ", pageOpStat="
          + pageOpStat
          + ", dxbId="
          + dxbId
          + '}';
    }

    public static final class Geo {
      /// Lower limit for datafile size
      public long lower;
      /// Upper limit for datafile size
      public long upper;
      /// Current datafile size
      public long current;
      /// Shrink threshold for datafile
      public long shrink;
      /// Growth step for datafile
      public long grow;

      @Override
      public String toString() {
        return "Geo["
            + "lower="
            + lower
            + ", "
            + "upper="
            + upper
            + ", "
            + "current="
            + current
            + ", "
            + "shrink="
            + shrink
            + ", "
            + "grow="
            + grow
            + ']';
      }
    }

    /// Statistics of page operations.
    /// \details Overall statistics of page operations of all (running, completed
    /// and aborted) transactions in the current multi-process session (since the
    /// first process opened the database after everyone had previously closed it).
    public static final class PageOpStat {
      /// Quantity of a new pages added
      public long newly;
      /// Quantity of pages copied for update
      public long cow;
      /// Quantity of parent's dirty pages clones for nested transactions
      public long cloned;
      /// Page splits
      public long split;
      /// Page merges
      public long merge;
      /// Quantity of spilled dirty pages
      public long spill;
      /// Quantity of unspilled/reloaded pages
      public long unspill;
      /// Number of explicit write operations (not a pages) to a disk
      public long wops;
      /// Number of prefault write operations (not a pages)
      public long prefault;
      /// Number of mincore() calls
      public long mincore;
      /// Number of explicit msync-to-disk operations (not a pages)
      public long msync;
      /// Number of explicit fsync-to-disk operations (not a pages)
      public long fsync;

      @Override
      public String toString() {
        return "PageOpStat["
            + "newly="
            + newly
            + ", "
            + "cow="
            + cow
            + ", "
            + "cloned="
            + cloned
            + ", "
            + "split="
            + split
            + ", "
            + "merge="
            + merge
            + ", "
            + "spill="
            + spill
            + ", "
            + "unspill="
            + unspill
            + ", "
            + "wops="
            + wops
            + ", "
            + "prefault="
            + prefault
            + ", "
            + "mincore="
            + mincore
            + ", "
            + "msync="
            + msync
            + ", "
            + "fsync="
            + fsync
            + ']';
      }
    }

    public static final class XY {
      public long x;
      public long y;

      public XY() {}

      public XY(long x, long y) {
        this.x = x;
        this.y = y;
      }

      @Override
      public String toString() {
        return "XY[" + "x=" + x + ", " + "y=" + y + ']';
      }
    }

    public static final class BootID {
      public final XY current = new XY();
      public final XY meta0 = new XY();
      public final XY meta1 = new XY();
      public final XY meta2 = new XY();

      @Override
      public String toString() {
        return "BootID["
            + "current="
            + current
            + ", "
            + "meta0="
            + meta0
            + ", "
            + "meta1="
            + meta1
            + ", "
            + "meta2="
            + meta2
            + ']';
      }
    }
  }
}
