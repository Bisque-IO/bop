package bop.unsafe;

import bop.c.Memory;
import java.lang.reflect.Field;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.security.ProtectionDomain;

import jdk.internal.misc.Unsafe;

public class Danger {
  public static final Unsafe ZONE;
  public static final int BYTE_BASE;
  public static final long STRING_VALUE_OFFSET;
  public static final long STRING_HASH_OFFSET;
  public static final long STRING_HASH_IS_ZERO_OFFSET;
  public static final long OBJECT_ARRAY_BASE;
  public static final long OBJECT_ARRAY_SHIFT_FOR_SCALE;
  public static final long BUFFER_ADDRESS_OFFSET;

  static {
    try {
      ZONE = Unsafe.getUnsafe();
      BYTE_BASE = ZONE.arrayBaseOffset(byte[].class);

      {
        final Field field = String.class.getDeclaredField("value");
        field.setAccessible(true);
        STRING_VALUE_OFFSET = ZONE.objectFieldOffset(field);
      }

      {
        final Field field = String.class.getDeclaredField("hash");
        field.setAccessible(true);
        STRING_HASH_OFFSET = ZONE.objectFieldOffset(field);
      }

      {
        final Field field = String.class.getDeclaredField("hashIsZero");
        field.setAccessible(true);
        STRING_HASH_IS_ZERO_OFFSET = ZONE.objectFieldOffset(field);
      }

      OBJECT_ARRAY_BASE = ZONE.arrayBaseOffset(Object[].class);
      OBJECT_ARRAY_SHIFT_FOR_SCALE = calculateShiftForScale(ZONE.arrayIndexScale(Object[].class));

      BUFFER_ADDRESS_OFFSET = ZONE.objectFieldOffset(Buffer.class.getDeclaredField("address"));
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void unsafeSetString(String s, byte[] value) {
    putReference(s, STRING_VALUE_OFFSET, value);
    putInt(s, STRING_HASH_OFFSET, 0);
    putBoolean(s, STRING_HASH_IS_ZERO_OFFSET, false);
  }

  /**
   * Compute buffer offset based on the given sequence and the mask.
   *
   * @param sequence to compute the offset from.
   * @param mask to apply.
   * @return buffer offset.
   */
  public static long sequenceToBufferOffset(final long sequence, final long mask) {
    return OBJECT_ARRAY_BASE + ((sequence & mask) << OBJECT_ARRAY_SHIFT_FOR_SCALE);
  }

  /**
   * Calculate the shift value to scale a number based on how refs are compressed or not.
   *
   * @param scale of the number reported by Unsafe.
   * @return how many times the number needs to be shifted to the left.
   */
  public static int calculateShiftForScale(final int scale) {
    if (4 == scale) {
      return 2;
    } else if (8 == scale) {
      return 3;
    }

    throw new IllegalArgumentException("unknown pointer size for scale=" + scale);
  }

  public static long getAddress(final Buffer buffer) {
    return ZONE.getLong(buffer, BUFFER_ADDRESS_OFFSET);
  }

  public static byte[] getBytes(String s) {
    return (byte[]) ZONE.getReference(s, STRING_VALUE_OFFSET);
  }

  public static void setBytes(String s, byte[] bytes) {
    ZONE.putReference(s, STRING_VALUE_OFFSET, bytes);
    ZONE.putInt(s, STRING_HASH_OFFSET, 0);
    ZONE.putBoolean(s, STRING_HASH_IS_ZERO_OFFSET, false);
  }

  public static long alloc(final long size) {
    return Memory.alloc(size);
  }

  public static long realloc(final long address, final long size) {
    return Memory.realloc(address, size);
  }

  public static void dealloc(final long address) {
    Memory.dealloc(address);
  }

  public static void put(final long address, final long value) {
    ZONE.putLong(address, value);
  }


  public static int getInt(Object o, long offset) {
    return ZONE.getInt(o, offset);
  }

  public static void putInt(Object o, long offset, int x) {
    ZONE.putInt(o, offset, x);
  }

  public static Object getReference(Object o, long offset) {
    return ZONE.getReference(o, offset);
  }

  public static void putReference(Object o, long offset, Object x) {
    ZONE.putReference(o, offset, x);
  }

  public static boolean getBoolean(Object o, long offset) {
    return ZONE.getBoolean(o, offset);
  }

  public static void putBoolean(Object o, long offset, boolean x) {
    ZONE.putBoolean(o, offset, x);
  }

  public static byte getByte(Object o, long offset) {
    return ZONE.getByte(o, offset);
  }

  public static void putByte(Object o, long offset, byte x) {
    ZONE.putByte(o, offset, x);
  }

  public static short getShort(Object o, long offset) {
    return ZONE.getShort(o, offset);
  }

  public static void putShort(Object o, long offset, short x) {
    ZONE.putShort(o, offset, x);
  }

  public static char getChar(Object o, long offset) {
    return ZONE.getChar(o, offset);
  }

  public static void putChar(Object o, long offset, char x) {
    ZONE.putChar(o, offset, x);
  }

  public static long getLong(final long address) {
    return ZONE.getLong(address);
  }

  public static long getLong(Object o, long offset) {
    return ZONE.getLong(o, offset);
  }

  public static void putLong(Object o, long offset, long x) {
    ZONE.putLong(o, offset, x);
  }

  public static float getFloat(Object o, long offset) {
    return ZONE.getFloat(o, offset);
  }

  public static void putFloat(Object o, long offset, float x) {
    ZONE.putFloat(o, offset, x);
  }

  public static double getDouble(Object o, long offset) {
    return ZONE.getDouble(o, offset);
  }

  public static void putDouble(Object o, long offset, double x) {
    ZONE.putDouble(o, offset, x);
  }

  public static long getAddress(Object o, long offset) {
    return ZONE.getAddress(o, offset);
  }

  public static void putAddress(Object o, long offset, long x) {
    ZONE.putAddress(o, offset, x);
  }

  public static Object getUncompressedObject(long address) {
    return ZONE.getUncompressedObject(address);
  }

  public static byte getByte(long address) {
    return ZONE.getByte(address);
  }

  public static void putByte(long address, byte x) {
    ZONE.putByte(address, x);
  }

  public static short getShort(long address) {
    return ZONE.getShort(address);
  }

  public static void putShort(long address, short x) {
    ZONE.putShort(address, x);
  }

  public static char getChar(long address) {
    return ZONE.getChar(address);
  }

  public static void putChar(long address, char x) {
    ZONE.putChar(address, x);
  }

  public static int getInt(long address) {
    return ZONE.getInt(address);
  }

  public static void putInt(long address, int x) {
    ZONE.putInt(address, x);
  }

  public static void putLong(long address, long x) {
    ZONE.putLong(address, x);
  }

  public static float getFloat(long address) {
    return ZONE.getFloat(address);
  }

  public static void putFloat(long address, float x) {
    ZONE.putFloat(address, x);
  }

  public static double getDouble(long address) {
    return ZONE.getDouble(address);
  }

  public static void putDouble(long address, double x) {
    ZONE.putDouble(address, x);
  }

  public static long getAddress(long address) {
    return ZONE.getAddress(address);
  }

  public static void putAddress(long address, long x) {
    ZONE.putAddress(address, x);
  }

  public static long allocateMemory(long bytes) {
    return alloc(bytes);
  }

  public static long reallocateMemory(long address, long bytes) {
    return realloc(address, bytes);
  }

  public static void setMemory(Object o, long offset, long bytes, byte value) {
    ZONE.setMemory(o, offset, bytes, value);
  }

  public static void setMemory(long address, long bytes, byte value) {
    ZONE.setMemory(address, bytes, value);
  }

  public static void copyMemory(long srcAddress, long destAddress, long bytes) {
    ZONE.copyMemory(srcAddress, destAddress, bytes);
  }

  public static void copyMemory(
    Object srcBase, long srcOffset, Object destBase, long destOffset, long bytes) {
    ZONE.copyMemory(srcBase, srcOffset, destBase, destOffset, bytes);
  }
  
  public static void copySwapMemory(
    Object srcBase, long srcOffset, Object destBase, long destOffset,
    long bytes, long elemSize) {
    ZONE.copySwapMemory(srcBase, srcOffset, destBase, destOffset, bytes, elemSize);
  }
  
  public static void copySwapMemory(long srcAddress, long destAddress, long bytes, long elemSize) {
    ZONE.copySwapMemory(srcAddress, destAddress, bytes, elemSize);
  }

  public static void freeMemory(long address) {
    dealloc(address);
  }

  public static void writebackMemory(long address, long length) {
    ZONE.writebackMemory(address, length);
  }

  public static long objectFieldOffset(Field f) {
    return ZONE.objectFieldOffset(f);
  }

  public static long objectFieldOffset(Class<?> c, String name) {
    return ZONE.objectFieldOffset(c, name);
  }

  public static long staticFieldOffset(Field f) {
    return ZONE.staticFieldOffset(f);
  }

  public static Object staticFieldBase(Field f) {
    return ZONE.staticFieldBase(f);
  }

  public static boolean shouldBeInitialized(Class<?> c) {
    return ZONE.shouldBeInitialized(c);
  }

  public static void ensureClassInitialized(Class<?> c) {
    ZONE.ensureClassInitialized(c);
  }

  public static int arrayBaseOffset(Class<?> arrayClass) {
    return ZONE.arrayBaseOffset(arrayClass);
  }

  public static int arrayIndexScale(Class<?> arrayClass) {
    return ZONE.arrayIndexScale(arrayClass);
  }

  public static int addressSize() {
    return ZONE.addressSize();
  }

  public static int pageSize() {
    return ZONE.pageSize();
  }

  public static int dataCacheLineFlushSize() {
    return ZONE.dataCacheLineFlushSize();
  }

  public static long dataCacheLineAlignDown(long address) {
    return ZONE.dataCacheLineAlignDown(address);
  }

  public static Class<?> defineClass(
    String name, byte[] b, int off, int len, ClassLoader loader,
    ProtectionDomain protectionDomain) {
    return ZONE.defineClass(name, b, off, len, loader, protectionDomain);
  }

  
  public static Class<?> defineClass0(
    String name, byte[] b, int off, int len, ClassLoader loader,
    ProtectionDomain protectionDomain) {
    return ZONE.defineClass0(name, b, off, len, loader, protectionDomain);
  }

  public static Object allocateInstance(Class<?> cls) throws InstantiationException {
    return ZONE.allocateInstance(cls);
  }

  public static Object allocateUninitializedArray(Class<?> componentType, int length) {
    return ZONE.allocateUninitializedArray(componentType, length);
  }

  public static void throwException(Throwable ee) {
    ZONE.throwException(ee);
  }

  
  public static boolean compareAndSetReference(Object o, long offset, Object expected, Object x) {
    return ZONE.compareAndSetReference(o, offset, expected, x);
  }

  
  public static Object compareAndExchangeReference(Object o, long offset, Object expected, Object x) {
    return ZONE.compareAndExchangeReference(o, offset, expected, x);
  }

  public static Object compareAndExchangeReferenceAcquire(
    Object o, long offset, Object expected,
    Object x) {
    return ZONE.compareAndExchangeReferenceAcquire(o, offset, expected, x);
  }

  public static Object compareAndExchangeReferenceRelease(
    Object o, long offset, Object expected,
    Object x) {
    return ZONE.compareAndExchangeReferenceRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetReferencePlain(Object o, long offset, Object expected, Object x) {
    return ZONE.weakCompareAndSetReferencePlain(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetReferenceAcquire(
    Object o, long offset, Object expected,
    Object x) {
    return ZONE.weakCompareAndSetReferenceAcquire(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetReferenceRelease(
    Object o, long offset, Object expected,
    Object x) {
    return ZONE.weakCompareAndSetReferenceRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetReference(Object o, long offset, Object expected, Object x) {
    return ZONE.weakCompareAndSetReference(o, offset, expected, x);
  }

  public static boolean compareAndSetInt(Object o, long offset, int expected, int x) {
    return ZONE.compareAndSetInt(o, offset, expected, x);
  }

  public static int compareAndExchangeInt(Object o, long offset, int expected, int x) {
    return ZONE.compareAndExchangeInt(o, offset, expected, x);
  }

  public static int compareAndExchangeIntAcquire(Object o, long offset, int expected, int x) {
    return ZONE.compareAndExchangeIntAcquire(o, offset, expected, x);
  }

  public static int compareAndExchangeIntRelease(Object o, long offset, int expected, int x) {
    return ZONE.compareAndExchangeIntRelease(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetIntPlain(Object o, long offset, int expected, int x) {
    return ZONE.weakCompareAndSetIntPlain(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetIntAcquire(Object o, long offset, int expected, int x) {
    return ZONE.weakCompareAndSetIntAcquire(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetIntRelease(Object o, long offset, int expected, int x) {
    return ZONE.weakCompareAndSetIntRelease(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetInt(Object o, long offset, int expected, int x) {
    return ZONE.weakCompareAndSetInt(o, offset, expected, x);
  }

  public static byte compareAndExchangeByte(Object o, long offset, byte expected, byte x) {
    return ZONE.compareAndExchangeByte(o, offset, expected, x);
  }

  public static boolean compareAndSetByte(Object o, long offset, byte expected, byte x) {
    return ZONE.compareAndSetByte(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetByte(Object o, long offset, byte expected, byte x) {
    return ZONE.weakCompareAndSetByte(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetByteAcquire(Object o, long offset, byte expected, byte x) {
    return ZONE.weakCompareAndSetByteAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetByteRelease(Object o, long offset, byte expected, byte x) {
    return ZONE.weakCompareAndSetByteRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetBytePlain(Object o, long offset, byte expected, byte x) {
    return ZONE.weakCompareAndSetBytePlain(o, offset, expected, x);
  }

  
  public static byte compareAndExchangeByteAcquire(Object o, long offset, byte expected, byte x) {
    return ZONE.compareAndExchangeByteAcquire(o, offset, expected, x);
  }

  
  public static byte compareAndExchangeByteRelease(Object o, long offset, byte expected, byte x) {
    return ZONE.compareAndExchangeByteRelease(o, offset, expected, x);
  }

  public static short compareAndExchangeShort(Object o, long offset, short expected, short x) {
    return ZONE.compareAndExchangeShort(o, offset, expected, x);
  }

  public static boolean compareAndSetShort(Object o, long offset, short expected, short x) {
    return ZONE.compareAndSetShort(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetShort(Object o, long offset, short expected, short x) {
    return ZONE.weakCompareAndSetShort(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetShortAcquire(Object o, long offset, short expected, short x) {
    return ZONE.weakCompareAndSetShortAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetShortRelease(Object o, long offset, short expected, short x) {
    return ZONE.weakCompareAndSetShortRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetShortPlain(Object o, long offset, short expected, short x) {
    return ZONE.weakCompareAndSetShortPlain(o, offset, expected, x);
  }

  
  public static short compareAndExchangeShortAcquire(Object o, long offset, short expected, short x) {
    return ZONE.compareAndExchangeShortAcquire(o, offset, expected, x);
  }

  
  public static short compareAndExchangeShortRelease(Object o, long offset, short expected, short x) {
    return ZONE.compareAndExchangeShortRelease(o, offset, expected, x);
  }

  public static boolean compareAndSetChar(Object o, long offset, char expected, char x) {
    return ZONE.compareAndSetChar(o, offset, expected, x);
  }

  public static char compareAndExchangeChar(Object o, long offset, char expected, char x) {
    return ZONE.compareAndExchangeChar(o, offset, expected, x);
  }

  
  public static char compareAndExchangeCharAcquire(Object o, long offset, char expected, char x) {
    return ZONE.compareAndExchangeCharAcquire(o, offset, expected, x);
  }

  
  public static char compareAndExchangeCharRelease(Object o, long offset, char expected, char x) {
    return ZONE.compareAndExchangeCharRelease(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetChar(Object o, long offset, char expected, char x) {
    return ZONE.weakCompareAndSetChar(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetCharAcquire(Object o, long offset, char expected, char x) {
    return ZONE.weakCompareAndSetCharAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetCharRelease(Object o, long offset, char expected, char x) {
    return ZONE.weakCompareAndSetCharRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetCharPlain(Object o, long offset, char expected, char x) {
    return ZONE.weakCompareAndSetCharPlain(o, offset, expected, x);
  }

  
  public static boolean compareAndSetBoolean(Object o, long offset, boolean expected, boolean x) {
    return ZONE.compareAndSetBoolean(o, offset, expected, x);
  }

  
  public static boolean compareAndExchangeBoolean(Object o, long offset, boolean expected, boolean x) {
    return ZONE.compareAndExchangeBoolean(o, offset, expected, x);
  }

  public static boolean compareAndExchangeBooleanAcquire(
    Object o, long offset, boolean expected,
    boolean x) {
    return ZONE.compareAndExchangeBooleanAcquire(o, offset, expected, x);
  }

  public static boolean compareAndExchangeBooleanRelease(
    Object o, long offset, boolean expected,
    boolean x) {
    return ZONE.compareAndExchangeBooleanRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetBoolean(Object o, long offset, boolean expected, boolean x) {
    return ZONE.weakCompareAndSetBoolean(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetBooleanAcquire(
    Object o, long offset, boolean expected,
    boolean x) {
    return ZONE.weakCompareAndSetBooleanAcquire(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetBooleanRelease(
    Object o, long offset, boolean expected,
    boolean x) {
    return ZONE.weakCompareAndSetBooleanRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetBooleanPlain(Object o, long offset, boolean expected, boolean x) {
    return ZONE.weakCompareAndSetBooleanPlain(o, offset, expected, x);
  }

  public static boolean compareAndSetFloat(Object o, long offset, float expected, float x) {
    return ZONE.compareAndSetFloat(o, offset, expected, x);
  }

  public static float compareAndExchangeFloat(Object o, long offset, float expected, float x) {
    return ZONE.compareAndExchangeFloat(o, offset, expected, x);
  }

  
  public static float compareAndExchangeFloatAcquire(Object o, long offset, float expected, float x) {
    return ZONE.compareAndExchangeFloatAcquire(o, offset, expected, x);
  }

  
  public static float compareAndExchangeFloatRelease(Object o, long offset, float expected, float x) {
    return ZONE.compareAndExchangeFloatRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetFloatPlain(Object o, long offset, float expected, float x) {
    return ZONE.weakCompareAndSetFloatPlain(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetFloatAcquire(Object o, long offset, float expected, float x) {
    return ZONE.weakCompareAndSetFloatAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetFloatRelease(Object o, long offset, float expected, float x) {
    return ZONE.weakCompareAndSetFloatRelease(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetFloat(Object o, long offset, float expected, float x) {
    return ZONE.weakCompareAndSetFloat(o, offset, expected, x);
  }

  public static boolean compareAndSetDouble(Object o, long offset, double expected, double x) {
    return ZONE.compareAndSetDouble(o, offset, expected, x);
  }

  
  public static double compareAndExchangeDouble(Object o, long offset, double expected, double x) {
    return ZONE.compareAndExchangeDouble(o, offset, expected, x);
  }

  
  public static double compareAndExchangeDoubleAcquire(Object o, long offset, double expected, double x) {
    return ZONE.compareAndExchangeDoubleAcquire(o, offset, expected, x);
  }

  
  public static double compareAndExchangeDoubleRelease(Object o, long offset, double expected, double x) {
    return ZONE.compareAndExchangeDoubleRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetDoublePlain(Object o, long offset, double expected, double x) {
    return ZONE.weakCompareAndSetDoublePlain(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetDoubleAcquire(Object o, long offset, double expected, double x) {
    return ZONE.weakCompareAndSetDoubleAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetDoubleRelease(Object o, long offset, double expected, double x) {
    return ZONE.weakCompareAndSetDoubleRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetDouble(Object o, long offset, double expected, double x) {
    return ZONE.weakCompareAndSetDouble(o, offset, expected, x);
  }

  public static boolean compareAndSetLong(Object o, long offset, long expected, long x) {
    return ZONE.compareAndSetLong(o, offset, expected, x);
  }

  public static long compareAndExchangeLong(Object o, long offset, long expected, long x) {
    return ZONE.compareAndExchangeLong(o, offset, expected, x);
  }

  
  public static long compareAndExchangeLongAcquire(Object o, long offset, long expected, long x) {
    return ZONE.compareAndExchangeLongAcquire(o, offset, expected, x);
  }

  
  public static long compareAndExchangeLongRelease(Object o, long offset, long expected, long x) {
    return ZONE.compareAndExchangeLongRelease(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetLongPlain(Object o, long offset, long expected, long x) {
    return ZONE.weakCompareAndSetLongPlain(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetLongAcquire(Object o, long offset, long expected, long x) {
    return ZONE.weakCompareAndSetLongAcquire(o, offset, expected, x);
  }

  
  public static boolean weakCompareAndSetLongRelease(Object o, long offset, long expected, long x) {
    return ZONE.weakCompareAndSetLongRelease(o, offset, expected, x);
  }

  public static boolean weakCompareAndSetLong(Object o, long offset, long expected, long x) {
    return ZONE.weakCompareAndSetLong(o, offset, expected, x);
  }

  public static Object getReferenceVolatile(Object o, long offset) {
    return ZONE.getReferenceVolatile(o, offset);
  }

  public static void putReferenceVolatile(Object o, long offset, Object x) {
    ZONE.putReferenceVolatile(o, offset, x);
  }

  public static int getIntVolatile(Object o, long offset) {
    return ZONE.getIntVolatile(o, offset);
  }

  public static void putIntVolatile(Object o, long offset, int x) {
    ZONE.putIntVolatile(o, offset, x);
  }

  public static boolean getBooleanVolatile(Object o, long offset) {
    return ZONE.getBooleanVolatile(o, offset);
  }

  public static void putBooleanVolatile(Object o, long offset, boolean x) {
    ZONE.putBooleanVolatile(o, offset, x);
  }

  public static byte getByteVolatile(Object o, long offset) {
    return ZONE.getByteVolatile(o, offset);
  }

  public static void putByteVolatile(Object o, long offset, byte x) {
    ZONE.putByteVolatile(o, offset, x);
  }

  public static short getShortVolatile(Object o, long offset) {
    return ZONE.getShortVolatile(o, offset);
  }

  public static void putShortVolatile(Object o, long offset, short x) {
    ZONE.putShortVolatile(o, offset, x);
  }

  public static char getCharVolatile(Object o, long offset) {
    return ZONE.getCharVolatile(o, offset);
  }

  public static void putCharVolatile(Object o, long offset, char x) {
    ZONE.putCharVolatile(o, offset, x);
  }

  public static long getLongVolatile(Object o, long offset) {
    return ZONE.getLongVolatile(o, offset);
  }

  public static void putLongVolatile(Object o, long offset, long x) {
    ZONE.putLongVolatile(o, offset, x);
  }

  public static float getFloatVolatile(Object o, long offset) {
    return ZONE.getFloatVolatile(o, offset);
  }

  public static void putFloatVolatile(Object o, long offset, float x) {
    ZONE.putFloatVolatile(o, offset, x);
  }

  public static double getDoubleVolatile(Object o, long offset) {
    return ZONE.getDoubleVolatile(o, offset);
  }

  public static void putDoubleVolatile(Object o, long offset, double x) {
    ZONE.putDoubleVolatile(o, offset, x);
  }

  public static Object getReferenceAcquire(Object o, long offset) {
    return ZONE.getReferenceAcquire(o, offset);
  }

  public static boolean getBooleanAcquire(Object o, long offset) {
    return ZONE.getBooleanAcquire(o, offset);
  }

  public static byte getByteAcquire(Object o, long offset) {
    return ZONE.getByteAcquire(o, offset);
  }

  public static short getShortAcquire(Object o, long offset) {
    return ZONE.getShortAcquire(o, offset);
  }

  public static char getCharAcquire(Object o, long offset) {
    return ZONE.getCharAcquire(o, offset);
  }

  public static int getIntAcquire(Object o, long offset) {
    return ZONE.getIntAcquire(o, offset);
  }

  public static float getFloatAcquire(Object o, long offset) {
    return ZONE.getFloatAcquire(o, offset);
  }

  public static long getLongAcquire(Object o, long offset) {
    return ZONE.getLongAcquire(o, offset);
  }

  public static double getDoubleAcquire(Object o, long offset) {
    return ZONE.getDoubleAcquire(o, offset);
  }

  public static void putReferenceRelease(Object o, long offset, Object x) {
    ZONE.putReferenceRelease(o, offset, x);
  }

  public static void putBooleanRelease(Object o, long offset, boolean x) {
    ZONE.putBooleanRelease(o, offset, x);
  }

  public static void putByteRelease(Object o, long offset, byte x) {
    ZONE.putByteRelease(o, offset, x);
  }

  public static void putShortRelease(Object o, long offset, short x) {
    ZONE.putShortRelease(o, offset, x);
  }

  public static void putCharRelease(Object o, long offset, char x) {
    ZONE.putCharRelease(o, offset, x);
  }

  public static void putIntRelease(Object o, long offset, int x) {
    ZONE.putIntRelease(o, offset, x);
  }

  public static void putFloatRelease(Object o, long offset, float x) {
    ZONE.putFloatRelease(o, offset, x);
  }

  public static void putLongRelease(Object o, long offset, long x) {
    ZONE.putLongRelease(o, offset, x);
  }

  public static void putDoubleRelease(Object o, long offset, double x) {
    ZONE.putDoubleRelease(o, offset, x);
  }

  public static Object getReferenceOpaque(Object o, long offset) {
    return ZONE.getReferenceOpaque(o, offset);
  }

  public static boolean getBooleanOpaque(Object o, long offset) {
    return ZONE.getBooleanOpaque(o, offset);
  }

  public static byte getByteOpaque(Object o, long offset) {
    return ZONE.getByteOpaque(o, offset);
  }

  public static short getShortOpaque(Object o, long offset) {
    return ZONE.getShortOpaque(o, offset);
  }

  public static char getCharOpaque(Object o, long offset) {
    return ZONE.getCharOpaque(o, offset);
  }

  public static int getIntOpaque(Object o, long offset) {
    return ZONE.getIntOpaque(o, offset);
  }

  public static float getFloatOpaque(Object o, long offset) {
    return ZONE.getFloatOpaque(o, offset);
  }

  public static long getLongOpaque(Object o, long offset) {
    return ZONE.getLongOpaque(o, offset);
  }

  public static double getDoubleOpaque(Object o, long offset) {
    return ZONE.getDoubleOpaque(o, offset);
  }

  public static void putReferenceOpaque(Object o, long offset, Object x) {
    ZONE.putReferenceOpaque(o, offset, x);
  }

  public static void putBooleanOpaque(Object o, long offset, boolean x) {
    ZONE.putBooleanOpaque(o, offset, x);
  }

  public static void putByteOpaque(Object o, long offset, byte x) {
    ZONE.putByteOpaque(o, offset, x);
  }

  public static void putShortOpaque(Object o, long offset, short x) {
    ZONE.putShortOpaque(o, offset, x);
  }

  public static void putCharOpaque(Object o, long offset, char x) {
    ZONE.putCharOpaque(o, offset, x);
  }

  public static void putIntOpaque(Object o, long offset, int x) {
    ZONE.putIntOpaque(o, offset, x);
  }

  public static void putFloatOpaque(Object o, long offset, float x) {
    ZONE.putFloatOpaque(o, offset, x);
  }

  public static void putLongOpaque(Object o, long offset, long x) {
    ZONE.putLongOpaque(o, offset, x);
  }

  public static void putDoubleOpaque(Object o, long offset, double x) {
    ZONE.putDoubleOpaque(o, offset, x);
  }

  public static void unpark(Object thread) {
    ZONE.unpark(thread);
  }

  public static void park(boolean isAbsolute, long time) {
    ZONE.park(isAbsolute, time);
  }

  public static int getLoadAverage(double[] loadavg, int nelems) {
    return ZONE.getLoadAverage(loadavg, nelems);
  }

  public static int getAndAddInt(Object o, long offset, int delta) {
    return ZONE.getAndAddInt(o, offset, delta);
  }

  public static int getAndAddIntRelease(Object o, long offset, int delta) {
    return ZONE.getAndAddIntRelease(o, offset, delta);
  }

  public static int getAndAddIntAcquire(Object o, long offset, int delta) {
    return ZONE.getAndAddIntAcquire(o, offset, delta);
  }

  public static long getAndAddLong(Object o, long offset, long delta) {
    return ZONE.getAndAddLong(o, offset, delta);
  }

  public static long getAndAddLongRelease(Object o, long offset, long delta) {
    return ZONE.getAndAddLongRelease(o, offset, delta);
  }

  public static long getAndAddLongAcquire(Object o, long offset, long delta) {
    return ZONE.getAndAddLongAcquire(o, offset, delta);
  }

  public static byte getAndAddByte(Object o, long offset, byte delta) {
    return ZONE.getAndAddByte(o, offset, delta);
  }

  public static byte getAndAddByteRelease(Object o, long offset, byte delta) {
    return ZONE.getAndAddByteRelease(o, offset, delta);
  }

  public static byte getAndAddByteAcquire(Object o, long offset, byte delta) {
    return ZONE.getAndAddByteAcquire(o, offset, delta);
  }

  public static short getAndAddShort(Object o, long offset, short delta) {
    return ZONE.getAndAddShort(o, offset, delta);
  }

  public static short getAndAddShortRelease(Object o, long offset, short delta) {
    return ZONE.getAndAddShortRelease(o, offset, delta);
  }

  public static short getAndAddShortAcquire(Object o, long offset, short delta) {
    return ZONE.getAndAddShortAcquire(o, offset, delta);
  }

  public static char getAndAddChar(Object o, long offset, char delta) {
    return ZONE.getAndAddChar(o, offset, delta);
  }

  public static char getAndAddCharRelease(Object o, long offset, char delta) {
    return ZONE.getAndAddCharRelease(o, offset, delta);
  }

  public static char getAndAddCharAcquire(Object o, long offset, char delta) {
    return ZONE.getAndAddCharAcquire(o, offset, delta);
  }

  public static float getAndAddFloat(Object o, long offset, float delta) {
    return ZONE.getAndAddFloat(o, offset, delta);
  }

  public static float getAndAddFloatRelease(Object o, long offset, float delta) {
    return ZONE.getAndAddFloatRelease(o, offset, delta);
  }

  public static float getAndAddFloatAcquire(Object o, long offset, float delta) {
    return ZONE.getAndAddFloatAcquire(o, offset, delta);
  }

  public static double getAndAddDouble(Object o, long offset, double delta) {
    return ZONE.getAndAddDouble(o, offset, delta);
  }

  public static double getAndAddDoubleRelease(Object o, long offset, double delta) {
    return ZONE.getAndAddDoubleRelease(o, offset, delta);
  }

  public static double getAndAddDoubleAcquire(Object o, long offset, double delta) {
    return ZONE.getAndAddDoubleAcquire(o, offset, delta);
  }

  public static int getAndSetInt(Object o, long offset, int newValue) {
    return ZONE.getAndSetInt(o, offset, newValue);
  }

  public static int getAndSetIntRelease(Object o, long offset, int newValue) {
    return ZONE.getAndSetIntRelease(o, offset, newValue);
  }

  public static int getAndSetIntAcquire(Object o, long offset, int newValue) {
    return ZONE.getAndSetIntAcquire(o, offset, newValue);
  }

  public static long getAndSetLong(Object o, long offset, long newValue) {
    return ZONE.getAndSetLong(o, offset, newValue);
  }

  public static long getAndSetLongRelease(Object o, long offset, long newValue) {
    return ZONE.getAndSetLongRelease(o, offset, newValue);
  }

  public static long getAndSetLongAcquire(Object o, long offset, long newValue) {
    return ZONE.getAndSetLongAcquire(o, offset, newValue);
  }

  public static Object getAndSetReference(Object o, long offset, Object newValue) {
    return ZONE.getAndSetReference(o, offset, newValue);
  }

  public static Object getAndSetReferenceRelease(Object o, long offset, Object newValue) {
    return ZONE.getAndSetReferenceRelease(o, offset, newValue);
  }

  public static Object getAndSetReferenceAcquire(Object o, long offset, Object newValue) {
    return ZONE.getAndSetReferenceAcquire(o, offset, newValue);
  }

  public static byte getAndSetByte(Object o, long offset, byte newValue) {
    return ZONE.getAndSetByte(o, offset, newValue);
  }

  public static byte getAndSetByteRelease(Object o, long offset, byte newValue) {
    return ZONE.getAndSetByteRelease(o, offset, newValue);
  }

  public static byte getAndSetByteAcquire(Object o, long offset, byte newValue) {
    return ZONE.getAndSetByteAcquire(o, offset, newValue);
  }

  public static boolean getAndSetBoolean(Object o, long offset, boolean newValue) {
    return ZONE.getAndSetBoolean(o, offset, newValue);
  }

  public static boolean getAndSetBooleanRelease(Object o, long offset, boolean newValue) {
    return ZONE.getAndSetBooleanRelease(o, offset, newValue);
  }

  public static boolean getAndSetBooleanAcquire(Object o, long offset, boolean newValue) {
    return ZONE.getAndSetBooleanAcquire(o, offset, newValue);
  }

  public static short getAndSetShort(Object o, long offset, short newValue) {
    return ZONE.getAndSetShort(o, offset, newValue);
  }

  public static short getAndSetShortRelease(Object o, long offset, short newValue) {
    return ZONE.getAndSetShortRelease(o, offset, newValue);
  }

  public static short getAndSetShortAcquire(Object o, long offset, short newValue) {
    return ZONE.getAndSetShortAcquire(o, offset, newValue);
  }

  public static char getAndSetChar(Object o, long offset, char newValue) {
    return ZONE.getAndSetChar(o, offset, newValue);
  }

  public static char getAndSetCharRelease(Object o, long offset, char newValue) {
    return ZONE.getAndSetCharRelease(o, offset, newValue);
  }

  public static char getAndSetCharAcquire(Object o, long offset, char newValue) {
    return ZONE.getAndSetCharAcquire(o, offset, newValue);
  }

  public static float getAndSetFloat(Object o, long offset, float newValue) {
    return ZONE.getAndSetFloat(o, offset, newValue);
  }

  public static float getAndSetFloatRelease(Object o, long offset, float newValue) {
    return ZONE.getAndSetFloatRelease(o, offset, newValue);
  }

  public static float getAndSetFloatAcquire(Object o, long offset, float newValue) {
    return ZONE.getAndSetFloatAcquire(o, offset, newValue);
  }

  public static double getAndSetDouble(Object o, long offset, double newValue) {
    return ZONE.getAndSetDouble(o, offset, newValue);
  }

  public static double getAndSetDoubleRelease(Object o, long offset, double newValue) {
    return ZONE.getAndSetDoubleRelease(o, offset, newValue);
  }

  public static double getAndSetDoubleAcquire(Object o, long offset, double newValue) {
    return ZONE.getAndSetDoubleAcquire(o, offset, newValue);
  }

  public static boolean getAndBitwiseOrBoolean(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseOrBoolean(o, offset, mask);
  }

  public static boolean getAndBitwiseOrBooleanRelease(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseOrBooleanRelease(o, offset, mask);
  }

  public static boolean getAndBitwiseOrBooleanAcquire(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseOrBooleanAcquire(o, offset, mask);
  }

  public static boolean getAndBitwiseAndBoolean(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseAndBoolean(o, offset, mask);
  }

  public static boolean getAndBitwiseAndBooleanRelease(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseAndBooleanRelease(o, offset, mask);
  }

  public static boolean getAndBitwiseAndBooleanAcquire(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseAndBooleanAcquire(o, offset, mask);
  }

  public static boolean getAndBitwiseXorBoolean(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseXorBoolean(o, offset, mask);
  }

  public static boolean getAndBitwiseXorBooleanRelease(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseXorBooleanRelease(o, offset, mask);
  }

  public static boolean getAndBitwiseXorBooleanAcquire(Object o, long offset, boolean mask) {
    return ZONE.getAndBitwiseXorBooleanAcquire(o, offset, mask);
  }

  public static byte getAndBitwiseOrByte(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseOrByte(o, offset, mask);
  }

  public static byte getAndBitwiseOrByteRelease(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseOrByteRelease(o, offset, mask);
  }

  public static byte getAndBitwiseOrByteAcquire(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseOrByteAcquire(o, offset, mask);
  }

  public static byte getAndBitwiseAndByte(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseAndByte(o, offset, mask);
  }

  public static byte getAndBitwiseAndByteRelease(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseAndByteRelease(o, offset, mask);
  }

  public static byte getAndBitwiseAndByteAcquire(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseAndByteAcquire(o, offset, mask);
  }

  public static byte getAndBitwiseXorByte(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseXorByte(o, offset, mask);
  }

  public static byte getAndBitwiseXorByteRelease(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseXorByteRelease(o, offset, mask);
  }

  public static byte getAndBitwiseXorByteAcquire(Object o, long offset, byte mask) {
    return ZONE.getAndBitwiseXorByteAcquire(o, offset, mask);
  }

  public static char getAndBitwiseOrChar(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseOrChar(o, offset, mask);
  }

  public static char getAndBitwiseOrCharRelease(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseOrCharRelease(o, offset, mask);
  }

  public static char getAndBitwiseOrCharAcquire(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseOrCharAcquire(o, offset, mask);
  }

  public static char getAndBitwiseAndChar(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseAndChar(o, offset, mask);
  }

  public static char getAndBitwiseAndCharRelease(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseAndCharRelease(o, offset, mask);
  }

  public static char getAndBitwiseAndCharAcquire(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseAndCharAcquire(o, offset, mask);
  }

  public static char getAndBitwiseXorChar(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseXorChar(o, offset, mask);
  }

  public static char getAndBitwiseXorCharRelease(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseXorCharRelease(o, offset, mask);
  }

  public static char getAndBitwiseXorCharAcquire(Object o, long offset, char mask) {
    return ZONE.getAndBitwiseXorCharAcquire(o, offset, mask);
  }

  public static short getAndBitwiseOrShort(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseOrShort(o, offset, mask);
  }

  public static short getAndBitwiseOrShortRelease(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseOrShortRelease(o, offset, mask);
  }

  public static short getAndBitwiseOrShortAcquire(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseOrShortAcquire(o, offset, mask);
  }

  public static short getAndBitwiseAndShort(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseAndShort(o, offset, mask);
  }

  public static short getAndBitwiseAndShortRelease(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseAndShortRelease(o, offset, mask);
  }

  public static short getAndBitwiseAndShortAcquire(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseAndShortAcquire(o, offset, mask);
  }

  public static short getAndBitwiseXorShort(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseXorShort(o, offset, mask);
  }

  public static short getAndBitwiseXorShortRelease(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseXorShortRelease(o, offset, mask);
  }

  public static short getAndBitwiseXorShortAcquire(Object o, long offset, short mask) {
    return ZONE.getAndBitwiseXorShortAcquire(o, offset, mask);
  }

  public static int getAndBitwiseOrInt(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseOrInt(o, offset, mask);
  }

  public static int getAndBitwiseOrIntRelease(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseOrIntRelease(o, offset, mask);
  }

  public static int getAndBitwiseOrIntAcquire(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseOrIntAcquire(o, offset, mask);
  }

  public static int getAndBitwiseAndInt(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseAndInt(o, offset, mask);
  }

  public static int getAndBitwiseAndIntRelease(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseAndIntRelease(o, offset, mask);
  }

  public static int getAndBitwiseAndIntAcquire(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseAndIntAcquire(o, offset, mask);
  }

  public static int getAndBitwiseXorInt(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseXorInt(o, offset, mask);
  }

  public static int getAndBitwiseXorIntRelease(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseXorIntRelease(o, offset, mask);
  }

  public static int getAndBitwiseXorIntAcquire(Object o, long offset, int mask) {
    return ZONE.getAndBitwiseXorIntAcquire(o, offset, mask);
  }

  public static long getAndBitwiseOrLong(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseOrLong(o, offset, mask);
  }

  public static long getAndBitwiseOrLongRelease(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseOrLongRelease(o, offset, mask);
  }

  public static long getAndBitwiseOrLongAcquire(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseOrLongAcquire(o, offset, mask);
  }

  public static long getAndBitwiseAndLong(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseAndLong(o, offset, mask);
  }

  public static long getAndBitwiseAndLongRelease(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseAndLongRelease(o, offset, mask);
  }

  public static long getAndBitwiseAndLongAcquire(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseAndLongAcquire(o, offset, mask);
  }

  public static long getAndBitwiseXorLong(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseXorLong(o, offset, mask);
  }

  public static long getAndBitwiseXorLongRelease(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseXorLongRelease(o, offset, mask);
  }

  public static long getAndBitwiseXorLongAcquire(Object o, long offset, long mask) {
    return ZONE.getAndBitwiseXorLongAcquire(o, offset, mask);
  }

  public static void loadFence() {
    ZONE.loadFence();
  }

  public static void storeFence() {
    ZONE.storeFence();
  }

  public static void fullFence() {
    ZONE.fullFence();
  }

  public static void loadLoadFence() {
    ZONE.loadLoadFence();
  }

  public static void storeStoreFence() {
    ZONE.storeStoreFence();
  }

  public static boolean isBigEndian() {
    return ZONE.isBigEndian();
  }

  public static boolean unalignedAccess() {
    return ZONE.unalignedAccess();
  }

  public static long getLongUnaligned(Object o, long offset) {
    return ZONE.getLongUnaligned(o, offset);
  }

  public static long getLongUnaligned(Object o, long offset, boolean bigEndian) {
    return ZONE.getLongUnaligned(o, offset, bigEndian);
  }

  public static int getIntUnaligned(Object o, long offset) {
    return ZONE.getIntUnaligned(o, offset);
  }

  public static int getIntUnaligned(Object o, long offset, boolean bigEndian) {
    return ZONE.getIntUnaligned(o, offset, bigEndian);
  }

  public static short getShortUnaligned(Object o, long offset) {
    return ZONE.getShortUnaligned(o, offset);
  }

  public static short getShortUnaligned(Object o, long offset, boolean bigEndian) {
    return ZONE.getShortUnaligned(o, offset, bigEndian);
  }

  public static char getCharUnaligned(Object o, long offset) {
    return ZONE.getCharUnaligned(o, offset);
  }

  public static char getCharUnaligned(Object o, long offset, boolean bigEndian) {
    return ZONE.getCharUnaligned(o, offset, bigEndian);
  }

  public static void putLongUnaligned(Object o, long offset, long x) {
    ZONE.putLongUnaligned(o, offset, x);
  }

  public static void putLongUnaligned(Object o, long offset, long x, boolean bigEndian) {
    ZONE.putLongUnaligned(o, offset, x, bigEndian);
  }

  public static void putIntUnaligned(Object o, long offset, int x) {
    ZONE.putIntUnaligned(o, offset, x);
  }

  public static void putIntUnaligned(Object o, long offset, int x, boolean bigEndian) {
    ZONE.putIntUnaligned(o, offset, x, bigEndian);
  }

  public static void putShortUnaligned(Object o, long offset, short x) {
    ZONE.putShortUnaligned(o, offset, x);
  }

  public static void putShortUnaligned(Object o, long offset, short x, boolean bigEndian) {
    ZONE.putShortUnaligned(o, offset, x, bigEndian);
  }

  public static void putCharUnaligned(Object o, long offset, char x) {
    ZONE.putCharUnaligned(o, offset, x);
  }

  public static void putCharUnaligned(Object o, long offset, char x, boolean bigEndian) {
    ZONE.putCharUnaligned(o, offset, x, bigEndian);
  }

  public static long getLongUnalignedLE(Object o, long offset) {
    return ZONE.getLongUnaligned(o, offset, false);
  }

  public static long getLongUnalignedBE(Object o, long offset) {
    return ZONE.getLongUnaligned(o, offset, true);
  }

  public static int getIntUnalignedLE(Object o, long offset) {
    return ZONE.getIntUnaligned(o, offset, false);
  }

  public static int getIntUnalignedBE(Object o, long offset) {
    return ZONE.getIntUnaligned(o, offset, true);
  }

  public static short getShortUnalignedLE(Object o, long offset) {
    return ZONE.getShortUnaligned(o, offset, false);
  }

  public static short getShortUnalignedBE(Object o, long offset) {
    return ZONE.getShortUnaligned(o, offset, true);
  }

  public static char getCharUnalignedLE(Object o, long offset) {
    return ZONE.getCharUnaligned(o, offset, false);
  }

  public static char getCharUnalignedBE(Object o, long offset) {
    return ZONE.getCharUnaligned(o, offset, true);
  }

  public static void putLongUnalignedLE(Object o, long offset, long x) {
    ZONE.putLongUnaligned(o, offset, x, false);
  }
  public static void putLongUnalignedBE(Object o, long offset, long x) {
    ZONE.putLongUnaligned(o, offset, x, true);
  }

  public static void putIntUnalignedLE(Object o, long offset, int x) {
    ZONE.putIntUnaligned(o, offset, x, false);
  }

  public static void putIntUnalignedBE(Object o, long offset, int x) {
    ZONE.putIntUnaligned(o, offset, x, true);
  }

  public static void putShortUnalignedLE(Object o, long offset, short x) {
    ZONE.putShortUnaligned(o, offset, x, false);
  }

  public static void putShortUnalignedBE(Object o, long offset, short x) {
    ZONE.putShortUnaligned(o, offset, x, true);
  }

  public static void putCharUnalignedLE(Object o, long offset, char x) {
    ZONE.putCharUnaligned(o, offset, x, false);
  }

  public static void putCharUnalignedBE(Object o, long offset, char x) {
    ZONE.putCharUnaligned(o, offset, x, true);
  }


  public static void invokeCleaner(ByteBuffer directBuffer) {
    ZONE.invokeCleaner(directBuffer);
  }
}
