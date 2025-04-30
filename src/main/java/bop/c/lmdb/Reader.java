package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.util.HashSet;

public class Reader {
  final Env env;
  final Arena arena;
  final HashSet<Cursor> cursors = new HashSet<>();
  long txnAddress;
  MemorySegment txnSegment;
  Val.Segment key;
  Val.Segment value;
  Stat.Struct stat;

  public Reader(Env env, Arena arena, long txnAddress, MemorySegment txnSegment) {
    this.env = env;
    this.arena = arena;
    this.txnAddress = txnAddress;
    this.txnSegment = txnSegment;
    this.key = Val.create(arena);
    this.value = Val.create(arena);
    this.stat = Stat.create(arena);
  }

  public long id() {
    try {
      return (long) Library.MDB_TXN_ID.invokeExact(txnSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Retrieve statistics for a database.
  ///
  /// @param dbi A database handle returned by #mdb_dbi_open()
  /// @param stat The address of an #MDB_stat structure where the statistics will be copied
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - EINVAL - an invalid parameter was specified.
  ///
  public int stat(int dbi, Stat.Struct stat) {
    try {
      return (int) Library.MDB_STAT.invokeExact(txnSegment, dbi, stat.segment());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Commit all the operations of a transaction into the database.
  ///
  /// The transaction handle is freed. It and its cursors must not be used again after this
  /// call, except with #mdb_cursor_renew().
  ///
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - EINVAL - an invalid parameter was specified.
  ///   - ENOSPC - no more disk space.
  ///   - EIO - a low-level I/O error occurred while writing.
  ///   - ENOMEM - out of memory.
  ///
  /// @apiNote Earlier documentation incorrectly said all cursors would be freed. Only
  ///     write-transactions free cursors.
  public int commit() {
    try {
      return (int) Library.MDB_TXN_COMMIT.invokeExact(txnSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Abandon all the operations of the transaction instead of saving them.
  ///
  /// The transaction handle is freed. It and its cursors must not be used again after this
  /// call, except with #mdb_cursor_renew().
  ///
  /// @apiNote Earlier documentation incorrectly said all cursors would be freed. Only
  ///     write-transactions free cursors.
  public int abort() {
    try {
      return (int) Library.MDB_TXN_ABORT.invokeExact(txnSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Reset a read-only transaction.
  ///
  /// Abort the transaction like #mdb_txn_abort(), but keep the transaction handle.
  /// #mdb_txn_renew() may reuse the handle. This saves allocation overhead if the process will
  /// start a new read-only transaction soon, and also locking overhead if #MDB_NOTLS is in use.
  /// The reader table lock is released, but the table slot stays tied to its thread or #MDB_txn.
  /// Use mdb_txn_abort() to discard a reset handle, and to free its lock table slot if MDB_NOTLS
  /// is in use. Cursors opened within the transaction must not be used again after this call,
  /// except with #mdb_cursor_renew(). Reader locks generally don't interfere with writers, but
  /// they keep old versions of database pages allocated. Thus they prevent the old pages from
  /// being reused when writers commit new data, and so under heavy load the database size may
  /// grow much more rapidly than otherwise.
  public int reset() {
    try {
      return (int) Library.MDB_TXN_RESET.invokeExact(txnSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Renew a read-only transaction.
  ///
  /// This acquires a new reader lock for a transaction handle that had been released by
  /// #mdb_txn_reset(). It must be called before a reset transaction may be used again.
  ///
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - #MDB_PANIC - a fatal error occurred earlier and the environment must be shut down.
  ///   - EINVAL - an invalid parameter was specified.
  ///
  public int renew() {
    try {
      return (int) Library.MDB_TXN_RENEW.invokeExact(txnSegment);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Get items from a database.
  ///
  /// This function retrieves key/data pairs from the database. The address and length of the
  /// data associated with the specified \b key are returned in the structure to which \b data
  /// refers. If the database supports duplicate keys (#MDB_DUPSORT) then the first data item for
  /// the key will be returned. Retrieval of other items requires the use of #mdb_cursor_get().
  ///
  /// @param dbi A database handle returned by #mdb_dbi_open()
  /// @param key The key to search for in the database
  /// @param data The data corresponding to the key
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - #MDB_NOTFOUND - the key was not in the database.
  ///   - EINVAL - an invalid parameter was specified.
  ///
  /// @apiNote The memory pointed to by the returned values is owned by the database. The caller
  ///     need not dispose of the memory, and may not modify it in any way. For values returned in
  ///     a read-only transaction any modification attempts will cause a SIGSEGV.
  /// @apiNote Values returned from the database are valid only until a subsequent update
  ///     operation, or the end of the transaction.
  public int get(int dbi, Val.Segment key, Val.Segment data) {
    try {
      return (int) Library.MDB_GET.invokeExact(txnSegment, dbi, key.segment(), data.segment());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }
}
