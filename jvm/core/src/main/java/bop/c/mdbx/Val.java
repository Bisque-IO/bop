package bop.c.mdbx;

import bop.c.LongRef;
import bop.c.Memory;
import bop.unsafe.Danger;
import java.nio.charset.StandardCharsets;

public class Val {
  public static final long SIZE;
  public static final long BASE_OFFSET;
  public static final long LEN_OFFSET;

  static {
    try {
      SIZE = (long) CFunctions.MDBX_VAL_SIZE.invokeExact();
      BASE_OFFSET = (long) CFunctions.MDBX_VAL_IOV_BASE_OFFSET.invokeExact();
      LEN_OFFSET = (long) CFunctions.MDBX_VAL_IOV_LEN_OFFSET.invokeExact();
    } catch (Throwable e) {
      throw new ExceptionInInitializerError(e);
    }
  }

  public static void init() {}

  private long address;

  Val(long address) {
    this.address = address;
  }

  public static Val allocate() {
    return new Val(Memory.zalloc(SIZE));
  }

  public void close() {
    if (address != 0L) {
      Memory.dealloc(address);
      address = 0L;
    }
  }

  public long address() {
    return address;
  }

  public long base() {
    return address == 0L ? -1L : Danger.getLong(address + BASE_OFFSET);
  }

  public void base(long base) {
    if (address != 0L) {
      Danger.putLong(address + BASE_OFFSET, base);
    }
  }

  public long len() {
    return address == 0L ? 0L : Danger.getLong(address + LEN_OFFSET);
  }

  public void len(long len) {
    if (address != 0L) {
      Danger.putLong(address + LEN_OFFSET, len);
    }
  }

  public void set(long base, long len) {
    base(base);
    len(len);
  }

  public void clear() {
    if (address != 0L) {
      Danger.putLong(address + BASE_OFFSET, 0L);
      Danger.putLong(address + LEN_OFFSET, 0L);
    }
  }

  public void set(LongRef ref) {
    base(ref.address());
    len(LongRef.SIZE);
  }

  public long asLong() {
    if (len() == 8L) {
      final var base = base();
      final var len = len();
      return base == 0L ? 0L : Danger.getLong(base);
    } else {
      return 0L;
    }
  }

  ///
  public long asNativeBytes() {
    final var base = base();
    final var len = len();
    if (base == 0L || len <= 0L) {
      return 0;
    }
    final var ptr = Memory.alloc(len);
    Danger.copyMemory(base, ptr, len);
    return ptr;
  }

  public int copyTo(byte[] bytes) {
    final var base = base();
    final var len = len();
    if (base == 0L || len <= 0L) {
      return 0;
    }
    final var copied = Math.min(len, bytes.length);
    Danger.copyMemory(null, base, bytes, Danger.BYTE_BASE, copied);
    return (int) copied;
  }

  public static final byte[] EMPTY_BYTES = new byte[0];

  public byte[] asBytes() {
    final var base = base();
    final var len = len();
    if (base == 0L || len <= 0L) {
      return EMPTY_BYTES;
    }
    final var b = new byte[(int) len];
    Danger.copyMemory(null, base, b, Danger.BYTE_BASE, len);
    return b;
  }

  public String asString() {
    final var base = base();
    final var len = len();
    if (base == 0L || len <= 0L) {
      return "";
    }
    final var b = new byte[(int) len];
    Danger.copyMemory(null, base, b, Danger.BYTE_BASE, len);
    return new String(b, StandardCharsets.UTF_8);
  }

  public static class Vec {
    private long ptr;
    private int length;
    private int capacity;

    public static Vec alloc(int size) {
      final var v = new Vec();
      v.ptr = Memory.zalloc(size * SIZE);
      v.capacity = size;
      return v;
    }

    public long address() {
      return ptr;
    }

    public int length() {
      return length;
    }

    public int capacity() {
      return capacity;
    }

    public void grow() {
      int newCapacity = capacity * 2;
      ptr = Memory.realloc(ptr, newCapacity * SIZE);
      capacity = newCapacity;
    }

    public void add(long address, long len) {
      if (length + 1 > capacity) {
        grow();
      }
      Danger.putLong(ptr + (length * SIZE) + BASE_OFFSET, address);
      Danger.putLong(ptr + (length * SIZE) + LEN_OFFSET, len);
      length++;
    }

    public long address(int index) {
      if (index < 0 || index >= length) {
        return -1L;
      }
      return Danger.getLong(ptr + (index * SIZE) + BASE_OFFSET);
    }

    public long len(int index) {
      if (index < 0 || index >= length) {
        return -1L;
      }
      return Danger.getLong(ptr + (index * SIZE) + LEN_OFFSET);
    }

    public void clear() {
      length = 0;
    }
  }
}
