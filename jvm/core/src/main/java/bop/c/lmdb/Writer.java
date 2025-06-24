package bop.c.lmdb;

import bop.c.IntRef;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.util.HashSet;

public class Writer {
  final Env env;
  final Arena arena;
  final HashSet<Cursor> cursors = new HashSet<>();
  long txnAddress;
  MemorySegment txnSegment;
  Val.Segment key;
  Val.Segment value;
  IntRef dbiRef;

  public Writer(Env env, Arena arena, long txnAddress, MemorySegment txnSegment) {
    this.env = env;
    this.arena = arena;
    this.txnAddress = txnAddress;
    this.txnSegment = txnSegment;
    this.key = Val.create(arena);
    this.value = Val.create(arena);
    this.dbiRef = IntRef.allocate(arena);
  }

  void reset() {}

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

  public int flags(int dbi, IntRef flags) {
    try {
      return (int) Library.MDB_DBI_FLAGS.invokeExact(txnSegment, dbi, flags);
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

  /// Store items into a database.
  ///
  /// This function stores key/data pairs in the database. The default behavior is to enter the
  /// new key/data pair, replacing any previously existing key if duplicates are disallowed, or
  /// adding a duplicate data item if duplicates are allowed (#MDB_DUPSORT).
  ///
  /// @param dbi A database handle returned by #mdb_dbi_open()
  /// @param key The key to store in the database
  /// @param data The data to store
  /// @param flags Special options for this operation. This parameter must be set to 0 or by
  ///     bitwise OR'ing together one or more of the values described here.
  ///
  ///   - #MDB_NODUPDATA - enter the new key/data pair only if it does not already appear in
  ///     the database. This flag may only be specified if the database was opened with
  ///     #MDB_DUPSORT. The function will return #MDB_KEYEXIST if the key/data pair already
  ///     appears in the database.
  ///   - #MDB_NOOVERWRITE - enter the new key/data pair only if the key does not already
  ///     appear in the database. The function will return #MDB_KEYEXIST if the key already
  ///     appears in the database, even if the database supports duplicates (#MDB_DUPSORT).
  ///     The \b data parameter will be set to point to the existing item.
  ///   - #MDB_RESERVE - reserve space for data of the given size, but don't copy the given
  ///     data. Instead, return a pointer to the reserved space, which the caller can fill in
  ///     later - before the next update operation or the transaction ends. This saves an
  ///     extra memcpy if the data is being generated later. LMDB does nothing else with this
  ///     memory, the caller is expected to modify all of the space requested. This flag must
  ///     not be specified if the database was opened with #MDB_DUPSORT.
  ///   - #MDB_APPEND - append the given key/data pair to the end of the database. This
  ///     option allows fast bulk loading when keys are already known to be in the correct
  ///     order. Loading unsorted keys with this flag will cause a #MDB_KEYEXIST error.
  ///   - #MDB_APPENDDUP - as above, but for sorted dup data.
  ///
  ///
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - #MDB_MAP_FULL - the database is full, see #mdb_env_set_mapsize().
  ///   - #MDB_TXN_FULL - the transaction has too many dirty pages.
  ///   - EACCES - an attempt was made to write in a read-only transaction.
  ///   - EINVAL - an invalid parameter was specified.
  ///
  public int put(int dbi, Val.Segment key, Val.Segment data, int flags) {
    try {
      return (int)
          Library.MDB_PUT.invokeExact(txnSegment, dbi, key.segment(), data.segment(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Delete items from a database.
  ///
  /// This function removes key/data pairs from the database. If the database does not support
  /// sorted duplicate data items (#MDB_DUPSORT) the data parameter is ignored. If the database
  /// supports sorted duplicates and the data parameter is NULL, all the duplicate data items for
  /// the key will be deleted. Otherwise, if the data parameter is non-NULL only the matching data
  /// item will be deleted. This function will return #MDB_NOTFOUND if the specified key/data pair
  /// is not in the database.
  ///
  /// @param dbi A database handle returned by #mdb_dbi_open()
  /// @param key The key to delete from the database
  /// @param data The data to delete
  /// @return A non-zero error value on failure and 0 on success. Some possible errors are:
  ///
  ///   - EACCES - an attempt was made to write in a read-only transaction.
  ///   - EINVAL - an invalid parameter was specified.
  ///
  public int del(int dbi, Val.Segment key, Val.Segment data) {
    try {
      return (int) Library.MDB_DEL.invokeExact(txnSegment, dbi, key.segment(), data.segment());
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }
}
