package bop.c;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public record PointerRef(MemorySegment segment) {
  public static PointerRef allocate(Arena arena) {
    return new PointerRef(arena.allocate(ValueLayout.ADDRESS));
  }

  public MemorySegment value() {
    return segment.get(ValueLayout.ADDRESS, 0L);
  }
}
