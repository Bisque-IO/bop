package bop.kernel

import bop.bench.Bench
import bop.bench.Operation
import org.junit.jupiter.api.Test
import java.lang.Long
import kotlin.Int
import kotlin.Throwable
import kotlin.Throws

class SignalTest {
   @Test
   fun set() {
      val s = Signal()
      s.set(26)
      println("leading zeroes: " + Long.numberOfTrailingZeros(s.value))
      println("trailing zeroes: " + Long.numberOfTrailingZeros(s.value shr 26))
      println("trailing zeroes: " + Long.numberOfTrailingZeros(s.value shr 27))

      println("nearest 37: " + s.nearest(39))
      println("nearest 26: " + s.nearest(26))
      println("nearest 15: " + s.nearest(15))
      println("nearest 0: " + s.nearest(0))
      println("nearest 63: " + s.nearest(63))

      s.set(7)
      s.set(58)
      println("nearest 0: " + s.nearest(0))
      println("nearest 63: " + s.nearest(63))

      s.value = 0
      println("nearest 26: " + s.nearest(26))
      println("nearest 26: " + s.nearest(0))
      println("nearest 26: " + s.nearest(63))

      val index = 22
      println("is set " + index + ":  " + s.isSet(index.toLong()))
      println("set " + index + ":     " + s.set(index.toLong()))
      println("set " + index + ":     " + s.set(index.toLong()))
      println("is set " + index + ":  " + s.isSet(index.toLong()))
      println("acquire " + index + ": " + s.acquire(index.toLong()))
      println("acquire " + index + ": " + s.acquire(index.toLong()))
      println("acquire " + index + ": " + s.acquire(index.toLong()))
      println("set " + index + ":     " + s.set(index.toLong()))
      println("is set " + index + ":  " + s.isSet(index.toLong()))
      println("acquire " + index + ": " + s.acquire(index.toLong()))
      println("is set " + index + ":  " + s.isSet(index.toLong()))
      println("set " + index + ":     " + s.set(index.toLong()))
      println()

      s.value = 0
      for (i in 0..<Signal.CAPACITY) {
         s.set(i.toLong())
      }

      println("size: " + s.size())

      for (i in 0..<Signal.CAPACITY) {
         s.acquire(i.toLong())
      }

      println("size: " + s.size())
   }

   @Test
   @Throws(Throwable::class)
   fun benchmarkSet() {
      val signal = Signal()

      Bench.printHeader()
      Bench.threaded(
         "Signal.set",
         1,
         25,
         1000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.set(0) })
      Bench.threaded(
         "Signal.set",
         2,
         25,
         1000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.set(0) })
      Bench.threaded(
         "Signal.set",
         4,
         25,
         1000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.set(0) })
      Bench.threaded(
         "Signal.set",
         4,
         25,
         1000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.set(threadId.toLong()) })
      Bench.printFooter()
   }

   @Test
   @Throws(Throwable::class)
   fun benchmarkNearest() {
      val signal = Signal()

      signal.set(20)

      Bench.printHeader()
      Bench.threaded(
         "Signal.findNearest 20|5",
         1,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.nearest(5) })
      Bench.threaded(
         "Signal.findNearest 20|38",
         1,
         25,
         5000000,
         Operation { threadId: Int, cycle: Int, iteration: Int -> signal.nearest(38) })
      Bench.printFooter()
   }
}
