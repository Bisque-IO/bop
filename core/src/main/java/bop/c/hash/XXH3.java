package bop.c.hash;

import bop.unsafe.Danger;
import java.lang.foreign.MemorySegment;

public class XXH3 {
  public static long hash(final String input) {
    return hash(Danger.getBytes(input));
  }

  public static long hash(final byte[] input) {
    return hash(input, 0, input.length);
  }

  public static long hash(final byte[] input, int offset, int length) {
    try {
      return (long) CFunctions.BOP_XXH3_SEGMENT.invokeExact(
          MemorySegment.ofArray(input), (long) offset, (long) length);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long hash(final long address, long length) {
    try {
      return (long) CFunctions.BOP_XXH3.invokeExact(address, length);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static class Digest implements AutoCloseable {
    private long address;

    private Digest(final long address) {
      this.address = address;
    }

    public static Digest create() {
      try {
        return new Digest((long) CFunctions.BOP_XXH3_ALLOC.invokeExact());
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public void close() {
      final long address = this.address;
      if (address == 0L) {
        return;
      }
      try {
        CFunctions.BOP_XXH3_DEALLOC.invokeExact(address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public void update(final long address, final long length) {
      try {
        CFunctions.BOP_XXH3_UPDATE.invokeExact(this.address, address, length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public void update(final String input) {
      update(Danger.getBytes(input));
    }

    public void update(final byte[] input) {
      update(input, 0, input.length);
    }

    public void update(final byte[] input, int offset, int length) {
      try {
        CFunctions.BOP_XXH3_UPDATE_SEGMENT.invokeExact(
            this.address, MemorySegment.ofArray(input), (long) offset, (long) length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long updateFinal(final String input) {
      return updateFinal(Danger.getBytes(input));
    }

    public long updateFinal(final byte[] input) {
      return updateFinal(input, 0, input.length);
    }

    public long updateFinal(final long address, final long length) {
      try {
        return (long) CFunctions.BOP_XXH3_UPDATE_FINAL.invokeExact(this.address, address, length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long updateFinal(final byte[] input, int offset, int length) {
      try {
        return (long) CFunctions.BOP_XXH3_UPDATE_FINAL_SEGMENT.invokeExact(
            this.address, MemorySegment.ofArray(input), (long) offset, (long) length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long updateFinalReset(final long address, final long length) {
      try {
        return (long)
            CFunctions.BOP_XXH3_UPDATE_FINAL_RESET.invokeExact(this.address, address, length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long updateFinalReset(final String input) {
      return updateFinalReset(Danger.getBytes(input));
    }

    public long updateFinalReset(final byte[] input) {
      return updateFinalReset(input, 0, input.length);
    }

    public long updateFinalReset(final byte[] input, int offset, int length) {
      try {
        return (long) CFunctions.BOP_XXH3_UPDATE_FINAL_RESET_SEGMENT.invokeExact(
            address, MemorySegment.ofArray(input), (long) offset, (long) length);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long digest() {
      try {
        return (long) CFunctions.BOP_XXH3_DIGEST.invokeExact(this.address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public long digestReset() {
      try {
        return (long) CFunctions.BOP_XXH3_DIGEST_RESET.invokeExact(this.address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }

    public void reset() {
      try {
        CFunctions.BOP_XXH3_RESET.invokeExact(this.address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }
    }
  }
}
