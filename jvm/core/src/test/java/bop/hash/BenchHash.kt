package bop.hash

import bop.bench.Bench
import bop.bench.Operation
import bop.c.hash.RapidHash
import bop.c.hash.XXH3
import bop.io.DirectBytes
import bop.unsafe.Danger
import org.junit.jupiter.api.Test

class BenchHash {
   @Test
   fun bench() {
      //    rounds(1, 8, 12, 16, 24, 25, 32, 48, 64, 65, 127, 128, 256, 512);
      //    rounds(128);
//      rounds(128, 256, 512)
          rounds(1024, 4096)
   }

   private fun rounds(vararg lengths: Int) {
      Bench.printHeader()
      for (i in lengths.indices) {
         if (i > 0) {
            Bench.printSeparator()
         }
         //      round(lengths[i]);
         roundNative(lengths[i])
      }
      Bench.printFooter()
   }

   private fun round(length: Int) {
      val sb = StringBuilder(length)
      for (i in 0..<length) {
         sb.append('a')
      }
      val str = sb.toString()

      Bench.threaded(
         "rapid - String(" + length + ")",
         1,
         CYCLES,
         ITERS,
         Operation { threadId: Int, cycle: Int, iteration: Int -> RapidHash.hash(str) })

      //    Bench.printSeparator();
      //    Bench.threaded(
      //      "XXH3 - String(" + length + ")", 1, CYCLES, 10, (threadId, cycle, iteration) ->
      // XXH3.hash(str));
   }

   private fun roundNative(length: Int) {
      val sb = StringBuilder(length)
      for (i in 0..<length) {
         sb.append('a')
      }
      val str = sb.toString()
      val buf = DirectBytes.allocateDirect(str.length)
      buf.writeString(str)
      val bytes = Danger.getBytes(str)

      val result = LongArray(1)

      Bench.threaded(
         "rapid - native(" + length + ")",
         1,
         CYCLES,
         ITERS,
         Operation { threadId: Int, cycle: Int, iteration: Int ->
            result[0] = RapidHash.hash(buf.address, length.toLong())
         })
      //    Bench.threaded(
      //      "crc32 - NativeBuffer(" + length + ")", 1, CYCLES, ITERS, (threadId, cycle, iteration)
      // -> Crc32.INSTANCE.compute(buf.address(), 0, length));
      //    Bench.printSeparator();
      Bench.threaded(
         "XXH3 (j) - byte[" + length + "]",
         1,
         CYCLES,
         ITERS,
         Operation { threadId: Int, cycle: Int, iteration: Int ->
            result[0] = xxh3.hash(bytes)
         })
      Bench.threaded(
         "XXH3 (j) - native[" + length + "]",
         1,
         CYCLES,
         ITERS,
         Operation { threadId: Int, cycle: Int, iteration: Int ->
            result[0] = xxh3.hash(buf.address, 0, length)
         })
      Bench.threaded(
         "XXH3 - native(" + length + ")",
         1,
         CYCLES,
         ITERS,
         Operation { threadId: Int, cycle: Int, iteration: Int ->
            result[0] = XXH3.hash(buf.address, length.toLong())
         })

      println(result[0])
   }

   companion object {
      const val STR_15: String = "hello world!!!!"
      const val STR_16: String = "hello world!!!!!"
      val STR_64: String = STR_16 + STR_16 + STR_16 + STR_16

      val xxh3: XXH3_64 = XXH3_64.create()

      const val CYCLES: Int = 10
      const val ITERS: Int = 5000000
   }
}
