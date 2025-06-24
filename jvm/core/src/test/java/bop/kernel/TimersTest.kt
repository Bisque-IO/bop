package bop.kernel

import bop.bench.Bench
import bop.bench.Operation
import org.junit.jupiter.api.Test

class TimersTest {
   @Test
   fun benchmarkTimers() {
      val timers = Timers()

      Bench.printHeader()
      Bench.threaded(
         "Timers.poll",
         1,
         10,
         1000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> timers.poll(1024) })
      //    Bench.threaded("Timers.schedule", 1, 10, 1000000, (threadId, cycle, iteration) ->
      // timers.schedule(1024));
      Bench.printFooter()
   }

   @Test
   fun testTimersList() {
   }

   internal class TimersList
}
