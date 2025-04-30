package bop.c;

import bop.bench.Bench;
import java.lang.foreign.AddressLayout;
import java.util.stream.IntStream;
import jdk.internal.foreign.SegmentFactories;
import org.junit.jupiter.api.Test;

public class MemoryTest {
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
  public void benchAllocZeroedDealloc() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8, 16).forEach(threads -> {
      try {
        Bench.threaded(
            "bop_alloc/bop_dealloc", threads, 10, 10000000, (threadId, cycle, iteration) -> {
              Memory.dealloc(Memory.allocZeroed(16L));
            });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }
}
