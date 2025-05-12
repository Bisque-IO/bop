package bop.kernel;

import bop.unsafe.Danger;
import jdk.internal.misc.Unsafe;
import jdk.internal.vm.annotation.Contended;

public class Signal {
  static final long CAPACITY = 64;
  static final long CAPACITY_MASK = CAPACITY - 1L;
  static final long VALUE_OFFSET;

  @Contended
  volatile long value = 0L;

  static {
    try {
      {
        var field = Signal.class.getDeclaredField("value");
        field.setAccessible(true);
        VALUE_OFFSET = Danger.objectFieldOffset(field);
      }
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public Signal() {}

  public long nearest(long signalIndex) {
    var value = this.value;
    var found = Long.numberOfTrailingZeros(value >> signalIndex) + signalIndex;
    return found < 64
        ? found
        : signalIndex - Long.numberOfLeadingZeros(value << (63 - signalIndex));
  }

  public static long nearest(long value, long signalIndex) {
    var found = Long.numberOfTrailingZeros(value >> signalIndex) + signalIndex;
    return found < 64
        ? found
        : signalIndex - Long.numberOfLeadingZeros(value << (63 - signalIndex));
  }

  public boolean isEmpty() {
    return value == 0L;
  }

  public int size() {
    return Long.bitCount(value);
  }

  public boolean set(long signalIndex) {
    var bit = 1L << signalIndex;
    var prev = Danger.getAndBitwiseOrLong(this, VALUE_OFFSET, bit);
    return (prev & bit) == 0L;
  }

  public boolean acquire(long signalIndex) {
    var bit = 1L << signalIndex;
    return (Danger.getAndBitwiseAndLong(this, VALUE_OFFSET, ~bit) & bit) == bit;
  }

  public boolean isSet(long signalIndex) {
    var bit = 1L << signalIndex;
    return (value & bit) != 0;
  }
}
