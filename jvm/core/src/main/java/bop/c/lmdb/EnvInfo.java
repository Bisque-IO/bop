package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;

/// @brief Information about the environment
public interface EnvInfo {
  MemoryLayout LAYOUT = MemoryLayout.structLayout(
          ValueLayout.JAVA_LONG.withName("me_mapaddr"),
          ValueLayout.JAVA_LONG.withName("me_mapsize"),
          ValueLayout.JAVA_LONG.withName("me_last_pgno"),
          ValueLayout.JAVA_LONG.withName("me_last_txnid"),
          ValueLayout.JAVA_INT.withName("me_maxreaders"),
          ValueLayout.JAVA_INT.withName("me_numreaders"))
      .withName("MDB_envinfo");

  VarHandle ME_MAPADDR = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_mapaddr"));
  VarHandle ME_MAPSIZE = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_mapsize"));
  VarHandle ME_LAST_PGNO = LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_last_pgno"));
  VarHandle ME_LAST_TXNID =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_last_txnid"));
  VarHandle ME_MAXREADERS =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_maxreaders"));
  VarHandle ME_NUMREADERS =
      LAYOUT.varHandle(MemoryLayout.PathElement.groupElement("me_numreaders"));

  static Segment create(Arena arena) {
    return new Segment(arena.allocate(LAYOUT));
  }

  static Rec asRecord(EnvInfo info) {
    if (info instanceof Rec rec) {
      return rec;
    }
    return new Rec(
        info.me_mapaddr(),
        info.me_mapsize(),
        info.me_last_pgno(),
        info.me_last_txnid(),
        info.me_maxreaders(),
        info.me_numreaders());
  }

  /// Address of map, if fixed
  long me_mapaddr();

  /// Size of the data memory map
  long me_mapsize();

  /// ID of the last used page
  long me_last_pgno();

  /// ID of the last committed transaction
  long me_last_txnid();

  /// max reader slots in the environment
  int me_maxreaders();

  /// max reader slots used in the environment
  int me_numreaders();

  record Rec(
      long me_mapaddr,
      long me_mapsize,
      long me_last_pgno,
      long me_last_txnid,
      int me_maxreaders,
      int me_numreaders)
      implements EnvInfo {}

  record Segment(MemorySegment segment) implements EnvInfo {
    @Override
    public long me_mapaddr() {
      return (long) ME_MAPADDR.get(segment, 0L);
    }

    @Override
    public long me_mapsize() {
      return (long) ME_MAPSIZE.get(segment, 0L);
    }

    @Override
    public long me_last_pgno() {
      return (long) ME_LAST_PGNO.get(segment, 0L);
    }

    @Override
    public long me_last_txnid() {
      return (long) ME_LAST_TXNID.get(segment, 0L);
    }

    @Override
    public int me_maxreaders() {
      return (int) ME_MAXREADERS.get(segment, 0L);
    }

    @Override
    public int me_numreaders() {
      return (int) ME_NUMREADERS.get(segment, 0L);
    }
  }
}
