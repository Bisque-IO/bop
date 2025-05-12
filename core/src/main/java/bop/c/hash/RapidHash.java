package bop.c.hash;

import bop.unsafe.Danger;

import java.lang.foreign.MemorySegment;

public class RapidHash {
  public static long hash(final String input) {
    return hash(Danger.getBytes(input));
  }

  public static long hash(final byte[] input) {
    return hash(input, 0, input.length);
  }

  public static long hash(final byte[] input, int offset, int length) {
    try {
      return (long)CFunctions.BOP_RAPIDHASH_SEGMENT.invokeExact(
        MemorySegment.ofArray(input),
        (long)offset,
        (long)length
      );
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long hash(final long address, long length) {
    try {
      return (long)CFunctions.BOP_RAPIDHASH.invokeExact(
        address,
        length
      );
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }
}
