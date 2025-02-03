package bop.unsafe;

import java.lang.reflect.Field;
import jdk.internal.misc.Unsafe;

public class Danger {
  public static final Unsafe UNSAFE;
  public static final int BYTE_BASE;
  public static final long STRING_VALUE_OFFSET;
  public static final long STRING_HASH_OFFSET;
  public static final long STRING_HASH_IS_ZERO_OFFSET;
  public static final long OBJECT_ARRAY_BASE;
  public static final long OBJECT_ARRAY_SHIFT_FOR_SCALE;

  static {
    try {
      UNSAFE = Unsafe.getUnsafe();
      BYTE_BASE = UNSAFE.arrayBaseOffset(byte[].class);

      {
        final Field field = String.class.getDeclaredField("value");
        field.setAccessible(true);
        STRING_VALUE_OFFSET = UNSAFE.objectFieldOffset(field);
      }

      {
        final Field field = String.class.getDeclaredField("hash");
        field.setAccessible(true);
        STRING_HASH_OFFSET = UNSAFE.objectFieldOffset(field);
      }

      {
        final Field field = String.class.getDeclaredField("hashIsZero");
        field.setAccessible(true);
        STRING_HASH_IS_ZERO_OFFSET = UNSAFE.objectFieldOffset(field);
      }

      OBJECT_ARRAY_BASE = UNSAFE.arrayBaseOffset(Object[].class);
      OBJECT_ARRAY_SHIFT_FOR_SCALE = calculateShiftForScale(UNSAFE.arrayIndexScale(Object[].class));
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  /**
   * Compute buffer offset based on the given sequence and the mask.
   *
   * @param sequence to compute the offset from.
   * @param mask to apply.
   * @return buffer offset.
   */
  public static long sequenceToBufferOffset(final long sequence, final long mask) {
    return OBJECT_ARRAY_BASE + ((sequence & mask) << OBJECT_ARRAY_SHIFT_FOR_SCALE);
  }

  /**
   * Calculate the shift value to scale a number based on how refs are compressed or not.
   *
   * @param scale of the number reported by Unsafe.
   * @return how many times the number needs to be shifted to the left.
   */
  public static int calculateShiftForScale(final int scale) {
    if (4 == scale) {
      return 2;
    } else if (8 == scale) {
      return 3;
    }

    throw new IllegalArgumentException("unknown pointer size for scale=" + scale);
  }

  public static byte[] getBytes(String s) {
    return (byte[]) UNSAFE.getReference(s, STRING_VALUE_OFFSET);
  }

  public static void setBytes(String s, byte[] bytes) {
    UNSAFE.putReference(s, STRING_VALUE_OFFSET, bytes);
    UNSAFE.putInt(s, STRING_HASH_OFFSET, 0);
    UNSAFE.putBoolean(s, STRING_HASH_IS_ZERO_OFFSET, false);
  }

  public static long allocate(final int size) {
    return UNSAFE.allocateMemory(size);
  }

  public static long reallocate(final long address, final int size) {
    return UNSAFE.reallocateMemory(address, size);
  }

  public static void deallocate(final long address) {
    UNSAFE.freeMemory(address);
  }

  public static long getLong(final long address) {
    return UNSAFE.getLong(address);
  }

  public static void put(final long address, final long value) {
    UNSAFE.putLong(address, value);
  }
}
