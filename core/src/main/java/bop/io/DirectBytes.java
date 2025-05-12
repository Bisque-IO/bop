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

  private long address;
  private int size;
  private int capacity;
  private int offset;
  private int position;
  private int sliceMin;
  private int sliceCount;
  private boolean owned;
  private DirectBytes parent;
  private InputStream inputStream;
  private OutputStream outputStream;
  private Finalizer.Handle finalizer;

  protected DirectBytes(long address, int size, int capacity, boolean owned) {
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

  public static DirectBytes wrap(long address, int size) {
    return new DirectBytes(address, size, size, false);
  }

  public static DirectBytes wrap(long address, int size, int capacity) {
    return new DirectBytes(address, size, capacity, false);
  }

  public static DirectBytes allocate(int size) {
    final var address = Memory.zalloc(size);
    return new DirectBytes(address, 0, size, true);
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

  public long rootAddress() {
    return address - offset;
  }

  @Override
  public long address() {
    return address;
  }

  @Override
  public int position() {
    return position;
  }

  @Override
  public DirectBytes position(int newPosition) {
    this.position = Math.min(Math.abs(newPosition), size);
    return this;
  }

  @Override
  public DirectBytes skip(int length) {
    advance(position, length);
    return this;
  }

  @Override
  public int size() {
    return size;
  }

  @Override
  public DirectBytes size(int newSize) {
    extend(0, newSize);
    return this;
  }

  @Override
  public int capacity() {
    return capacity;
  }

  @Override
  public boolean owned() {
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
    if (newSize < size) return;
    if (capacity < newSize) {
      if (!owned) throw new IllegalStateException("cannot grow a buffer without ownership");
      resize(newSize);
    }
    this.size = newSize;
  }

  @Override
  public void close() {
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
    this.address = Memory.realloc(address, capacity);
    this.capacity = capacity;
  }

  ///
  @Override
  public boolean outOfBounds(int offset, int length) {
    return capacity < offset + length;
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
    return Danger.getByte(address + offset);
  }

  @Override
  public byte getByteVolatile(int offset) {
    assert address != 0L;
    if (outOfBounds(offset, 1)) return 0;
    return Danger.getByteVolatile(null, address + offset);
  }

  public byte readByte() throws BufferOverflowException {
    return Danger.getByte(address + advance(position, 1));
  }

  ///
  @Override
  public byte getByteUnsafe(int offset) {
    assert address != 0L;
    return Danger.getByte(address + offset);
  }

  ///
  @Override
  public DirectBytes writeByte(int value) {
    return putByte(size, (byte) value);
  }

  ///
  @Override
  public DirectBytes writeByte(byte value) {
    return putByte(size, value);
  }

  ///
  @Override
  public DirectBytes putByte(int offset, int value) {
    return putByte(offset, value);
  }

  ///
  @Override
  public DirectBytes putByte(int offset, byte value) {
    assert address != 0L;
    extend(offset, 1);
    Danger.putByte(address + offset, value);
    return this;
  }

  @Override
  public DirectBytes putByteUnsafe(int offset, byte value) {
    Danger.putByte(address + offset, value);
    return this;
  }

  ///
  @Override
  public short getShort(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 2)) return 0;
    return Danger.getShortUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public short getShortUnsafe(int offset, boolean bigEndian) {
    return Danger.getShortUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public short readShort(boolean bigEndian) throws BufferOverflowException {
    return Danger.getShortUnaligned(null, address + advance(position, 2), bigEndian);
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
    Danger.putShortUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putShortUnsafe(int offset, short value, boolean bigEndian) {
    Danger.putShortUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putCharUnsafe(int offset, char value, boolean bigEndian) {
    Danger.putCharUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public char getChar(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 2)) return 0;
    return Danger.getCharUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public char getCharUnsafe(int offset, boolean bigEndian) {
    return Danger.getCharUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public char readChar(boolean bigEndian) throws BufferOverflowException {
    return Danger.getCharUnaligned(null, address + advance(position, 2), bigEndian);
  }

  @Override
  public DirectBytes writeChar(char value, boolean bigEndian) {
    assert address != 0L;
    extend(size, 2);
    Danger.putCharUnaligned(null, address + size, value, bigEndian);
    return this;
  }

  ///
  @Override
  public DirectBytes putChar(int offset, char value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 2);
    Danger.putCharUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public int getInt(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 4)) return 0;
    return Danger.getIntUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public int getIntUnsafe(int offset, boolean bigEndian) {
    return Danger.getIntUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public int readInt(boolean bigEndian) throws BufferOverflowException {
    return Danger.getIntUnaligned(null, address + advance(position, 4), bigEndian);
  }

  @Override
  public DirectBytes writeInt(int value, boolean bigEndian) {
    return putInt(size, value, bigEndian);
  }

  ///
  @Override
  public DirectBytes putInt(int offset, int value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 4);
    Danger.putIntUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putIntUnsafe(int offset, int value, boolean bigEndian) {
    Danger.putIntUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  ///
  @Override
  public long getLong(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 8)) return 0;
    return Danger.getLongUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public long getLongUnsafe(int offset, boolean bigEndian) {
    return Danger.getLongUnaligned(null, address + offset, bigEndian);
  }

  @Override
  public long readLong(boolean bigEndian) throws BufferOverflowException {
    return Danger.getLongUnaligned(null, address + advance(position, 8), bigEndian);
  }

  @Override
  public float getFloat(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 4)) return 0;
    return Float.intBitsToFloat(Danger.getIntUnaligned(null, address + offset, bigEndian));
  }

  @Override
  public float getFloatUnsafe(int offset, boolean bigEndian) {
    return Float.intBitsToFloat(Danger.getIntUnaligned(null, address + offset, bigEndian));
  }

  @Override
  public float readFloat(boolean bigEndian) throws BufferOverflowException {
    return Float.intBitsToFloat(
        Danger.getIntUnaligned(null, address + advance(position, 4), bigEndian));
  }

  @Override
  public double getDouble(int offset, boolean bigEndian) {
    assert address != 0L;
    if (outOfBounds(offset, 8)) return 0.0;
    return Double.longBitsToDouble(Danger.getLongUnaligned(null, address + offset, bigEndian));
  }

  @Override
  public double getDoubleUnsafe(int offset, boolean bigEndian) {
    return Double.longBitsToDouble(Danger.getLongUnaligned(null, address + offset, bigEndian));
  }

  @Override
  public double readDouble(boolean bigEndian) throws BufferOverflowException {
    return Double.longBitsToDouble(
        Danger.getLongUnaligned(null, address + advance(position, 8), bigEndian));
  }

  @Override
  public DirectBytes writeLong(long value, boolean bigEndian) {
    return putLong(size, value, bigEndian);
  }

  ///
  @Override
  public DirectBytes putLong(int offset, long value, boolean bigEndian) {
    assert address != 0L;
    extend(offset, 8);
    Danger.putLongUnaligned(null, address + offset, value, bigEndian);
    return this;
  }

  @Override
  public DirectBytes putLongUnsafe(int offset, long value, boolean bigEndian) {
    Danger.putLongUnaligned(null, address + offset, value, bigEndian);
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
    Danger.putIntUnaligned(null, address + offset, Float.floatToIntBits(value), bigEndian);
    return this;
  }

  @Override
  public DirectBytes putFloatUnsafe(int offset, float value, boolean bigEndian) {
    Danger.putIntUnaligned(null, address + offset, Float.floatToIntBits(value), bigEndian);
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
    Danger.putLongUnaligned(null, address + offset, Double.doubleToLongBits(value), bigEndian);
    return this;
  }

  @Override
  public DirectBytes putDoubleUnsafe(int offset, double value, boolean bigEndian) {
    Danger.putLongUnaligned(null, address + offset, Double.doubleToLongBits(value), bigEndian);
    return this;
  }

  @Override
  public String readString(int length) {
    final var offset = advance(position, length);
    final var bytes = new byte[length];
    getBytes(offset, bytes, 0, length);
    final var str = new String(bytes, StandardCharsets.UTF_8);
    position += length;
    return str;
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

  ///
  @Override
  public DirectBytes writeString(String value) {
    return putString(size, value);
  }

  ///
  @Override
  public DirectBytes putString(int offset, String value) {
    final var bytes = Danger.getBytes(value);
    extend(offset, bytes.length);
    putBytes(offset, bytes, 0, bytes.length);
    return this;
  }

  ///
  @Override
  public void getBytes(int offset, byte[] value, int valueOffset, int length) {
    if (outOfBounds(offset, length)) return;
    Danger.copyMemory(null, address + offset, value, Danger.BYTE_BASE + valueOffset, length);
  }

  ///
  @Override
  public DirectBytes putBytes(int offset, byte[] value, int valueOffset, int length) {
    extend(offset, length);
    Danger.copyMemory(value, Danger.BYTE_BASE + valueOffset, null, address + offset, length);
    return this;
  }

  ///
  @Override
  public DirectBytes putBuffer(int offset, Bytes bytes) {
    assert address != 0L;
    extend(offset, bytes.size());
    Danger.copyMemory(bytes.address(), address + offset, bytes.size());
    return this;
  }

  ///
  @Override
  public DirectBytes putBuffer(int offset, Bytes bytes, int bufferOffset, int length) {
    assert address != 0L;
    extend(offset, length);
    Danger.copyMemory(bytes.address() + bufferOffset, address + offset, length);
    return this;
  }

  ///
  @Override
  public DirectBytes putUnsafe(int offset, long srcAddress, int length) {
    assert this.address != 0L;
    assert srcAddress != 0L;
    extend(offset, length);
    Danger.copyMemory(srcAddress, this.address + offset, length);
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
    return "Buffer{" + "address="
        + address + ", size="
        + size + ", capacity="
        + capacity + ", owned="
        + owned + '}';
  }
}
