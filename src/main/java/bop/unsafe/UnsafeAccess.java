/*
 * Copyright 2014 Higher Frequency Trading http://www.higherfrequencytrading.com
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package bop.unsafe;

import bop.bit.Primitives;
import java.lang.reflect.Field;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import jdk.internal.misc.Unsafe;

public class UnsafeAccess extends Access<Object> {
  public static final UnsafeAccess INSTANCE;
  public static final Unsafe UNSAFE;
  public static final long BYTE_BASE;
  public static final long BYTE_BUFFER_ADDRESS_OFFSET;
  public static final long BASE_OFFSET;
  public static final long BOOLEAN_BASE;
  public static final long CHAR_BASE;
  public static final long SHORT_BASE;
  public static final long INT_BASE;
  public static final long LONG_BASE;
  public static final byte TRUE_BYTE_VALUE;
  public static final byte FALSE_BYTE_VALUE;
  public static final long STRING_VALUE_OFFSET;
  public static final Field STRING_VALUE_FIELD;
  public static final Access<Object> INSTANCE_NON_NATIVE;
  // for test only
  static final UnsafeAccess OLD_INSTANCE = ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN
      ? new OldUnsafeAccessLittleEndian()
      : new OldUnsafeAccessBigEndian();

  static Field findField(final Class<?> clazz, final String name) {
    if (clazz == null) {
      throw new RuntimeException("field: " + name + " is null");
    }
    Field field = null;
    try {
      field = clazz.getDeclaredField(name);
    } catch (NoSuchFieldException e) {
      // Ignore
    }
    if (field != null) {
      return field;
    }
    return findField(clazz.getSuperclass(), name);
  }

  static {
    try {
      UNSAFE = Unsafe.getUnsafe();

      BOOLEAN_BASE = UNSAFE.arrayBaseOffset(boolean[].class);
      BYTE_BASE = UNSAFE.arrayBaseOffset(byte[].class);
      CHAR_BASE = UNSAFE.arrayBaseOffset(char[].class);
      SHORT_BASE = UNSAFE.arrayBaseOffset(short[].class);
      INT_BASE = UNSAFE.arrayBaseOffset(int[].class);
      LONG_BASE = UNSAFE.arrayBaseOffset(long[].class);

      TRUE_BYTE_VALUE = (byte) UNSAFE.getInt(new boolean[] {true, true, true, true}, BOOLEAN_BASE);
      FALSE_BYTE_VALUE =
          (byte) UNSAFE.getInt(new boolean[] {false, false, false, false}, BOOLEAN_BASE);
    } catch (final Exception e) {
      throw new AssertionError(e);
    }

    try {
      //      UNSAFE.arrayBaseOffset(byte[].class);
      //      final var cls = MemorySegment.ofArray(new byte[0]).getClass();
      //      final var field = findField(cls, "base");
      //      BASE_OFFSET = UNSAFE.objectFieldOffset(field);
      BASE_OFFSET = UNSAFE.arrayBaseOffset(byte[].class);
    } catch (final Throwable e) {
      throw new AssertionError(e);
    }

    try {
      final var field = findField(Class.forName("java.nio.Buffer"), "address");
      BYTE_BUFFER_ADDRESS_OFFSET = UNSAFE.objectFieldOffset(field);
    } catch (final Throwable e) {
      throw new AssertionError(e);
    }

    boolean hasGetByte = true;
    try {
      UNSAFE.getByte(new byte[1], BYTE_BASE);
    } catch (final Throwable ignore) {
      // Unsafe in pre-Nougat Android does not have getByte(), fall back to workround
      hasGetByte = false;
    }

    INSTANCE = hasGetByte ? new UnsafeAccess() : OLD_INSTANCE;
    INSTANCE_NON_NATIVE = newDefaultReverseAccess(INSTANCE);

    try {
      final Field valueField = String.class.getDeclaredField("value");
      STRING_VALUE_FIELD = valueField;
      STRING_VALUE_OFFSET = UnsafeAccess.UNSAFE.objectFieldOffset(valueField);
    } catch (final NoSuchFieldException e) {
      throw new AssertionError(e);
    }
  }

  private UnsafeAccess() {}

  @Override
  protected Access<Object> reverseAccess() {
    return INSTANCE_NON_NATIVE;
  }

  public long getAddress(final ByteBuffer buffer) {
    return UNSAFE.getLong(buffer, BYTE_BUFFER_ADDRESS_OFFSET);
  }

  public long getLong(Object input, long offset) {
    return UNSAFE.getLong(input, offset);
  }

  public long getUnsignedInt(Object input, long offset) {
    return Primitives.unsignedInt(getInt(input, offset));
  }

  public int getInt(Object input, long offset) {
    return UNSAFE.getInt(input, offset);
  }

  public int getUnsignedShort(Object input, long offset) {
    return Primitives.unsignedShort(getShort(input, offset));
  }

  public int getShort(Object input, long offset) {
    return UNSAFE.getShort(input, offset);
  }

  public int getUnsignedByte(Object input, long offset) {
    return Primitives.unsignedByte(getByte(input, offset));
  }

  public int getByte(Object input, long offset) {
    return UNSAFE.getByte(input, offset);
  }

  public byte getByte(long address) {
    return UNSAFE.getByte(address);
  }

  public ByteOrder byteOrder(Object input) {
    return ByteOrder.nativeOrder();
  }

  public byte[] getBytes(String s) {
    return (byte[]) UNSAFE.getReference(s, STRING_VALUE_OFFSET);
  }

  public byte[] getBytesByReflection(String s) {
    try {
      return (byte[]) STRING_VALUE_FIELD.get(s);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  private static class OldUnsafeAccessLittleEndian extends UnsafeAccess {
    @Override
    public int getShort(final Object input, final long offset) {
      return UNSAFE.getInt(input, offset - 2) >> 16;
    }

    @Override
    public int getByte(final Object input, final long offset) {
      return UNSAFE.getInt(input, offset - 3) >> 24;
    }
  }

  private static class OldUnsafeAccessBigEndian extends UnsafeAccess {
    @Override
    public int getShort(final Object input, final long offset) {
      return (short) UNSAFE.getInt(input, offset - 2);
    }

    @Override
    public int getByte(final Object input, final long offset) {
      return (byte) UNSAFE.getInt(input, offset - 3);
    }
  }
}
