package bop.hash;

import bop.c.hash.RapidHash;
import bop.c.hash.XXH3;
import bop.io.Bytes;
import bop.unsafe.Danger;
import java.lang.foreign.MemorySegment;
import java.nio.ByteBuffer;
import org.agrona.DirectBuffer;

public class Hash {

  public static long rapid(String s) {
    if (s == null || s.isEmpty()) {
      return 0L;
    }
    return RapidHash.hash(s);
  }

  public static long rapid(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return RapidHash.hash(buf);
  }

  public static long rapid(byte[] buf, int offset, int length) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    return RapidHash.hash(buf, offset, length);
  }

  public static long rapid(DirectBuffer buf, int offset, int length) {
    if (buf == null || length == 0) {
      return 0L;
    }
    final var hb = buf.byteArray();
    if (hb != null) {
      return RapidHash.hash(hb, (int) buf.addressOffset(), length);
    }
    final var bb = buf.byteBuffer();
    if (bb != null) {
      return rapid(bb);
    }
    final var address = buf.addressOffset();
    if (address == 0L) {
      return 0L;
    }
    return RapidHash.hash(address + offset, length);
  }

  public static long rapid(ByteBuffer buffer) {
    if (buffer == null) {
      return 0L;
    }
    int pos = buffer.position();
    int limit = buffer.limit();
    assert (pos <= limit);
    int rem = limit - pos;
    if (rem <= 0) {
      return 0L;
    }

    if (buffer.hasArray()) {
      buffer.position(limit);
      return RapidHash.hash(buffer.array(), buffer.arrayOffset() + pos, rem);
    } else {
      final var address = Danger.getAddress(buffer);
      if (address == 0L) {
        return 0L;
      }
      buffer.position(limit);
      return RapidHash.hash(address + pos, rem);
    }
  }

  public static long rapid(final Bytes bytes) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    return RapidHash.hash(address, bytes.size());
  }

  public static long rapid(final Bytes bytes, int offset) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    return RapidHash.hash(address + offset, bytes.size() - offset);
  }

  public static long rapid(final Bytes bytes, int offset, int length) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    return RapidHash.hash(address + offset, length);
  }

  public static long rapid(MemorySegment segment) {
    if (segment == null || segment.byteSize() == 0) {
      return 0L;
    }
    return rapid(segment.address(), (int) segment.byteSize());
  }

  public static long rapid(long address, int length) {
    if (address == 0L || length == 0) {
      return 0L;
    }
    return RapidHash.hash(address, length);
  }

  static final int XXH3_C_THRESHOLD = 128;

  public static long xxh3(String s) {
    if (s == null || s.isEmpty()) {
      return 0L;
    }
    if (s.length() < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(Danger.getBytes(s));
    }
    return XXH3.hash(s);
  }

  public static long xxh3(byte[] buf) {
    return xxh3(buf, 0, buf.length);
  }

  public static long xxh3(byte[] buf, int offset, int length) {
    if (buf == null || buf.length == 0) {
      return 0L;
    }
    if (length < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(buf, offset, length);
    }
    return XXH3.hash(buf, offset, length);
  }

  public static long xxh3(DirectBuffer buf, int offset, int length) {
    if (buf == null || length == 0) {
      return 0L;
    }
    final var hb = buf.byteArray();
    if (hb != null) {
      if (length < XXH3_C_THRESHOLD) {
        return XXH3_64.DEFAULT.hash(hb, (int) buf.addressOffset(), length);
      }
      return XXH3.hash(hb, (int) buf.addressOffset(), length);
    }
    final var bb = buf.byteBuffer();
    if (bb != null) {
      return xxh3(bb);
    }
    final var address = buf.addressOffset();
    if (address == 0L) {
      return 0L;
    }
    if (length < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(address, offset, length);
    }
    return XXH3.hash(address + offset, length);
  }

  public static long xxh3(ByteBuffer buffer) {
    if (buffer == null) {
      return 0L;
    }
    int pos = buffer.position();
    int limit = buffer.limit();
    assert (pos <= limit);
    int rem = limit - pos;
    if (rem <= 0) {
      return 0L;
    }

    if (buffer.hasArray()) {
      buffer.position(limit);
      if (rem < XXH3_C_THRESHOLD) {
        return XXH3_64.DEFAULT.hash(buffer.array(), (int) buffer.arrayOffset(), rem);
      }
      return XXH3.hash(buffer.array(), buffer.arrayOffset() + pos, rem);
    } else {
      final var address = Danger.getAddress(buffer);
      if (address == 0L) {
        return 0L;
      }
      buffer.position(limit);
      if (rem < XXH3_C_THRESHOLD) {
        return XXH3_64.DEFAULT.hash(address, pos, rem);
      }
      return XXH3.hash(address + pos, rem);
    }
  }

  public static long xxh3(MemorySegment segment) {
    if (segment == null || segment.byteSize() == 0) {
      return 0L;
    }
    return xxh3(segment.address(), (int) segment.byteSize());
  }

  public static long xxh3(long address, int length) {
    if (address == 0L || length == 0) {
      return 0L;
    }
    if (length < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(address, 0, length);
    }
    return XXH3.hash(address, length);
  }

  public static long xxh3(final Bytes bytes) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    if (bytes.size() < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(address, 0, bytes.size());
    }
    return XXH3.hash(address, bytes.size());
  }

  public static long xxh3(final Bytes bytes, int offset) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    final int length = bytes.size() - offset;
    if (length < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(address, 0, length);
    }
    return XXH3.hash(address + offset, length);
  }

  public static long xxh3(final Bytes bytes, int offset, int length) {
    if (bytes == null) {
      return 0L;
    }
    final var address = bytes.address();
    if (address == 0L) {
      return 0L;
    }
    if (length < XXH3_C_THRESHOLD) {
      return XXH3_64.DEFAULT.hash(address, offset, length);
    }
    return XXH3.hash(address + offset, length);
  }
}
