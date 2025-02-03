package bop.concurrent;

import bop.bench.Bench;
import java.util.concurrent.locks.ReentrantLock;
import org.junit.jupiter.api.Test;

public class SpinLockTest {
  //    @Test
  public void console() throws Throwable {
    final SpinLock spinLock = new SpinLock();
    final ReentrantLock reentrantLock = new ReentrantLock();
    final int[] threadCounts = new int[] {
      1,
      2,
      4,
      //      6,
      8,
      //      10,
      //      12,
      //      14,
      16,
      //      24,
      32,
      //      64,
      //      128
    };

    //    var started = Epoch.nanos();
    //    for (int i = 0; i < 100; i++) {
    //      LockSupport.parkNanos(1_000_000L);
    ////      UnsafeUtils.UNSAFE.park(true, 10000000000L);
    ////      System.out.println("unparked");
    //    }
    //    var elapsed = Epoch.nanos() - started;
    //    System.out.printf("%.2f", elapsed / 100.0);

    System.out.println();
    System.out.println("Unsafe.park");
    Bench.printHeader();
    for (int i = 0; i < threadCounts.length; i++) {
      Bench.threaded(
          "Thread.sleep(0, 1)", threadCounts[i], 2, 1000, (threadId, cycle, iteration) -> {
            try {
              Thread.sleep(0, 1);
            } catch (InterruptedException e) {
              throw new RuntimeException(e);
            }
          });
      Bench.threaded("Thread.yield", threadCounts[i], 2, 1000000, (threadId, cycle, iteration) -> {
        Thread.yield();
      });
      Bench.threaded(
          "Thread.onSpinWait", threadCounts[i], 2, 1000000, (threadId, cycle, iteration) -> {
            Thread.onSpinWait();
          });
    }
    Bench.printHeader();
    System.out.println();
    System.out.println();
  }

  @Test
  public void benchmark() throws Throwable {
    final var spinLock = new SpinLock();
    final var reentrantLock = new java.util.concurrent.locks.ReentrantLock(false);
    //        final var threadCounts = new int[] {1, 2, 4, 6, 8, 10, 16, 24, 32, 64, 128};
    final var threadCounts = new int[] {1, 2, 4, 8, 16, 32};
    //                final var threadCounts = new int[] {32};

    System.out.println();

    System.out.println("Lock Benchmark");
    Bench.printHeader();
    for (int i = 0; i < threadCounts.length; i++) {
      Bench.threaded(
          "SpinLock",
          threadCounts[i],
          7,
          threadCounts[i] >= 32 ? 500_000 : 1_000_000,
          (threadId, cycle, iteration) -> {
            spinLock.lock();
            spinLock.unlock();
          });
      Bench.threaded(
          "SpinLock guard",
          threadCounts[i],
          7,
          threadCounts[i] >= 32 ? 500_000 : 1_000_000,
          (threadId, cycle, iteration) -> {
            try (var guard = spinLock.guard()) {}
          });
    }

    Bench.printSeparator();
    for (int i = 0; i < threadCounts.length; i++) {
      Bench.threaded(
          "ReentrantLock", threadCounts[i], 10, 1_000_000, (threadId, cycle, iteration) -> {
            reentrantLock.lock();
            reentrantLock.unlock();
          });
    }
    Bench.printHeader();
    System.out.println();
    System.out.println();
  }
}
