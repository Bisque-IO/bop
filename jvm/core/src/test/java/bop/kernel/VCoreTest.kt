package bop.kernel

import bop.bench.Bench
import bop.bench.Operation
import bop.kernel.VCore.WithStep
import org.junit.jupiter.api.Test

class VCoreTest {
   @Test
   fun benchSchedule() {
      val cpu = VCpu.NonBlocking(64)
      val core = cpu.createCore<WithStep>(VCore.of(VCore.Step { 0.toByte() }))

      Bench.printHeader()

      //    Bench.threaded(
      //        "VCore.schedule", 1, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.threaded(
      //        "VCore.schedule", 2, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.threaded(
      //        "VCore.schedule", 4, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.threaded(
      //        "VCore.schedule", 8, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.threaded(
      //        "VCore.schedule", 16, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.threaded(
      //        "VCore.schedule", 32, 25, 5000000, (threadId, cycle, iteration) -> core.schedule());
      //    Bench.printSeparator();
      val thread = Thread.ofPlatform().start(Runnable {
         val selector = Selector.random()
         while (!Thread.interrupted()) {
            cpu.execute(selector)
         }
      })

      Thread.sleep(10L)

      Bench.threaded(
         "VCore.schedule contended",
         1,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.threaded(
         "VCore.schedule contended",
         2,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.threaded(
         "VCore.schedule contended",
         4,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.threaded(
         "VCore.schedule contended",
         8,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.threaded(
         "VCore.schedule contended",
         16,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.threaded(
         "VCore.schedule contended",
         32,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> core.schedule() })
      Bench.printFooter()

      thread.interrupt()
      thread.join()
   }
}
