package bop.kernel;

import bop.unsafe.Danger;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.util.concurrent.Executor;
import java.util.concurrent.locks.LockSupport;
import java.util.function.Consumer;
import jdk.internal.access.JavaLangAccess;
import jdk.internal.access.SharedSecrets;
import jdk.internal.misc.Unsafe;
import jdk.internal.vm.Continuation;
import jdk.internal.vm.ContinuationScope;

public class VThread implements Executor {
  /// Virtual thread state transitions:
  ///
  ///      NEW -> STARTED         // Thread.start, schedule to run
  ///  STARTED -> TERMINATED      // failed to start
  ///  STARTED -> RUNNING         // first run
  ///  RUNNING -> TERMINATED      // done
  ///
  ///  RUNNING -> PARKING         // Thread parking with LockSupport.park
  ///  PARKING -> PARKED          // cont.yield successful, parked indefinitely
  ///  PARKING -> PINNED          // cont.yield failed, parked indefinitely on carrier
  ///   PARKED -> UNPARKED        // unparked, may be scheduled to continue
  ///   PINNED -> RUNNING         // unparked, continue execution on same carrier
  /// UNPARKED -> RUNNING         // continue execution after park
  ///
  ///       RUNNING -> TIMED_PARKING   // Thread parking with LockSupport.parkNanos
  /// TIMED_PARKING -> TIMED_PARKED    // cont.yield successful, timed-parked
  /// TIMED_PARKING -> TIMED_PINNED    // cont.yield failed, timed-parked on carrier
  ///  TIMED_PARKED -> UNPARKED        // unparked, may be scheduled to continue
  ///  TIMED_PINNED -> RUNNING         // unparked, continue execution on same carrier
  ///
  ///   RUNNING -> BLOCKING       // blocking on monitor enter
  ///  BLOCKING -> BLOCKED        // blocked on monitor enter
  ///   BLOCKED -> UNBLOCKED      // unblocked, may be scheduled to continue
  /// UNBLOCKED -> RUNNING        // continue execution after blocked on monitor enter
  ///
  ///   RUNNING -> WAITING        // transitional state during wait on monitor
  ///   WAITING -> WAIT           // waiting on monitor
  ///      WAIT -> BLOCKED        // notified, waiting to be unblocked by monitor owner
  ///      WAIT -> UNBLOCKED      // timed-out/interrupted
  ///
  ///       RUNNING -> TIMED_WAITING   // transition state during timed-waiting on monitor
  /// TIMED_WAITING -> TIMED_WAIT      // timed-waiting on monitor
  ///    TIMED_WAIT -> BLOCKED         // notified, waiting to be unblocked by monitor owner
  ///    TIMED_WAIT -> UNBLOCKED       // timed-out/interrupted
  ///
  ///  RUNNING -> YIELDING        // Thread.yield
  /// YIELDING -> YIELDED         // cont.yield successful, may be scheduled to continue
  /// YIELDING -> RUNNING         // cont.yield failed
  ///  YIELDED -> RUNNING         // continue execution after Thread.yield
  ///
  public static final int NEW = 0;
  public static final int STARTED = 1;
  public static final int RUNNING = 2; // runnable-mounted

  // untimed and timed parking
  public static final int PARKING = 3;
  public static final int PARKED = 4; // unmounted
  public static final int PINNED = 5; // mounted
  public static final int TIMED_PARKING = 6;
  public static final int TIMED_PARKED = 7; // unmounted
  public static final int TIMED_PINNED = 8; // mounted
  public static final int UNPARKED = 9; // unmounted but runnable

  // Thread.yield
  public static final int YIELDING = 10;
  public static final int YIELDED = 11; // unmounted but runnable

  // monitor enter
  public static final int BLOCKING = 12;
  public static final int BLOCKED = 13; // unmounted
  public static final int UNBLOCKED = 14; // unmounted but runnable

  // monitor wait/timed-wait
  public static final int WAITING = 15;
  public static final int WAIT = 16; // waiting in Object.wait
  public static final int TIMED_WAITING = 17;
  public static final int TIMED_WAIT = 18; // waiting in timed-Object.wait

  public static final int TERMINATED = 99; // final state

  // can be suspended from scheduling when unmounted
  public static final int SUSPENDED = 1 << 8;

  public static final JavaLangAccess JLA;
  public static final ContinuationScope VTHREAD_SCOPE;
  static final long NEXT_OFFSET;
  static final long CARRIER_THREAD_OFFSET;
  static final long SCHEDULER_OFFSET;
  static final long STATE_OFFSET;
  static final long CONT_OFFSET;
  static final long RUN_CONTINUATION_OFFSET;
  static final long TASK_OFFSET;
  static final MethodHandle YIELD_CONTINUATION_HANDLE;
  static final MethodHandle SET_CURRENT_THREAD_HANDLE;
  static final MethodHandle VIRTUAL_THREAD_CTOR_HANDLE;
  static final Unsafe U = Danger.ZONE;

  static {
    try {
      JLA = SharedSecrets.getJavaLangAccess();
      //      SCHEDULED_OFFSET = fieldOffset("scheduled");
      var virtualThreadClass = Class.forName("java.lang.VirtualThread");
      {
        MethodHandle handle = null;
        try {
          var ctor = virtualThreadClass.getDeclaredConstructors()[0];
          ctor.setAccessible(true);
          handle = MethodHandles.lookup().unreflectConstructor(ctor);
        } catch (Throwable e) {
          // ignore.
        } finally {
          VIRTUAL_THREAD_CTOR_HANDLE = handle;
        }
      }
      MethodHandles.Lookup lookup = MethodHandles.lookup();
      {
        var yieldContinuationMethod = virtualThreadClass.getDeclaredMethod("yieldContinuation");
        yieldContinuationMethod.setAccessible(true);
        YIELD_CONTINUATION_HANDLE = lookup.unreflect(yieldContinuationMethod);
      }
      {
        var method = Thread.class.getDeclaredMethod("setCurrentThread", Thread.class);
        method.setAccessible(true);
        SET_CURRENT_THREAD_HANDLE = lookup.unreflect(method);
      }
      {
        var field = virtualThreadClass.getDeclaredField("VTHREAD_SCOPE");
        field.setAccessible(true);
        VTHREAD_SCOPE = (ContinuationScope) field.get(virtualThreadClass);
      }
      {
        var field = virtualThreadClass.getDeclaredField("carrierThread");
        field.setAccessible(true);
        CARRIER_THREAD_OFFSET = U.objectFieldOffset(field);
      }
      {
        var field = virtualThreadClass.getDeclaredField("scheduler");
        field.setAccessible(true);
        SCHEDULER_OFFSET = U.objectFieldOffset(field);
      }
      {
        var field = virtualThreadClass.getDeclaredField("state");
        field.setAccessible(true);
        STATE_OFFSET = U.objectFieldOffset(field);
      }
      {
        var field = virtualThreadClass.getDeclaredField("cont");
        field.setAccessible(true);
        CONT_OFFSET = U.objectFieldOffset(field);
      }
      {
        var field = virtualThreadClass.getDeclaredField("runContinuation");
        field.setAccessible(true);
        RUN_CONTINUATION_OFFSET = U.objectFieldOffset(field);
      }
      NEXT_OFFSET = U.objectFieldOffset(VThread.class.getDeclaredField("next"));
      TASK_OFFSET = U.objectFieldOffset(VThread.class.getDeclaredField("task"));
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public final Thread thread;
  public final Continuation cont;
  public final Runnable runContinuation;
  public Executor executor;
  volatile boolean scheduled;
  volatile VThread next;
  private volatile Runnable task;
  private int continuation;

  VThread(Executor executor) {
    this.executor = executor;
    this.thread = newVirtualThread(this, "", 0, this::run);
    this.cont = (Continuation) U.getReference(thread, CONT_OFFSET);
    this.runContinuation = (Runnable) U.getReference(thread, RUN_CONTINUATION_OFFSET);
  }

  VThread(Executor executor, Runnable task) {
    this.executor = executor;
    this.task = task;
    this.thread = newVirtualThread(this, "", 0, this::run);
    this.cont = (Continuation) U.getReference(thread, CONT_OFFSET);
    this.runContinuation = (Runnable) U.getReference(thread, RUN_CONTINUATION_OFFSET);
  }

  public static VThread current() {
    final var current = Thread.currentThread();
    if (!current.isVirtual()) {
      return null;
    }
    if (U.getReference(current, SCHEDULER_OFFSET) instanceof VThread vt) {
      return vt;
    }
    return null;
  }

  public static Thread newVirtualThread(
      Executor scheduler, String name, int characteristics, Runnable task) {
    if (VIRTUAL_THREAD_CTOR_HANDLE != null) {
      try {
        final var thread =
            (Thread) VIRTUAL_THREAD_CTOR_HANDLE.invoke(scheduler, name, characteristics, task);
        return thread;
      } catch (Throwable e) {
        // ignore
      }
    }
    // fallback to constructing normally and flip the scheduler manually
    var thread = Thread.ofVirtual().name(name).unstarted(task);
    U.putReference(thread, SCHEDULER_OFFSET, scheduler);
    return thread;
  }

  public static VThread start(Executor executor) {
    final var thread = new VThread(executor);
    thread.thread.start();
    thread.resume();
    return thread;
  }

  public static VThread start(Executor executor, Consumer<VThread> runnable) {
    final var thread = new VThread(executor);
    thread.task = () -> runnable.accept(thread);
    thread.thread.start();
    thread.resume();
    return thread;
  }

  @Override
  public void execute(Runnable command) {
    if (command != runContinuation) {
      return;
    }
    //    assert Thread.currentThread() != thread;
    if (Thread.currentThread() == thread) {
      // Schedule later
      return;
    }

    resume();
  }

  public void join() throws InterruptedException {
    thread.join();
  }

  public int state() {
    return U.getIntVolatile(thread, STATE_OFFSET);
  }

  public Thread carrierThread() {
    return (Thread) U.getReferenceVolatile(thread, CARRIER_THREAD_OFFSET);
  }

  void nextOrdered(final VThread next) {
    U.putReferenceRelease(this, NEXT_OFFSET, next);
  }

  public void park() {
    JLA.parkVirtualThread();
  }

  public boolean unpark() {
    JLA.unparkVirtualThread(thread);
    return true;
  }

  void setState(int state) {
    U.putIntVolatile(thread, STATE_OFFSET, state);
  }

  /// suspends this VThread only if Thread.currentThread() is this VThread
  public boolean suspend() {
    if (Thread.currentThread() != thread) {
      return false;
    }
    setState(SUSPENDED);
    return Continuation.yield(VTHREAD_SCOPE);
  }

  public void resume() {
    // Cannot be called from itself. It is already running.
    if (Thread.currentThread() == thread) {
      return;
    }
    switch (state()) {
      case SUSPENDED -> {
        try {
          mount();
          setState(RUNNING);
          cont.run();
        } catch (Throwable e) {
          try {
            onException(e);
          } catch (Throwable e2) {
            // ignore.
          }
        } finally {
          if (state() == VThread.RUNNING && !cont.isDone()) {
            setState(SUSPENDED);
          }
          unmount();
        }
      }

      case WAITING -> {}
      case RUNNING -> {}
      case PINNED -> {}

      case PARKED | PARKING -> unpark();

      default -> {
        try {
          runContinuation.run();
        } catch (Throwable e) {
          try {
            onException(e);
          } catch (Throwable e2) {
            // ignore.
          }
        }
      }
    }
  }

  protected void onException(Throwable e) {
    e.printStackTrace();
  }

  protected void onException(Runnable task, Throwable e) {
    e.printStackTrace();
  }

  boolean mount() {
    if (Thread.currentThread() == thread) {
      return true;
    }
    var carrierThread = JLA.currentCarrierThread();
    U.putReferenceVolatile(thread, CARRIER_THREAD_OFFSET, carrierThread);
    try {
      SET_CURRENT_THREAD_HANDLE.invokeExact(carrierThread, thread);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    return true;
  }

  boolean unmount() {
    var carrierThread = JLA.currentCarrierThread();
    U.putReferenceVolatile(thread, CARRIER_THREAD_OFFSET, null);
    try {
      SET_CURRENT_THREAD_HANDLE.invokeExact(carrierThread, carrierThread);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    return true;
  }

  private void run() {
    suspend();

    long start = 0L;
    while (!Thread.interrupted()) {
      var task = this.task;
      if (task == null) {
        this.suspend();
        continue;
      }

      start = Epoch.nanos();
      try {
        task.run();
      } catch (Throwable e) {
        onException(e);
      } finally {
        this.task = null;
        long elapsed = Epoch.nanos() - start;
      }
    }
  }

  public boolean run(Runnable task) {
    return U.compareAndSetReference(this, TASK_OFFSET, null, task);
  }

  void start(Runnable task) {
    this.task = task;
    LockSupport.unpark(thread);
  }

  /** @param continuation */
  void step(Runnable continuation) {
    this.continuation++;
    final long begin = Epoch.nanos();
    try {
      continuation.run();
    } finally {
      final long elapsed = Epoch.nanos() - begin;
    }
  }
}
