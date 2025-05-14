package bop.io;

import bop.unsafe.Danger;
import java.io.InputStream;
import java.nio.BufferOverflowException;
import java.nio.charset.Charset;

public interface Bytes extends AutoCloseable {
  boolean IS_BIG_ENDIAN = Danger.isBigEndian();
  boolean IS_LITTLE_ENDIAN = !Danger.isBigEndian();
  boolean UNALIGNED_ACCESS = Danger.unalignedAccess();

  static Bytes allocate(int size) {
    return DirectBytes.allocate(size);
  }

  default boolean isSlice() {
    return rootAddress() != address();
  }

  InputStream asInputStream();

  long rootAddress();

  long address();

  int position();

  Bytes position(int newPosition);

  Bytes skip(int length);

  int size();

  Bytes size(int newSize);

  int capacity();

  boolean owned();

  int sliceCount();

  void close();

  void resize(int capacity);

  boolean outOfBounds(int offset, int length);

  byte getByte(int offset);

  byte getByteVolatile(int offset);

  byte readByte() throws BufferOverflowException;

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

  void getBytes(int offset, byte[] value, int valueOffset, int length);

  Bytes slice(int offset, int length);

  BytesMut asMut();
}
