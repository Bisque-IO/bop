package bop.io;

import bop.unsafe.Danger;

import java.io.IOException;
import java.io.InputStream;
import java.nio.BufferOverflowException;
import java.nio.charset.Charset;

public interface Bytes extends AutoCloseable {
  boolean IS_BIG_ENDIAN = Danger.isBigEndian();
  boolean IS_LITTLE_ENDIAN = !Danger.isBigEndian();
  boolean UNALIGNED_ACCESS = Danger.unalignedAccess();

  static Bytes allocate(int size) {
    return DirectBytes.allocateDirect(size);
  }

  default boolean isSlice() {
    return getRootAddress() != getAddress();
  }

  InputStream asInputStream();

  Object getBase();

  int getRemaining();

  int getAvailable();

  long getRootAddress();

  Bytes reset();

  long getAddress();

  int getPosition();

  Bytes setPosition(int newPosition);

  Bytes skip(int length);

  int getSize();

  Bytes setSize(int newSize);

  int getCapacity();

  boolean isOwned();

  int sliceCount();

  void close();

  void resize(int capacity);

  boolean outOfBounds(int offset, int length);

  int peek() throws BufferOverflowException;

  byte getByte(int offset);

  int getUByte(int offset);

  byte getByteVolatile(int offset);

  byte readByte() throws BufferOverflowException;

  default int readUByte() throws BufferOverflowException {
    return (int)readByte() & 0xFF;
  }

  byte getByteUnsafe(int offset);

  default short getShort(int offset) {
    return getShort(offset, IS_BIG_ENDIAN);
  }

  default short getShortLE(int offset) {
    return getShort(offset, false);
  }

  default short getShortBE(int offset) {
    return getShort(offset, true);
  }

  short getShort(int offset, boolean bigEndian);

  default short getShortUnsafe(int offset) {
    return getShortUnsafe(offset, IS_BIG_ENDIAN);
  }

  default short getShortUnsafeLE(int offset) {
    return getShortUnsafe(offset, false);
  }

  default short getShortUnsafeBE(int offset) {
    return getShortUnsafe(offset, true);
  }

  short getShortUnsafe(int offset, boolean bigEndian);

  default short readShort() throws BufferOverflowException {
    return readShort(IS_BIG_ENDIAN);
  }

  default short readShortLE() throws BufferOverflowException {
    return readShort(false);
  }

  default short readShortBE() throws BufferOverflowException {
    return readShort(true);
  }

  short readShort(boolean bigEndian) throws BufferOverflowException;

  default char getChar(int offset) {
    return getChar(offset, IS_BIG_ENDIAN);
  }

  default char getCharLE(int offset) {
    return getChar(offset, false);
  }

  default char getCharBE(int offset) {
    return getChar(offset, true);
  }

  char getChar(int offset, boolean bigEndian);

  default char getCharUnsafe(int offset) {
    return getCharUnsafe(offset, IS_BIG_ENDIAN);
  }

  default char getCharUnsafeLE(int offset) {
    return getCharUnsafe(offset, false);
  }

  default char getCharUnsafeBE(int offset) {
    return getCharUnsafe(offset, true);
  }

  char getCharUnsafe(int offset, boolean bigEndian);

  default char readChar() throws BufferOverflowException {
    return readChar(IS_BIG_ENDIAN);
  }

  default char readCharLE() throws BufferOverflowException {
    return readChar(false);
  }

  default char readCharBE() throws BufferOverflowException {
    return readChar(true);
  }

  char readChar(boolean bigEndian) throws BufferOverflowException;

  default int getInt(int offset) {
    return getInt(offset, IS_BIG_ENDIAN);
  }

  default int getIntLE(int offset) {
    return getInt(offset, false);
  }

  default int getIntBE(int offset) {
    return getInt(offset, true);
  }

  int getInt(int offset, boolean bigEndian);

  default int getIntUnsafe(int offset) {
    return getIntUnsafe(offset, IS_BIG_ENDIAN);
  }

  default int getIntUnsafeLE(int offset) {
    return getIntUnsafe(offset, false);
  }

  default int getIntUnsafeBE(int offset) {
    return getIntUnsafe(offset, true);
  }

  int getIntUnsafe(int offset, boolean bigEndian);

  int getIntLE(int offset, int length);

  int readIntLE(int length) throws BufferOverflowException;

  int getInt24LE(int offset);

  int readInt24LE() throws BufferOverflowException;

  int getIntBE(int offset, int length);

  int readIntBE(int length) throws BufferOverflowException;

  int getInt24BE(int offset);

  int readInt24BE() throws BufferOverflowException;

  default int readInt() throws BufferOverflowException {
    return readInt(IS_BIG_ENDIAN);
  }

  default int readIntLE() throws BufferOverflowException {
    return readInt(false);
  }

  default int readIntBE() throws BufferOverflowException {
    return readInt(true);
  }

  int readInt(boolean bigEndian) throws BufferOverflowException;

  default long getLong(int offset) {
    return getLong(offset, IS_BIG_ENDIAN);
  }

  default long getLongLE(int offset) {
    return getLong(offset, false);
  }

  default long getLongBE(int offset) {
    return getLong(offset, true);
  }

  long getLong(int offset, boolean bigEndian);

  default long getLongUnsafe(int offset) {
    return getLongUnsafe(offset, IS_BIG_ENDIAN);
  }

  default long getLongUnsafeLE(int offset) {
    return getLongUnsafe(offset, false);
  }

  default long getLongUnsafeBE(int offset) {
    return getLongUnsafe(offset, true);
  }

  long getLongUnsafe(int offset, boolean bigEndian);

  long getLongLE(int offset, int length);

  long readLongLE(int length) throws BufferOverflowException;

  long getLong24LE(int offset);

  long getLong40LE(int offset);

  long getLong48LE(int offset);

  long getLong56LE(int offset);

  long readLong24LE() throws BufferOverflowException;

  long readLong40LE() throws BufferOverflowException;

  long readLong48LE() throws BufferOverflowException;

  long readLong56LE() throws BufferOverflowException;

  long getLongBE(int offset, int length);

  long readLongBE(int length) throws BufferOverflowException;

  long getLong24BE(int offset);

  long getLong40BE(int offset);

  long getLong48BE(int offset);

  long getLong56BE(int offset);

  long readLong24BE() throws BufferOverflowException;

  long readLong40BE() throws BufferOverflowException;

  long readLong48BE() throws BufferOverflowException;

  long readLong56BE() throws BufferOverflowException;

  default long readLong() throws BufferOverflowException {
    return readLong(IS_BIG_ENDIAN);
  }

  default long readLongLE() throws BufferOverflowException {
    return readLong(false);
  }

  default long readLongBE() throws BufferOverflowException {
    return readLong(true);
  }

  long readLong(boolean bigEndian) throws BufferOverflowException;

  default float getFloat(int offset) {
    return getFloat(offset, IS_BIG_ENDIAN);
  }

  default float getFloatLE(int offset) {
    return getFloat(offset, false);
  }

  default float getFloatBE(int offset) {
    return getFloat(offset, true);
  }

  float getFloat(int offset, boolean bigEndian);

  default float getFloatUnsafe(int offset) {
    return getFloatUnsafe(offset, IS_BIG_ENDIAN);
  }

  default float getFloatUnsafeLE(int offset) {
    return getFloatUnsafe(offset, false);
  }

  default float getFloatUnsafeBE(int offset) {
    return getFloatUnsafe(offset, true);
  }

  float getFloatUnsafe(int offset, boolean bigEndian);

  default float readFloat() throws BufferOverflowException {
    return readFloat(IS_BIG_ENDIAN);
  }

  default float readFloatLE() throws BufferOverflowException {
    return readFloat(false);
  }

  default float readFloatBE() throws BufferOverflowException {
    return readFloat(true);
  }

  float readFloat(boolean bigEndian) throws BufferOverflowException;

  default double getDouble(int offset) {
    return getDouble(offset, IS_BIG_ENDIAN);
  }

  default double getDoubleLE(int offset) {
    return getDouble(offset, false);
  }

  default double getDoubleBE(int offset) {
    return getDouble(offset, true);
  }

  double getDouble(int offset, boolean bigEndian);

  default double getDoubleUnsafe(int offset) {
    return getDoubleUnsafe(offset, IS_LITTLE_ENDIAN);
  }

  default double getDoubleUnsafeLE(int offset) {
    return getDoubleUnsafe(offset, false);
  }

  default double getDoubleUnsafeBE(int offset) {
    return getDoubleUnsafe(offset, true);
  }

  double getDoubleUnsafe(int offset, boolean bigEndian);

  default double readDouble() throws BufferOverflowException {
    return readDouble(IS_BIG_ENDIAN);
  }

  default double readDoubleLE() throws BufferOverflowException {
    return readDouble(false);
  }

  default double readDoubleBE() throws BufferOverflowException {
    return readDouble(true);
  }

  double readDouble(boolean bigEndian) throws BufferOverflowException;

  String readString(int length);

  String getString(int offset, int length);

  String getString(int offset, int length, Charset charset);

  String readCString();

  void getBytes(int offset, byte[] value, int valueOffset, int length);

  byte[] readBytes(int length);

  default int readBytes(byte[] bytes) {
    return readBytes(bytes, 0, bytes.length);
  }

  int readBytes(byte[] bytes, int offset, int length);

  int indexOf(byte b, int offset, int length);

  Bytes slice(int offset, int length);

  BytesMut asMut();
}
