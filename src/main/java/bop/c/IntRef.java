package bop.c;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public record IntRef(MemorySegment segment) {
  public static IntRef allocate(Arena arena) {
    return new IntRef(arena.allocate(ValueLayout.ADDRESS));
  }

  public int value() {
    return segment.get(ValueLayout.JAVA_INT, 0L);
  }
}
