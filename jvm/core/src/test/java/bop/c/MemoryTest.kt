package bop.c

import bop.bench.Bench
import bop.bench.Operation
import org.junit.jupiter.api.Test
import java.util.function.IntConsumer
import java.util.stream.IntStream

class MemoryTest {
   @Test
   fun benchFFMOverhead() {
      Bench.printHeader()
      IntStream.of(1, 2, 4, 8, 16).forEach(IntConsumer { threads: Int ->
         try {
            Bench.threaded(
               "c stub",
               threads,
               10,
               10000000,
               Operation { threadId: Int, cycle: Int, iteration: Int ->
                  Memory.hello()
               })
         } catch (e: InterruptedException) {
            throw RuntimeException(e)
         }
      })
      Bench.printFooter()
   }

   @Test
   fun alloc() {
      val address = Memory.alloc(12)
      println(address)
      Memory.dealloc(address)
   }

   @Test
   fun benchHeapAccess() {
      Bench.printHeader()
      IntStream.of(1, 2, 4, 8).forEach(IntConsumer { threads: Int ->
         try {
            Bench.threaded(
               "bop_heap_access",
               threads,
               25,
               10000000,
               Operation { threadId: Int, cycle: Int, iteration: Int ->
                  var d: ByteArray? = BYTE_LOCAL.get()
                  if (d == null) {
                     d = ByteArray(16)
                     BYTE_LOCAL.set(d)
                  }
                  Memory.heapAccess(d)
               })
         } catch (e: InterruptedException) {
            throw RuntimeException(e)
         }
      })
      Bench.printFooter()
   }

   @Test
   fun benchAllocDealloc() {
      Bench.printHeader()
      IntStream.of(1, 2, 4, 8, 16).forEach(IntConsumer { threads: Int ->
         try {
            Bench.threaded(
               "bop_alloc/bop_dealloc",
               threads,
               10,
               10000000,
               Operation { threadId: Int, cycle: Int, iteration: Int ->
                  Memory.dealloc(
                     Memory.alloc(16L)
                  )
               })
         } catch (e: InterruptedException) {
            throw RuntimeException(e)
         }
      })
      Bench.printFooter()
   }

   @Test
   fun benchZallocDealloc() {
      Bench.printHeader()
      IntStream.of(1, 2, 4, 8, 16).forEach(IntConsumer { threads: Int ->
         try {
            Bench.threaded(
               "bop_alloc/bop_dealloc",
               threads,
               10,
               10000000,
               Operation { threadId: Int, cycle: Int, iteration: Int ->
                  Memory.dealloc(
                     Memory.zalloc(16L)
                  )
               })
         } catch (e: InterruptedException) {
            throw RuntimeException(e)
         }
      })
      Bench.printFooter()
   }

   companion object {
      val BYTE_LOCAL: ThreadLocal<ByteArray?> = ThreadLocal<ByteArray?>()
   }
}
