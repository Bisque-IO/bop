package bop.kernel

import bop.bench.Bench
import bop.bench.Operation
import org.junit.jupiter.api.Test

class EpochTest {
   @Test
   fun epochBench() {
      Bench.printHeader()
      Bench.threaded(
         "Epoch.nanos",
         8,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> Epoch.nanos() })
      Bench.printSeparator()
      Bench.threaded(
         "System.nanoTime",
         8,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> System.nanoTime() })
      Bench.printSeparator()
      Bench.threaded(
         "System.currentTimeMillis",
         8,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> System.currentTimeMillis() })
      Bench.printFooter()
   }
}
