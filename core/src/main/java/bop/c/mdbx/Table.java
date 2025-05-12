package bop.c.mdbx;

import bop.c.Memory;

/// Table represents a key-value namespace backed by its own unique B-Tree.
/// MDBX supports up to 32,765 tables in a single file.
public class Table {
  String name;
  long namePtr;
  int flags;
  int state;
  int dbi;
  Stat stat;
  long statPtr;

  Table() {}

  public String name() {
    return name;
  }

  public int flags() {
    return flags;
  }

  public int state() {
    return state;
  }

  public int dbi() {
    return dbi;
  }

  public boolean isIntegerKey() {
    return (flags & Flags.INT_KEY) != 0;
  }

  public boolean isDuplicateSort() {
    return (flags & Flags.DUPLICATE_SORT) != 0;
  }

  public boolean isDuplicateFixed() {
    return (flags & Flags.DUPLICATE_FIXED) != 0;
  }

  public boolean isDuplicateInteger() {
    return (flags & Flags.DUPLICATE_INT) != 0;
  }

  public boolean isReverseSort() {
    return (flags & Flags.REVERSE) != 0;
  }

  public Stat stat() {
    if (stat == null) {
      stat = new Stat();
    }
    return stat;
  }

  public static Table create(String name) {
    final var dbi = new Table();
    dbi.init(name);
    return dbi;
  }

  public void init(String name) {
    if (name == null) {
      name = "";
    }
    reset();
    this.name = name;
    this.namePtr = Memory.allocCString(name);
  }

  void reset() {
    if (namePtr != 0L) {
      Memory.dealloc(namePtr);
      namePtr = 0;
    }
    if (statPtr != 0L) {
      Memory.dealloc(statPtr);
      statPtr = 0L;
    }
    name = "";
    flags = 0;
    state = 0;
    dbi = -1;
  }

  public interface State {
    /// DB was written in this txn
    int DIRTY = 0x01;

    /// Cached Named-DB record is older than txnID
    int STALE = 0x02;

    /// Named-DB handle opened in this txn
    int FRESH = 0x04;

    /// Named-DB handle created in this txn
    int CREAT = 0x08;
  }

  public interface Flags {
    /// Variable length unique keys with usual byte-by-byte string comparison.
    int DEFAULTS = 0;

    /// Use reverse string comparison for keys.
    int REVERSE = 0x02;

    /// Use sorted duplicates, i.e. allow multi-values for a keys.
    int DUPLICATE_SORT = 0x04;

    /// Numeric keys in native byte order either uint32_t or uint64_t
    /// (must be one of uint32_t or uint64_t, other integer types, for example,
    /// signed integer or uint16_t will not work).
    /// The keys must all be of the same size and must be aligned while passing as
    /// arguments.
    int INT_KEY = 0x08;

    /// With \ref MDBX_DUPSORT; sorted dup items have fixed size. The data values
    /// must all be of the same size.
    int DUPLICATE_FIXED = 0x10;

    /// With \ref MDBX_DUPSORT and with \ref MDBX_DUPFIXED; dups are fixed size
    /// like \ref MDBX_INTEGERKEY -style integers. The data values must all be of
    /// the same size and must be aligned while passing as arguments.
    int DUPLICATE_INT = 0x20;

    /// With \ref MDBX_DUPSORT; use reverse string comparison for data values.
    int DUPLICATE_REVERSE = 0x40;

    /// Create DB if not already existing.
    int CREATE = 0x40000;

    /// Opens an existing table created with unknown flags.
    /// The `MDBX_DB_ACCEDE` flag is intend to open a existing table which
    /// was created with unknown flags (\ref MDBX_REVERSEKEY, \ref MDBX_DUPSORT,
    /// \ref MDBX_INTEGERKEY, \ref MDBX_DUPFIXED, \ref MDBX_INTEGERDUP and
    /// \ref MDBX_REVERSEDUP).
    /// In such cases, instead of returning the \ref MDBX_INCOMPATIBLE error, the
    /// table will be opened with flags which it was created, and then an
    /// application could determine the actual flags by \ref mdbx_dbi_flags().
    int ACCEDE = Env.Flags.ACCEDE;
  }
}
