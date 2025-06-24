package bop.c.mdbx;

public interface PutFlags {
  /// Upsertion by default (without any other flags)
  int UPSERT = 0;

  /// For insertion: Don't write if the key already exists.
  int NOOVERWRITE = 0x10;

  /// Has effect only for \ref MDBX_DUPSORT tables.
  /// For upsertion: don't write if the key-value pair already exist.
  int NODUPDATA = 0x20;

  /// For upsertion: overwrite the current key/data pair.
  /// MDBX allows this flag for \ref mdbx_put() for explicit overwrite/update
  /// without insertion.
  /// For deletion: remove only single entry at the current cursor position.
  int CURRENT = 0x40;

  /// Has effect only for \ref MDBX_DUPSORT tables.
  /// For deletion: remove all multi-values (aka duplicates) for given key.
  /// For upsertion: replace all multi-values for given key with a new one.
  int ALLDUPS = 0x80;

  /// For upsertion: Just reserve space for data, don't copy it.
  /// Return a pointer to the reserved space.
  int RESERVE = 0x10000;

  /// Data is being appended.
  /// Don't split full pages, continue on a new instead.
  int APPEND = 0x20000;

  /// Has effect only for \ref MDBX_DUPSORT tables.
  /// Duplicate data is being appended.
  /// Don't split full pages, continue on a new instead.
  int APPENDDUP = 0x40000;

  /// Only for \ref MDBX_DUPFIXED.
  /// Store multiple data items in one call.
  int MULTIPLE = 0x80000;
}
