package bop.bench;

import bop.kernel.Epoch;
import java.text.DecimalFormat;

public class Bench {
  public static class MultiThread {}

  public static void printTitle() {}

  public static void printHeader() {
    System.out.println(
        "===================================================================================================================================");
    System.out.printf(
        "| %-25s | %-12s | %-12s | %-12s | %-12s | %-18s | %-18s |%n",
        "name", "threads", "min", "max", "avg", "thread ops/sec", "ops/sec");
    System.out.println(
        "===================================================================================================================================");
  }

  public static void printFooter() {
    System.out.println(
        "===================================================================================================================================");
  }

  public static void printSeparator() {
    System.out.println(
        "| -------------------------------------------------------------------------------------------------------------------------------"
            + " |");
  }

  public static String withLargeIntegers(double value) {
    DecimalFormat df = new DecimalFormat("###,###,###");
    return df.format(value);
  }

  static void printStats(String name, ThreadRunner[] stats) {
    double min = Long.MAX_VALUE;
    double max = 0L;
    double total = 0L;

    for (int i = 0; i < stats.length; i++) {
      min = Math.min(min, stats[i].min);
      max = Math.max(max, stats[i].max);
      total += stats[i].total;
    }

    var avg = total / 1.0 / stats.length / stats[0].cycles / stats[0].iterations;

    System.out.printf(
        "| %-25s | %-12d | %-12.2f | %-12.2f | %-12.2f | %-18s | %-18s |%n",
        name,
        stats.length,
        min,
        max,
        avg,
        withLargeIntegers(1_000_000_000.0 / avg),
        withLargeIntegers(1_000_000_000.0 / avg * stats.length));
  }

  public static void threaded(
      String name, int threadCount, int cycles, int iterations, Operation op)
      throws InterruptedException {
    var threads = new ThreadRunner[threadCount];
    //    System.out.println(name + ": " + "running " + " with " + threadCount + " threads" +
    // "...");
    long begin = Epoch.nanos();
    for (int i = 0; i < threads.length; i++) {
      threads[i] = new ThreadRunner(i, cycles, iterations, op);
    }
    for (int i = 0; i < threads.length; i++) {
      threads[i].join();
    }
    long elapsed = Epoch.nanos() - begin;

    printStats(name, threads);
    //    System.out.println(name + ":  " + "elapsed: " + elapsed);
    //    System.out.println();
  }

  public static ThreadRunner benchOp(Operation op) {
    for (int i = 0; i < 10000; i++) {
      op.run(-1, -1, 0);
    }

    var started = Epoch.nanos();
    for (int i = 0; i < 10000; i++) {
      op.run(-1, -1, 0);
    }
    var elapsed = (Epoch.nanos() - started) / 10000;

    int iterations;
    if (elapsed < 100) {
      iterations = 2_000_000;
    } else if (elapsed < 200) {
      iterations = 1_000_000;
    } else if (elapsed < 500) {
      iterations = 500_000;
    } else if (elapsed < 1000) {
      iterations = 100_000;
    } else if (elapsed < 5000) {
      iterations = 50_000;
    } else if (elapsed < 10000) {
      iterations = 25_000;
    } else if (elapsed < 100000) {
      iterations = 2_500;
    } else {
      iterations = 1_000;
    }

    var tr = new ThreadRunner(0, 10, iterations, op);
    try {
      tr.thread.join();
    } catch (InterruptedException e) {
      throw new RuntimeException(e);
    }
    return tr;
  }

  public static class ThreadRunner {
    public long begin, end, total;
    public double min, max;
    public double avg;
    public double opsPerSec;
    Thread thread;
    Operation op;
    int id;
    int cycles;
    int iterations;

    public ThreadRunner(int id, int cycles, int iterations, Operation op) {
      this.id = id;
      this.cycles = cycles;
      this.iterations = iterations;
      this.op = op;
      this.max = 0L;
      this.min = Long.MAX_VALUE;
      this.thread = new Thread(this::run);
      this.thread.start();
    }

    private void run() {
      int id = this.id;
      int iterations = this.iterations;
      for (int i = 0; i < iterations; i++) {
        op.run(id, -1, i);
      }
      for (int cycle = 0; cycle < cycles; cycle++) {
        long start = Epoch.nanos();
        for (int i = 0; i < iterations; i++) {
          op.run(id, cycle, i);
        }
        long end = Epoch.nanos();
        long elapsed = end - start;
        double avg = (double) elapsed / (double) iterations;
        total += elapsed;
        max = Math.max(max, avg);
        min = Math.min(min, avg);
      }
      avg = (double) total / (double) iterations / (double) cycles;
      opsPerSec = 1_000_000_000.0 / avg;
    }

    public void begin() {
      begin = Epoch.nanos();
    }

    public void end() {
      end = Epoch.nanos();
      long elapsed = end - begin;
      max = Math.max(max, elapsed);
    }

    public void join() throws InterruptedException {
      thread.join();
    }
  }
}
