package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;

/// Generic structure used for passing keys and data in and out of the database.
///
/// Values returned from the database are valid only until a subsequent update operation, or the
/// end of the transaction. Do not modify or free them, they commonly point into the database
/// itself.
///
/// Key sizes must be between 1 and #mdb_env_get_maxkeysize() inclusive. The same applies to
/// data sizes in databases with the #MDB_DUPSORT flag. Other data items can in theory be from 0
/// to 0xffffffff bytes long.
public interface Val {
  MemoryLayout LAYOUT = MemoryLayout.structLayout(
          ValueLayout.JAVA_LONG.withName("mv_size"), ValueLayout.ADDRESS.withName("mv_data"))
      .withName("MDB_val");

  VarHandle MV_SIZE = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("mv_size"));
  VarHandle MV_DATA = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("mv_data"));

  static Segment create(Arena arena) {
    return new Segment(arena.allocate(LAYOUT));
  }

  /// size of the data item
  long mv_size();

  /// address of the data item
  long mv_data();

  record Rec(long mv_size, long mv_data) implements Val {}

  record Segment(MemorySegment segment) implements Val {
    public long address() {
      return segment.address();
    }

    @Override
    public long mv_size() {
      return (long) MV_SIZE.get(segment, 0L);
    }

    @Override
    public long mv_data() {
      return (long) MV_DATA.get(segment, 0L);
    }
  }
}
