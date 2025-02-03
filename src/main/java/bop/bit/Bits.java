package bop.bit;

public class Bits {
  /// Calculate the shift value to scale a number based on how refs are compressed or not.
  ///
  /// @param scale of the number reported by Unsafe.
  /// @return how many times the number needs to be shifted to the left.
  public static int calculateShiftForScale(final int scale) {
    if (4 == scale) {
      return 2;
    } else if (8 == scale) {
      return 3;
    }
    throw new IllegalArgumentException("unknown pointer size for scale=" + scale);
  }

  static long pack(int a, int b) {
    return ((long) b << 32) | (a & 0xffffffffL);
  }

  static int first(long packed) {
    return (int) packed;
  }

  static int second(long packed) {
    return (int) (packed >> 32);
  }

  public static long minimumBitCount(long value) {
    return 64L - Long.numberOfLeadingZeros(value);
  }

  /// Fast method of finding the next power of 2 greater than or equal to the supplied value.
  ///
  /// <p>If the value is &lt;= 0 then 1 will be returned.
  ///
  /// <p>This method is not suitable for {@link Long#MIN_VALUE} or numbers greater than 2^62. When
  /// provided then {@link Long#MIN_VALUE} will be returned.
  ///
  /// @param value from which to search for next power of 2.
  /// @return The next power of 2 or the value itself if it is a power of 2.
  public static int findNextPositivePowerOfTwo(final int value) {
    return (int) findNextPositivePowerOfTwo((long) value);
  }

  /// Fast method of finding the next power of 2 greater than or equal to the supplied value.
  ///
  /// <p>If the value is &lt;= 0 then 1 will be returned.
  ///
  /// <p>This method is not suitable for {@link Long#MIN_VALUE} or numbers greater than 2^62. When
  /// provided then {@link Long#MIN_VALUE} will be returned.
  ///
  /// @param value from which to search for next power of 2.
  /// @return The next power of 2 or the value itself if it is a power of 2.
  public static long findNextPositivePowerOfTwo(final long value) {
    return 1L << (Long.SIZE - Long.numberOfLeadingZeros(value - 1));
  }
}
