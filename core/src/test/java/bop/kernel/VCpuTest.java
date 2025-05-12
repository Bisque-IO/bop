package bop.kernel;

import bop.bench.Bench;
import java.text.DecimalFormat;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicLong;
import lombok.extern.slf4j.Slf4j;
import lombok.val;
import net.openhft.affinity.AffinityLock;
import org.junit.jupiter.api.Test;

@Slf4j
public class VCpuTest {
  static final AtomicLong COUNTER = new AtomicLong();
  static Object set;

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

  public static String withLargeIntegers(double value) {
    DecimalFormat df = new DecimalFormat("###,###,###");
    return df.format(value);
  }

  @Test
  public void benchLoop() throws InterruptedException {
    final var size = 32;
    final AtomicLong[] counters = new AtomicLong[size];
    final Callable[] tasks = new Callable[size];
    for (int i = 0; i < tasks.length; i++) {
      counters[i] = new AtomicLong();
      final var index = i;
      tasks[i] = () -> counters[index];
    }
    Bench.printHeader();
    for (int x = 0; x < 10; x++) {
      Bench.threaded("4096 Loop", 32, 1, 100000, (threadId, cycle, iteration) -> {
        for (int i = 0; i < tasks.length; i++) {
          //                      counters[threadId].incrementAndGet();
          try {
            set = tasks[threadId].call();
          } catch (Exception e) {
            throw new RuntimeException(e);
          }
        }
      });
    }
    Bench.printFooter();
    System.out.println(set);
  }

  @Test
  public void testBlocking() throws Throwable {
    final var group = new VCpu.Blocking(4096);

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

    for (int i = 0; i < 1000; i++) {
      Thread.sleep(350L);

      core.schedule();
    }
  }

  @Test
  public void benchmark1to2microsTask() throws Throwable {
    final var skip = 64;
    final var threadCount = 8;
    final var signalsPerThread = 32;
    final var iterations = 5;
    var cpu = new VCpu.Blocking(signalsPerThread * 64 * threadCount);

    for (int i = 0; i < cpu.cores.length; i += skip) {
      cpu.createCore(VCore.of(() -> {
        //        var value = cpuTask(ThreadLocalRandom.current().nextInt(1, 200));
        var value = cpuTask(1);
        ////        var value = 0L;
        if (value == 0) {
          COUNTER.incrementAndGet();
        }
        //        // ensure cpuTask does not get optimized away
        //        if (value == 0L) {
        //          return VCore.SCHEDULE;
        //        }
        return VCore.SCHEDULE;
      }));
    }

    for (int i = 0; i < cpu.cores.length; i += skip) {
      //    for (int i = 0; i < cpu.cores.length; i += 32) {
      if (cpu.cores[i] != null) {
        cpu.cores[i].schedule();
      }
    }

    {
      var s = Selector.simple();
      for (int i = 0; i < cpu.cores.length; i += skip) {
        cpu.execute(s);
      }
    }

    val threads = new Thread[threadCount];
    for (int i = 0; i < threads.length; i++) {
      final int id = i;
      threads[i] = new Thread(() -> {
        //        final var core = AffinityLock.acquireLock(id+1);
        final var core = AffinityLock.acquireCore(true);
        //        core.bind(true);
        //        core.acquireLock(AffinityLock.PROCESSORS);
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
      final var core = cpu.cores[p];
      if (core == null) continue;
      cpu.cores[p].contentionReset();
      cpu.cores[p].cpuTimeReset();
      cpu.cores[p].countReset();
      cpu.cores[p].exceptionsReset();
    }
    for (int i = 0; i < iterations; i++) {
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

  @FunctionalInterface
  public interface Supply {
    long get();
  }
}
