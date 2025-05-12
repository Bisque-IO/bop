package bop.c.lmdb;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;

public class Cursor {
  final Env env;
  final Arena arena;
  final long cursorAddress;
  final MemorySegment cursorSegment;
  final Val.Segment key;
  final Val.Segment value;

  public Cursor(Env env, Arena arena, long cursorAddress, MemorySegment cursorSegment) {
    this.env = env;
    this.arena = arena;
    this.cursorAddress = cursorAddress;
    this.cursorSegment = cursorSegment;
    this.key = Val.create(arena);
    this.value = Val.create(arena);
  }

  public void close() {
    arena.close();
  }
}
