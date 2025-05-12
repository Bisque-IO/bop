package bop.c.lmdb;

/** MDB_put_flags */
public interface PutFlags {
  /// For put: Don't write if the key already exists.
  int MDB_NOOVERWRITE = 0x10;
  /// Only for #MDB_DUPSORT
  /// For put: don't write if the key and data pair already exist.
  /// For mdb_cursor_del: remove all duplicate data items.
  int MDB_NODUPDATA = 0x20;
  /// For mdb_cursor_put: overwrite the current key/data pair
  int MDB_CURRENT = 0x40;
  /// For put: Just reserve space for data, don't copy it. Return a pointer to the reserved space.
  int MDB_RESERVE = 0x10000;
  /// Data is being appended, don't split full pages.
  int MDB_APPEND = 0x20000;
  /// Duplicate data is being appended, don't split full pages.
  int MDB_APPENDDUP = 0x40000;
  /// Store multiple data items in one call. Only for #MDB_DUPFIXED.
  int MDB_MULTIPLE = 0x80000;
}
