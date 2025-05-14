package bop.alloc;

import bop.bench.Bench;
import bop.unsafe.Danger;
import java.util.List;
import org.junit.jupiter.api.Test;

public class AllocatorTest {
  static class Data {
    static final Factory<Data> FACTORY = Factory.of(Data.class);
    long a;
    long b, c, d, e, f, g, h;
    String s1, s2, s3, s4;
    List<String> l1, l2, l3, l4;

    public void recycle() {
      //      a = 0;
      //      b = 0;
      //      c = 0;
      //      d = 0;
      //      e = 0;
      //      f = 0;
      //      g = 0;
      //      h = 0;
      //      s1 = null;
      //      s2 = null;
      //      s3 = null;
      //      s4 = null;
      //      l1 = null;
      //      l2 = null;
      //      l3 = null;
      //      l4 = null;
      FACTORY.recycle(this);
    }
  }

  @Test
  public void benchFactory() throws Throwable {
    final var list = new Data[32 * 512];
    for (int j = 0; j < list.length; j++) {
      list[j] = new Data();
    }

    final var factory = Data.FACTORY;
    final var allocators = new Allocator[64];
    final var tls = new ThreadLocal<Data>();

    Bench.printHeader();
    Bench.threaded("Recycled", 1, 10, 10000000, (threadId, iteration, index) -> {
      final var allocator = factory.allocator();
      var data = allocator.allocate();
      allocator.recycle(data);
    });
    Bench.threaded("Recycled", 2, 10, 10000000, (threadId, iteration, index) -> {
      final var allocator = factory.allocator();
      var data = allocator.allocate();
      allocator.recycle(data);
    });
    Bench.threaded("Recycled", 4, 10, 10000000, (threadId, iteration, index) -> {
      final var allocator = factory.allocator();
      var data = allocator.allocate();
      allocator.recycle(data);
    });
    //    Bench.threaded("Recycled", 8, 10, 10000000, (threadId, iteration, index) -> {
    //      final var allocator = factory.allocator();
    //      var data = allocator.allocate();
    //      allocator.recycle(data);
    //    });
    //    Bench.threaded("Recycled", 16, 50, 1000000, (threadId, iteration, index) -> {
    //      var data = factory.alloc();
    //      data.recycle();
    //    });
    //    Bench.threaded("Recycled", 32, 50, 1000000, (threadId, iteration, index) -> {
    //      var data = factory.alloc();
    //      data.recycle();
    //    });

    Bench.printSeparator();
    Bench.threaded("GC", 1, 10, 10000000, (threadId, iteration, index) -> {
      //      tls.set(new Data());
      list[threadId * 64] = new Data();
    });
    Bench.threaded("GC", 2, 10, 10000000, (threadId, iteration, index) -> {
      list[threadId * 64] = new Data();
    });
    Bench.threaded("GC", 4, 10, 10000000, (threadId, iteration, index) -> {
      list[threadId * 64] = new Data();
    });
    //    Bench.threaded("GC", 8, 10, 10000000, (threadId, iteration, index) -> {
    //      list[threadId*64] = new Data();
    //    });

    //    Bench.printSeparator();
    //    Bench.threaded("snmalloc", 1, 10, 10000000, (threadId, iteration, index) -> {
    //      Memory.dealloc(Memory.alloc(16));
    //    });
    //    Bench.threaded("snmalloc", 2, 10, 10000000, (threadId, iteration, index) -> {
    //      Memory.dealloc(Memory.alloc(16));
    //    });
    //    Bench.threaded("snmalloc", 4, 10, 10000000, (threadId, iteration, index) -> {
    //      Memory.dealloc(Memory.alloc(16));
    //    });
    //    Bench.threaded("snmalloc", 8, 10, 10000000, (threadId, iteration, index) -> {
    //      Memory.dealloc(Memory.alloc(16));
    //    });
    //    Bench.threaded("GC", 16, 50, 1000000, (threadId, iteration, index) -> {
    //      tls.set(new Data());
    //    });
    //    Bench.threaded("GC", 32, 50, 1000000, (threadId, iteration, index) -> {
    //      tls.set(new Data());
    //    });
    Bench.printFooter();

    if (list[0] == list[1]) {
      System.out.println();
    }

    for (int i = 0; i < 32; i++) {
      System.out.println(list[i]);
    }
  }

  @Test
  public void hackString() {
    final var bytes = "this is patched".getBytes();
    final var str = new StringBuilder().append("hello world").toString();
    System.out.println(str);
    Danger.unsafeSetString(str, bytes);
    System.out.println(str);
  }
}
