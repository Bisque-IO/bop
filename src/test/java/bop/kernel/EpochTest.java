package bop.kernel;

import bop.bench.Bench;
import org.junit.jupiter.api.Test;

public class EpochTest {
  @Test
  public void epochBench() throws Throwable {
    Bench.printHeader();
    Bench.threaded("Epoch.nanos", 16, 25, 5000000, (threadId, cycle, iteration) -> Epoch.nanos());
    Bench.printSeparator();
    Bench.threaded(
        "System.nanoTime", 16, 25, 5000000, (threadId, cycle, iteration) -> System.nanoTime());
    Bench.printSeparator();
    Bench.threaded(
        "System.currentTimeMillis",
        16,
        25,
        5000000,
        (threadId, cycle, iteration) -> System.currentTimeMillis());
    Bench.printFooter();
  }
}
