package bop.kernel;

import bop.bench.Bench;
import org.junit.jupiter.api.Test;

public class SignalTest {
  @Test
  public void set() {
    final var s = new Signal();
    s.set(26);
    System.out.println("leading zeroes: " + Long.numberOfTrailingZeros(s.value));
    System.out.println("trailing zeroes: " + Long.numberOfTrailingZeros(s.value >> 26));
    System.out.println("trailing zeroes: " + Long.numberOfTrailingZeros(s.value >> 27));

    System.out.println("nearest 37: " + s.findNearest(39));
    System.out.println("nearest 26: " + s.findNearest(26));
    System.out.println("nearest 15: " + s.findNearest(15));
    System.out.println("nearest 0: " + s.findNearest(0));
    System.out.println("nearest 63: " + s.findNearest(63));

    s.set(7);
    s.set(58);
    System.out.println("nearest 0: " + s.findNearest(0));
    System.out.println("nearest 63: " + s.findNearest(63));

    s.value = 0;
    System.out.println("nearest 26: " + s.findNearest(26));
    System.out.println("nearest 26: " + s.findNearest(0));
    System.out.println("nearest 26: " + s.findNearest(63));

    var index = 22;
    System.out.println("is set " + index + ":  " + s.isSet(index));
    System.out.println("set " + index + ":     " + s.set(index));
    System.out.println("set " + index + ":     " + s.set(index));
    System.out.println("is set " + index + ":  " + s.isSet(index));
    System.out.println("acquire " + index + ": " + s.acquire(index));
    System.out.println("acquire " + index + ": " + s.acquire(index));
    System.out.println("acquire " + index + ": " + s.acquire(index));
    System.out.println("set " + index + ":     " + s.set(index));
    System.out.println("is set " + index + ":  " + s.isSet(index));
    System.out.println("acquire " + index + ": " + s.acquire(index));
    System.out.println("is set " + index + ":  " + s.isSet(index));
    System.out.println("set " + index + ":     " + s.set(index));
    System.out.println();
  }

  @Test
  public void benchmarkSet() throws Throwable {
    final var signal = new Signal();

    Bench.printHeader();
    Bench.threaded("Signal.set", 1, 25, 1000000, (threadId, cycle, iteration) -> signal.set(0));
    Bench.threaded("Signal.set", 2, 25, 1000000, (threadId, cycle, iteration) -> signal.set(0));
    Bench.threaded("Signal.set", 4, 25, 1000000, (threadId, cycle, iteration) -> signal.set(0));
    Bench.threaded(
        "Signal.set", 4, 25, 1000000, (threadId, cycle, iteration) -> signal.set(threadId));
    Bench.printFooter();
  }

  @Test
  public void benchmarkFindNearest() throws Throwable {
    final var signal = new Signal();

    signal.set(20);

    Bench.printHeader();
    Bench.threaded(
        "Signal.findNearest 20|5",
        1,
        25,
        5000000,
        (threadId, cycle, iteration) -> signal.findNearest(5));
    Bench.threaded(
        "Signal.findNearest 20|38",
        1,
        25,
        5000000,
        (threadId, cycle, iteration) -> signal.findNearest(38));
    Bench.printFooter();
  }
}
