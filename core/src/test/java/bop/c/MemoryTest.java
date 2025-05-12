package bop.c;

import bop.bench.Bench;
import java.lang.foreign.AddressLayout;
import java.util.stream.IntStream;
import jdk.internal.foreign.SegmentFactories;
import org.junit.jupiter.api.Test;

public class MemoryTest {
  @Test
  public void benchFFMOverhead() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8, 16).forEach(threads -> {
      try {
        Bench.threaded("c stub", threads, 10, 10000000, (threadId, cycle, iteration) -> {
          Memory.hello();
        });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }

  @Test
  public void alloc() {
    final var address = Memory.alloc(12);
    System.out.println(address);
    Memory.dealloc(address);

    final var segment = SegmentFactories.makeNativeSegmentUnchecked(Memory.alloc(64L), 64L);
    //    final var segment = MemorySegment.ofAddress(Memory.alloc(64L));
    segment.set(AddressLayout.JAVA_LONG, 0, address);

    final var addr = segment.get(AddressLayout.JAVA_LONG, 0);
    System.out.println(addr);
  }

  static final ThreadLocal<byte[]> BYTE_LOCAL = new ThreadLocal<>();

  @Test
  public void benchHeapAccess() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8).forEach(threads -> {
      try {
        Bench.threaded("bop_heap_access", threads, 25, 10000000, (threadId, cycle, iteration) -> {
          byte[] d = BYTE_LOCAL.get();
          if (d == null) {
            d = new byte[16];
            BYTE_LOCAL.set(d);
          }
          Memory.heapAccess(d);
        });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }

  @Test
  public void benchAllocDealloc() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8, 16).forEach(threads -> {
      try {
        Bench.threaded(
            "bop_alloc/bop_dealloc", threads, 10, 10000000, (threadId, cycle, iteration) -> {
              Memory.dealloc(Memory.alloc(16L));
            });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }

  @Test
  public void benchZallocDealloc() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8, 16).forEach(threads -> {
      try {
        Bench.threaded(
            "bop_alloc/bop_dealloc", threads, 10, 10000000, (threadId, cycle, iteration) -> {
              Memory.dealloc(Memory.zalloc(16L));
            });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }
}
