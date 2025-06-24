package bop.io;

import bop.alloc.Finalizer;
import bop.c.Memory;
import bop.unsafe.Danger;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.BufferOverflowException;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;

/// A simple, safe and handy native memory buffer.
///
/// All integer and float access uses unaligned loads and stores intrinsics
/// which makes it safe without losing performance on machines that allow
/// unaligned access (x86_64, arm64, etc.).
///
/// Supports big, little and native endian gets and puts.
///
/// Assertions prevent nil pointer usage.
///
/// Unsafe variants of all methods provide no safety features.
public class DirectBytes implements Bytes, BytesMut {

  private final boolean owned;
  private Object base;
  private long address;
  private int size;
  private int capacity;
  private int offset;
  private int position;
  private int sliceMin;
  private int sliceCount;
  private DirectBytes parent;
  private InputStream inputStream;
  private OutputStream outputStream;
  private Finalizer.Handle finalizer;

  protected DirectBytes(Object base, long address, int size, int capacity, boolean owned) {
    this.base = base;
    this.address = address;
    this.offset = 0;
    this.position = 0;
    this.size = size;
    this.capacity = capacity;
    this.owned = owned;
  }

  protected DirectBytes(DirectBytes parent, int offset, int size, int capacity) {
    this.parent = parent;
    this.offset = offset;
    this.position = 0;
    this.address = parent.address + offset;
    this.size = size;
    this.capacity = capacity;
    this.owned = false;
  }

  public static DirectBytes wrap(long address, int size) {
    return new DirectBytes(null, address, size, size, false);
  }

  public static DirectBytes wrap(long address, int size, int capacity) {
    return new DirectBytes(null, address, size, capacity, false);
  }

  public static DirectBytes allocateDirect(int size) {
    final var address = Memory.zalloc(size);
    return new DirectBytes(null, address, 0, size, true);
  }

  public static DirectBytes allocateHeap(int size) {
    return wrap(new byte[size]);
  }

  public static DirectBytes wrap(byte[] buffer) {
    return new DirectBytes(buffer, Danger.BYTE_BASE, buffer.length, buffer.length, false);
  }

  public static DirectBytes wrap(byte[] buffer, int offset, int length) {
    if (offset < 0 || length < 0 || offset + length > buffer.length) {
      throw new IndexOutOfBoundsException(
          String.format("offset=%d, length=%d, buffer.length=%d", offset, length, buffer.length));
    }
    return new DirectBytes(buffer, Danger.BYTE_BASE + offset, length, length, false);
  }

  @Override
  public InputStream asInputStream() {
    if (inputStream == null) {
      inputStream = new InputStreamWrapper();
    }
    return inputStream;
  }

  @Override
  public OutputStream asOutputStream() {
    if (outputStream == null) {
      outputStream = new OutputStreamWrapper();
    }
    return outputStream;
  }

  @Override
  public Object getBase() {
    return base;
  }

  @Override
  public int getRemaining() {
    return size - position;
  }

  @Override
  public int getAvailable() {
    return size - position;
  }

  public long getRootAddress() {
    return address - offset;
  }

  @Override
  public DirectBytes reset() {
    position = 0;
    size = 0;
    return this;
  }

  @Override
  public long getAddress() {
    return address;
  }

  @Override
  public int getPosition() {
    return position;
  }

  @Override
  public DirectBytes setPosition(int newPosition) {
    this.position = Math.min(Math.abs(newPosition), size);
    return this;
  }

  @Override
  public DirectBytes skip(int length) {
    if (length <= 0) return this;
    advance(position, length);
    return this;
  }

  @Override
  public int getSize() {
    return size;
  }

  @Override
  public DirectBytes setSize(int newSize) {
    if (newSize < size) {
      size = newSize;
      return this;
    }
    extend(0, newSize);
    return this;
  }

  @Override
  public int getCapacity() {
    return capacity;
  }

  @Override
  public boolean isOwned() {
    return owned;
  }

  public int sliceCount() {
    return sliceCount;
  }

  @Override
  public DirectBytes asMut() {
    return this;
  }

  ///
  @Override
  public void extend(int offset, int size) {
    final var newSize = offset + size;
    if (newSize < this.size) {
      return;
    }
    if (capacity < newSize) {
      if (!owned) throw new IllegalStateException("cannot grow a buffer without ownership");
      resize(newSize);
    }
    this.size = newSize;
  }

  @Override
  public void close() {
    if (base != null) {
      return;
    }

    if (!owned) {
      this.address = 0;
      this.size = 0;
      this.capacity = 0;
      this.offset = 0;
      final var parent = this.parent;
      this.parent = null;
      if (parent != null) {
        if (--parent.sliceCount == 0) {
          parent.sliceMin = 0;
        }
      }
    } else {
      if (sliceCount > 0) {
        throw new IllegalStateException("cannot close a Buffer that has open slices");
      }
      final var address = this.address;
      if (address == 0L) return;
      this.size = 0;
      this.capacity = 0;
      this.offset = 0;
      this.parent = null;
      Memory.dealloc(address);
      this.address = 0L;
    }
  }

  @Override
  public void resize(int capacity) {
    capacity = Math.max(sliceMin, capacity);
    final var previousCapacity = this.capacity;
    this.capacity = capacity;
    if (base == null) {
      this.address = Memory.realloc(address, capacity);
    } else {
      final var next = new byte[capacity];
      Danger.copyMemory(
          base, address, next, Danger.BYTE_BASE, Math.min(previousCapacity, capacity));
    }
  }

  ///
  @Override
  public boolean outOfBounds(int offset, int length) {
    return capacity < offset + length;
  }

  @Override
  public int peek() throws BufferOverflowException {
    if (position >= size - 1) throw new BufferOverflowException();
    return (int) getByteUnsafe(position + 1) & 0xFF;
  }

  private int advance(int position, int length) {
    if (this.position + length > size) {
      throw new BufferOverflowException();
    }
    this.position += length;
    return position;
  }

  ///
  @Override
  public byte getByte(int offset) {
    assert address != 0L;
    if (outOfBounds(offset, 1)) return 0;
    return Danger.getByte(base, address + offset);
  }

  @Override
  public int getUByte(int offset) {
    assert address != 0L;
    if (outOfBounds(offset, 1)) return 0;
    return (int) Danger.getByte(base, address + offset) & 0XFF;
  }

  @Override
  public byte getByteVolatile(int offset) {
    assert address != 0L;
    if (outOfBounds(offset, 1)) return 0;
    return Danger.getByteVolatile(base, address + offset);
  }

  public byte readByte() throws BufferOverflowException {
    return Danger.getByte(base, address + advance(position, 1));
  }

  ///
  @Override
  public byte getByteUnsafe(int offset) {
    assert address != 0L;
    return Danger.getByte(base, address + offset);
  }

  ///
  @Override
  public DirectBytes writeByte(int value) {
    return writeByte((byte) value);
  }

  @Override
  public BytesMut writeUByte(int value) {
    return writeByte((byte)value);
  }

  ///
  @Override
  public DirectBytes writeByte(byte value) {
    final int offset = size;
    extend(size, 1);
    Danger.putByte(base, address + offset, value);
    return this;
  }

  ///
  @Override
  public DirectBytes putByte(int offset, int value) {
    return putByte(offset, (byte) value);
  }

  @Override
  public BytesMut putUByte(int offset, int value) {
    return putByte(offset, (byte) value);
  }

  ///
  @Override
  public DirectBytes putByte(int offset, byte value) {
    assert address != 0L;
    extend(offset, 1);
    Danger.putByte(base, address + offset, value);
    return this;
  }

  @Override
  public DirectBytes putByteUnsafe(int offset, byte value) {
    Danger.putByte(base, address + offset, value);
    return this;
  }

  ///
  @Override
  public short getShort(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 2)) return 0;
    return Danger.getShortUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public short getShortUnsafe(int offset, boolean bigEndian) {
    return Danger.getShortUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public short readShort(boolean bigEndian) throws BufferOverflowException {
    return Danger.getShortUnaligned(base, address + advance(position, 2), bigEndian);
  }

  @Override
  public DirectBytes writeShort(int value, boolean bigEndian) {
    return writeShort((short) value, bigEndian);
  }

  @Override
  public DirectBytes putShort(int offset, int value, boolean bigEndian) {
    return putShort(offset, (short) value, bigEndian);
  }

  @Override
  public DirectBytes writeShort(short value, boolean bigEndian) {
    return putShort(size, value, bigEndian);
  }

  ///
  @Override
  public DirectBytes putShort(int offset, short value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 2);
    Danger.putShortUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putShortUnsafe(int offset, short value, boolean bigEndian) {
    Danger.putShortUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putCharUnsafe(int offset, char value, boolean bigEndian) {
    Danger.putCharUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public char getChar(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 2)) return 0;
    return Danger.getCharUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public char getCharUnsafe(int offset, boolean bigEndian) {
    return Danger.getCharUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public char readChar(boolean bigEndian) throws BufferOverflowException {
    return Danger.getCharUnaligned(base, address + advance(position, 2), bigEndian);
  }

  @Override
  public DirectBytes writeChar(char value, boolean bigEndian) {
    assert address != 0L;
    extend(size, 2);
    Danger.putCharUnaligned(base, address + size, value, bigEndian);
    return this;
  }

  ///
  @Override
  public DirectBytes putChar(int offset, char value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 2);
    Danger.putCharUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public int getInt(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 4)) return 0;
    return Danger.getIntUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public int getIntUnsafe(int offset, boolean bigEndian) {
    return Danger.getIntUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public int getIntLE(int offset, int length) {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(offset);
      case 2 -> getCharLE(offset);
      case 3 -> getInt24LE(offset);
      case 4 -> getIntLE(offset);
      default -> throw new IllegalArgumentException("length must between 1 and 4");
    };
  }

  @Override
  public int readIntLE(int length) throws BufferOverflowException {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(advance(position, 1));
      case 2 -> getCharLE(advance(position, 2));
      case 3 -> getInt24LE(advance(position, 3));
      case 4 -> getIntLE(advance(position, 4));
      default -> throw new IllegalArgumentException("length must between 1 and 4");
    };
  }

  @Override
  public int getInt24LE(int offset) {
    if (outOfBounds(offset, 3)) return 0;
    return ((int) Danger.getByte(base, address + offset) & 0xFF)
      | (((int) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((int) Danger.getByte(base, address + offset + 2) & 0xFF) << 16);
  }

  @Override public int readInt24LE() throws BufferOverflowException {
    final var offset = advance(position, 3);
    return ((int) Danger.getByte(base, address + offset) & 0xFF)
      | (((int) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((int) Danger.getByte(base, address + offset + 2) & 0xFF) << 16);
  }

  @Override
  public int getIntBE(int offset, int length) {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(offset);
      case 2 -> getCharBE(offset);
      case 3 -> getInt24BE(offset);
      case 4 -> getIntBE(offset);
      default -> throw new IllegalArgumentException("length must between 1 and 4");
    };
  }

  @Override
  public int readIntBE(int length) throws BufferOverflowException {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(advance(position, 1));
      case 2 -> getCharBE(advance(position, 2));
      case 3 -> getInt24BE(advance(position, 3));
      case 4 -> getIntBE(advance(position, 4));
      default -> throw new IllegalArgumentException("length must between 1 and 4");
    };
  }

  @Override
  public int getInt24BE(int offset) {
    if (outOfBounds(offset, 3)) return 0;
    return (((int) Danger.getByte(base, address + offset) & 0xFF) << 16)
      | (((int) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | ((int) Danger.getByte(base, address + offset + 2) & 0xFF);
  }

  @Override public int readInt24BE() throws BufferOverflowException {
    final var offset = advance(position, 3);
    return (((int) Danger.getByte(base, address + offset) & 0xFF) << 16)
      | (((int) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | ((int) Danger.getByte(base, address + offset + 2) & 0xFF);
  }

  @Override
  public int readInt(boolean bigEndian) throws BufferOverflowException {
    return Danger.getIntUnaligned(base, address + advance(position, 4), bigEndian);
  }

  @Override
  public DirectBytes writeInt(int value, boolean bigEndian) {
    return putInt(size, value, bigEndian);
  }

  @Override
  public DirectBytes writeIntLE(int value, int length) {
    switch (length) {
      case 0:
        return this;
      case 1:
        writeByte(value);
      case 2:
        writeCharLE(value);
      case 3:
        writeInt24LE(value);
      case 4:
        writeIntLE(value);
    }
    return this;
  }

  @Override
  public DirectBytes writeInt24LE(int value) {
    return putInt24LE(size, value);
  }

  @Override
  public DirectBytes putInt24LE(int offset, int value) {
    extend(offset, 3);
    Danger.putByte(base, address + offset, (byte) (value & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    return this;
  }

  ///
  @Override
  public DirectBytes putInt(int offset, int value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 4);
    Danger.putIntUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putIntUnsafe(int offset, int value, boolean bigEndian) {
    Danger.putIntUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public long getLong(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 8)) return 0;
    return Danger.getLongUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public long getLongUnsafe(int offset, boolean bigEndian) {
    return Danger.getLongUnaligned(base, address + offset, bigEndian);
  }

  @Override
  public long getLongLE(int offset, int length) {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(offset);
      case 2 -> getCharLE(offset);
      case 3 -> getLong24LE(offset);
      case 4 -> getIntLE(offset);
      case 5 -> getLong40LE(offset);
      case 6 -> getLong48LE(offset);
      case 7 -> getLong56LE(offset);
      case 8 -> getLongLE(offset);
      default -> throw new IllegalArgumentException("length must between 1 and 8");
    };
  }

  @Override
  public long readLongLE(int length) throws BufferOverflowException {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(advance(position, 1));
      case 2 -> getCharLE(advance(position, 2));
      case 3 -> getLong24LE(advance(position, 3));
      case 4 -> getIntLE(advance(position, 4));
      case 5 -> getLong40LE(advance(position, 5));
      case 6 -> getLong48LE(advance(position, 6));
      case 7 -> getLong56LE(advance(position, 7));
      case 8 -> getLongLE(advance(position, 8));
      default -> throw new IllegalArgumentException("length must between 1 and 8");
    };
  }

  @Override
  public long getLong24LE(int offset) {
    if (outOfBounds(offset, 3)) return 0;
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16);
  }

  @Override public long readLong24LE() throws BufferOverflowException {
    final var offset = advance(position, 3);
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16);
  }

  @Override
  public long getLong40LE(int offset) {
    if (outOfBounds(offset, 5)) return 0;
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32);
  }

  @Override public long readLong40LE() throws BufferOverflowException {
    final var offset = advance(position, 5);
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32);
  }

  @Override
  public long getLong48LE(int offset) {
    if (outOfBounds(offset, 6)) return 0;
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 40);
  }

  @Override public long readLong48LE() throws BufferOverflowException {
    final var offset = advance(position, 6);
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 40);
  }

  @Override
  public long getLong56LE(int offset) {
    if (outOfBounds(offset, 7)) return 0;
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 6) & 0xFF) << 48);
  }

  @Override public long readLong56LE() throws BufferOverflowException {
    final var offset = advance(position, 7);
    return ((long) Danger.getByte(base, address + offset) & 0xFF)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 6) & 0xFF) << 48);
  }

  @Override
  public long getLongBE(int offset, int length) {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(offset);
      case 2 -> getCharBE(offset);
      case 3 -> getLong24BE(offset);
      case 4 -> getIntBE(offset);
      case 5 -> getLong40BE(offset);
      case 6 -> getLong48BE(offset);
      case 7 -> getLong56BE(offset);
      case 8 -> getLongBE(offset);
      default -> throw new IllegalArgumentException("length must between 1 and 8");
    };
  }

  @Override
  public long readLongBE(int length) throws BufferOverflowException {
    return switch (length) {
      case 0 -> 0;
      case 1 -> getUByte(advance(position, 1));
      case 2 -> getCharBE(advance(position, 2));
      case 3 -> getLong24BE(advance(position, 3));
      case 4 -> getIntBE(advance(position, 4));
      case 5 -> getLong40BE(advance(position, 5));
      case 6 -> getLong48BE(advance(position, 6));
      case 7 -> getLong56BE(advance(position, 7));
      case 8 -> getLongBE(advance(position, 8));
      default -> throw new IllegalArgumentException("length must between 1 and 8");
    };
  }

  @Override
  public long getLong24BE(int offset) {
    if (outOfBounds(offset, 3)) return 0;
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 2) & 0xFF);
  }

  @Override public long readLong24BE() throws BufferOverflowException {
    final var offset = advance(position, 3);
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 2) & 0xFF);
  }

  @Override
  public long getLong40BE(int offset) {
    if (outOfBounds(offset, 5)) return 0;
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 4) & 0xFF);
  }

  @Override public long readLong40BE() throws BufferOverflowException {
    final var offset = advance(position, 5);
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 4) & 0xFF);
  }

  @Override
  public long getLong48BE(int offset) {
    if (outOfBounds(offset, 6)) return 0;
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 5) & 0xFF);
  }

  @Override public long readLong48BE() throws BufferOverflowException {
    final var offset = advance(position, 6);
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 5) & 0xFF);
  }

  @Override
  public long getLong56BE(int offset) {
    if (outOfBounds(offset, 7)) return 0;
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 48)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 6) & 0xFF);
  }

  @Override public long readLong56BE() throws BufferOverflowException {
    final var offset = advance(position, 7);
    return (((long) Danger.getByte(base, address + offset) & 0xFF) << 48)
      | (((long) Danger.getByte(base, address + offset + 1) & 0xFF) << 40)
      | (((long) Danger.getByte(base, address + offset + 2) & 0xFF) << 32)
      | (((long) Danger.getByte(base, address + offset + 3) & 0xFF) << 24)
      | (((long) Danger.getByte(base, address + offset + 4) & 0xFF) << 16)
      | (((long) Danger.getByte(base, address + offset + 5) & 0xFF) << 8)
      | ((long) Danger.getByte(base, address + offset + 6) & 0xFF);
  }


  @Override
  public long readLong(boolean bigEndian) throws BufferOverflowException {
    return Danger.getLongUnaligned(base, address + advance(position, 8), bigEndian);
  }

  @Override
  public float getFloat(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 4)) return 0;
    return Float.intBitsToFloat(Danger.getIntUnaligned(base, address + offset, bigEndian));
  }

  @Override
  public float getFloatUnsafe(int offset, boolean bigEndian) {
    return Float.intBitsToFloat(Danger.getIntUnaligned(base, address + offset, bigEndian));
  }

  @Override
  public float readFloat(boolean bigEndian) throws BufferOverflowException {
    return Float.intBitsToFloat(
        Danger.getIntUnaligned(base, address + advance(position, 4), bigEndian));
  }

  @Override
  public double getDouble(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 8)) return 0.0;
    return Double.longBitsToDouble(Danger.getLongUnaligned(base, address + offset, bigEndian));
  }

  @Override
  public double getDoubleUnsafe(int offset, boolean bigEndian) {
    return Double.longBitsToDouble(Danger.getLongUnaligned(base, address + offset, bigEndian));
  }

  @Override
  public double readDouble(boolean bigEndian) throws BufferOverflowException {
    return Double.longBitsToDouble(
        Danger.getLongUnaligned(base, address + advance(position, 8), bigEndian));
  }

  @Override
  public DirectBytes writeLong(long value, boolean bigEndian) {
    return putLong(size, value, bigEndian);
  }

  @Override
  public DirectBytes writeLongLE(long value, int length) {
    return switch (length) {
      case 0 -> this;
      case 1 -> writeByte((int) value);
      case 2 -> (DirectBytes)writeCharLE((char) value);
      case 3 -> writeLong24LE(value);
      case 4 -> (DirectBytes)writeIntLE((int) value);
      case 5 -> writeLong40LE(value);
      case 6 -> writeLong48LE(value);
      case 7 -> writeLong56LE(value);
      default -> (DirectBytes)writeLongLE(value);
    };
  }

  @Override public BytesMut putLongLE(int offset, long value, int length) {
    return switch (length) {
      case 0 -> this;
      case 1 -> putByte(offset, (int) value);
      case 2 -> (DirectBytes)putCharLE(offset, (char) value);
      case 3 -> putLong24LE(offset, value);
      case 4 -> (DirectBytes)putIntLE(offset, (int) value);
      case 5 -> putLong40LE(offset, value);
      case 6 -> putLong48LE(offset, value);
      case 7 -> putLong56LE(offset, value);
      default -> (DirectBytes)putLongLE(offset, value);
    };
  }

  @Override
  public DirectBytes writeLong24LE(long value) {
    return putLong24LE(size, value);
  }

  @Override
  public DirectBytes putLong24LE(int offset, long value) {
    extend(offset, 3);
    Danger.putByte(base, address + offset, (byte) (value & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong40LE(long value) {
    return putLong40LE(size, value);
  }

  @Override
  public DirectBytes putLong40LE(int offset, long value) {
    extend(offset, 5);
    Danger.putByte(base, address + offset, (byte) (value & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) ((value >> 32) & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong48LE(long value) {
    return putLong48LE(size, value);
  }

  @Override
  public DirectBytes putLong48LE(int offset, long value) {
    extend(offset, 6);
    Danger.putByte(base, address + offset, (byte) (value & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) ((value >> 32) & 0xFF));
    Danger.putByte(base, address + offset + 5, (byte) ((value >> 40) & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong56LE(long value) {
    return putLong56LE(size, value);
  }

  @Override
  public DirectBytes putLong56LE(int offset, long value) {
    extend(offset, 7);
    Danger.putByte(base, address + offset, (byte) (value & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) ((value >> 32) & 0xFF));
    Danger.putByte(base, address + offset + 5, (byte) ((value >> 40) & 0xFF));
    Danger.putByte(base, address + offset + 6, (byte) ((value >> 48) & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLongBE(long value, int length) {
    return switch (length) {
      case 0 -> this;
      case 1 -> writeByte((int) value);
      case 2 -> (DirectBytes)writeCharBE((char) value);
      case 3 -> writeLong24BE(value);
      case 4 -> (DirectBytes)writeIntBE((int) value);
      case 5 -> writeLong40BE(value);
      case 6 -> writeLong48BE(value);
      case 7 -> writeLong56BE(value);
      default -> (DirectBytes)writeLongBE(value);
    };
  }

  @Override
  public BytesMut putLongBE(int offset, long value, int length) {
    return switch (length) {
      case 0 -> this;
      case 1 -> putByte(offset, (int) value);
      case 2 -> (DirectBytes)putCharBE(offset, (char) value);
      case 3 -> putLong24BE(offset, value);
      case 4 -> (DirectBytes)putIntBE(offset, (int) value);
      case 5 -> putLong40BE(offset, value);
      case 6 -> putLong48BE(offset, value);
      case 7 -> putLong56BE(offset, value);
      case 8 -> (DirectBytes)putLongBE(offset, value);
      default -> {
        if (length < 0) yield this;
        else yield (DirectBytes)putLongBE(offset, value);
      }
    };
  }

  @Override
  public DirectBytes writeLong24BE(long value) {
    return putLong24BE(size, value);
  }

  @Override
  public DirectBytes putLong24BE(int offset, long value) {
    extend(offset, 3);
    Danger.putByte(base, address + offset, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) (value & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong40BE(long value) {
    return putLong40BE(size, value);
  }

  @Override
  public DirectBytes putLong40BE(int offset, long value) {
    extend(offset, 5);
    Danger.putByte(base, address + offset, (byte) ((value >> 32) & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) (value & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong48BE(long value) {
    return putLong48BE(size, value);
  }

  @Override
  public DirectBytes putLong48BE(int offset, long value) {
    extend(offset, 6);
    Danger.putByte(base, address + offset, (byte) ((value >> 40) & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 32) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 5, (byte) (value & 0xFF));
    return this;
  }

  @Override
  public DirectBytes writeLong56BE(long value) {
    return putLong56BE(size, value);
  }

  @Override
  public DirectBytes putLong56BE(int offset, long value) {
    extend(offset, 7);
    Danger.putByte(base, address + offset, (byte) ((value >> 48) & 0xFF));
    Danger.putByte(base, address + offset + 1, (byte) ((value >> 40) & 0xFF));
    Danger.putByte(base, address + offset + 2, (byte) ((value >> 32) & 0xFF));
    Danger.putByte(base, address + offset + 3, (byte) ((value >> 24) & 0xFF));
    Danger.putByte(base, address + offset + 4, (byte) ((value >> 16) & 0xFF));
    Danger.putByte(base, address + offset + 5, (byte) ((value >> 8) & 0xFF));
    Danger.putByte(base, address + offset + 6, (byte) (value & 0xFF));
    return this;
  }

  ///
  @Override
  public DirectBytes putLong(int offset, long value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 8);
    Danger.putLongUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putLongUnsafe(int offset, long value, boolean bigEndian) {
    Danger.putLongUnaligned(base, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes writeFloat(float value, boolean bigEndian) {
    return putFloat(size, value, bigEndian);
  }

  @Override
  public DirectBytes putFloat(int offset, float value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 4);
    Danger.putIntUnaligned(base, address + offset, Float.floatToIntBits(value), bigEndian);
    return this;
  }

  @Override
  public DirectBytes putFloatUnsafe(int offset, float value, boolean bigEndian) {
    Danger.putIntUnaligned(base, address + offset, Float.floatToIntBits(value), bigEndian);
    return this;
  }

  @Override
  public DirectBytes writeDouble(double value, boolean bigEndian) {
    return putDouble(size, value, bigEndian);
  }

  @Override
  public DirectBytes putDouble(int offset, double value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 8);
    Danger.putLongUnaligned(base, address + offset, Double.doubleToLongBits(value), bigEndian);
    return this;
  }

  @Override
  public DirectBytes putDoubleUnsafe(int offset, double value, boolean bigEndian) {
    Danger.putLongUnaligned(base, address + offset, Double.doubleToLongBits(value), bigEndian);
    return this;
  }

  @Override
  public String readString(int length) {
    final var offset = advance(position, length);
    final var bytes = new byte[length];
    getBytes(offset, bytes, 0, length);
    return new String(bytes, StandardCharsets.UTF_8);
  }

  ///
  @Override
  public String getString(int offset, int length) {
    return getString(offset, length, StandardCharsets.UTF_8);
  }

  ///
  @Override
  public String getString(int offset, int length, Charset charset) {
    if (outOfBounds(offset, length)) return "";
    final var bytes = new byte[length];
    getBytes(offset, bytes, 0, length);
    return new String(bytes, charset);
  }

  @Override
  public String readCString() {
    final var endIndex = indexOf((byte) 0, position, size - position);
    if (endIndex == -1) {
      throw new RuntimeException("unexpected EOF while reading c-string (null terminated)");
    }
    final var length = endIndex - position;
    final var str = readString(length);
    final var b = readByte();
    return str;
  }

  ///
  @Override
  public DirectBytes writeString(String value) {
    return putString(size, value);
  }

  @Override
  public DirectBytes writeCString(String value) {
    if (value != null && !value.isEmpty()) {
      writeString(value);
    }
    writeByte(0);
    return this;
  }

  ///
  @Override
  public DirectBytes putString(int offset, String value) {
    if (value == null || value.isEmpty()) {
      return this;
    }
    final var bytes = Danger.getBytes(value);
    extend(offset, bytes.length);
    putBytes(offset, bytes, 0, bytes.length);
    return this;
  }

  ///
  @Override
  public void getBytes(int offset, byte[] value, int valueOffset, int length) {
    if (outOfBounds(offset, length)) return;
    Danger.copyMemory(base, address + offset, value, Danger.BYTE_BASE + valueOffset, length);
  }

  @Override
  public byte[] readBytes(int length) {
    final var offset = advance(position, length);
    final var bytes = new byte[length];
    getBytes(offset, bytes, 0, length);
    return bytes;
  }

  @Override
  public int readBytes(byte[] bytes, int offset, int length) {
    final var off = advance(position, length);
    getBytes(off, bytes, offset, length);
    return length;
  }

  @Override
  public int indexOf(byte b, int offset, int length) {
    if (outOfBounds(offset, length)) {
      length = size - offset;
    }
    for (int i = offset; i < offset + length && i < size; i++) {
      if (getByteUnsafe(i) == b) {
        return i;
      }
    }
    return -1;
  }

  ///
  @Override
  public DirectBytes putBytes(int offset, byte[] value, int valueOffset, int length) {
    extend(offset, length);
    Danger.copyMemory(value, Danger.BYTE_BASE + valueOffset, base, address + offset, length);
    return this;
  }

  ///
  @Override
  public DirectBytes putBuffer(int offset, Bytes bytes) {
    assert address != 0L;
    extend(offset, bytes.getSize());
    Danger.copyMemory(bytes.getBase(), bytes.getAddress(), base, address + offset, bytes.getSize());
    return this;
  }

  ///
  @Override
  public DirectBytes putBuffer(int offset, Bytes bytes, int bufferOffset, int length) {
    assert address != 0L;
    extend(offset, length);
    Danger.copyMemory(
        bytes.getBase(), bytes.getAddress() + bufferOffset, base, address + offset, length);
    return this;
  }

  ///
  @Override
  public DirectBytes putUnsafe(int offset, long srcAddress, int length) {
    assert this.address != 0L;
    assert srcAddress != 0L;
    extend(offset, length);
    Danger.copyMemory(null, srcAddress, base, this.address + offset, length);
    return this;
  }

  @Override
  public DirectBytes writeBytes(byte[] bytes) {
    return writeBytes(bytes, 0, bytes.length);
  }

  @Override
  public DirectBytes writeBytes(byte[] bytes, int offset, int length) {
    putBytes(size, bytes, offset, length);
    return this;
  }

  ///
  @Override
  public DirectBytes slice(int offset, int length) {
    extend(offset, length);
    sliceMin = offset + length;
    sliceCount++;
    return new DirectBytes(this, offset, length, length);
  }

  @Override
  public String toString() {
    return "Buffer{"
        + "address="
        + address
        + ", size="
        + size
        + ", capacity="
        + capacity
        + ", owned="
        + owned
        + '}';
  }

  private class InputStreamWrapper extends InputStream {
    @Override
    public int available() throws IOException {
      return size - position;
    }

    @Override
    public long skip(long n) throws IOException {
      advance(position, (int) n);
      return n;
    }

    @Override
    public void skipNBytes(long n) throws IOException {
      advance(position, (int) n);
    }

    @Override
    public int read() throws IOException {
      return readByte() & 0xFF;
    }

    @Override
    public int read(byte[] b, int off, int len) throws IOException {
      getBytes(size, b, off, len);
      return len;
    }
  }

  private class OutputStreamWrapper extends OutputStream {
    @Override
    public void write(int b) throws IOException {
      writeByte((byte) b);
    }

    @Override
    public void write(byte[] b, int off, int len) throws IOException {
      putBytes(size, b, off, len);
    }
  }
}
