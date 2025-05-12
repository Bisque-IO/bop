package bop.kernel;

import java.io.Closeable;
import java.text.DecimalFormat;
import java.util.concurrent.Executor;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;

public class VThreadTest {
  static class MyCoolThread extends Thread {
    @Override
    public void run() {
      var current = Thread.currentThread();
      System.out.println(current.getClass().getName());
    }
  }

  @Test
  public void testThreadCurrent() throws Throwable {
    var thread = new MyCoolThread();
    thread.start();
    thread.join();
  }

  @Test
  public void benchSuspendResume() throws Throwable {
    final var counter = new AtomicLong();
    Thread.ofPlatform().start(() -> {
      final var opsPerSecFormat = new DecimalFormat("###,###,###");
      for (int i = 0; i < 100000; i++) {
        try {
          Thread.sleep(1000L);
        } catch (InterruptedException e) {
          throw new RuntimeException(e);
        }
        var count = counter.getAndSet(0L);
        if (count == 0L) {
          continue;
        }
        if (count < 1000) {
          continue;
        }
        var perOp = 1_000_000_000L / count - 5;
        System.out.println(perOp + " ns/op    " + opsPerSecFormat.format(count) + " ops/sec");
      }
    });
    var thread = VThread.start(new InlineExecutor(), (vthread) -> {
      while (true) {
        vthread.suspend();
        counter.incrementAndGet();
        //                System.out.println("after yield: " + Thread.currentThread() + "   state: "
        // + vthread.threadState() + "   carrier thread: " + vthread.carrierThread());
      }
    });

    for (int i = 0; i < 100_000_000; i++) {
      thread.resume();
      //      System.out.println("after continuation: " + Thread.currentThread() + "   state: " +
      // thread.threadState() + "    carrier thread: " + thread.carrierThread());
      //      Thread.sleep(1000L);
    }
  }

  private static class InlineExecutor implements Closeable, Executor {
    @Override
    public void execute(Runnable command) {
      command.run();
    }

    @Override
    public void close() {}
  }
}
