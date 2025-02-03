package bop.kernel;

import bop.bench.Bench;
import java.text.DecimalFormat;
import java.util.concurrent.ThreadLocalRandom;
import lombok.val;
import org.junit.jupiter.api.Test;

public class VCpuTest {
  static long unsignedLongMulXorFold(final long lhs, final long rhs) {
    final long upper = Math.multiplyHigh(lhs, rhs) + ((lhs >> 63) & rhs) + ((rhs >> 63) & lhs);
    final long lower = lhs * rhs;
    return lower ^ upper;
  }

  static long cpuTask(int iterations) {
    long seed = iterations;
    for (int i = 0; i < iterations; i++) {
      seed += 0x2d358dccaa6c78a5L;
      seed = unsignedLongMulXorFold(seed, seed ^ 0x8bb84b93962eacc9L);
    }
    return seed;
  }

  @Test
  public void testBlocking() throws Throwable {
    final var group = new VCpu.Blocking(64);

    var threads = new Thread[64];
    for (int i = 0; i < threads.length; i++) {
      threads[i] = new Thread(() -> {
        try {
          var selector = Selector.random();
          while (true) {
            group.execute(selector);
          }
        } catch (Throwable e) {
          e.printStackTrace();
        } finally {
          System.out.println("thread: " + Thread.currentThread().threadId() + " is stopping");
        }
      });
      threads[i].start();
    }

    var core = group.createCore(VCore.of(() -> {
      System.out.println(Thread.currentThread().threadId());
      return (byte) 0;
    }));

    for (int i = 0; i < 20; i++) {
      Thread.sleep(750L);

      core.schedule();
    }
  }

  static void printHeader() {
    System.out.println(
        "======================================================================================================================================");
    System.out.printf(
        "| %-12s | %-12s | %-10s | %-15s | %-15s | %-15s | %-15s | %-15s | %n",
        "cores",
        "cores used",
        "threads",
        "contended",
        "ops/thread",
        "ops/sec",
        "cpu time",
        "thread cpu time");
    System.out.println(
        "======================================================================================================================================");
  }

  static void printFooter() {
    System.out.println(
        "======================================================================================================================================");
  }

  @Test
  public void benchmark1to2microsTask() throws Throwable {
    var cpu = new VCpu.Blocking(VCpu.DEFAULT_CAPACITY);

    for (int i = 0; i < cpu.cores.length; i++) {
      cpu.createCore(VCore.of(() -> {
//        var value = cpuTask(ThreadLocalRandom.current().nextInt(1, 5));
////        var value = 0L;
//        // ensure cpuTask does not get optimized away
//        if (value == 0L) {
//          return VCore.SCHEDULE;
//        }
        return VCore.SCHEDULE;
      }));
    }

    for (int i = 0; i < cpu.cores.length; i++) {
      if (cpu.cores[i] != null) {
        cpu.cores[i].schedule();
      }
    }

    {
      var s = Selector.random();
      for (int i = 0; i < cpu.cores.length; i++) {
        cpu.execute(s);
      }
    }

    val threads = new Thread[32];
    for (int i = 0; i < threads.length; i++) {
      final int id = i;
      threads[i] = new Thread(() -> {
        final var selector = Selector.random();
        while (true) {
          cpu.execute(selector);
        }
      });
      threads[i].start();
    }

    final var worker = cpu.cores[0];
    var opStats = Bench.benchOp((threadId, cycle, iteration) -> worker.step());

    System.out.println();
    System.out.printf("%.2f ns / op%n", opStats.avg);
    printHeader();

    // clear stats
    for (int p = 0; p < cpu.cores.length; p++) {
      cpu.cores[p].contentionReset();
      cpu.cores[p].cpuTimeReset();
      cpu.cores[p].countReset();
      cpu.cores[p].exceptionsReset();
    }
    for (int i = 0; i < 10; i++) {
      Thread.sleep(1000L);
      long cpuTime = 0;
      int coreCount = 0;
      long count = 0L;
      long lostCount = 0L;
      for (int p = 0; p < cpu.cores.length; p++) {
        var core = cpu.cores[p];
        if (core == null) {
          continue;
        }
        cpuTime += core.cpuTimeReset();
        long c = core.countReset();
        if (c > 0L) {
          coreCount++;
        }
        count += c;
        lostCount += core.contentionReset();
      }
      //      count += cnt.sumThenReset();
      //      count = cnt.sumThenReset();
      double c = count / 1000000.0;
      double l = lostCount / 1000000.0;

      System.out.printf(
          "| %-12d | %-12d | %-10d | %-15s | %-15s | %-15s | %-15s | %-15s |%n",
          cpu.cores.length,
          coreCount,
          threads.length,
          withLargeIntegers((double) lostCount),
          withLargeIntegers((double) (count / threads.length)),
          withLargeIntegers(count),
          withLargeIntegers(cpuTime),
          withLargeIntegers(cpuTime / threads.length));
    }

    printFooter();
  }

  public static String withLargeIntegers(double value) {
    DecimalFormat df = new DecimalFormat("###,###,###");
    return df.format(value);
  }

  @FunctionalInterface
  public interface Supply {
    long get();
  }
}
