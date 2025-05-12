package bop.kernel;

import bop.alloc.Factory;

@Lifetime(Managed.MANUALLY)
public class Timer implements Runnable {
  public static final Factory<Timer> FACTORY = Factory.of(Timer.class);

  public static Timer alloc() {
    return FACTORY.alloc();
  }

  public long deadline;
  public long timerId;
  public Timer prev;
  public Timer next;
  public boolean canceled;
  public Runnable runnable;
  public volatile Timers owner;

  public Timer() {}

  void recycle() {
    deadline = 0;
    timerId = 0;
    prev = null;
    next = null;
    canceled = false;
    runnable = null;
    FACTORY.recycle(this);
  }

  public void cancel() {}

  @Override
  public void run() {
    final var runnable = this.runnable;
    try {
      if (runnable != null) {
        runnable.run();
      }
    } catch (Throwable t) {
      canceled = true;
    }
    final var next = this.next;
    this.next = null;
    prev = null;
    recycle();
  }
}
