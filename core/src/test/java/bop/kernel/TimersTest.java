package bop.kernel;

import bop.bench.Bench;
import org.junit.jupiter.api.Test;

public class TimersTest {
  @Test
  public void benchmarkTimers() throws Throwable {
    final var timers = new Timers();

    Bench.printHeader();
    Bench.threaded(
        "Timers.poll", 1, 10, 1000000, (threadId, cycle, iteration) -> timers.poll(1024));
    //    Bench.threaded("Timers.schedule", 1, 10, 1000000, (threadId, cycle, iteration) ->
    // timers.schedule(1024));
    Bench.printFooter();
  }

  @Test
  public void testTimersList() {}

  static class TimersList {}
}
