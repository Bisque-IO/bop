package bop.io;

import java.io.OutputStream;

public interface BytesMut extends Bytes {
  OutputStream asOutputStream();

  ///
  void extend(int offset, int size);

  BytesMut writeByte(int value);

  BytesMut writeByte(byte value);

  BytesMut putByte(int offset, int value);

  BytesMut putByte(int offset, byte value);

  default BytesMut putByteUnsafe(int offset, int value) {
    return putByteUnsafe(offset, (byte) value);
  }

  BytesMut putByteUnsafe(int offset, byte value);

  default BytesMut writeShort(int value) {
    return writeShort((short)value, IS_BIG_ENDIAN);
  }

  default BytesMut writeShortLE(int value) {
    return writeShort((short)value, false);
  }

  default BytesMut writeShortBE(int value) {
    return writeShort((short)value, true);
  }

  BytesMut writeShort(int value, boolean bigEndian);

  default BytesMut putShort(int offset, int value) {
    return putShort(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putShortLE(int offset, int value) {
    return putShort(offset, value, false);
  }

  default BytesMut putShortBE(int offset, int value) {
    return putShort(offset, value, true);
  }

  BytesMut putShort(int offset, int value, boolean bigEndian);

  default BytesMut writeShort(short value) {
    return writeShort(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeShortLE(short value) {
    return writeShort(value, false);
  }

  default BytesMut writeShortBE(short value) {
    return writeShort(value, true);
  }

  BytesMut writeShort(short value, boolean bigEndian);

  default BytesMut putShort(int offset, short value) {
    return putShort(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putShortLE(int offset, short value) {
    return putShort(offset, value, false);
  }

  default BytesMut putShortBE(int offset, short value) {
    return putShort(offset, value, true);
  }

  BytesMut putShort(int offset, short value, boolean bigEndian);

  default BytesMut putShortUnsafe(int offset, int value) {
    return putShortUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putShortUnsafeLE(int offset, int value) {
    return putShortUnsafe(offset, value, false);
  }

  default BytesMut putShortUnsafeBE(int offset, int value) {
    return putShortUnsafe(offset, value, true);
  }

  default BytesMut putShortUnsafe(int offset, int value, boolean bigEndian) {
    return putShortUnsafe(offset, (short)value, bigEndian);
  }

  default BytesMut putShortUnsafe(int offset, short value) {
    return putShortUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putShortUnsafeLE(int offset, short value) {
    return putShortUnsafe(offset, value, false);
  }

  default BytesMut putShortUnsafeBE(int offset, short value) {
    return putShortUnsafe(offset, value, true);
  }

  BytesMut putShortUnsafe(int offset, short value, boolean bigEndian);

  default BytesMut writeChar(int value) {
    return writeChar(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeCharLE(int value) {
    return writeChar(value, false);
  }

  default BytesMut writeCharBE(int value) {
    return writeChar(value, true);
  }

  default BytesMut writeChar(int value, boolean bigEndian) {
    return writeChar((char)value, bigEndian);
  }

  default BytesMut putChar(int offset, int value) {
    return putChar(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putCharLE(int offset, int value) {
    return putChar(offset, value, false);
  }

  default BytesMut putCharBE(int offset, int value) {
    return putChar(offset, value, true);
  }

  default BytesMut putChar(int offset, int value, boolean bigEndian) {
    return putChar((char)offset, (char)value, bigEndian);
  }

  default BytesMut writeChar(char value) {
    return writeChar(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeCharLE(char value) {
    return writeChar(value, false);
  }

  default BytesMut writeCharBE(char value) {
    return writeChar(value, true);
  }

  BytesMut writeChar(char value, boolean bigEndian);

  default BytesMut putChar(int offset, char value) {
    return putChar(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putCharLE(int offset, char value) {
    return putChar(offset, value, false);
  }

  default BytesMut putCharBE(int offset, char value) {
    return putChar(offset, value, true);
  }

  BytesMut putChar(int offset, char value, boolean bigEndian);

  default BytesMut putCharUnsafe(int offset, int value) {
    return putCharUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putCharUnsafeLE(int offset, int value) {
    return putCharUnsafe(offset, value, false);
  }

  default BytesMut putCharUnsafeBE(int offset, int value) {
    return putCharUnsafe(offset, value, true);
  }

  default BytesMut putCharUnsafe(int offset, int value, boolean bigEndian) {
    return putCharUnsafe(offset, (char)value, bigEndian);
  }

  default BytesMut putCharUnsafe(int offset, char value) {
    return putCharUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putCharUnsafeLE(int offset, char value) {
    return putCharUnsafe(offset, value, false);
  }

  default BytesMut putCharUnsafeBE(int offset, char value) {
    return putCharUnsafe(offset, value, true);
  }

  BytesMut putCharUnsafe(int offset, char value, boolean bigEndian);

  default BytesMut writeInt(int value) {
    return writeInt(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeIntLE(int value) {
    return writeInt(value, false);
  }

  default BytesMut writeIntBE(int value) {
    return writeInt(value, true);
  }

  BytesMut writeInt(int value, boolean bigEndian);

  default BytesMut putInt(int offset, int value) {
    return putInt(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putIntLE(int offset, int value) {
    return putInt(offset, value, false);
  }

  default BytesMut putIntBE(int offset, int value) {
    return putInt(offset, value, true);
  }

  BytesMut putInt(int offset, int value, boolean bigEndian);

  default BytesMut putIntUnsafe(int offset, int value) {
    return putIntUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putIntUnsafeLE(int offset, int value) {
    return putIntUnsafe(offset, value, false);
  }

  default BytesMut putIntUnsafeBE(int offset, int value) {
    return putIntUnsafe(offset, value, true);
  }

  BytesMut putIntUnsafe(int offset, int value, boolean bigEndian);

  default BytesMut writeLong(long value) {
    return writeLong(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeLongLE(long value) {
    return writeLong(value, false);
  }

  default BytesMut writeLongBE(long value) {
    return writeLong(value, true);
  }

  BytesMut writeLong(long value, boolean bigEndian);

  default BytesMut putLong(int offset, long value) {
    return putLong(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putLongLE(int offset, long value) {
    return putLong(offset, value, false);
  }

  default BytesMut putLongBE(int offset, long value) {
    return putLong(offset, value, true);
  }

  BytesMut putLong(int offset, long value, boolean bigEndian);

  default BytesMut putLongUnsafe(int offset, long value) {
    return putLongUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putLongUnsafeLE(int offset, long value) {
    return putLongUnsafe(offset, value, false);
  }

  default BytesMut putLongUnsafeBE(int offset, long value) {
    return putLongUnsafe(offset, value, true);
  }

  BytesMut putLongUnsafe(int offset, long value, boolean bigEndian);

  default BytesMut writeFloat(float value) {
    return writeFloat(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeFloatLE(float value) {
    return writeFloat(value, false);
  }

  default BytesMut writeFloatBE(float value) {
    return writeFloat(value, true);
  }

  BytesMut writeFloat(float value, boolean bigEndian);

  default BytesMut putFloat(int offset, float value) {
    return putFloat(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putFloatLE(int offset, float value) {
    return putFloat(offset, value, false);
  }

  default BytesMut putFloatBE(int offset, float value) {
    return putFloat(offset, value, true);
  }

  BytesMut putFloat(int offset, float value, boolean bigEndian);

  default BytesMut putFloatUnsafe(int offset, float value) {
    return putFloatUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putFloatUnsafeLE(int offset, float value) {
    return putFloatUnsafe(offset, value, false);
  }

  default BytesMut putFloatUnsafeBE(int offset, float value) {
    return putFloatUnsafe(offset, value, true);
  }

  BytesMut putFloatUnsafe(int offset, float value, boolean bigEndian);

  default BytesMut writeDouble(double value) {
    return writeDouble(value, IS_BIG_ENDIAN);
  }

  default BytesMut writeDoubleLE(double value) {
    return writeDouble(value, false);
  }

  default BytesMut writeDoubleBE(double value) {
    return writeDouble(value, true);
  }

  BytesMut writeDouble(double value, boolean bigEndian);

  default BytesMut putDouble(int offset, double value) {
    return putDouble(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putDoubleLE(int offset, double value) {
    return putDouble(offset, value, false);
  }

  default BytesMut putDoubleBE(int offset, double value) {
    return putDouble(offset, value, true);
  }

  BytesMut putDouble(int offset, double value, boolean bigEndian);

  default BytesMut putDoubleUnsafe(int offset, double value) {
    return putDoubleUnsafe(offset, value, IS_BIG_ENDIAN);
  }

  default BytesMut putDoubleUnsafeLE(int offset, double value) {
    return putDoubleUnsafe(offset, value, false);
  }

  default BytesMut putDoubleUnsafeBE(int offset, double value) {
    return putDoubleUnsafe(offset, value, true);
  }

  BytesMut putDoubleUnsafe(int offset, double value, boolean bigEndian);

  BytesMut writeString(String value);

  BytesMut putString(int offset, String value);

  BytesMut putBytes(int offset, byte[] value, int valueOffset, int length);

  BytesMut putBuffer(int offset, Bytes bytes);

  BytesMut putBuffer(int offset, Bytes bytes, int bufferOffset, int length);

  BytesMut putUnsafe(int offset, long srcAddress, int length);
}
