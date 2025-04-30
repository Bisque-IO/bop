package bop.c.lmdb;

/** MDB_dbi_open_flags */
public interface DbiOpenFlags {
  /// use reverse string keys
  int MDB_REVERSEKEY = 0x02;
  /// use sorted duplicates
  int MDB_DUPSORT = 0x04;
  /// numeric keys in native byte order, either unsigned int or #mdb_size_t. (lmdb expects 32-bit
  /// int <= size_t <= 32/64-bit mdb_size_t.) The keys must all be of the same size.
  int MDB_INTEGERKEY = 0x08;
  /// with #MDB_DUPSORT, sorted dup items have fixed size
  int MDB_DUPFIXED = 0x10;
  /// with #MDB_DUPSORT, dups are #MDB_INTEGERKEY-style integers
  int MDB_INTEGERDUP = 0x20;
  /// with #MDB_DUPSORT, use reverse string dups
  int MDB_REVERSEDUP = 0x40;
  /// create DB if not already existing
  int MDB_CREATE = 0x40000;
}
