package bop.alloc

import bop.bench.Bench
import bop.bench.Operation
import bop.unsafe.Danger
import org.junit.jupiter.api.Test

class AllocatorTest {
   internal class Data {
      var a: Long = 0
      var b: Long = 0
      var c: Long = 0
      var d: Long = 0
      var e: Long = 0
      var f: Long = 0
      var g: Long = 0
      var h: Long = 0
      var s1: String? = null
      var s2: String? = null
      var s3: String? = null
      var s4: String? = null
      var l1: MutableList<String?>? = null
      var l2: MutableList<String?>? = null
      var l3: MutableList<String?>? = null
      var l4: MutableList<String?>? = null

      fun recycle() {
         //      a = 0;
         //      b = 0;
         //      c = 0;
         //      d = 0;
         //      e = 0;
         //      f = 0;
         //      g = 0;
         //      h = 0;
         //      s1 = null;
         //      s2 = null;
         //      s3 = null;
         //      s4 = null;
         //      l1 = null;
         //      l2 = null;
         //      l3 = null;
         //      l4 = null;
         FACTORY.recycle(this)
      }

      companion object {
         val FACTORY: Factory<Data?> = Factory.of<Data?>(Data::class.java)
      }
   }

   @Test
   @Throws(Throwable::class)
   fun benchFactory() {
      val list = arrayOfNulls<Data>(32 * 512)
      for (j in list.indices) {
         list[j] = Data()
      }

      val factory: Factory<Data?> = Data.Companion.FACTORY
      val allocators = arrayOfNulls<Allocator<*>>(64)
      val tls = ThreadLocal<Data?>()

      Bench.printHeader()
      Bench.threaded(
         "Recycled",
         1,
         10,
         10000000,
         Operation { threadId: Int, iteration: Int, index: Int ->
            val allocator = factory.allocator()
            val data = allocator.allocate()
            allocator.recycle(data)
         })
      Bench.threaded(
         "Recycled",
         2,
         10,
         10000000,
         Operation { threadId: Int, iteration: Int, index: Int ->
            val allocator = factory.allocator()
            val data = allocator.allocate()
            allocator.recycle(data)
         })
      Bench.threaded(
         "Recycled",
         4,
         10,
         10000000,
         Operation { threadId: Int, iteration: Int, index: Int ->
            val allocator = factory.allocator()
            val data = allocator.allocate()
            allocator.recycle(data)
         })

      //    Bench.threaded("Recycled", 8, 10, 10000000, (threadId, iteration, index) -> {
      //      final var allocator = factory.allocator();
      //      var data = allocator.allocate();
      //      allocator.recycle(data);
      //    });
      //    Bench.threaded("Recycled", 16, 50, 1000000, (threadId, iteration, index) -> {
      //      var data = factory.alloc();
      //      data.recycle();
      //    });
      //    Bench.threaded("Recycled", 32, 50, 1000000, (threadId, iteration, index) -> {
      //      var data = factory.alloc();
      //      data.recycle();
      //    });
      Bench.printSeparator()
      Bench.threaded("GC", 1, 10, 10000000, Operation { threadId: Int, iteration: Int, index: Int ->
         //      tls.set(new Data());
         list[threadId * 64] = Data()
      })
      Bench.threaded("GC", 2, 10, 10000000, Operation { threadId: Int, iteration: Int, index: Int ->
         list[threadId * 64] = Data()
      })
      Bench.threaded("GC", 4, 10, 10000000, Operation { threadId: Int, iteration: Int, index: Int ->
         list[threadId * 64] = Data()
      })

      //    Bench.threaded("GC", 8, 10, 10000000, (threadId, iteration, index) -> {
      //      list[threadId*64] = new Data();
      //    });

      //    Bench.printSeparator();
      //    Bench.threaded("snmalloc", 1, 10, 10000000, (threadId, iteration, index) -> {
      //      Memory.dealloc(Memory.alloc(16));
      //    });
      //    Bench.threaded("snmalloc", 2, 10, 10000000, (threadId, iteration, index) -> {
      //      Memory.dealloc(Memory.alloc(16));
      //    });
      //    Bench.threaded("snmalloc", 4, 10, 10000000, (threadId, iteration, index) -> {
      //      Memory.dealloc(Memory.alloc(16));
      //    });
      //    Bench.threaded("snmalloc", 8, 10, 10000000, (threadId, iteration, index) -> {
      //      Memory.dealloc(Memory.alloc(16));
      //    });
      //    Bench.threaded("GC", 16, 50, 1000000, (threadId, iteration, index) -> {
      //      tls.set(new Data());
      //    });
      //    Bench.threaded("GC", 32, 50, 1000000, (threadId, iteration, index) -> {
      //      tls.set(new Data());
      //    });
      Bench.printFooter()

      if (list[0] === list[1]) {
         println()
      }

      for (i in 0..31) {
         println(list[i])
      }
   }

   @Test
   fun hackString() {
      val bytes = "this is patched".toByteArray()
      val str = StringBuilder().append("hello world").toString()
      println(str)
      Danger.unsafeSetString(str, bytes)
      println(str)
   }
}
