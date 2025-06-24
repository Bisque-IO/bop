package bop.c;

import bop.unsafe.Danger;

public final class LongRef {
  public static final long SIZE = 8L;
  private long ptr;

  private LongRef(long ptr) {
    this.ptr = ptr;
  }

  public static LongRef allocate() {
    return new LongRef(Memory.zalloc(8L));
  }

  public long address() {
    return ptr;
  }

  public long value() {
    return ptr == 0L ? 0L : Danger.getLong(ptr);
  }

  public void value(long value) {
    if (ptr != 0L) {
      Danger.putLong(ptr, value);
    }
  }

  public void close() {
    final var ptr = this.ptr;
    if (ptr != 0L) {
      Memory.dealloc(ptr);
      this.ptr = 0L;
    }
  }
}
