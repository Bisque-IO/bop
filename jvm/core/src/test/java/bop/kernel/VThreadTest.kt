package bop.kernel

import org.junit.jupiter.api.Test
import java.io.Closeable
import java.text.DecimalFormat
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicLong
import java.util.function.Consumer

class VThreadTest {
   internal class MyCoolThread : Thread() {
      override fun run() {
         val current = currentThread()
         println(current.javaClass.getName())
      }
   }

   @Test
   fun testThreadCurrent() {
      val thread = MyCoolThread()
      thread.start()
      thread.join()
   }

   @Test
   fun benchSuspendResume() {
      val counter = AtomicLong()
      Thread.ofPlatform().start(Runnable {
         val opsPerSecFormat = DecimalFormat("###,###,###")
         for (i in 0..99999) {
            try {
               Thread.sleep(1000L)
            } catch (e: InterruptedException) {
               throw RuntimeException(e)
            }
            val count = counter.getAndSet(0L)
            if (count == 0L) {
               continue
            }
            if (count < 1000) {
               continue
            }
            val perOp = 1000000000L / count - 5
            println(perOp.toString() + " ns/op    " + opsPerSecFormat.format(count) + " ops/sec")
         }
      })
      val thread = VThread.start(InlineExecutor(), Consumer { vthread: VThread? ->
         while (true) {
            vthread!!.suspend()
            counter.incrementAndGet()
            //                System.out.println("after yield: " + Thread.currentThread() + "   state: "
            // + vthread.threadState() + "   carrier thread: " + vthread.carrierThread());
         }
      })

      for (i in 0..100000000 - 1) {
         thread.resume()
         //      System.out.println("after continuation: " + Thread.currentThread() + "   state: " +
         // thread.threadState() + "    carrier thread: " + thread.carrierThread());
         //      Thread.sleep(1000L);
      }
   }

   private class InlineExecutor : Closeable, Executor {
      override fun execute(command: Runnable) {
         command.run()
      }

      override fun close() {}
   }
}
