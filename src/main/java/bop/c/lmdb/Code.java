package bop.c.lmdb;

/// MDB_return_code
public interface Code {
  /// Successful result
  int MDB_SUCCESS = 0;

  /// key/data pair already exists
  int MDB_KEYEXIST = -30799;
  /// key/data pair not found (EOF)
  int MDB_NOTFOUND = -30798;
  /// Requested page not found - this usually indicates corruption
  int MDB_PAGE_NOTFOUND = -30797;
  /// Located page was wrong type
  int MDB_CORRUPTED = -30796;
  /// Update of meta page failed or environment had fatal error
  int MDB_PANIC = -30795;
  /// Environment version mismatch
  int MDB_VERSION_MISMATCH = -30794;
  /// File is not a valid LMDB file
  int MDB_INVALID = -30793;
  /// Environment mapsize reached
  int MDB_MAP_FULL = -30792;
  /// Environment maxdbs reached
  int MDB_DBS_FULL = -30791;
  /// Environment maxreaders reached
  int MDB_READERS_FULL = -30790;
  /// Too many TLS keys in use - Windows only
  int MDB_TLS_FULL = -30789;
  /// Txn has too many dirty pages
  int MDB_TXN_FULL = -30788;
  /// Cursor stack too deep - internal error
  int MDB_CURSOR_FULL = -30787;
  /// Page has not enough space - internal error
  int MDB_PAGE_FULL = -30786;
  /// Database contents grew beyond environment mapsize
  int MDB_MAP_RESIZED = -30785;
  /// Operation and DB incompatible, or DB type changed. This can mean:
  ///
  ///   - The operation expects an #MDB_DUPSORT / #MDB_DUPFIXED database.
  ///   - Opening a named DB when the unnamed DB has #MDB_DUPSORT / #MDB_INTEGERKEY.
  ///   - Accessing a data record as a database, or vice versa.
  ///   - The database was dropped and recreated with different flags.
  ///
  int MDB_INCOMPATIBLE = -30784;
  /// Invalid reuse of reader locktable slot
  int MDB_BAD_RSLOT = -30783;
  /// Transaction must abort, has a child, or is invalid
  int MDB_BAD_TXN = -30782;
  /// Unsupported size of key/DB name/data, or wrong DUPFIXED size
  int MDB_BAD_VALSIZE = -30781;
  /// The specified DBI was changed unexpectedly
  int MDB_BAD_DBI = -30780;
  /// Unexpected problem - txn should abort
  int MDB_PROBLEM = -30779;
  /// The last defined error code
  int MDB_LAST_ERRCODE = MDB_PROBLEM;

  static String message(int code) {
    return switch (code) {
      case MDB_SUCCESS -> "SUCCESS";
      case MDB_KEYEXIST -> "KEYEXIST";
      case MDB_NOTFOUND -> "NOT FOUND";
      case MDB_PAGE_NOTFOUND -> "PAGE NOT FOUND";
      case MDB_CORRUPTED -> "CORRUPTED";
      case MDB_PANIC -> "PANIC";
      case MDB_VERSION_MISMATCH -> "VERSION_MISMATCH";
      case MDB_INVALID -> "INVALID";
      case MDB_MAP_FULL -> "MAP_FULL";
      case MDB_DBS_FULL -> "DBS_FULL";
      case MDB_READERS_FULL -> "READERS_FULL";
      case MDB_TLS_FULL -> "TLS_FULL";
      case MDB_TXN_FULL -> "TXN_FULL";
      case MDB_CURSOR_FULL -> "CURSOR_FULL";
      case MDB_PAGE_FULL -> "PAGE_FULL";
      case MDB_MAP_RESIZED -> "MAP_RESIZED";
      case MDB_INCOMPATIBLE -> "INCOMPATIBLE";
      case MDB_BAD_RSLOT -> "BAD_RSLOT";
      case MDB_BAD_TXN -> "BAD_TXN";
      case MDB_BAD_VALSIZE -> "BAD_VALSIZE";
      case MDB_BAD_DBI -> "BAD_DBI";
      case MDB_PROBLEM -> "PROBLEM";
      default -> "UNKNOWN";
        //      case MDB_LAST_ERRCODE -> MDB_PROBLEM;
    };
  }
}
