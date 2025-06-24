package bop.c.mdbx;

import bop.c.SysError;

/// Errors and return codes
///
/// BerkeleyDB uses -30800 to -30999, we'll go under them
/// \see mdbx_strerror() \see mdbx_strerror_r() \see mdbx_liberr2str()
public interface Error {
  static boolean isSuccess(int code) {
    return code == SUCCESS || code == RESULT_TRUE;
  }

  /// Successful result
  int SUCCESS = 0;

  /// Alias for \ref MDBX_SUCCESS
  int RESULT_FALSE = SUCCESS;

  /// Successful result with special meaning or a flag
  int RESULT_TRUE = -1;

  /// key/data pair already exists
  int KEYEXIST = -30799;

  /// key/data pair not found (EOF)
  int NOTFOUND = -30798;

  /// Requested page not found - this usually indicates corruption
  int PAGE_NOTFOUND = -30797;

  /// Database is corrupted (page was wrong type and so on)
  int CORRUPTED = -30796;

  /// Environment had fatal error,
  /// i.e. update of meta page failed and so on.
  int PANIC = -30795;

  /// DB file version mismatch with libmdbx
  int VERSION_MISMATCH = -30794;

  /// File is not a valid MDBX file
  int INVALID = -30793;

  /// Environment mapsize reached
  int MAP_FULL = -30792;

  /// Environment maxdbs reached
  int DBS_FULL = -30791;

  /// Environment maxreaders reached
  int READERS_FULL = -30790;

  /// Transaction has too many dirty pages, i.e transaction too big
  int TXN_FULL = -30788;

  /// Cursor stack too deep - this usually indicates corruption,
  /// i.e branch-pages loop
  int CURSOR_FULL = -30787;

  /// Page has not enough space - internal error
  int PAGE_FULL = -30786;

  /// Database engine was unable to extend mapping, e.g. since address space
  /// is unavailable or busy. This can mean:
  ///  - Database size extended by other process beyond to environment mapsize
  ///    and engine was unable to extend mapping while starting read
  ///    transaction. Environment should be reopened to continue.
  ///  - Engine was unable to extend mapping during write transaction
  ///    or explicit call of \ref mdbx_env_set_geometry().
  int UNABLE_EXTEND_MAPSIZE = -30785;

  /// Environment or table is not compatible with the requested operation
  /// or the specified flags. This can mean:
  ///  - The operation expects an \ref MDBX_DUPSORT / \ref MDBX_DUPFIXED
  ///    table.
  ///  - Opening a named DB when the unnamed DB has \ref MDBX_DUPSORT /
  ///    \ref MDBX_INTEGERKEY.
  ///  - Accessing a data record as a named table, or vice versa.
  ///  - The table was dropped and recreated with different flags.
  int INCOMPATIBLE = -30784;

  /// Invalid reuse of reader locktable slot,
  /// e.g. read-transaction already run for current thread
  int BAD_RSLOT = -30783;

  /// Transaction is not valid for requested operation,
  /// e.g. had errored and be must aborted, has a child/nested transaction,
  /// or is invalid
  int BAD_TXN = -30782;

  /// Invalid size or alignment of key or data for target table,
  /// either invalid table name
  int BAD_VALSIZE = -30781;

  /// The specified DBI-handle is invalid
  /// or changed by another thread/transaction
  int BAD_DBI = -30780;

  /// Unexpected internal error, transaction should be aborted
  int PROBLEM = -30779;

  /// Another write transaction is running or environment is already used while
  /// opening with \ref MDBX_EXCLUSIVE flag
  int BUSY = -30778;

  /// The specified key has more than one associated value
  int EMULTIVAL = -30421;

  /// Bad signature of a runtime object(s), this can mean:
  ///  - memory corruption or double-free;
  ///  - ABI version mismatch (rare case);
  int EBADSIGN = -30420;

  /// Database should be recovered, but this could NOT be done for now
  /// since it opened in read-only mode
  int WANNA_RECOVERY = -30419;

  /// The given key value is mismatched to the current cursor position
  int EKEYMISMATCH = -30418;

  /// Database is too large for current system,
  /// e.g. could NOT be mapped into RAM.
  int TOO_LARGE = -30417;

  /// A thread has attempted to use a not owned object,
  /// e.g. a transaction that started by another thread
  int THREAD_MISMATCH = -30416;

  /// Overlapping read and write transactions for the current thread
  int TXN_OVERLAPPING = -30415;
  int BACKLOG_DEPLETED = -30414;

  /// Alternative/Duplicate LCK-file is exists and should be removed manually
  int DUPLICATED_CLK = -30413;

  /// Some cursors and/or other resources should be closed before table or
  /// corresponding DBI-handle could be (re)used and/or closed.
  int DANGLING_DBI = -30412;

  /// The parked read transaction was outed for the sake of
  /// recycling old MVCC snapshots.
  int OUSTED = -30411;

  /// MVCC snapshot used by parked transaction was bygone.
  int MVCC_RETARDED = -30410;

  /// The last of MDBX-added error codes
  int LAST_ADDED_ERRCODE = MVCC_RETARDED;

  int ENODATA = SysError.ENODATA;
  int EINVAL = SysError.EINVAL;
  int EACCESS = SysError.EACCESS;
  int ENOMEM = SysError.ENOMEM;
  int EROFS = SysError.EROFS;
  int ENOSYS = SysError.ENOSYS;
  int EIO = SysError.EIO;
  int EPERM = SysError.EPERM;
  int EINTR = SysError.EINTR;
  int ENOFILE = SysError.ENOFILE;
  int EREMOTE = SysError.EREMOTE;
  int EDEADLK = SysError.EDEADLK;
}
