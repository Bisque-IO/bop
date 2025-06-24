package bop.kernel

import bop.bench.Bench
import bop.bench.Operation
import bop.kernel.VCore.WithStep
import lombok.extern.slf4j.Slf4j
import net.openhft.affinity.AffinityLock
import org.junit.jupiter.api.Test
import java.text.DecimalFormat
import java.util.concurrent.Callable
import java.util.concurrent.atomic.AtomicLong

@Slf4j
class VCpuTest {
   @Test
   @Throws(InterruptedException::class)
   fun benchLoop() {
      val size = 32
      val counters = arrayOfNulls<AtomicLong>(size)
      val tasks = arrayOfNulls<Callable<*>>(size)
      for (i in tasks.indices) {
         counters[i] = AtomicLong()
         val index = i
         tasks[i] = Callable { counters[index] }
      }
      Bench.printHeader()
      for (x in 0..9) {
         Bench.threaded(
            "4096 Loop",
            32,
            1,
            100000,
            Operation { threadId: Int, cycle: Int, iteration: Int ->
               for (i in tasks.indices) {
                  //                      counters[threadId].incrementAndGet();
                  try {
                     set = tasks[threadId]!!.call()
                  } catch (e: Exception) {
                     throw RuntimeException(e)
                  }
               }
            })
      }
      Bench.printFooter()
      println(set)
   }

   @Test
   @Throws(Throwable::class)
   fun testBlocking() {
      val group = VCpu.Blocking(4096)

      val threads = arrayOfNulls<Thread>(64)
      for (i in threads.indices) {
         threads[i] = Thread(Runnable {
            try {
               val selector = Selector.random()
               while (true) {
                  group.execute(selector)
               }
            } catch (e: Throwable) {
               e.printStackTrace()
            } finally {
               println("thread: " + Thread.currentThread().threadId() + " is stopping")
            }
         })
         threads[i]!!.start()
      }

      val core = group.createCore<WithStep>(VCore.of(VCore.Step {
         println(Thread.currentThread().threadId())
         0.toByte()
      }))

      for (i in 0..999) {
         Thread.sleep(350L)

         core?.schedule()
      }
   }

   @Test
   fun benchmark1to2microsTask() {
      val skip = 8
      val threadCount = 24
      val signalsPerThread = 32
      val iterations = 5
      val cpu = VCpu.Blocking((signalsPerThread * 64 * threadCount).toLong())

      run {
         var i = 0
         while (i < cpu.cores.size) {
            cpu.createCore(VCore.of({
               //        var value = cpuTask(ThreadLocalRandom.current().nextInt(1, 200));
               val value: Long = cpuTask(1)
               if (value == 0L) {
                  COUNTER.incrementAndGet()
               }
               VCore.SCHEDULE
            }))
            i += skip
         }
      }

      run {
         var i = 0
         while (i < cpu.cores.size) {
            //    for (int i = 0; i < cpu.cores.length; i += 32) {
            if (cpu.cores[i] != null) {
               cpu.cores[i]?.schedule()
            }
            i += skip
         }
      }

      run {
         val s = Selector.simple()
         var i = 0
         while (i < cpu.cores.size) {
            cpu.execute(s)
            i += skip
         }
      }

      val threads = arrayOfNulls<Thread>(threadCount)
      for (i in threads.indices) {
         val id = i
         threads[i] = Thread(Runnable {
            //        final var core = AffinityLock.acquireLock(id+1);
            val core = AffinityLock.acquireCore(true)
            //        core.bind(true);
            //        core.acquireLock(AffinityLock.PROCESSORS);
            val selector = Selector.random()
            while (true) {
               cpu.execute(selector)
            }
         })
         threads[i]!!.start()
      }

      val worker = cpu.cores[0]
      val opStats =
         Bench.benchOp(Operation { threadId: Int, cycle: Int, iteration: Int -> worker?.step() })

      println()
      System.out.printf("%.2f ns / op%n", opStats.avg)
      printHeader()

      // clear stats
      for (p in cpu.cores.indices) {
         val core = cpu.cores[p]
         if (core == null) continue
         cpu.cores[p]?.contentionReset()
         cpu.cores[p]?.cpuTimeReset()
         cpu.cores[p]?.countReset()
         cpu.cores[p]?.exceptionsReset()
      }
      for (i in 0..<iterations) {
         Thread.sleep(1000L)
         var cpuTime: Long = 0
         var coreCount = 0
         var count = 0L
         var lostCount = 0L
         for (p in cpu.cores.indices) {
            val core = cpu.cores[p]
            if (core == null) {
               continue
            }
            cpuTime += core.cpuTimeReset()
            val c = core.countReset()
            if (c > 0L) {
               coreCount++
            }
            count += c
            lostCount += core.contentionReset()
         }
         //      count += cnt.sumThenReset();
         //      count = cnt.sumThenReset();
         val c = count / 1000000.0
         val l = lostCount / 1000000.0

         System.out.printf(
            "| %-12d | %-12d | %-10d | %-15s | %-15s | %-15s | %-15s | %-15s |%n",
            cpu.cores.size,
            coreCount,
            threads.size,
            withLargeIntegers(lostCount.toDouble()),
            withLargeIntegers((count / threads.size).toDouble()),
            withLargeIntegers(count.toDouble()),
            withLargeIntegers(cpuTime.toDouble()),
            withLargeIntegers((cpuTime / threads.size).toDouble())
         )
      }

      printFooter()
   }

   fun interface Supply {
      fun get(): Long
   }

   companion object {
      val COUNTER: AtomicLong = AtomicLong()
      var set: Any? = null

      fun unsignedLongMulXorFold(
         lhs: Long,
         rhs: Long
      ): Long {
         val upper = Math.multiplyHigh(lhs, rhs) + ((lhs shr 63) and rhs) + ((rhs shr 63) and lhs)
         val lower = lhs * rhs
         return lower xor upper
      }

      fun cpuTask(iterations: Int): Long {
         var seed = iterations.toLong()
         for (i in 0..<iterations) {
            seed += 0x2d358dccaa6c78a5L
            seed = unsignedLongMulXorFold(seed, seed xor -0x7447b46c69d15337L)
         }
         return seed
      }

      fun printHeader() {
         println(
            "======================================================================================================================================"
         )
         System.out.printf(
            "| %-12s | %-12s | %-10s | %-15s | %-15s | %-15s | %-15s | %-15s | %n",
            "cores",
            "cores used",
            "threads",
            "contended",
            "ops/thread",
            "ops/sec",
            "cpu time",
            "thread cpu time"
         )
         println(
            "======================================================================================================================================"
         )
      }

      fun printFooter() {
         println(
            "======================================================================================================================================"
         )
      }

      fun withLargeIntegers(value: Double): String? {
         val df = DecimalFormat("###,###,###")
         return df.format(value)
      }
   }
}
