package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;

/** Statistics for a database in the environment */
public interface Stat {
  /// @brief Statistics for a database in the environment
  MemoryLayout LAYOUT = MemoryLayout.structLayout(
          ValueLayout.JAVA_INT.withName("ms_psize"),
          ValueLayout.JAVA_INT.withName("ms_depth"),
          ValueLayout.JAVA_LONG.withName("ms_branch_pages"),
          ValueLayout.JAVA_LONG.withName("ms_leaf_pages"),
          ValueLayout.JAVA_LONG.withName("ms_overflow_pages"),
          ValueLayout.JAVA_LONG.withName("ms_entries"))
      .withName("MDB_stat");

  VarHandle MS_PSIZE = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_psize"));

  VarHandle MS_DEPTH = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_depth"));

  VarHandle MS_BRANCH_PAGES =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_branch_pages"));

  VarHandle MS_LEAF_PAGES =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_leaf_pages"));

  VarHandle MS_OVERFLOW_PAGES =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_overflow_pages"));

  VarHandle MS_ENTRIES = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("ms_entries"));
  Rec EMPTY = new Rec(0, 0, 0L, 0L, 0L, 0L);

  static Struct create(Arena arena) {
    return new Struct(arena.allocate(LAYOUT));
  }

  static Rec asRecord(Stat stat) {
    if (stat == null) {
      return EMPTY;
    }
    return new Rec(
        stat.ms_psize(),
        stat.ms_depth(),
        stat.ms_branch_pages(),
        stat.ms_leaf_pages(),
        stat.ms_overflow_pages(),
        stat.ms_entries());
  }

  /// Size of a database page. This is currently the same for all databases.
  int ms_psize();

  /// Depth (height) of the B-tree
  int ms_depth();

  /// Number of internal (non-leaf) pages
  long ms_branch_pages();

  /// Number of leaf pages
  long ms_leaf_pages();

  /// Number of overflow pages
  long ms_overflow_pages();

  /// Number of data items
  long ms_entries();

  record Rec(
      int ms_psize,
      int ms_depth,
      long ms_branch_pages,
      long ms_leaf_pages,
      long ms_overflow_pages,
      long ms_entries)
      implements Stat {}

  record Struct(MemorySegment segment) implements Stat {

    public long address() {
      return segment.address();
    }

    @Override
    public int ms_psize() {
      return (int) MS_PSIZE.get(segment, 0L);
    }

    @Override
    public int ms_depth() {
      return (int) MS_DEPTH.get(segment, 0L);
    }

    @Override
    public long ms_branch_pages() {
      return (long) MS_BRANCH_PAGES.get(segment, 0L);
    }

    @Override
    public long ms_leaf_pages() {
      return (long) MS_LEAF_PAGES.get(segment, 0L);
    }

    @Override
    public long ms_overflow_pages() {
      return (long) MS_OVERFLOW_PAGES.get(segment, 0L);
    }

    @Override
    public long ms_entries() {
      return (long) MS_ENTRIES.get(segment, 0L);
    }
  }
}
