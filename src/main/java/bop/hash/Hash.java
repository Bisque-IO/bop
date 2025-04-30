package bop.hash;

import bop.unsafe.Danger;
import java.lang.foreign.MemorySegment;
import java.nio.ByteBuffer;

public class Hash {
  static final LongHashFunction XX3 = LongHashFunction.xx3();
  static final LongHashFunction WY3 = LongHashFunction.wy_3();

  public static long xx3(String s) {
    if (s == null || s.isEmpty()) {
      return 0L;
    }
    return XX3.hashBytes(Danger.getBytes(s));
  }

  public static long xx3(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return XX3.hashBytes(buf);
  }

  public static long xx3(byte[] buf, int offset, int length) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return XX3.hashBytes(buf, offset, length);
  }

  public static long xx3(ByteBuffer buf) {
    if (buf == null || buf.limit() == 0) {
      return 0L;
    }
    return XX3.hashBytes(buf);
  }

  public static long xx3(MemorySegment segment) {
    return xx3(segment.address(), (int) segment.byteSize());
  }

  public static long xx3(long address, int length) {
    if (address == 0L || length == 0) {
      return 0L;
    }
    return XX3.hashMemory(address, length);
  }

  public static long wy3(String s) {
    if (s == null || s.isEmpty()) {
      return 0L;
    }
    return WY3.hashBytes(Danger.getBytes(s));
  }

  public static long wy3(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return WY3.hashBytes(buf);
  }

  public static long wy3(byte[] buf, int offset, int length) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return WY3.hashBytes(buf, offset, length);
  }

  public static long wy3(ByteBuffer buf) {
    if (buf == null || buf.limit() == 0) {
      return 0L;
    }
    return WY3.hashBytes(buf);
  }

  public static long wy3(MemorySegment segment) {
    return wy3(segment.address(), (int) segment.byteSize());
  }

  public static long wy3(long address, int length) {
    if (address == 0L || length == 0) {
      return 0L;
    }
    return WY3.hashMemory(address, length);
  }
}
