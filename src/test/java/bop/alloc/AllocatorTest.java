package bop.alloc;

import bop.bench.Bench;
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
      a = 0;
      b = 0;
      c = 0;
      d = 0;
      e = 0;
      f = 0;
      g = 0;
      h = 0;
      s1 = null;
      s2 = null;
      s3 = null;
      s4 = null;
      l1 = null;
      l2 = null;
      l3 = null;
      l4 = null;
      FACTORY.recycle(this);
    }
  }

  @Test
  public void benchFactory() throws Throwable {
    final var list = new Data[500000];
    for (int j = 0; j < list.length; j++) {
      list[j] = new Data();
    }

    final var factory = Data.FACTORY;
    final var tls = new ThreadLocal<Data>();

    Bench.printHeader();
    Bench.threaded("Recycled", 1, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });
    Bench.threaded("Recycled", 2, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });
    Bench.threaded("Recycled", 4, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });
    Bench.threaded("Recycled", 8, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });
    Bench.threaded("Recycled", 16, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });
    Bench.threaded("Recycled", 32, 50, 1000000, (threadId, iteration, index) -> {
      var data = factory.alloc();
      data.recycle();
    });

    Bench.printSeparator();
    Bench.threaded("GC", 1, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.threaded("GC", 2, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.threaded("GC", 4, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.threaded("GC", 8, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.threaded("GC", 16, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.threaded("GC", 32, 50, 1000000, (threadId, iteration, index) -> {
      tls.set(new Data());
    });
    Bench.printFooter();

    if (list[0] == list[1]) {
      System.out.println();
    }
  }
}
