package bop.kernel;

import bop.concurrent.SpinLock;
import java.util.ArrayList;
import java.util.concurrent.TimeUnit;
import org.agrona.collections.Long2ObjectHashMap;

public class Timers {
  final Long2ObjectHashMap<Timer> timers = new Long2ObjectHashMap<>();
  final TimerWheel wheel =
      new TimerWheel(TimeUnit.MILLISECONDS, System.currentTimeMillis(), 1024, 1024);
  final SpinLock lock = new SpinLock();
  final ArrayList<Timer> expired = new ArrayList<>(1024);
  final ExpiryHandler expiryHandler = new ExpiryHandler();

  public Timer schedule(long deadline) {
    var existing = timers.get(deadline);
    var timer = Timer.FACTORY.alloc();
    if (existing == null) {
      timer.deadline = deadline;
      timer.timerId = wheel.scheduleTimer(deadline);
      timers.put(deadline, timer);
    } else {
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
    return timer;
  }

  public int poll(int max) {
    int count = 0;
    while (count < max) {
      wheel.poll(System.currentTimeMillis(), expiryHandler, 1024);

      if (expired.isEmpty()) {
        return count;
      }

      if (expired.size() < 1024) {
        max = 0;
      }

      count += expired.size();

      for (int i = 0; i < expired.size(); i++) {
        expired.get(i).run();
        expired.get(i).recycle();
      }

      expired.clear();
    }
    return count;
  }

  private class ExpiryHandler implements TimerWheel.TimerHandler {
    @Override
    public boolean onTimerExpiry(TimeUnit timeUnit, long now, long timerId, long deadline) {
      final var timer = timers.get(deadline);
      if (timer != null) {
        expired.add(timer);
      }
      return true;
    }
  }
}
