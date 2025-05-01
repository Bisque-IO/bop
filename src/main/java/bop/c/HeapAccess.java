package bop.c;

import java.lang.foreign.MemorySegment;

public class HeapAccess {
  public static final boolean AVAILABLE;

  static {
    final var segment = MemorySegment.ofArray(new byte[8]);
    AVAILABLE = false;
  }
}
