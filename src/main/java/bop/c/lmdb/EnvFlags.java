package bop.c.lmdb;

/// MDB_env_flags
public interface EnvFlags {
  /// mmap at a fixed address (experimental)
  int MDB_FIXEDMAP = 0x01;
  /// no environment directory
  int MDB_NOSUBDIR = 0x4000;
  /// don't fsync after commit
  int MDB_NOSYNC = 0x10000;
  /// read only
  int MDB_RDONLY = 0x20000;
  /// don't fsync metapage after commit
  int MDB_NOMETASYNC = 0x40000;
  /// use writable mmap
  int MDB_WRITEMAP = 0x80000;
  /// use asynchronous msync when #MDB_WRITEMAP is used
  int MDB_MAPASYNC = 0x100000;
  /// tie reader locktable slots to #MDB_txn objects instead of to threads
  int MDB_NOTLS = 0x200000;
  /// don't do any locking, caller must manage their own locks
  int MDB_NOLOCK = 0x400000;
  /// don't do readahead (no effect on Windows)
  int MDB_NORDAHEAD = 0x800000;
  /// don't initialize malloc'd memory before writing to datafile
  int MDB_NOMEMINIT = 0x1000000;
  /// use the previous snapshot rather than the latest one
  int MDB_PREVSNAPSHOT = 0x2000000;
}
