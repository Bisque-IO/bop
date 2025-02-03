package bop.kernel;

import bop.bench.Bench;
import org.junit.jupiter.api.Test;

public class VCoreTest {
  @Test
  public void benchSchedule() throws Throwable {
    final var cpu = new VCpu.NonBlocking(64);
    final var core = cpu.createCore(VCore.of(() -> (byte) 0));

    Bench.printHeader();
    Bench.threaded(
        "VCore.schedule", 1, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule", 2, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule", 4, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule", 8, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule", 16, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule", 32, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
    Bench.printSeparator();

    final var thread = Thread.ofPlatform().start(() -> {
      var selector = Selector.random();
      while (!Thread.interrupted()) {
        cpu.execute(selector);
      }
    });

    Thread.sleep(10L);

    Bench.threaded(
        "VCore.schedule contended",
        1,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule contended",
        2,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule contended",
        4,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule contended",
        8,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule contended",
        16,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.threaded(
        "VCore.schedule contended",
        32,
        25,
        5000000,
        (threadId, cycle, iteration) -> core.schedule());
    Bench.printFooter();

    thread.interrupt();
    thread.join();
  }
}
