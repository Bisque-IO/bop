package bop.concurrent

import bop.bench.Bench
import bop.bench.Operation
import org.junit.jupiter.api.Test
import java.util.concurrent.locks.ReentrantLock

class SpinLockTest {
   //    @Test
   fun console() {
      SpinLock()
      ReentrantLock()
      val threadCounts = intArrayOf(
         1,
         2,
         4,  //      6,
         8,  //      10,
         //      12,
         //      14,
         16,  //      24,
         32,  //      64,
         //      128
      )

      println()
      println("Unsafe.park")
      Bench.printHeader()
      for (i in threadCounts.indices) {
         Bench.threaded(
            "Thread.sleep(0, 1)",
            threadCounts[i],
            2,
            1000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               try {
                  Thread.sleep(0, 1)
               } catch (e: InterruptedException) {
                  throw RuntimeException(e)
               }
            })
         Bench.threaded(
            "Thread.yield",
            threadCounts[i],
            2,
            1000000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               Thread.yield()
            })
         Bench.threaded(
            "Thread.onSpinWait",
            threadCounts[i],
            2,
            1000000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               Thread.onSpinWait()
            })
      }
      Bench.printHeader()
      println()
      println()
   }

   @Test
   fun benchmark() {
      val spinLock = SpinLock()
      val reentrantLock = ReentrantLock(false)
      //        final var threadCounts = new int[] {1, 2, 4, 6, 8, 10, 16, 24, 32, 64, 128};
      val threadCounts = intArrayOf(1, 2, 4, 8, 16, 32)

      //                final var threadCounts = new int[] {32};
      println()

      println("Lock Benchmark")
      Bench.printHeader()
      for (i in threadCounts.indices) {
         Bench.threaded(
            "SpinLock",
            threadCounts[i],
            7,
            if (threadCounts[i] >= 32) 500000 else 1000000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               spinLock.lock()
               spinLock.unlock()
            })
         Bench.threaded(
            "SpinLock guard",
            threadCounts[i],
            7,
            if (threadCounts[i] >= 32) 500000 else 1000000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               spinLock.guard().use { guard -> }
            })
      }

      Bench.printSeparator()
      for (i in threadCounts.indices) {
         Bench.threaded(
            "ReentrantLock",
            threadCounts[i],
            10,
            1000000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               reentrantLock.lock()
               reentrantLock.unlock()
            })
      }
      Bench.printHeader()
      println()
      println()
   }
}
