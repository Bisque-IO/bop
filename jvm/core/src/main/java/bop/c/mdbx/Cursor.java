package bop.c.mdbx;

import static bop.c.mdbx.Val.EMPTY_BYTES;

import bop.c.Memory;
import bop.unsafe.Danger;
import java.lang.foreign.MemorySegment;
import org.agrona.DirectBuffer;

public class Cursor {
  public final Val key = Val.allocate();
  public final Val data = Val.allocate();
  long address;
  long context;
  Txn txn;
  Table table;
  long scratchAddress;
  long scratchSize;

  private Cursor(final long scratchAddress, final long scratchSize) {
    this.scratchAddress = scratchAddress;
    this.scratchSize = scratchSize;
  }

  /// Create a cursor handle but not bind it to transaction nor DBI-handle.
  ///
  /// A cursor cannot be used when its table handle is closed. Nor when its
  /// transaction has ended, except with \ref mdbx_cursor_bind() and \ref
  /// mdbx_cursor_renew(). Also, it can be discarded with \ref mdbx_cursor_close().
  ///
  /// A cursor must be closed explicitly always, before or after its transaction
  /// ends. It can be reused with \ref mdbx_cursor_bind()
  /// or \ref mdbx_cursor_renew() before finally closing it.
  ///
  /// \note In contrast to LMDB, the MDBX required that any opened cursors can be
  /// reused and must be freed explicitly, regardless ones was opened in a
  /// read-only or write transaction. The REASON for this is eliminating ambiguity
  /// which helps to avoid errors such as: use-after-free, double-free, i.e.
  /// memory corruption and segfaults.
  ///
  /// \returns Created cursor handle or NULL in case out of memory.
  public static Cursor create() {
    return create(0L);
  }

  /// Create a cursor handle but not bind it to transaction nor DBI-handle.
  ///
  /// A cursor cannot be used when its table handle is closed. Nor when its
  /// transaction has ended, except with \ref mdbx_cursor_bind() and \ref
  /// mdbx_cursor_renew(). Also, it can be discarded with \ref mdbx_cursor_close().
  ///
  /// A cursor must be closed explicitly always, before or after its transaction
  /// ends. It can be reused with \ref mdbx_cursor_bind()
  /// or \ref mdbx_cursor_renew() before finally closing it.
  ///
  /// \note In contrast to LMDB, the MDBX required that any opened cursors can be
  /// reused and must be freed explicitly, regardless ones was opened in a
  /// read-only or write transaction. The REASON for this is eliminating ambiguity
  /// which helps to avoid errors such as: use-after-free, double-free, i.e.
  /// memory corruption and segfaults.
  ///
  /// @param context      A pointer to application context to be associated with
  ///                     created cursor and could be retrieved by
  ///                     \ref mdbx_cursor_get_userctx() until cursor closed.
  /// \returns Created cursor handle or NULL in case out of memory.
  public static Cursor create(long context) {
    final long pointer;
    try {
      pointer = (long) CFunctions.MDBX_CURSOR_CREATE.invokeExact(context);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    final Cursor cursor = new Cursor(Memory.zalloc(256L), 256L);
    cursor.address = pointer;
    cursor.context = context;
    return cursor;
  }

  public void ensureTempStorage(long size) {
    if (scratchAddress == 0L) {
      size = Math.max(256L, size);
      scratchAddress = Memory.zalloc(size);
      scratchSize = size;
      return;
    }

    if (scratchSize >= size) {
      return;
    }

    scratchAddress = Memory.realloc(scratchAddress, size);
  }

  /// Set application information associated with the cursor.
  ///
  /// \see mdbx_cursor_get_userctx()
  ///
  /// @param context  An arbitrary pointer for whatever the application needs.
  /// \returns A non-zero error value on failure and 0 on success.
  public int setContext(long context) {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    try {
      return (int) CFunctions.MDBX_CURSOR_SET_USERCTX.invokeExact(cursorPointer, context);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Get the application information associated with the MDBX_cursor.
  ///
  /// \see mdbx_cursor_set_userctx()
  ///
  /// \returns The pointer which was passed via the `context` parameter
  ///          of `mdbx_cursor_create()` or set by \ref mdbx_cursor_set_userctx(),
  ///          or `NULL` if something wrong.
  public long getContext() {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return 0L;
    }
    try {
      this.context = (long) CFunctions.MDBX_CURSOR_GET_USERCTX.invokeExact(cursorPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    return this.context;
  }

  /// Closes a cursor handle with returning error code.
  ///
  /// The cursor handle will be freed and must not be used again after this call,
  /// but its transaction may still be live.
  ///
  /// This function returns `void` but panic in case of error. Use \ref mdbx_cursor_close2()
  /// if you need to receive an error code instead of an app crash.
  ///
  /// \see mdbx_cursor_close2
  ///
  /// \note In contrast to LMDB, the MDBX required that any opened cursors can be
  /// reused and must be freed explicitly, regardless ones was opened in a
  /// read-only or write transaction. The REASON for this is eliminating ambiguity
  /// which helps to avoid errors such as: use-after-free, double-free, i.e.
  /// memory corruption and segfaults.
  public void close() {
    final var cursorPointer = this.address;
    if (cursorPointer != 0L) {
      try {
        CFunctions.MDBX_CURSOR_CLOSE.invokeExact(cursorPointer);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
      this.address = 0L;
    }
    final var scratchPointer = this.scratchAddress;
    if (scratchPointer != 0L) {
      Memory.dealloc(scratchPointer);
      this.scratchAddress = 0L;
    }

    if (key.address() != 0L) {
      key.close();
    }
    if (data.address() != 0L) {
      data.close();
    }
  }

  /// Bind cursor to specified transaction and DBI-handle.
  ///
  /// Using of the `mdbx_cursor_bind()` is equivalent to calling
  /// \ref mdbx_cursor_renew() but with specifying an arbitrary DBI-handle.
  /// A cursor may be associated with a new transaction, and referencing a new or
  /// the same table handle as it was created with. This may be done whether the
  /// previous transaction is live or dead.
  ///
  /// If the transaction is nested, then the cursor should not be used in its parent transaction.
  /// Otherwise, it is no way to restore state if this nested transaction will be aborted,
  /// nor impossible to define the expected behavior.
  ///
  /// \note In contrast to LMDB, the MDBX required that any opened cursors can be
  /// reused and must be freed explicitly, regardless ones was opened in a
  /// read-only or write transaction. The REASON for this is eliminating ambiguity
  /// which helps to avoid errors such as: use-after-free, double-free, i.e.
  /// memory corruption and segfaults.
  ///
  /// @param txn      A transaction handle returned by \ref mdbx_txn_begin().
  /// @param table      A table handle returned by \ref mdbx_dbi_open().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL  An invalid parameter was specified.
  public int bind(Txn txn, Table table) {
    assert txn != null;
    assert table != null;
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_CURSOR_BIND.invokeExact(txn.txnPointer, cursorPointer, table.dbi);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      return err;
    }
    this.txn = txn;
    this.table = table;
    return err;
  }

  /// Unbind cursor from a transaction.
  ///
  /// Unbinded cursor is disassociated with any transactions but still holds
  /// the original DBI-handle internally. Thus, it could be renewed with any running
  /// transaction or closed.
  ///
  /// If the transaction is nested, then the cursor should not be used in its parent transaction.
  /// Otherwise, it is no way to restore state if this nested transaction will be aborted,
  /// nor impossible to define the expected behavior.
  ///
  /// \see mdbx_cursor_renew()
  /// \see mdbx_cursor_bind()
  /// \see mdbx_cursor_close()
  /// \see mdbx_cursor_reset()
  ///
  /// \note In contrast to LMDB, the MDBX required that any opened cursors can be
  /// reused and must be freed explicitly, regardless ones was opened in a
  /// read-only or write transaction. The REASON for this is eliminating ambiguity
  /// which helps to avoid errors such as: use-after-free, double-free, i.e.
  /// memory corruption and segfaults.
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int unbind() {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_CURSOR_UNBIND.invokeExact(cursorPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    final var txn = this.txn;
    this.txn = null;
    this.table = null;
    if (txn != null) {
      txn.unbound(this);
    }
    return err;
  }

  /// Resets the cursor state.
  ///
  /// Resetting the cursor causes it to become unset and prevents any relative positioning,
  /// retrieval, or modification of data until it is set to a position independent of the
  /// current one. This allows the application to abort further operations without first
  /// positioning the cursor.
  ///
  /// \returns The result of the scan operation, or an error code.
  public int reset() {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_CURSOR_RESET.invokeExact(cursorPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    return err;
  }

  /// Renew a cursor handle for use within the given transaction.
  ///
  /// A cursor may be associated with a new transaction whether the previous
  /// transaction is running or finished.
  ///
  /// Using of the `mdbx_cursor_renew()` is equivalent to calling
  /// \ref mdbx_cursor_bind() with the DBI-handle that previously
  /// the cursor was used with.
  ///
  /// \note In contrast to LMDB, the MDBX allow any cursor to be re-used by using
  /// \ref mdbx_cursor_renew(), to avoid unnecessary malloc/free overhead until it
  /// freed by \ref mdbx_cursor_close().
  ///
  /// @param txn      A transaction handle returned by \ref mdbx_txn_begin().
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EINVAL  An invalid parameter was specified.
  /// \retval MDBX_BAD_DBI The cursor was not bound to a DBI-handle
  ///                      or such a handle became invalid.
  public int renew(final Txn txn) {
    if (txn == null || txn.txnPointer == 0L) {
      return Error.BAD_TXN;
    }
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_CURSOR_RENEW.invokeExact(txn.txnPointer, cursorPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    this.txn = txn;
    return err;
  }

  /// Copy cursor position and state.
  ///
  /// by \ref mdbx_cursor_create() or \ref mdbx_cursor_open().
  ///
  /// @param dst  A destination cursor handle returned
  /// by \ref mdbx_cursor_create() or \ref mdbx_cursor_open().
  ///
  /// \returns A non-zero error value on failure and 0 on success.
  public int copyTo(final Cursor dst) {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    if (dst == null || dst.address == 0L) {
      return Error.INVALID;
    }
    try {
      return (int) CFunctions.MDBX_CURSOR_COPY.invokeExact(cursorPointer, dst.address);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Compares cursor positions.
  ///
  /// The function is intended to compare the positions of two
  /// initialized/installed cursors associated with one transaction and
  /// one table (DBI descriptor).
  /// If the cursors are associated with different transactions, or with different tables,
  /// or one of them is not initialized, the result of the comparison is undefined
  /// (the behavior may be changed in future versions).
  ///
  /// @param right          Right cursor for comparing positions.
  ///
  /// @param ignoreMultiVal Boolean flag that affects the result only when
  ///
  /// comparing cursors for tables with multi-values, i.e. with the
  ///
  /// \ref MDBX_DUPSORT flag. If `true`, cursor positions are compared
  /// only by keys, without taking into account positioning among multi-values.
  /// Otherwise, if `false`, if the positions by keys match,
  /// the positions by multi-values are also compared.
  ///
  /// \retval A signed value in the semantics of the `<=>` operator (less than zero, zero,
  /// or greater than zero) as a result of comparing cursor positions.
  public int compare(Cursor right, boolean ignoreMultiVal) {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return right == null || right.address == 0L ? 0 : -1;
    }
    if (right == null || right.address == 0L) {
      return 1;
    }
    try {
      return (int)
          CFunctions.MDBX_CURSOR_COMPARE.invokeExact(cursorPointer, right.address, ignoreMultiVal);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Retrieve by cursor.
  ///
  /// This function retrieves key/data pairs from the table. The address and
  /// length of the key are returned in the object to which key refers (except
  /// for the case of the \ref MDBX_SET option, in which the key object is
  /// unchanged), and the address and length of the data are returned in the object
  /// to which data refers.
  /// \see mdbx_get()
  ///
  /// \note The memory pointed to by the returned values is owned by the
  /// database. The caller MUST not dispose of the memory, and MUST not modify it
  /// in any way regardless in a read-only nor read-write transactions!
  /// For case a database opened without the \ref MDBX_WRITEMAP modification
  /// attempts likely will cause a `SIGSEGV`. However, when a database opened with
  /// the \ref MDBX_WRITEMAP or in case values returned inside read-write
  /// transaction are located on a "dirty" (modified and pending to commit) pages,
  /// such modification will silently accepted and likely will lead to DB and/or
  /// data corruption.
  ///
  /// @param key   The key for a retrieved item.
  /// @param data  The data of a retrieved item.
  /// @param op    A cursor operation \ref MDBX_cursor_op.
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_NOTFOUND  No matching key found.
  /// \retval MDBX_EINVAL    An invalid parameter was specified.
  public int get(Val key, Val data, int op) {
    assert key != null;
    assert data != null;
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    final int err;
    try {
      err = (int)
          CFunctions.MDBX_CURSOR_GET.invokeExact(cursorPointer, key.address(), data.address(), op);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    return err;
  }

  public int get(int op) {
    if (key.len() == 0L) {
      return Error.NOTFOUND;
    }
    return get(this.key, this.data, op);
  }

  public int get(Val key, int op) {
    assert key != null;
    return get(key, this.data, op);
  }

  public int get(String key, int op) {
    final var b = Danger.getBytes(key);
    return get(b, 0, b.length, op);
  }

  public int get(byte[] key, int op) {
    return get(key, 0, key.length, op);
  }

  public int get(byte[] key, int offset, int length, int op) {
    assert key != null;
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_GET.invokeExact(
          cursorPointer,
          MemorySegment.ofArray(key),
          (long) offset,
          (long) length,
          this.key.address(),
          this.data.address(),
          op);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int get(long key, int op) {
    return get(key, data, op);
  }

  public int get(long key, Val data, int op) {
    assert data != null;
    if (table == null || table.dbi < 0) {
      return Error.BAD_DBI;
    }
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_GET_INT.invokeExact(
          address, key, this.key.address(), data.address(), op);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Utility function for use in utilities.
  ///
  /// When using user-defined comparison functions (aka custom
  /// comparison functions), checking the order of keys may lead to incorrect
  /// results and return the error \ref MDBX_CORRUPTED.
  ///
  /// This function disables the control of the order of keys on pages when
  /// reading database pages for this cursor, and thus allows reading
  /// data in the absence/unavailability of the used comparison functions.
  ///
  /// \see avoid_custom_comparators
  ///
  /// \returns The result of the scan operation, or an error code.
  public int ignored() {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    try {
      return (int) CFunctions.MDBX_CURSOR_IGNORED.invokeExact(cursorPointer);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Store by cursor.
  ///
  /// This function stores key/data pairs into the table. The cursor is
  /// positioned at the new item, or on failure usually near it.
  ///
  /// @param key       The key operated on.
  /// @param data      The data operated on.
  /// @param flags     Options for this operation. This parameter
  ///                  must be set to 0 or by bitwise OR'ing together
  ///                  one or more of the values described here:
  ///  - \ref MDBX_CURRENT
  ///      Replace the item at the current cursor position. The key parameter
  ///      must still be provided, and must match it, otherwise the function
  ///      return \ref MDBX_EKEYMISMATCH. With combination the
  ///      \ref MDBX_ALLDUPS will replace all multi-values.
  ///
  ///      \note MDBX allows (unlike LMDB) you to change the size of the data and
  ///      automatically handles reordering for sorted duplicates
  ///      (see \ref MDBX_DUPSORT).
  ///
  ///  - \ref MDBX_NODUPDATA
  ///      Enter the new key-value pair only if it does not already appear in the
  ///      table. This flag may only be specified if the table was opened
  ///      with \ref MDBX_DUPSORT. The function will return \ref MDBX_KEYEXIST
  ///      if the key/data pair already appears in the table.
  ///
  ///  - \ref MDBX_NOOVERWRITE
  ///      Enter the new key/data pair only if the key does not already appear
  ///      in the table. The function will return \ref MDBX_KEYEXIST if the key
  ///      already appears in the table, even if the table supports
  ///      duplicates (\ref MDBX_DUPSORT).
  ///
  ///  - \ref MDBX_RESERVE
  ///      Reserve space for data of the given size, but don't copy the given
  ///      data. Instead, return a pointer to the reserved space, which the
  ///      caller can fill in later - before the next update operation or the
  ///      transaction ends. This saves an extra memcpy if the data is being
  ///      generated later. This flag must not be specified if the table
  ///      was opened with \ref MDBX_DUPSORT.
  ///
  ///  - \ref MDBX_APPEND
  ///      Append the given key/data pair to the end of the table. No key
  ///      comparisons are performed. This option allows fast bulk loading when
  ///      keys are already known to be in the correct order. Loading unsorted
  ///      keys with this flag will cause a \ref MDBX_KEYEXIST error.
  ///
  ///  - \ref MDBX_APPENDDUP
  ///      As above, but for sorted dup data.
  ///
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
  ///
  /// \see \ref c_crud_hints "Quick reference for Insert/Update/Delete operations"
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_EKEYMISMATCH  The given key value is mismatched to the current
  ///                            cursor position
  /// \retval MDBX_MAP_FULL      The database is full,
  ///                             see \ref mdbx_env_set_mapsize().
  /// \retval MDBX_TXN_FULL      The transaction has too many dirty pages.
  /// \retval MDBX_EACCES        An attempt was made to write in a read-only
  ///                            transaction.
  /// \retval MDBX_EINVAL        An invalid parameter was specified.
  public int put(Val key, Val data, int flags) {
    assert key != null;
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    if (data == null) {
      data = this.data;
      data.clear();
    }
    try {
      return (int) CFunctions.MDBX_CURSOR_PUT.invokeExact(
          cursorPointer, key.address(), data.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(
      DirectBuffer key,
      int keyOffset,
      int keyLength,
      DirectBuffer data,
      int dataOffset,
      int dataLength,
      int flags) {
    var keyBytes = key.byteArray();
    if (keyBytes != null) {
      return put(keyBytes, keyOffset, keyLength, data, dataOffset, dataLength, flags);
    }

    final var keyBuf = key.byteBuffer();
    if (keyBuf != null) {
      if (keyBuf.hasArray()) {
        keyBytes = keyBuf.array();
        return put(keyBytes, keyOffset, keyLength, data, dataOffset, dataLength, flags);
      }
      this.key.set(Danger.getAddress(keyBuf) + keyOffset, keyLength);
      return put(this.key, data, dataOffset, dataLength, flags);
    }

    this.key.set(key.addressOffset() + keyOffset, keyLength);
    return put(this.key, data, dataOffset, dataLength, flags);
  }

  public int put(Val key, DirectBuffer data, int dataOffset, int dataLength, int flags) {
    var dataBytes = data.byteArray();
    if (dataBytes != null) {
      return put(key, dataBytes, dataOffset, dataLength, flags);
    }
    var dataBuf = data.byteBuffer();
    if (dataBuf != null) {
      if (dataBuf.hasArray()) {
        return put(key, dataBuf.array(), dataOffset, dataLength, flags);
      }
      this.data.set(Danger.getAddress(dataBuf), dataLength);
      return put(key, this.data, flags);
    }
    this.data.set(data.addressOffset() + dataOffset, dataLength);
    return put(key, this.data, flags);
  }

  public int put(
      byte[] key,
      int keyOffset,
      int keyLength,
      DirectBuffer data,
      int dataOffset,
      int dataLength,
      int flags) {
    var dataBytes = data.byteArray();
    if (dataBytes != null) {
      return put(key, keyOffset, keyLength, dataBytes, dataOffset, dataLength, flags);
    }
    var dataBuf = data.byteBuffer();
    if (dataBuf != null) {
      if (dataBuf.hasArray()) {
        return put(key, keyOffset, keyLength, dataBuf.array(), dataOffset, dataLength, flags);
      }
      this.data.set(Danger.getAddress(dataBuf), dataLength);
      return put(key, keyOffset, keyLength, this.data, flags);
    }
    this.data.set(data.addressOffset() + dataOffset, dataLength);
    return put(key, keyOffset, keyLength, this.data, flags);
  }

  public int put(String key, String data) {
    return put(key, data, PutFlags.UPSERT);
  }

  public int put(String key, String data, int flags) {
    final var keyBytes = key == null ? EMPTY_BYTES : Danger.getBytes(key);
    final var dataBytes = data == null ? EMPTY_BYTES : Danger.getBytes(data);
    return put(keyBytes, 0, keyBytes.length, dataBytes, 0, dataBytes.length, flags);
  }

  public int put(String key, byte[] data, int offset, int length, int flags) {
    final var keyBytes = key == null ? EMPTY_BYTES : Danger.getBytes(key);
    return put(keyBytes, 0, keyBytes.length, data, offset, length, flags);
  }

  public int put(byte[] key, String data, int flags) {
    if (key == null) {
      key = EMPTY_BYTES;
    }
    final var dataBytes = data == null ? EMPTY_BYTES : Danger.getBytes(data);
    return put(key, 0, key.length, dataBytes, 0, dataBytes.length, flags);
  }

  public int put(byte[] key, int offset, int length, String data, int flags) {
    final var dataBytes = data == null ? EMPTY_BYTES : Danger.getBytes(data);
    return put(key, offset, length, dataBytes, 0, dataBytes.length, flags);
  }

  public int put(byte[] key, byte[] data, int flags) {
    return put(key, 0, key.length, data, 0, data.length, flags);
  }

  public int put(byte[] key, byte[] data, int offset, int length, int flags) {
    return put(key, 0, key.length, data, offset, length, flags);
  }

  public int put(
      byte[] key, int keyOffset, int keyLen, byte[] data, int dataOffset, int dataLen, int flags) {
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_PUT.invokeExact(
          address,
          MemorySegment.ofArray(key),
          (long) keyOffset,
          (long) keyLen,
          MemorySegment.ofArray(data),
          (long) dataOffset,
          (long) dataLen,
          this.data.address(),
          flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(byte[] key, int offset, int length, Val data, int flags) {
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_PUT2.invokeExact(
          address, MemorySegment.ofArray(key), (long) offset, (long) length, data.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(Val key, byte[] data, int offset, int length) {
    return put(key, data, offset, length, PutFlags.UPSERT);
  }

  public int put(Val key, byte[] data, int offset, int length, int flags) {
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_PUT3.invokeExact(
          address,
          key,
          MemorySegment.ofArray(data),
          (long) offset,
          (long) length,
          this.data.address(),
          flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(long key, String data) {
    return put(key, data, PutFlags.UPSERT);
  }

  public int put(long key, String data, int flags) {
    final var b = data == null ? EMPTY_BYTES : Danger.getBytes(data);
    return put(key, b, 0, b.length, flags);
  }

  public int put(long key, byte[] data) {
    final var b = data == null ? EMPTY_BYTES : data;
    return put(key, b, 0, b.length, PutFlags.UPSERT);
  }

  public int put(long key, byte[] data, int flags) {
    final var b = data == null ? EMPTY_BYTES : data;
    return put(key, b, 0, b.length, flags);
  }

  public int put(long key, byte[] data, int offset, int length, int flags) {
    try {
      return (int) CFunctions.BOP_MDBX_CURSOR_PUT_INT.invokeExact(
          address,
          key,
          MemorySegment.ofArray(data),
          (long) offset,
          (long) length,
          this.data.address(),
          flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public int put(long key, int flags) {
    return put(key, data, flags);
  }

  public int put(long key, Val data, int flags) {
    try {
      return (int)
          CFunctions.BOP_MDBX_CURSOR_PUT_INT2.invokeExact(address, key, data.address(), flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Delete current key/data pair.
  ///
  /// This function deletes the key/data pair to which the cursor refers. This
  /// does not invalidate the cursor, so operations such as \ref MDBX_NEXT can
  /// still be used on it. Both \ref MDBX_NEXT and \ref MDBX_GET_CURRENT will
  /// return the same record after this operation.
  ///
  /// @param flags   Options for this operation. This parameter must be set
  /// to one of the values described here.
  /// @see PutFlags
  ///
  ///  - \ref MDBX_CURRENT Delete only single entry at current cursor position.
  ///  - \ref MDBX_ALLDUPS
  ///    or \ref MDBX_NODUPDATA (supported for compatibility)
  ///      Delete all of the data items for the current key. This flag has effect
  ///      only for table(s) was created with \ref MDBX_DUPSORT.
  ///
  /// \see \ref c_crud_hints "Quick reference for Insert/Update/Delete operations"
  ///
  /// \returns A non-zero error value on failure and 0 on success,
  ///          some possible errors are:
  /// \retval MDBX_THREAD_MISMATCH  Given transaction is not owned
  ///                               by current thread.
  /// \retval MDBX_MAP_FULL      The database is full,
  ///                            see \ref mdbx_env_set_mapsize().
  /// \retval MDBX_TXN_FULL      The transaction has too many dirty pages.
  /// \retval MDBX_EACCES        An attempt was made to write in a read-only
  ///                            transaction.
  /// \retval MDBX_EINVAL        An invalid parameter was specified.
  public int delete(int flags) {
    final var cursorPointer = this.address;
    if (cursorPointer == 0L) {
      return Error.INVALID;
    }
    try {
      return (int) CFunctions.MDBX_CURSOR_DEL.invokeExact(cursorPointer, flags);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /// Cursor operations
  ///
  /// This is the set of all operations for retrieving data using a cursor.
  /// \see mdbx_cursor_get()
  public interface Op {
    /// Position at first key/data item
    int FIRST = 0;

    /// \ref MDBX_DUPSORT -only: Position at first data item of current key.
    int FIRST_DUP = 1;

    /// \ref MDBX_DUPSORT -only: Position at key/data pair.
    int GET_BOTH = 2;

    /// \ref MDBX_DUPSORT -only: Position at given key and at first data greater
    /// than or equal to specified data.
    int GET_BOTH_RANGE = 3;

    /// Return key/data at current cursor position
    int GET_CURRENT = 4;

    /// \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    /// from current cursor position. Move cursor to prepare
    /// for \ref MDBX_NEXT_MULTIPLE. \see MDBX_SEEK_AND_GET_MULTIPLE
    int GET_MULTIPLE = 5;

    /// Position at last key/data item
    int LAST = 6;

    /// \ref MDBX_DUPSORT -only: Position at last data item of current key.
    int LAST_DUP = 7;

    /// Position at next data item
    int NEXT = 8;

    /// \ref MDBX_DUPSORT -only: Position at next data item of current key.
    int NEXT_DUP = 9;

    /// \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    /// from next cursor position. Move cursor to prepare for `MDBX_NEXT_MULTIPLE`.
    /// \see MDBX_SEEK_AND_GET_MULTIPLE \see MDBX_GET_MULTIPLE
    int NEXT_MULTIPLE = 10;

    /// Position at first data item of next key
    int NEXT_NODUP = 11;

    /// Position at previous data item
    int PREV = 12;

    /// \ref MDBX_DUPSORT -only: Position at previous data item of current key.
    int PREV_DUP = 13;

    /// Position at last data item of previous key
    int PREV_NODUP = 14;

    /// Position at specified key
    int SET = 15;

    /// Position at specified key, return both key and data
    int SET_KEY = 16;

    /// Position at first key greater than or equal to specified key.
    int SET_RANGE = 17;

    /// \ref MDBX_DUPFIXED -only: Position at previous page and return up to
    /// a page of duplicate data items.
    /// \see MDBX_SEEK_AND_GET_MULTIPLE \see MDBX_GET_MULTIPLE
    int PREV_MULTIPLE = 18;

    /// Positions cursor at first key-value pair greater than or equal to
    /// specified, return both key and data, and the return code depends on whether
    /// a exact match.
    ///
    /// For non DUPSORT-ed collections this work the same to \ref MDBX_SET_RANGE,
    /// but returns \ref MDBX_SUCCESS if key found exactly or
    /// \ref MDBX_RESULT_TRUE if greater key was found.
    ///
    /// For DUPSORT-ed a data value is taken into account for duplicates,
    /// i.e. for a pairs/tuples of a key and an each data value of duplicates.
    /// Returns \ref MDBX_SUCCESS if key-value pair found exactly or
    /// \ref MDBX_RESULT_TRUE if the next pair was returned.
    int SET_LOWERBOUND = 19;

    /// Positions cursor at first key-value pair greater than specified,
    /// return both key and data, and the return code depends on whether a
    /// upper-bound was found.
    ///
    /// For non DUPSORT-ed collections this work like \ref MDBX_SET_RANGE,
    /// but returns \ref MDBX_SUCCESS if the greater key was found or
    /// \ref MDBX_NOTFOUND otherwise.
    ///
    /// For DUPSORT-ed a data value is taken into account for duplicates,
    /// i.e. for a pairs/tuples of a key and an each data value of duplicates.
    /// Returns \ref MDBX_SUCCESS if the greater pair was returned or
    /// \ref MDBX_NOTFOUND otherwise.
    int SET_UPPERBOUND = 20;

    /** Doubtless cursor positioning at a specified key. */
    int TO_KEY_LESSER_THAN = 21;

    int TO_KEY_LESSER_OR_EQUAL = 22;
    int TO_KEY_EQUAL = 23;
    int TO_KEY_GREATER_OR_EQUAL = 24;
    int TO_KEY_GREATER_THAN = 25;

    /** Doubtless cursor positioning at a specified key-value pair for dupsort/multi-value hives. */
    int TO_EXACT_KEY_VALUE_LESSER_THAN = 26;

    int TO_EXACT_KEY_VALUE_LESSER_OR_EQUAL = 27;
    int TO_EXACT_KEY_VALUE_EQUAL = 28;
    int TO_EXACT_KEY_VALUE_GREATER_OR_EQUAL = 29;
    int TO_EXACT_KEY_VALUE_GREATER_THAN = 30;

    /** Doubtless cursor positioning at a specified key-value pair for dupsort/multi-value hives. */
    int TO_PAIR_LESSER_THAN = 31;

    int TO_PAIR_LESSER_OR_EQUAL = 32;
    int TO_PAIR_EQUAL = 33;
    int TO_PAIR_GREATER_OR_EQUAL = 34;
    int TO_PAIR_GREATER_THAN = 35;

    /// \ref MDBX_DUPFIXED -only: Seek to given key and return up to a page of
    /// duplicate data items from current cursor position. Move cursor to prepare
    /// for \ref MDBX_NEXT_MULTIPLE. \see MDBX_GET_MULTIPLE
    int SEEK_AND_GET_MULTIPLE = 36;
  }
}
