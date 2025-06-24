package bop.c;

import static bop.c.Loader.LINKER;

import bop.unsafe.Danger;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.charset.Charset;
import jdk.internal.foreign.SegmentFactories;

/// Memory provides access to a high quality native memory manager
/// called snmalloc. All methods are thread-safe and scale to any
/// number of cores.
public class Memory {
  interface CFunctions {
    MethodHandle BOP_HELLO = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_hello").orElseThrow(),
        FunctionDescriptor.ofVoid(),
        Linker.Option.critical(true));

    MethodHandle BOP_HEAP_ACCESS = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_heap_access").orElseThrow(),
        FunctionDescriptor.ofVoid(
            ValueLayout.ADDRESS, // uint8_t*
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_ALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_alloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_ZALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_zalloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_ALLOC_ALIGNED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_alloc_aligned").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t alignment
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_ZALLOC_ALIGNED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_zalloc_aligned").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t alignment
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_REALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_realloc").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG, // void*
            ValueLayout.JAVA_LONG, // size_t size
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_DEALLOC = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_dealloc").orElseThrow(),
        FunctionDescriptor.ofVoid(
            ValueLayout.JAVA_LONG // void* p
            ),
        Linker.Option.critical(true));

    MethodHandle BOP_DEALLOC_SIZED = LINKER.downcallHandle(
        Loader.LOOKUP.find("bop_dealloc_sized").orElseThrow(),
        FunctionDescriptor.ofVoid(
            ValueLayout.JAVA_LONG, // void* p
            ValueLayout.JAVA_LONG // size_t size
            ),
        Linker.Option.critical(true));
  }

  public static void hello() {
    try {
      CFunctions.BOP_HELLO.invokeExact();
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void heapAccess(byte[] data) {
    try {
      CFunctions.BOP_HEAP_ACCESS.invokeExact(MemorySegment.ofArray(data), (long) data.length);
      //      Library.BOP_HEAP_ACCESS.invoke(MemorySegment.ofArray(data), (long)data.length);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void heapAccess(MemorySegment data) {
    try {
      CFunctions.BOP_HEAP_ACCESS.invokeExact(data, data.byteSize());
      //      Library.BOP_HEAP_ACCESS.invoke(MemorySegment.ofArray(data), (long)data.length);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static String fromCString(MemorySegment segment, int limit, Charset charset) {
    return fromCString(segment.address(), limit, charset);
  }

  public static String fromCString(long address, int limit, Charset charset) {
    if (address == 0L) {
      return "";
    }
    for (int i = 0; i < limit; i++) {
      if (Danger.getByte(address + i) == 0) {
        final var b = new byte[i];
        Danger.copyMemory(null, address, b, Danger.BYTE_BASE, b.length);
        return new String(b, charset);
      }
    }
    final var b = new byte[limit];
    Danger.copyMemory(null, address, b, Danger.BYTE_BASE, b.length);
    return new String(b, charset);
  }

  public static long allocCString(String value) {
    final var b = Danger.getBytes(value);
    final var p = alloc(b.length + 1);
    Danger.putByte(p + b.length, (byte) 0);
    Danger.copyMemory(b, Danger.BYTE_BASE, null, p, b.length);
    return p;
  }

  ///
  public static long alloc(long size) {
    try {
      final var ptr = (long) CFunctions.BOP_ALLOC.invokeExact(size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static MemorySegment allocSegment(long size) {
    return SegmentFactories.makeNativeSegmentUnchecked(alloc(size), size);
  }

  /// Alloc and set all bytes to zero.
  public static long zalloc(long size) {
    try {
      final var ptr = (long) CFunctions.BOP_ZALLOC.invokeExact(size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static MemorySegment allocZeroedSegment(long size) {
    return SegmentFactories.makeNativeSegmentUnchecked(zalloc(size), size);
  }

  public static long allocAligned(long alignment, long size) {
    try {
      final var ptr = (long) CFunctions.BOP_ALLOC_ALIGNED.invokeExact(alignment, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long zallocAligned(long alignment, long size) {
    try {
      final var ptr = (long) CFunctions.BOP_ZALLOC_ALIGNED.invokeExact(alignment, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static long realloc(long address, long size) {
    try {
      final var ptr = (long) CFunctions.BOP_REALLOC.invokeExact(address, size);
      if (ptr == 0L) throw new OutOfMemoryError();
      return ptr;
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void dealloc(long ptr) {
    if (ptr == 0L) return;
    try {
      CFunctions.BOP_DEALLOC.invokeExact(ptr);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static void deallocSized(long ptr, long size) {
    if (ptr == 0L) return;
    try {
      CFunctions.BOP_DEALLOC_SIZED.invokeExact(ptr, size);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }
}
