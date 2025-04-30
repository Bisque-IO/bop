package bop.c;

import static bop.c.Loader.LINKER;

import bop.unsafe.Danger;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import jdk.internal.foreign.SegmentFactories;

public class Memory {
  interface Library {
    MethodHandle BOP_ALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_alloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG // size_t size
            ));

    MethodHandle BOP_ZALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_zalloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG // size_t size
            ));

    MethodHandle BOP_ALLOC_ALIGNED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_alloc_aligned").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t alignment
            ValueLayout.JAVA_LONG // size_t size
            ));

    MethodHandle BOP_ZALLOC_ALIGNED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_zalloc_aligned").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t alignment
            ValueLayout.JAVA_LONG // size_t size
            ));

    MethodHandle BOP_REALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_realloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t size
            ValueLayout.JAVA_LONG // size_t size
            ));

    MethodHandle BOP_DEALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_dealloc").orElseThrow(),
        FunctionDescriptor.ofVoid(
            ValueLayout.JAVA_LONG // void* p
            ));

    MethodHandle BOP_DEALLOC_SIZED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_dealloc_sized").orElseThrow(),
        FunctionDescriptor.ofVoid(
            ValueLayout.JAVA_LONG, // void* p
            ValueLayout.JAVA_LONG // size_t size
            ));
  }

  public static long allocCString(String value) {
    final var b = Danger.getBytes(value);
    final var p = alloc(b.length + 1);
    Danger.UNSAFE.putByte(p + b.length, (byte) 0);
    Danger.UNSAFE.copyMemory(b, Danger.BYTE_BASE, null, p, b.length);
    return p;
  }

  public static long alloc(long size) {
    try {
      final var ptr = (long) Library.BOP_ALLOC.invokeExact(size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static MemorySegment allocSegment(long size) {
    return SegmentFactories.makeNativeSegmentUnchecked(alloc(size), size);
  }

  public static long allocZeroed(long size) {
    try {
      final var ptr = (long) Library.BOP_ZALLOC.invokeExact(size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static MemorySegment allocZeroedSegment(long size) {
    return SegmentFactories.makeNativeSegmentUnchecked(allocZeroed(size), size);
  }

  public static long allocAligned(long alignment, long size) {
    try {
      final var ptr = (long) Library.BOP_ALLOC_ALIGNED.invokeExact(alignment, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long zallocAligned(long alignment, long size) {
    try {
      final var ptr = (long) Library.BOP_ZALLOC_ALIGNED.invokeExact(alignment, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long realloc(long address, long size) {
    try {
      final var ptr = (long) Library.BOP_REALLOC.invokeExact(address, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void dealloc(long ptr) {
    if (ptr == 0L) return;
    try {
      Library.BOP_DEALLOC.invokeExact(ptr);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void deallocSized(long ptr, long size) {
    if (ptr == 0L) return;
    try {
      Library.BOP_DEALLOC_SIZED.invokeExact(ptr, size);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }
}
