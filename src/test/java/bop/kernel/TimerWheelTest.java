package bop.kernel;

import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import org.agrona.collections.Long2ObjectHashMap;
import org.junit.jupiter.api.Test;

public class TimerWheelTest {
  public static void main(String[] args) throws Throwable {
    System.out.println("running wheel...");
    runWheel();
    System.out.println();
    System.out.println("running scheduled...");
    runScheduledExecutor();
  }

  static void runScheduledExecutor() throws Throwable {
    var wheel = Executors.newScheduledThreadPool(1);
    AtomicLong counter = new AtomicLong();

    Runnable runnable = () -> {
      counter.decrementAndGet();
    };

    for (int i = 0; i < 5000; i++) {
      Thread.sleep(1L);

      for (int j = 0; j < 500; j++) {
        counter.incrementAndGet();
        wheel.schedule(runnable, 1L, TimeUnit.MILLISECONDS);
      }
    }
    Thread.sleep(10L);

    System.out.println(counter.get());
  }

  static void runWheel() throws Throwable {
    TimerWheel wheel = new TimerWheel(TimeUnit.MILLISECONDS, System.currentTimeMillis(), 512, 1024);
    AtomicLong counter = new AtomicLong();

    var byDeadline = new Long2ObjectHashMap<Timer>();

    wheel.scheduleTimer(System.currentTimeMillis() + TimeUnit.HOURS.toMillis(24));

    Runnable process = () -> {
      while (wheel.poll(
              System.currentTimeMillis(),
              (TimeUnit timeUnit, long now, long timerId, long deadline) -> {
                var entry = byDeadline.remove(deadline);
                if (entry == null) {
                  return true;
                }
                byDeadline.remove(entry.deadline);
                do {
                  entry.prev = null;
                  counter.decrementAndGet();
                  if (entry.next != null) {
                    entry = entry.next;
                    entry.prev.next = null;
                  } else {
                    entry = null;
                  }
                } while (entry != null);
                //          System.out.println(now + " " + timerId);
                return true;
              },
              1024)
          == 1024) {}
    };

    for (int i = 0; i < 2000; i++) {
      Thread.sleep(1L);
      for (int x = 0; x < 500; x++) {
        long deadline = System.currentTimeMillis();
        //        var timerId = wheel.scheduleTimer(deadline);
        //        if (timerId == TimerWheel.NULL_DEADLINE) {
        //          var existing = byTimer.get(deadline);
        //          if (existing == null) {
        //            existing = new CoTimer();
        //            existing.deadline = deadline;
        //            existing.timerId = deadline;
        //            byDeadline.put(deadline, existing);
        //            byTimer.put(existing.timerId, existing);
        //          }
        //        }
        var existing = byDeadline.get(deadline);
        if (existing == null) {
          counter.incrementAndGet();
          var timer = new Timer();
          timer.deadline = deadline;
          timer.timerId = wheel.scheduleTimer(deadline);
          var timerId0 = timer.timerId + wheel.startTime();
          byDeadline.put(deadline, timer);
        } else {
          counter.incrementAndGet();
          var timer = new Timer();
          timer.deadline = deadline;
          timer.timerId = existing.timerId;

          if (existing.next != null) {
            existing.next.prev = timer;
            timer.next = existing.next;
            timer.prev = existing;
            existing.next = timer;
          } else {
            existing.next = timer;
            timer.prev = existing;
          }
        }
      }

      process.run();
    }

    Thread.sleep(1L);

    while (!byDeadline.isEmpty()) {
      Thread.sleep(10L);
      process.run();
    }
    wheel.timerCount();

    System.out.println(counter.get());
  }

  @Test
  public void allocate() throws InterruptedException {
    //    TimerWheel wheel = new TimerWheel(TimeUnit.MILLISECONDS, System.currentTimeMillis(),
    // 1024,
    // 1024*16);
    //    AtomicLong counter = new AtomicLong();
    //
    //    var thread = new Thread(() -> {
    //      while (!Thread.interrupted()) {
    //        try {
    //          Thread.sleep(2L);
    //        } catch (InterruptedException e) {
    //          throw new RuntimeException(e);
    //        }
    //
    //        wheel.poll(System.currentTimeMillis(), (TimeUnit timeUnit, long now, long timerId)
    // ->
    // {
    //          counter.decrementAndGet();
    //          System.out.println(now + " " + timerId);
    //          return true;
    //        }, 1024);
    //      }
    //    });
    //    thread.start();
    //
    //    for (int i = 0; i < 5000; i++) {
    //      Thread.sleep(1L);
    //      counter.incrementAndGet();
    //      wheel.scheduleTimer(System.currentTimeMillis()+1);
    //      counter.incrementAndGet();
    //      wheel.scheduleTimer(System.currentTimeMillis()+1);
    //      counter.incrementAndGet();
    //      wheel.scheduleTimer(System.currentTimeMillis()+1);
    //    }
    //
    //    System.out.println(counter.get());

    var wheel = Executors.newScheduledThreadPool(1);
    AtomicLong counter = new AtomicLong();

    Runnable runnable = () -> {
      counter.decrementAndGet();
    };

    for (int i = 0; i < 5000000; i++) {
      //      Thread.sleep(1L);
      //
      counter.incrementAndGet();
      wheel.schedule(
          () -> {
            counter.decrementAndGet();
          },
          1L,
          TimeUnit.MILLISECONDS);
      counter.incrementAndGet();
      wheel.schedule(
          () -> {
            counter.decrementAndGet();
          },
          1L,
          TimeUnit.MILLISECONDS);
      counter.incrementAndGet();
      wheel.schedule(
          () -> {
            counter.decrementAndGet();
          },
          1L,
          TimeUnit.MILLISECONDS);
    }
    Thread.sleep(10L);

    System.out.println(counter.get());
  }
}
