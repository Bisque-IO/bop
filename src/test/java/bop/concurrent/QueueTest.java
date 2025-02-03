package bop.concurrent;

import bop.bench.Bench;
import bop.bit.Bits;
import java.util.ArrayList;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicLong;
import org.agrona.concurrent.ManyToOneConcurrentArrayQueue;
import org.junit.jupiter.api.Test;

public class QueueTest {
  static class MockQueue {
    int offers = 0;
    int polls = 0;
    int pageSize = 4;
    int pageMask = 3;

    public int offer() {
      var offer = this.offers++;
      return offer;
    }

    public int pageOf(int index) {
      return index & pageMask;
    }
  }

  static void printOrdering(int index, int pageCount, int itemsPerPage, int maxPages) {
    long requiredPages = Bits.findNextPositivePowerOfTwo((long) (index / itemsPerPage) + 1);
    System.out.println("page count: " + pageCount + "  new page count: " + requiredPages
        + "  index: " + index + "  page: " + (index & (requiredPages - 1))
        + "  with required pages: " + (index & (requiredPages - 1)));
  }

  @Test
  public void enqueueDeque() throws Throwable {
    final Long value = 1L;
    //    {
    //      final AtomicLong counter = new AtomicLong();
    //      var mpmc = new ManyToManyConcurrentArrayQueue<Long>(4096*8);
    //      var thread = new Thread(() -> {
    //        ArrayList<Long> list = new ArrayList<>(4096*8);
    //        while (!Thread.interrupted()) {
    //          if (mpmc.drainTo(list, 4096*8) == 0) {
    //            Thread.onSpinWait();
    //          }
    //          counter.addAndGet(list.size());
    //          list.clear();
    //        }
    //      });
    //      thread.start();
    //      var thread2 = new Thread(() -> {
    //        ArrayList<Long> list = new ArrayList<>(4096*8);
    //        while (!Thread.interrupted()) {
    //          if (mpmc.drainTo(list, 4096*8) == 0) {
    //            Thread.onSpinWait();
    //          }
    //          counter.addAndGet(list.size());
    //          list.clear();
    //        }
    //      });
    //      thread2.start();
    //      Bench.threaded("MPMC", 4, 10, 1000000, (threadId, cycle, iteration) -> {
    ////      mpsc.offer(value);
    //        while (!mpmc.offer(value)) {
    //          Thread.yield();
    //        }
    //      });
    //      thread.interrupt();
    //      thread.join();
    //      thread2.interrupt();
    //      thread2.join();
    //      System.out.println("MPMC consumed count: " + counter.get());
    //    }

    //    var page = new Page.Page4096<Long>();
    //    Bench.threaded("Page", 2, 50, 1000000, (threadId, cycle, iteration) -> {
    //      page.offer(value);
    //      page.poll();
    //    });

    Bench.printHeader();
    {
      final AtomicLong counter = new AtomicLong();
      var mpsc = new ManyToOneConcurrentArrayQueue<Long>(4096 * 8);
      var thread = new Thread(() -> {
        ArrayList<Long> list = new ArrayList<>(4096 * 8);
        while (!Thread.interrupted()) {
          if (mpsc.drainTo(list, 4096 * 8) == 0) {
            Thread.onSpinWait();
          }
          counter.addAndGet(list.size());
          list.clear();
        }
      });
      thread.start();
      Bench.threaded("MPSC", 1, 10, 1000000, (threadId, cycle, iteration) -> {
        //      mpsc.offer(value);
        while (!mpsc.offer(value)) {
          Thread.yield();
        }
      });
      thread.interrupt();
      thread.join();
      //      System.out.println("MPSC consumed count: " + counter.get());
    }

    {
      final AtomicLong counter = new AtomicLong();
      var mpsc = new MpscSharded<Long>(16, 4096);
      var thread = new Thread(() -> {
        ArrayList<Long> list = new ArrayList<>(4096 * 8);
        while (!Thread.interrupted()) {
          if (mpsc.drain(list, 4096 * 8) == 0) {
            Thread.onSpinWait();
          }
          counter.addAndGet(list.size());
          list.clear();
        }
      });
      thread.start();
      Bench.threaded("MPSC Partitioned", 8, 10, 1000000, (threadId, cycle, iteration) -> {
        //      mpsc.offer(value);
        while (!mpsc.offer(value)) {
          Thread.yield();
        }
      });
      thread.interrupt();
      thread.join();
      //      System.out.println("MPSC consumed count: " + counter.get());
    }

    {
      final AtomicLong counter = new AtomicLong();
      var mpsc = new ArrayBlockingQueue<Long>(4096);
      var thread = new Thread(() -> {
        ArrayList<Long> list = new ArrayList<>(4096);
        while (!Thread.interrupted()) {
          if (mpsc.drainTo(list, 4096) == 0) {
            Thread.onSpinWait();
          }
          counter.addAndGet(list.size());
          list.clear();
        }
      });
      thread.start();
      Bench.threaded("ArrayBlockingQueue", 1, 10, 1000000, (threadId, cycle, iteration) -> {
        //      mpsc.offer(value);
        while (!mpsc.offer(value)) {
          Thread.yield();
        }
      });
      thread.interrupt();
      thread.join();
      //      System.out.println("ArrayBlockingQueue consumed count: " + counter.get());
    }

    {
      final AtomicLong counter = new AtomicLong();
      var mpsc = new ConcurrentLinkedQueue<Long>();
      var thread = new Thread(() -> {
        while (!Thread.interrupted()) {
          if (mpsc.poll() == null) {
            Thread.onSpinWait();
          }
          counter.addAndGet(1);
        }
      });
      thread.start();
      Bench.threaded("ConcurrentLinkedQueue", 1, 10, 1000000, (threadId, cycle, iteration) -> {
        //      mpsc.offer(value);
        while (!mpsc.offer(value)) {
          Thread.yield();
        }
      });
      thread.interrupt();
      thread.join();
      //      System.out.println("ConcurrentLinkedQueue consumed count: " + counter.get());
    }
    Bench.printFooter();
  }
}
