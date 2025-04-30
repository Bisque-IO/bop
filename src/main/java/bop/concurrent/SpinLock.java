package bop.concurrent;

import bop.alloc.Factory;
import bop.unsafe.Danger;
import java.io.Closeable;
import java.io.Serializable;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Date;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.LockSupport;
import jdk.internal.misc.Unsafe;
import jdk.internal.vm.annotation.Contended;

/// A non-reentrant and unfair lock that "spins" while waiting to lock. The lock will regularly
/// {@linkplain Thread#yield() yield} if it's spinning too much. Because the only wait mechanism is
/// spinning, and no queues are used, the order of lock acquisition is non-deterministic (and thus
/// unfair, possibly resulting in starvation if highly contended).
///
/// Timed lock operations are discouraged unless the wait times are measured in microseconds or
/// smaller, since spinning for long periods of time is extremely wasteful of CPU resources. For
/// that reason, this lock should only be used when the critical sections in which it is held are
/// very short/fast. Due to the use of spinning with periodic yields, timeout precision when
/// attempting a timed acquisition is limited.
///
/// This lock is non-reentrant and will deadlock if re-entrance is accidentally attempted. This
/// lock does not have an exclusive owner thread, so it can be locked in one thread and then
/// unlocked in another. However, this is discouraged since it implies that the duration for which
/// the lock is held is non-deterministic. (See previous paragraph, about using this to guard very
/// short/fast critical sections.)
///
/// Unlike lock acquisitions, awaiting a {@link Condition} created by this lock does not use
/// spinning. Threads awaiting conditions are enqueued and will be notified in FIFO order by calls
/// to {@link Condition#signal()}.
///
/// Like normal locks, awaiting or signaling a condition requires that the lock be locked. But
/// since the lock has no concept of an "owner", it cannot be verified that it is the awaiting or
/// signaling thread that has locked it.
///
/// Awaiting a condition will unlock the lock before parking the awaiting thread. If that unlock
/// operation fails, an {@link IllegalMonitorStateException} is thrown. Signaling a condition just
/// does a best effort check that the lock is locked. If the lock is not locked at the onset of the
/// operation, an {@link IllegalMonitorStateException} is thrown. But if the lock is concurrently
/// unlocked by another thread, awaiting threads are still signaled.
public class SpinLock implements Lock, Serializable {

  /// The number of times to "spin" in a busy-wait loop before yielding.
  private static final int SPIN_TIMES = 2;

  private static final Unsafe U = Danger.UNSAFE;
  private static final long OWNER_OFFSET;

  static {
    try {
      {
        var field = SpinLock.class.getDeclaredField("owner");
        field.setAccessible(true);
        OWNER_OFFSET = U.objectFieldOffset(field);
      }
    } catch (Throwable e) {
      throw new ExceptionInInitializerError(e);
    }
  }

  final Guard guard = new Guard();

  @Contended
  volatile Thread owner = null;

  public SpinLock() {}

  public boolean isHeldByCurrentThread() {
    return owner == Thread.currentThread();
  }

  public Guard guard() {
    return guard.acquire();
  }

  /// {@inheritDoc}
  @Override
  public void lock() {
    if (U.compareAndSetReference(this, OWNER_OFFSET, null, Thread.currentThread())) {
      return;
    }

    final var newOwner = Thread.currentThread();
    long totalSpins = 0L;
    int spins = 0;
    while (true) {
      Thread.onSpinWait();
      if (U.compareAndSetReference(this, OWNER_OFFSET, null, newOwner)) {
        return;
      }

      U.park(
          false, ThreadLocalRandom.current().nextLong(2000 + (newOwner.threadId() & 1023), 5157));
    }
  }

  /// {@inheritDoc}
  @Override
  public void lockInterruptibly() throws InterruptedException {
    while (true) {
      for (int i = 0; i < SPIN_TIMES; i++) {
        if (U.compareAndSetReference(this, OWNER_OFFSET, null, Thread.currentThread())) {
          return;
        }
        Thread.onSpinWait();
      }

      Thread.sleep(0, 1);

      if (Thread.interrupted()) {
        throw new InterruptedException();
      }
    }
  }

  /// {@inheritDoc}
  @Override
  public boolean tryLock() {
    return U.compareAndSetReference(this, OWNER_OFFSET, null, Thread.currentThread());
  }

  /// {@inheritDoc}
  @Override
  public boolean tryLock(long time, TimeUnit unit) throws InterruptedException {
    final var deadline = System.nanoTime() + unit.toNanos(time);
    while (true) {
      if (Thread.interrupted()) {
        throw new InterruptedException();
      }
      for (int i = 0; i < SPIN_TIMES; i++) {
        if (U.compareAndSetReference(this, OWNER_OFFSET, null, Thread.currentThread())) {
          return true;
        }
        Thread.onSpinWait();
      }
      long nanosRemaining = deadline - System.nanoTime();
      if (nanosRemaining <= 0L) {
        return false;
      }
      Thread.sleep(0, 1);
    }
  }

  /// {@inheritDoc}
  @Override
  public void unlock() {
    if (!U.compareAndSetReference(this, OWNER_OFFSET, Thread.currentThread(), null)) {
      throw new IllegalMonitorStateException();
    }
    //        if (!UNSAFE.getAndSetBoolean(this, LOCKED_OFFSET, false)) {
    //          throw new IllegalMonitorStateException();
    //        }
    //    locked = false;
  }

  @Override
  public Cond newCondition() {
    return Cond.create(this);
  }

  public final class Guard implements Closeable {
    public Guard acquire() {
      lock();
      return this;
    }

    @Override
    public void close() {
      unlock();
    }
  }

  /**
   * A thread in a condition wait queue. In addition to the thread reference, this also tracks a
   * flag indicating whether the thread has been signaled or not.
   */
  private static class Waiter {
    static final Factory<Waiter> FACTORY = Factory.of(Waiter.class);
    Thread thread;
    volatile boolean signaled;

    Waiter() {}

    public static Waiter alloc(Thread thread) {
      var v = FACTORY.alloc();
      v.thread = thread;
      v.signaled = false;
      return v;
    }

    void recycle() {
      thread = null;
      signaled = false;
      FACTORY.recycle(this);
    }
  }

  /** A simple condition queue associated with a {@link SpinLock}. */
  public class Cond implements Condition {
    static final Factory<Cond> FACTORY = Factory.of(Cond.class);
    final ArrayDeque<Waiter> queue = new ArrayDeque<>(16);

    Cond() {}

    static Cond create(SpinLock lock) {
      return FACTORY.alloc(lock);
    }

    public void waitingThreads(ArrayList<Thread> threads) {
      if (threads == null) {
        return;
      }
      lock();
      try {
        queue.forEach(t -> threads.add(t.thread));
      } finally {
        unlock();
      }
    }

    /**
     * Ensures that the spin lock associated with this condition is locked.
     *
     * @throws IllegalMonitorStateException if the lock is not locked
     */
    private void checkLock() {
      if (owner != Thread.currentThread()) {
        throw new IllegalMonitorStateException();
      }
    }

    /**
     * Adds the given thread to the condition queue and unlocks this lock. If the lock is already
     * unlocked then an {@link IllegalMonitorStateException} and the thread will not be in the
     * queue.
     *
     * @param th a thread to enqueue
     */
    private void enqueueAndUnlock(Waiter th) {
      // We need the thread in the queue before the lock is released to prevent race
      // conditions
      // between await and signal, so add it first.
      queue.add(th);
      //      if (!UNSAFE.compareAndSetBoolean(SpinLock.this, LOCKED_OFFSET, true, false)) {
      if (!U.compareAndSetReference(SpinLock.this, OWNER_OFFSET, Thread.currentThread(), null)) {
        // but we can't leave the thread in the queue if the lock was in an invalid state,
        // so
        // remove before throwing
        if (!queue.remove(th)) {
          // a concurrent thread tried (or is trying) to signal this thread, so we need to
          // propagate that signal to another waiting thread so it's not lost
          signal();
        }

        th.recycle();
        throw new IllegalMonitorStateException();
      }
    }

    /// {@inheritDoc}
    @Override
    public void await() throws InterruptedException {
      checkLock();
      if (Thread.interrupted()) {
        throw new InterruptedException();
      }
      var th = Waiter.alloc(Thread.currentThread());
      var failed = false;
      enqueueAndUnlock(th);
      try {
        LockSupport.park(this);
        if (Thread.interrupted()) {
          failed = true;
          throw new InterruptedException();
        }
      } catch (RuntimeException | Error e) {
        failed = true;

        throw e;
      } finally {
        try {
          lock();
          if (!th.signaled) {
            // if removal fails, a concurrent thread tried (or is trying) to signal this
            // thread,
            // so mark this operation as failed, and we'll propagate the signal below
            failed = !queue.remove(th);
          }
          if (failed) {
            // we were de-queued and should have been signaled; but since we're throwing
            // instead, propagate signal to next waiter
            signal();
          }
        } finally {
          th.recycle();
        }
      }
    }

    /// {@inheritDoc}
    @Override
    public void awaitUninterruptibly() {
      checkLock();
      var th = Waiter.alloc(Thread.currentThread());
      boolean interrupted = false;
      boolean failed = false;
      enqueueAndUnlock(th);
      try {
        do {
          LockSupport.park(this);
          if (Thread.interrupted()) {
            // save interrupt status so we can restore on exit
            interrupted = true;
          }
          // loop until we've been signaled, ignoring wake-ups caused by interruption
        } while (!th.signaled);
      } catch (RuntimeException | Error e) {
        failed = true;
        throw e;
      } finally {
        try {
          lock();
          if (!th.signaled) {
            // if removal fails, a concurrent thread tried (or is trying) to signal this
            // thread,
            // so mark this operation as failed, and we'll propagate the signal below
            failed = !queue.remove(th);
          }
          if (failed) {
            // we were de-queued and should have been signaled; but since we're throwing
            // instead, propagate signal to next waiter
            signal();
          }
          // restore interrupt status on exit
          if (interrupted) {
            th.thread.interrupt();
          }
        } finally {
          th.recycle();
        }
      }
    }

    /// {@inheritDoc}
    @Override
    public long awaitNanos(long nanosTimeout) throws InterruptedException {
      checkLock();
      if (Thread.interrupted()) {
        throw new InterruptedException();
      }
      long start = System.nanoTime();
      Waiter th = Waiter.alloc(Thread.currentThread());
      boolean failed = false;
      long ret;
      enqueueAndUnlock(th);
      try {
        LockSupport.parkNanos(this, nanosTimeout);
        if (Thread.interrupted()) {
          failed = true;
          throw new InterruptedException();
        }
        ret = nanosTimeout - (System.nanoTime() - start);
      } catch (RuntimeException | Error e) {
        failed = true;
        throw e;
      } finally {
        try {
          lock();
          if (!th.signaled) {
            // if removal fails, a concurrent thread tried (or is trying) to signal this
            // thread,
            // so mark this operation as failed, and we'll propagate the signal below
            failed = !queue.remove(th);
          }
          if (failed) {
            // we were de-queued and should have been signaled; but since we're throwing
            // instead, propagate signal to next waiter
            signal();
          }
        } finally {
          th.recycle();
        }
      }
      return ret;
    }

    /// {@inheritDoc}
    @Override
    public boolean await(long time, TimeUnit unit) throws InterruptedException {
      return awaitNanos(unit.toNanos(time)) > 0;
    }

    /// {@inheritDoc}
    @Override
    public boolean awaitUntil(Date deadline) throws InterruptedException {
      Date now = new Date();
      return awaitNanos(TimeUnit.MILLISECONDS.toNanos(deadline.getTime() - now.getTime())) > 0;
    }

    /// {@inheritDoc}
    @Override
    public void signal() {
      checkLock();
      Waiter th = queue.poll();
      if (th != null) {
        th.signaled = true;
        LockSupport.unpark(th.thread);
      }
    }

    /// {@inheritDoc}
    @Override
    public void signalAll() {
      checkLock();
      while (true) {
        Waiter th = queue.poll();
        if (th == null) {
          return;
        }
        th.signaled = true;
        LockSupport.unpark(th.thread);
      }
    }
  }
}
