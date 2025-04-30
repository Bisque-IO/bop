package bop.c.mdbx;

import bop.c.Memory;
import bop.unsafe.Danger;

public class Txn {
  Env env;
  long txnPointer;
  int flags;
  long scratchPointer;
  long scratchPointerLength;
  Info info;
  Stat stat;
  CommitLatency commitLatency;
  final Val keyValue = Val.allocate();
  final Val dataValue = Val.allocate();

  Txn() {}

  public void free() {
    final var scratchPointer = this.scratchPointer;
    if (scratchPointer != 0L) {
      Memory.dealloc(scratchPointer);
      this.scratchPointer = 0L;
    }
    keyValue.close();
    dataValue.close();
    env = null;
  }

  void init(Env env, long txnPointer, int flags) {
    this.env = env;
    this.txnPointer = txnPointer;
    this.flags = flags;
  }

  public CommitLatency commitLatency() {
    if (commitLatency == null) {
      commitLatency = new CommitLatency();
    }
    return commitLatency;
  }

  /// Return information about the MDBX transaction.
  ///
  /// @param scanReadLock    The boolean flag controls the scan of the read lock
  ///                        table to provide complete information. Such scan
  ///                        is relatively expensive and you can avoid it
  ///                        if corresponding fields are not needed.
  ///                        See description of \ref MDBX_txn_info.
  public Info info(boolean scanReadLock) {
    if (info == null) {
      info = new Info();
    }
    if (scratchPointer == 0L) {
      scratchPointer = Memory.allocZeroed(256L);
      scratchPointerLength = 256L;
    }
    int err;
    try {
      err = (int) CFunctions.MDBX_TXN_INFO.invokeExact(txnPointer, scratchPointer, scanReadLock);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    info.update(scratchPointer);
    return info;
  }

  /// Returns the transaction's MDBX_env.
  public long getEnv() {
    try {
      return (long) CFunctions.MDBX_TXN_ENV.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Return the transaction's flags.
  ///
  /// This returns the flags, including internal, associated with this transaction.
  ///
  /// \returns A transaction flags, valid if input is an valid transaction,
  ///          otherwise \ref MDBX_TXN_INVALID.
  public int getFlags() {
    try {
      return (int) CFunctions.MDBX_TXN_FLAGS.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Return the transaction's ID.
  ///
  /// This returns the identifier associated with this transaction. For a
  /// read-only transaction, this corresponds to the snapshot being read;
  /// concurrent readers will frequently have the same transaction ID.
  ///
  /// \returns A transaction ID, valid if input is an active transaction,
  ///          otherwise 0.
  public long getId() {
    try {
      return (long) CFunctions.MDBX_TXN_ID.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Commit all the operations of a transaction into the database.
  ///
  /// If the current thread is not eligible to manage the transaction then
  /// the \ref MDBX_THREAD_MISMATCH error will be returned. Otherwise, the transaction
  /// will be committed and its handle is freed. If the transaction cannot
  /// be committed, it will be aborted with the corresponding error returned.
  /// Thus, a result other than \ref MDBX_THREAD_MISMATCH means that the
  /// transaction is terminated:
  ///
  ///  - Resources are released;
  ///  - Transaction handle is invalid;
  ///  - Cursor(s) associated with transaction must not be used, except with
  ///    mdbx_cursor_renew() and \ref mdbx_cursor_close().
  ///    Such cursor(s) must be closed explicitly by \ref mdbx_cursor_close()
  ///    before or after transaction commit, either can be reused with
  ///    \ref mdbx_cursor_renew() until it will be explicitly closed by
  ///    \ref mdbx_cursor_close().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_RESULT_TRUE      Transaction was aborted since it should
  ///                               be aborted due to previous errors,
  ///                               either no changes were made during the transaction,
  ///                               and the build time option
  ///                               \ref MDBX_NOSUCCESS_PURE_COMMIT was enabled.
  /// \retval MDBX_PANIC            A fatal error occurred earlier
  ///                               and the environment must be shut down.
  /// \retval MDBX_BAD_TXN          Transaction is already finished or never began.
  /// \retval MDBX_EBADSIGN         Transaction object has invalid signature,
  ///                               e.g. transaction was already terminated
  ///                               or memory was corrupted.
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL           Transaction handle is NULL.
  /// \retval MDBX_ENOSPC           No more disk space.
  /// \retval MDBX_EIO              An error occurred during the flushing/writing
  ///                               data to a storage medium/disk.
  /// \retval MDBX_ENOMEM           Out of memory.
  public int commit() {
    return commit(false);
  }

  /// Commit all the operations of a transaction into the database.
  ///
  /// If the current thread is not eligible to manage the transaction then
  /// the \ref MDBX_THREAD_MISMATCH error will be returned. Otherwise, the transaction
  /// will be committed and its handle is freed. If the transaction cannot
  /// be committed, it will be aborted with the corresponding error returned.
  /// Thus, a result other than \ref MDBX_THREAD_MISMATCH means that the
  /// transaction is terminated:
  ///
  ///  - Resources are released;
  ///  - Transaction handle is invalid;
  ///  - Cursor(s) associated with transaction must not be used, except with
  ///    mdbx_cursor_renew() and \ref mdbx_cursor_close().
  ///    Such cursor(s) must be closed explicitly by \ref mdbx_cursor_close()
  ///    before or after transaction commit, either can be reused with
  ///    \ref mdbx_cursor_renew() until it will be explicitly closed by
  ///    \ref mdbx_cursor_close().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_RESULT_TRUE      Transaction was aborted since it should
  ///                               be aborted due to previous errors,
  ///                               either no changes were made during the transaction,
  ///                               and the build time option
  ///                               \ref MDBX_NOSUCCESS_PURE_COMMIT was enabled.
  /// \retval MDBX_PANIC            A fatal error occurred earlier
  ///                               and the environment must be shut down.
  /// \retval MDBX_BAD_TXN          Transaction is already finished or never began.
  /// \retval MDBX_EBADSIGN         Transaction object has invalid signature,
  ///                               e.g. transaction was already terminated
  ///                               or memory was corrupted.
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL           Transaction handle is NULL.
  /// \retval MDBX_ENOSPC           No more disk space.
  /// \retval MDBX_EIO              An error occurred during the flushing/writing
  ///                               data to a storage medium/disk.
  /// \retval MDBX_ENOMEM           Out of memory.
  public int commit(boolean withLatency) {
    final long latencyPtr;
    if (withLatency) {
      final int err;
      try {
        err = (int) CFunctions.MDBX_TXN_COMMIT_EX.invokeExact(txnPointer, scratchPointer);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
      if (err != Error.SUCCESS) {
        throw new MDBXError(err);
      }
      if (commitLatency == null) {
        commitLatency = new CommitLatency();
      }
      commitLatency.update(scratchPointer);
      return err;
    } else {
      try {
        return (int) CFunctions.MDBX_TXN_COMMIT_EX.invokeExact(txnPointer, 0L);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }
  }

  /// Abandon all the operations of the transaction instead of saving them.
  ///
  /// The transaction handle is freed. It and its cursors must not be used again
  /// after this call, except with \ref mdbx_cursor_renew() and
  /// \ref mdbx_cursor_close().
  ///
  /// If the current thread is not eligible to manage the transaction then
  /// the \ref MDBX_THREAD_MISMATCH error will returned. Otherwise the transaction
  /// will be aborted and its handle is freed. Thus, a result other than
  /// \ref MDBX_THREAD_MISMATCH means that the transaction is terminated:
  ///  - Resources are released;
  ///  - Transaction handle is invalid;
  ///  - Cursor(s) associated with transaction must not be used, except with
  ///    \ref mdbx_cursor_renew() and \ref mdbx_cursor_close().
  ///    Such cursor(s) must be closed explicitly by \ref mdbx_cursor_close()
  ///    before or after transaction abort, either can be reused with
  ///    \ref mdbx_cursor_renew() until it will be explicitly closed by
  ///    \ref mdbx_cursor_close().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_PANIC            A fatal error occurred earlier and
  ///                               the environment must be shut down.
  /// \retval MDBX_BAD_TXN          Transaction is already finished or never began.
  /// \retval MDBX_EBADSIGN         Transaction object has invalid signature,
  ///                               e.g. transaction was already terminated
  ///                               or memory was corrupted.
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL           Transaction handle is NULL.
  public int abort() {
    try {
      return (int) CFunctions.MDBX_TXN_ABORT.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Marks transaction as broken to prevent further operations.
  ///
  /// Function keeps the transaction handle and corresponding locks, but makes
  /// impossible to perform any operations within a broken transaction.
  ///
  /// Broken transaction must then be aborted explicitly later.
  ///
  /// \see mdbx_txn_abort() \see mdbx_txn_reset() \see mdbx_txn_commit()
  /// \returns A non-zero error value on failure and 0 on success.
  public int breakIt() {
    try {
      return (int) CFunctions.MDBX_TXN_BREAK.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Reset a read-only transaction.
  ///
  /// Abort the read-only transaction like \ref mdbx_txn_abort(), but keep the
  /// transaction handle. Therefore \ref mdbx_txn_renew() may reuse the handle.
  /// This saves allocation overhead if the process will start a new read-only
  /// transaction soon, and also locking overhead if \ref MDBX_NOSTICKYTHREADS is
  /// in use. The reader table lock is released, but the table slot stays tied to
  /// its thread or \ref MDBX_txn. Use \ref mdbx_txn_abort() to discard a reset
  /// handle, and to free its lock table slot if \ref MDBX_NOSTICKYTHREADS
  /// is in use.
  ///
  /// Cursors opened within the transaction must not be used again after this
  /// call, except with \ref mdbx_cursor_renew() and \ref mdbx_cursor_close().
  ///
  /// Reader locks generally don't interfere with writers, but they keep old
  /// versions of database pages allocated. Thus they prevent the old pages from
  /// being reused when writers commit new data, and so under heavy load the
  /// database size may grow much more rapidly than otherwise.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_PANIC            A fatal error occurred earlier and
  ///                               the environment must be shut down.
  /// \retval MDBX_BAD_TXN          Transaction is already finished or never began.
  /// \retval MDBX_EBADSIGN         Transaction object has invalid signature,
  ///                               e.g. transaction was already terminated
  ///                               or memory was corrupted.
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL           Transaction handle is NULL.
  public int reset() {
    try {
      return (int) CFunctions.MDBX_TXN_RESET.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Puts the reading transaction in the "parked" state.
  ///
  /// Running reading transactions do not allow recycling of old MVCC data snapshots, starting
  // with
  // the oldest used/read version and all subsequent ones. A parked transaction can be evicted by a
  // write transaction if it interferes with garbage recycling (old MVCC data snapshots).
  /// And if evicting does not occur, then recovery (transition to the working state and
  // continuation of execution) of the reading transaction will be significantly
  /// cheaper. Thus, parking transactions allows you to prevent the negative consequences associated
  // with stopping garbage recycling,
  /// while keeping overhead costs at a minimum.
  ///
  /// To continue execution (reading and/or using data), a parked
  /// transaction must be restored using \ref mdbx_txn_unpark().
  /// For ease of use and to prevent unnecessary API calls, the option of automatic
  /// "unparking" is provided via the
  /// parameter `autounpark` when using a parked transaction in API functions
  /// that involve reading data.
  ///
  /// Before restoring/unparking a transaction, regardless of the
  /// argument `autounpark`, dereferencing of pointers obtained
  /// previously when reading data within a parked transaction is not allowed, since the
  /// MVCC snapshot in which this data is placed is not retained and can
  /// be recycled at any time.
  ///
  /// A parked transaction without "unparking" can be aborted, reset
  /// or restarted at any time via \ref mdbx_txn_abort(),
  /// mdbx_txn_reset() and \ref mdbx_txn_renew(), respectively.
  ///
  /// \see mdbx_txn_unpark()
  /// \see mdbx_txn_flags()
  /// \see mdbx_env_set_hsr()
  /// \see <a href="intro.html#long-lived-read">Long-lived read transactions</a>
  ///
  /// \ref mdbx_txn_begin().
  ///
  /// @param autoUnpark Allows you to enable automatic
  ///
  /// unparking/recovery of a transaction when calling
  ///
  /// API functions that involve reading data.
  ///
  /// \returns Non-zero error code, or 0 on success.
  public int park(boolean autoUnpark) {
    try {
      return (int) CFunctions.MDBX_TXN_PARK.invokeExact(txnPointer, autoUnpark);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Unparks a previously parked read transaction.
  ///
  /// The function attempts to recover a previously parked transaction. If
  /// the parked transaction was ousted to recycle old MVCC snapshots, then depending
  /// on the `restart_if_ousted` argument, it is restarted similarly
  /// to \ref mdbx_txn_renew(), or the transaction is reset and the error code
  /// \ref MDBX_OUSTED is returned.
  ///
  /// \see mdbx_txn_park()
  /// \see mdbx_txn_flags()
  /// \see <a href="intro.html#long-lived-read">Long-lived read transactions</a>
  ///
  /// \ref mdbx_txn_begin() and then parked
  /// by \ref mdbx_txn_park.
  ///
  /// @param restartIfOusted Allows immediate restart of the transaction if it was ousted.
  ///
  /// \returns A non-zero error code, or 0 on success.
  /// Some specific result codes:
  ///
  /// \retval MDBX_SUCCESS A parked transaction was successfully recovered,
  /// or was not parked.
  ///
  /// \retval MDBX_OUSTED A reader transaction was ousted by a writer
  ///
  /// to recycle old MVCC snapshots,
  /// and the `restart_if_ousted` argument was `false`.
  /// The transaction is reset to a state similar to
  /// a call to \ref mdbx_txn_reset(), but the instance
  /// (handle) is not deallocated and can be reused
  /// via \ref mdbx_txn_renew(), or deallocated
  /// via \ref mdbx_txn_abort().
  ///
  /// \retval MDBX_RESULT_TRUE The reading transaction was ousted, but is now
  /// restarted to read another (latest) MVCC snapshot because restart_if_ousted` was set to
  /// `true`.
  ///
  /// \retval MDBX_BAD_TXN The transaction has already committed or was not started.
  public int unpark(boolean restartIfOusted) {
    try {
      return (int) CFunctions.MDBX_TXN_UNPARK.invokeExact(txnPointer, restartIfOusted);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Renew a read-only transaction.
  ///
  /// This acquires a new reader lock for a transaction handle that had been
  /// released by \ref mdbx_txn_reset(). It must be called before a reset
  /// transaction may be used again.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_PANIC            A fatal error occurred earlier and
  ///                               the environment must be shut down.
  /// \retval MDBX_BAD_TXN          Transaction is already finished or never began.
  /// \retval MDBX_EBADSIGN         Transaction object has invalid signature,
  ///                               e.g. transaction was already terminated
  ///                               or memory was corrupted.
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL           Transaction handle is NULL.
  public int renew() {
    try {
      return (int) CFunctions.MDBX_TXN_RENEW.invokeExact(txnPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
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
  ///               the statistics will be copied
  /// \param bytes  The size of \ref MDBX_stat.
  /// \returns A non-zero error value on failure and 0 on success.
  public Stat stat() {
    if (stat == null) {
      stat = new Stat();
    }
    if (scratchPointer == 0L) {
      scratchPointer = Memory.allocZeroed(256L);
      scratchPointerLength = 256L;
    }
    int err;
    try {
      err = (int) CFunctions.MDBX_ENV_STAT_EX.invokeExact(
          env.envPointer, txnPointer, scratchPointer, Stat.SIZE);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    stat.update(scratchPointer);
    return stat;
  }

  /// Open or Create a named table in the environment.
  ///
  /// A table handle denotes the name and parameters of a table,
  /// independently of whether such a table exists. The table handle may be
  /// discarded by calling \ref mdbx_dbi_close(). The old table handle is
  /// returned if the table was already open. The handle may only be closed
  /// once.
  ///
  /// \note A notable difference between MDBX and LMDB is that MDBX make handles
  /// opened for existing tables immediately available for other transactions,
  /// regardless this transaction will be aborted or reset. The REASON for this is
  /// avoiding the requirement for multiple opening a same handles in
  /// concurrent read transactions, and tracking of such open but hidden handles
  /// until the completion of read transactions which opened them.
  ///
  /// Nevertheless, the handle for the NEWLY CREATED table will be invisible
  /// for other transactions until the write transaction is successfully
  /// committed. If the write transaction is aborted the handle will be closed
  /// automatically. After a successful commit the such handle will reside in the
  /// shared environment, and may be used by other transactions.
  ///
  /// In contrast to LMDB, the MDBX allow this function to be called from multiple
  /// concurrent transactions or threads in the same process.
  ///
  /// To use named table (with name != NULL), \ref mdbx_env_set_maxdbs()
  /// must be called before opening the environment. Table names are
  /// keys in the internal unnamed table, and may be read but not written.
  ///
  /// \param txn    transaction handle returned by \ref mdbx_txn_begin().
  /// \param name   The name of the table to open. If only a single
  ///               table is needed in the environment,
  ///               this value may be NULL.
  /// \param flags  Special options for this table. This parameter must
  ///               be bitwise OR'ing together any of the constants
  ///               described here:
  ///
  ///  - \ref MDBX_DB_DEFAULTS
  ///      Keys are arbitrary byte strings and compared from beginning to end.
  ///  - \ref MDBX_REVERSEKEY
  ///      Keys are arbitrary byte strings to be compared in reverse order,
  ///      from the end of the strings to the beginning.
  ///  - \ref MDBX_INTEGERKEY
  ///      Keys are binary integers in native byte order, either uint32_t or
  ///      uint64_t, and will be sorted as such. The keys must all be of the
  ///      same size and must be aligned while passing as arguments.
  ///  - \ref MDBX_DUPSORT
  ///      Duplicate keys may be used in the table. Or, from another point of
  ///      view, keys may have multiple data items, stored in sorted order. By
  ///      default keys must be unique and may have only a single data item.
  ///  - \ref MDBX_DUPFIXED
  ///      This flag may only be used in combination with \ref MDBX_DUPSORT. This
  ///      option tells the library that the data items for this table are
  ///      all the same size, which allows further optimizations in storage and
  ///      retrieval. When all data items are the same size, the
  ///      \ref MDBX_GET_MULTIPLE, \ref MDBX_NEXT_MULTIPLE and
  ///      \ref MDBX_PREV_MULTIPLE cursor operations may be used to retrieve
  ///      multiple items at once.
  ///  - \ref MDBX_INTEGERDUP
  ///      This option specifies that duplicate data items are binary integers,
  ///      similar to \ref MDBX_INTEGERKEY keys. The data values must all be of the
  ///      same size and must be aligned while passing as arguments.
  ///  - \ref MDBX_REVERSEDUP
  ///      This option specifies that duplicate data items should be compared as
  ///      strings in reverse order (the comparison is performed in the direction
  ///      from the last byte to the first).
  ///  - \ref MDBX_CREATE
  ///      Create the named table if it doesn't exist. This option is not
  ///      allowed in a read-only transaction or a read-only environment.
  ///
  /// \param dbi     Address where the new \ref MDBX_dbi handle
  ///                will be stored.
  /// For \ref mdbx_dbi_open_ex() additional arguments allow you to set custom
  /// comparison functions for keys and values (for multimaps).
  /// \see avoid_custom_comparators
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_NOTFOUND   The specified table doesn't exist in the
  ///                         environment and \ref MDBX_CREATE was not specified.
  /// \retval MDBX_DBS_FULL   Too many tables have been opened.
  ///                         \see mdbx_env_set_maxdbs()
  /// \retval MDBX_INCOMPATIBLE  Table is incompatible with given flags,
  ///                         i.e. the passed flags is different with which the
  ///                         table was created, or the table was already
  ///                         opened with a different comparison function(s).
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  public int open(String name, int flags) {
    final var dbi = new Dbi();
    dbi.init(name);
    dbi.flags = flags;
    return open(dbi);
  }

  /// Open or Create a named table in the environment.
  ///
  /// A table handle denotes the name and parameters of a table,
  /// independently of whether such a table exists. The table handle may be
  /// discarded by calling \ref mdbx_dbi_close(). The old table handle is
  /// returned if the table was already open. The handle may only be closed
  /// once.
  ///
  /// \note A notable difference between MDBX and LMDB is that MDBX make handles
  /// opened for existing tables immediately available for other transactions,
  /// regardless this transaction will be aborted or reset. The REASON for this is
  /// avoiding the requirement for multiple opening a same handles in
  /// concurrent read transactions, and tracking of such open but hidden handles
  /// until the completion of read transactions which opened them.
  ///
  /// Nevertheless, the handle for the NEWLY CREATED table will be invisible
  /// for other transactions until the write transaction is successfully
  /// committed. If the write transaction is aborted the handle will be closed
  /// automatically. After a successful commit the such handle will reside in the
  /// shared environment, and may be used by other transactions.
  ///
  /// In contrast to LMDB, the MDBX allow this function to be called from multiple
  /// concurrent transactions or threads in the same process.
  ///
  /// To use named table (with name != NULL), \ref mdbx_env_set_maxdbs()
  /// must be called before opening the environment. Table names are
  /// keys in the internal unnamed table, and may be read but not written.
  ///
  /// \param txn    transaction handle returned by \ref mdbx_txn_begin().
  /// \param name   The name of the table to open. If only a single
  ///               table is needed in the environment,
  ///               this value may be NULL.
  /// \param flags  Special options for this table. This parameter must
  ///               be bitwise OR'ing together any of the constants
  ///               described here:
  ///
  ///  - \ref MDBX_DB_DEFAULTS
  ///      Keys are arbitrary byte strings and compared from beginning to end.
  ///  - \ref MDBX_REVERSEKEY
  ///      Keys are arbitrary byte strings to be compared in reverse order,
  ///      from the end of the strings to the beginning.
  ///  - \ref MDBX_INTEGERKEY
  ///      Keys are binary integers in native byte order, either uint32_t or
  ///      uint64_t, and will be sorted as such. The keys must all be of the
  ///      same size and must be aligned while passing as arguments.
  ///  - \ref MDBX_DUPSORT
  ///      Duplicate keys may be used in the table. Or, from another point of
  ///      view, keys may have multiple data items, stored in sorted order. By
  ///      default keys must be unique and may have only a single data item.
  ///  - \ref MDBX_DUPFIXED
  ///      This flag may only be used in combination with \ref MDBX_DUPSORT. This
  ///      option tells the library that the data items for this table are
  ///      all the same size, which allows further optimizations in storage and
  ///      retrieval. When all data items are the same size, the
  ///      \ref MDBX_GET_MULTIPLE, \ref MDBX_NEXT_MULTIPLE and
  ///      \ref MDBX_PREV_MULTIPLE cursor operations may be used to retrieve
  ///      multiple items at once.
  ///  - \ref MDBX_INTEGERDUP
  ///      This option specifies that duplicate data items are binary integers,
  ///      similar to \ref MDBX_INTEGERKEY keys. The data values must all be of the
  ///      same size and must be aligned while passing as arguments.
  ///  - \ref MDBX_REVERSEDUP
  ///      This option specifies that duplicate data items should be compared as
  ///      strings in reverse order (the comparison is performed in the direction
  ///      from the last byte to the first).
  ///  - \ref MDBX_CREATE
  ///      Create the named table if it doesn't exist. This option is not
  ///      allowed in a read-only transaction or a read-only environment.
  ///
  /// \param dbi     Address where the new \ref MDBX_dbi handle
  ///                will be stored.
  /// For \ref mdbx_dbi_open_ex() additional arguments allow you to set custom
  /// comparison functions for keys and values (for multimaps).
  /// \see avoid_custom_comparators
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_NOTFOUND   The specified table doesn't exist in the
  ///                         environment and \ref MDBX_CREATE was not specified.
  /// \retval MDBX_DBS_FULL   Too many tables have been opened.
  ///                         \see mdbx_env_set_maxdbs()
  /// \retval MDBX_INCOMPATIBLE  Table is incompatible with given flags,
  ///                         i.e. the passed flags is different with which the
  ///                         table was created, or the table was already
  ///                         opened with a different comparison function(s).
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  public int open(Dbi dbi) {
    if (scratchPointer == 0L) {
      scratchPointer = Memory.allocZeroed(256L);
      scratchPointerLength = 256L;
    }
    final int err;
    try {
      err = (int)
          CFunctions.MDBX_DBI_OPEN.invokeExact(txnPointer, dbi.namePtr, dbi.flags, scratchPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    dbi.dbi = Danger.UNSAFE.getInt(scratchPointer);
    return err;
  }

  /// Renames a table by DBI handle
  ///
  /// Renames a user-named table associated with the passed
  /// DBI handle.
  ///
  /// @param name The new name to rename.
  ///
  /// \returns A non-zero error code, or 0 on success.
  public int rename(Dbi dbi, String name) {
    if (name == null) {
      name = "";
    }
    final var namePtr = Memory.allocCString(name);
    final int err;
    try {
      err = (int) CFunctions.MDBX_DBI_RENAME.invokeExact(txnPointer, dbi.dbi, namePtr);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      throw new MDBXError(err);
    }
    dbi.name = name;
    if (dbi.namePtr != 0L) {
      Memory.dealloc(dbi.namePtr);
    }
    dbi.namePtr = namePtr;
    return err;
  }

  /// Retrieve statistics for a table.
  ///
  /// @param dbi     A table handle returned by \ref mdbx_dbi_open().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL   An invalid parameter was specified.
  public int stat(Dbi dbi) {
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    if (dbi.stat == null) {
      dbi.stat = new Stat();
    }
    if (dbi.statPtr == 0L) {
      dbi.statPtr = Memory.allocZeroed(Stat.SIZE);
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_DBI_STAT.invokeExact(txnPointer, dbi.dbi, dbi.statPtr, Stat.SIZE);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err == Error.SUCCESS) {
      dbi.stat.update(dbi.statPtr);
    }
    return err;
  }

  /// Retrieve the DB flags and status for a table handle.
  ///
  /// \param dbi     A table handle returned by \ref mdbx_dbi_open().
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int flagsAndState(Dbi dbi) {
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_DBI_FLAGS_EX.invokeExact(
          txnPointer, dbi.dbi, scratchPointer, scratchPointer + 8L);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err == Error.SUCCESS) {
      dbi.flags = Danger.UNSAFE.getInt(scratchPointer);
      dbi.state = Danger.UNSAFE.getInt(scratchPointer + 8L);
    }
    return err;
  }

  /// Close a table handle. Normally unnecessary.
  ///
  /// Closing a table handle is not necessary, but lets \ref mdbx_dbi_open()
  /// reuse the handle value. Usually it's better to set a bigger
  /// \ref mdbx_env_set_maxdbs(), unless that value would be large.
  /// \note Use with care.
  ///
  /// This call is synchronized via mutex with \ref mdbx_dbi_open(), but NOT with
  /// any transaction(s) running by other thread(s).
  ///
  /// So the `mdbx_dbi_close()` MUST NOT be called in-parallel/concurrently
  /// with any transactions using the closing dbi-handle, nor during other thread
  /// commit/abort a write transacton(s). The "next" version of libmdbx (\ref
  /// MithrilDB) will solve this issue.
  ///
  /// Handles should only be closed if no other threads are going to reference
  /// the table handle or one of its cursors any further. Do not close a handle
  /// if an existing transaction has modified its table. Doing so can cause
  /// misbehavior from table corruption to errors like \ref MDBX_BAD_DBI
  /// (since the DB name is gone).
  ///
  /// @param dbi  A table handle returned by \ref mdbx_dbi_open().
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int close(Dbi dbi) {
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_DBI_CLOSE.invokeExact(txnPointer, dbi.dbi);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err == Error.SUCCESS) {
      dbi.reset();
    }
    return err;
  }

  /// Empty or delete and close a table.
  ///
  /// \see mdbx_dbi_close() \see mdbx_dbi_open()
  ///
  /// @param dbi      A table handle returned by \ref mdbx_dbi_open().
  /// @param delete  `false` to empty the DB, `true` to delete it
  ///                 from the environment and close the DB handle.
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int drop(Dbi dbi, boolean delete) {
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_DROP.invokeExact(txnPointer, dbi.dbi, delete);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err == Error.SUCCESS) {
      dbi.reset();
    }
    return err;
  }

  /// Get items from a table.
  ///
  /// This function retrieves key/data pairs from the table. The address
  /// and length of the data associated with the specified key are returned
  /// in the structure to which data refers.
  ///
  /// If the table supports duplicate keys (\ref MDBX_DUPSORT) then the
  /// first data item for the key will be returned. Retrieval of other
  /// items requires the use of \ref mdbx_cursor_get().
  ///
  /// \note The memory pointed to by the returned values is owned by the
  /// table. The caller MUST not dispose of the memory, and MUST not modify it
  /// in any way regardless in a read-only nor read-write transactions!
  /// For case a table opened without the \ref MDBX_WRITEMAP modification
  /// attempts likely will cause a `SIGSEGV`. However, when a table opened with
  /// the \ref MDBX_WRITEMAP or in case values returned inside read-write
  /// transaction are located on a "dirty" (modified and pending to commit) pages,
  /// such modification will silently accepted and likely will lead to DB and/or
  /// data corruption.
  ///
  /// \note Values returned from the table are valid only until a
  /// subsequent update operation, or the end of the transaction.
  ///
  /// @param dbi       A table handle returned by \ref mdbx_dbi_open().
  /// @param key       The key to search for in the table.
  /// @param data      The data corresponding to the key.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_NOTFOUND  The key was not in the table.
  /// \retval MDBX_EINVAL    An invalid parameter was specified.
  public int get(Dbi dbi, Val key, Val data) {
    assert key != null;
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int)
          CFunctions.MDBX_GET.invokeExact(txnPointer, dbi.dbi, key.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int get(Dbi dbi, long key, Val data) {
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    Danger.UNSAFE.putLong(scratchPointer, key);
    keyValue.base(scratchPointer);
    keyValue.len(8L);
    try {
      return (int)
          CFunctions.MDBX_GET.invokeExact(txnPointer, dbi.dbi, keyValue.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Get equal or great item from a table.
  ///
  /// Briefly this function does the same as \ref mdbx_get() with a few
  /// differences:
  /// 1. Return equal or great (due comparison function) key-value
  ///    pair, but not only exactly matching with the key.
  /// 2. On success return \ref MDBX_SUCCESS if key found exactly,
  ///    and \ref MDBX_RESULT_TRUE otherwise. Moreover, for tables with
  ///    \ref MDBX_DUPSORT flag the data argument also will be used to match over
  ///    multi-value/duplicates, and \ref MDBX_SUCCESS will be returned only when
  ///    BOTH the key and the data match exactly.
  /// 3. Updates BOTH the key and the data for pointing to the actual key-value
  ///    pair inside the table.
  ///
  /// @param dbi       A table handle returned by \ref mdbx_dbi_open().
  /// @param key       The key to search for in the table.
  /// @param data      The data corresponding to the key.
  /// \returns A non-zero error value on failure and \ref MDBX_RESULT_FALSE
  ///          or \ref MDBX_RESULT_TRUE on success (as described above).
  ///          Some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_NOTFOUND      The key was not in the table.
  /// \retval MDBX_EINVAL        An invalid parameter was specified.
  public int getEqualOrGreater(Dbi dbi, Val key, Val data) {
    assert key != null;
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int) CFunctions.MDBX_GET_EQUAL_OR_GREATER.invokeExact(
          txnPointer, dbi.dbi, key.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int getEqualOrGreater(Dbi dbi, long key, Val data) {
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    Danger.UNSAFE.getLong(scratchPointer, key);
    keyValue.base(scratchPointer);
    keyValue.len(8L);
    try {
      return (int) CFunctions.MDBX_GET_EQUAL_OR_GREATER.invokeExact(
          txnPointer, dbi.dbi, keyValue.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Store items into a table.
  ///
  /// This function stores key/data pairs in the table. The default behavior
  /// is to enter the new key/data pair, replacing any previously existing key
  /// if duplicates are disallowed, or adding a duplicate data item if
  /// duplicates are allowed (see \ref MDBX_DUPSORT).
  ///
  /// @param dbi        A table handle returned by \ref mdbx_dbi_open().
  /// @param key        The key to store in the table.
  /// @param data       The data to store.
  /// @param flags      Special options for this operation.
  ///                   This parameter must be set to 0 or by bitwise OR'ing
  ///                   together one or more of the values described here:
  ///   - \ref MDBX_NODUPDATA
  ///      Enter the new key-value pair only if it does not already appear
  ///      in the table. This flag may only be specified if the table
  ///      was opened with \ref MDBX_DUPSORT. The function will return
  ///      \ref MDBX_KEYEXIST if the key/data pair already appears in the table.
  ///  - \ref MDBX_NOOVERWRITE
  ///      Enter the new key/data pair only if the key does not already appear
  ///      in the table. The function will return \ref MDBX_KEYEXIST if the key
  ///      already appears in the table, even if the table supports
  ///      duplicates (see \ref  MDBX_DUPSORT). The data parameter will be set
  ///      to point to the existing item.
  ///  - \ref MDBX_CURRENT
  ///      Update an single existing entry, but not add new ones. The function will
  ///      return \ref MDBX_NOTFOUND if the given key not exist in the table.
  ///      In case multi-values for the given key, with combination of
  ///      the \ref MDBX_ALLDUPS will replace all multi-values,
  ///      otherwise return the \ref MDBX_EMULTIVAL.
  ///  - \ref MDBX_RESERVE
  ///      Reserve space for data of the given size, but don't copy the given
  ///      data. Instead, return a pointer to the reserved space, which the
  ///      caller can fill in later - before the next update operation or the
  ///      transaction ends. This saves an extra memcpy if the data is being
  ///      generated later. MDBX does nothing else with this memory, the caller
  ///      is expected to modify all of the space requested. This flag must not
  ///      be specified if the table was opened with \ref MDBX_DUPSORT.
  ///  - \ref MDBX_APPEND
  ///      Append the given key/data pair to the end of the table. This option
  ///      allows fast bulk loading when keys are already known to be in the
  ///      correct order. Loading unsorted keys with this flag will cause
  ///      a \ref MDBX_EKEYMISMATCH error.
  ///  - \ref MDBX_APPENDDUP
  ///      As above, but for sorted dup data.
  ///  - \ref MDBX_MULTIPLE
  ///      Store multiple contiguous data elements in a single request. This flag
  ///      may only be specified if the table was opened with
  ///      \ref MDBX_DUPFIXED. With combination the \ref MDBX_ALLDUPS
  ///      will replace all multi-values.
  ///      The data argument must be an array of two \ref MDBX_val. The `iov_len`
  ///      of the first \ref MDBX_val must be the size of a single data element.
  ///      The `iov_base` of the first \ref MDBX_val must point to the beginning
  ///      of the array of contiguous data elements which must be properly aligned
  ///      in case of table with \ref MDBX_INTEGERDUP flag.
  ///      The `iov_len` of the second \ref MDBX_val must be the count of the
  ///      number of data elements to store. On return this field will be set to
  ///      the count of the number of elements actually written. The `iov_base` of
  ///      the second \ref MDBX_val is unused.
  /// \see \ref c_crud_hints "Quick reference for Insert/Update/Delete operations"
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_KEYEXIST  The key/value pair already exists in the table.
  /// \retval MDBX_MAP_FULL  The database is full, see \ref mdbx_env_set_mapsize().
  /// \retval MDBX_TXN_FULL  The transaction has too many dirty pages.
  /// \retval MDBX_EACCES    An attempt was made to write
  ///                        in a read-only transaction.
  /// \retval MDBX_EINVAL    An invalid parameter was specified.
  public int put(Dbi dbi, Val key, Val data, int flags) {
    assert key != null;
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int) CFunctions.MDBX_PUT.invokeExact(
          txnPointer, dbi.dbi, key.address(), data.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(Dbi dbi, long key, Val data, int flags) {
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    if (data == null) {
      data = dataValue;
      data.clear();
    }
    Danger.UNSAFE.putLong(scratchPointer, key);
    keyValue.base(scratchPointer);
    keyValue.len(8L);
    try {
      return (int) CFunctions.MDBX_PUT.invokeExact(
          txnPointer, dbi.dbi, keyValue.address(), data.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Replace items in a table.
  /// \ingroup c_crud
  /// This function allows to update or delete an existing value at the same time
  /// as the previous value is retrieved. If the argument new_data equal is NULL
  /// zero, the removal is performed, otherwise the update/insert.
  /// The current value may be in an already changed (aka dirty) page. In this
  /// case, the page will be overwritten during the update, and the old value will
  /// be lost. Therefore, an additional buffer must be passed via old_data
  /// argument initially to copy the old value. If the buffer passed in is too
  /// small, the function will return \ref MDBX_RESULT_TRUE by setting iov_len
  /// field pointed by old_data argument to the appropriate value, without
  /// performing any changes.
  /// For tables with non-unique keys (i.e. with \ref MDBX_DUPSORT flag),
  /// another use case is also possible, when by old_data argument selects a
  /// specific item from multi-value/duplicates with the same key for deletion or
  /// update. To select this scenario in flags should simultaneously specify
  /// \ref MDBX_CURRENT and \ref MDBX_NOOVERWRITE. This combination is chosen
  /// because it makes no sense, and thus allows you to identify the request of
  /// such a scenario.
  ///
  /// @param dbi           A table handle returned by \ref mdbx_dbi_open().
  /// @param key           The key to store in the table.
  /// @param newData       The data to store, if NULL then deletion will be performed.
  /// @param oldData       The buffer for retrieve previous value as describe
  ///                      above.
  /// @param flags         Special options for this operation.
  ///                      This parameter must be set to 0 or by bitwise
  ///                      OR'ing together one or more of the values
  ///                      described in \ref mdbx_put() description above,
  ///                      and additionally
  ///                      (\ref MDBX_CURRENT | \ref MDBX_NOOVERWRITE)
  ///                      combination for selection particular item from
  ///                      multi-value/duplicates.
  /// \see \ref c_crud_hints "Quick reference for Insert/Update/Delete operations"
  /// \returns A non-zero error value on failure and 0 on success.
  public int replace(Dbi dbi, Val key, Val newData, Val oldData, int flags) {
    assert key != null;
    assert newData != null;
    assert oldData != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int) CFunctions.MDBX_REPLACE.invokeExact(
          txnPointer, dbi.dbi, key.address(), newData.address(), oldData.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int replace(Dbi dbi, long key, Val newData, Val oldData, int flags) {
    assert newData != null;
    assert oldData != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    Danger.UNSAFE.putLong(scratchPointer, key);
    keyValue.base(scratchPointer);
    keyValue.len(8L);
    try {
      return (int) CFunctions.MDBX_REPLACE.invokeExact(
          txnPointer, dbi.dbi, keyValue.address(), newData.address(), oldData.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Delete items from a table.
  ///
  /// This function removes key/data pairs from the table.
  ///
  /// \note The data parameter is NOT ignored regardless the table does
  /// support sorted duplicate data items or not. If the data parameter
  /// is non-NULL only the matching data item will be deleted. Otherwise, if data
  /// parameter is NULL, any/all value(s) for specified key will be deleted.
  ///
  /// This function will return \ref MDBX_NOTFOUND if the specified key/data
  /// pair is not in the table.
  ///
  /// \see \ref c_crud_hints "Quick reference for Insert/Update/Delete operations"
  ///
  /// @param dbi   A table handle returned by \ref mdbx_dbi_open().
  /// @param key   The key to delete from the table.
  /// @param data  The data to delete.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_EACCES   An attempt was made to write
  ///                       in a read-only transaction.
  /// \retval MDBX_EINVAL   An invalid parameter was specified.
  public int delete(Dbi dbi, Val key, Val data) {
    assert key != null;
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int)
          CFunctions.MDBX_DEL.invokeExact(txnPointer, dbi.dbi, key.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int delete(Dbi dbi, long key, Val data) {
    assert data != null;
    if (dbi == null || dbi.dbi < 0) {
      return Error.BAD_DBI;
    }
    Danger.UNSAFE.putLong(scratchPointer, key);
    keyValue.base(scratchPointer);
    keyValue.len(8L);
    try {
      return (int)
          CFunctions.MDBX_DEL.invokeExact(txnPointer, dbi.dbi, keyValue.address(), data.address());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int close() {
    return 0;
  }

  public interface Flags {
    /// Start read-write transaction.
    ///
    /// Only one write transaction may be active at a time. Writes are fully
    /// serialized, which guarantees that writers can never deadlock.
    int READWRITE = 0;

    /// Start read-only transaction.
    ///
    /// There can be multiple read-only transactions simultaneously that do not
    /// block each other and a write transactions.
    int RDONLY = Env.Flags.RDONLY;

    /// Prepare but not start read-only transaction.
    ///
    /// Transaction will not be started immediately, but created transaction handle
    /// will be ready for use with \ref mdbx_txn_renew(). This flag allows to
    /// preallocate memory and assign a reader slot, thus avoiding these operations
    /// at the next start of the transaction.
    int RDONLY_PREPARE = RDONLY | Env.Flags.NOMEMINIT;

    /// Do not block when starting a write transaction.
    int TRY = 0x10000000;

    /// Exactly the same as \ref MDBX_NOMETASYNC,
    /// but for this transaction only.
    int NOMETASYNC = Env.Flags.NOMETASYNC;

    /// Exactly the same as \ref MDBX_SAFE_NOSYNC,
    /// but for this transaction only.
    int NOSYNC = Env.Flags.SAFE_NOSYNC;

    /// Transaction is invalid.
    /// \note Transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int INVALID = Integer.MIN_VALUE;

    /// Transaction is finished or never began.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int FINISHED = 0x01;

    /// Transaction is unusable after an error.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int ERROR = 0x02;

    /// Transaction must write, even if dirty list is empty.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int DIRTY = 0x04;

    /// Transaction or a parent has spilled pages.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int SPILLS = 0x08;

    /// Transaction has a nested child transaction.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int HAS_CHILD = 0x10;

    /// Transaction is parked by \ref mdbx_txn_park().
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int PARKED = 0x20;

    /// Transaction is parked by \ref mdbx_txn_park() with `autounpark=true`,
    /// and therefore it can be used without explicitly calling
    /// \ref mdbx_txn_unpark() first.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int AUTOPARK = 0x40;

    /// The transaction was blocked using the \ref mdbx_txn_park() function,
    /// and then ousted by a write transaction because
    /// this transaction was interfered with garbage recycling.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int OUSTED = 0x80;

    /// Most operations on the transaction are currently illegal.
    /// \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    /// but can't be used with \ref mdbx_txn_begin().
    int BLOCKED = FINISHED | ERROR | HAS_CHILD | PARKED;
  }

  public static class Info {
    public static final long SIZE = 64L;

    public long txnId;
    public long txnReaderLag;
    public long txnSpaceUsed;
    public long txnSpaceLimitSoft;
    public long txnSpaceLimitHard;
    public long txnSpaceRetired;
    public long txnSpaceLeftover;
    public long txnSpaceDirty;

    void update(long ptr) {
      txnId = Danger.UNSAFE.getLong(ptr);
      txnReaderLag = Danger.UNSAFE.getLong(ptr + 8L);
      txnSpaceUsed = Danger.UNSAFE.getLong(ptr + 16L);
      txnSpaceLimitSoft = Danger.UNSAFE.getLong(ptr + 24L);
      txnSpaceLimitHard = Danger.UNSAFE.getLong(ptr + 32L);
      txnSpaceRetired = Danger.UNSAFE.getLong(ptr + 40L);
      txnSpaceLeftover = Danger.UNSAFE.getLong(ptr + 48L);
      txnSpaceDirty = Danger.UNSAFE.getLong(ptr + 56L);
    }

    @Override
    public String toString() {
      return "TxnInfo{" + "txnId="
          + txnId + ", txnReaderLag="
          + txnReaderLag + ", txnSpaceUsed="
          + txnSpaceUsed + ", txnSpaceLimitSoft="
          + txnSpaceLimitSoft + ", txnSpaceLimitHard="
          + txnSpaceLimitHard + ", txnSpaceRetired="
          + txnSpaceRetired + ", txnSpaceLeftover="
          + txnSpaceLeftover + ", txnSpaceDirty="
          + txnSpaceDirty + '}';
    }
  }
}
