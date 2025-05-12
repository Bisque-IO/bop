package bop.kernel;

import bop.unsafe.Danger;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.Callable;
import java.util.concurrent.Executor;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.LockSupport;
import java.util.concurrent.locks.ReentrantLock;
import org.agrona.collections.Object2ObjectHashMap;

public class VThread0Test {
  static final Class<?> THREAD_BUILDERS_CLASS;
  static final Method NEW_VIRTUAL_THREAD;
  static final MethodHandle VIRTUAL_THREAD_RUN;
  static final MethodHandle VIRTUAL_THREAD_RUN_CONTINUATION;
  static final Class<?> VIRTUAL_THREAD_RUN_CONTINUATION_CLASS;
  static final long LAMBDA_ARG1_OFFSET;
  static final long CONT_OFFSET;

  static {
    try {
      var threadBuildersClass = Class.forName("java.lang.ThreadBuilders");
      THREAD_BUILDERS_CLASS = threadBuildersClass;
      var methods = threadBuildersClass.getDeclaredMethods();
      Method method = null;
      for (var m : methods) {
        if (m.getName().equals("newVirtualThread")) {
          method = m;
          break;
        }
      }
      if (method == null) {
        throw new RuntimeException("could not find newVirtualThread");
      }
      method.setAccessible(true);
      NEW_VIRTUAL_THREAD = method;

      var virtualThreadClass = Class.forName("java.lang.VirtualThread");
      Method runMethod = null;
      var vthreadScopeField = virtualThreadClass.getDeclaredField("VTHREAD_SCOPE");
      vthreadScopeField.setAccessible(true);

      var vthreadRunContinuationField = virtualThreadClass.getDeclaredField("runContinuation");
      vthreadRunContinuationField.setAccessible(true);
      try {
        VIRTUAL_THREAD_RUN_CONTINUATION =
            MethodHandles.lookup().unreflectGetter(vthreadRunContinuationField);
      } catch (IllegalAccessException e) {
        throw new RuntimeException(e);
      }
      //      VTHREAD_SCOPE = (ContinuationScope) vthreadScopeField.get(virtualThreadClass);
      try {
        runMethod = virtualThreadClass.getDeclaredMethod("run", Runnable.class);
        runMethod.setAccessible(true);
        VIRTUAL_THREAD_RUN = MethodHandles.lookup().unreflect(runMethod);
      } catch (NoSuchMethodException e) {
        throw new RuntimeException(e);
      } catch (IllegalAccessException e) {
        throw new RuntimeException(e);
      }
      var offset = Danger.objectFieldOffset(virtualThreadClass, "cont");
      CONT_OFFSET = offset;
    } catch (ClassNotFoundException e) {
      throw new RuntimeException(e);
    } catch (NoSuchFieldException e) {
      throw new RuntimeException(e);
    }

    final var thread = Thread.ofVirtual().unstarted(() -> {});

    try {
      var runContinuation = VIRTUAL_THREAD_RUN_CONTINUATION.invoke(thread);
      System.out.println("runContinuation");
      System.out.println(runContinuation.getClass().getSimpleName());
      VIRTUAL_THREAD_RUN_CONTINUATION_CLASS = runContinuation.getClass();
      var fields = runContinuation.getClass().getDeclaredFields();
      var arg1field = fields[0];
      arg1field.setAccessible(true);
      LAMBDA_ARG1_OFFSET = Danger.objectFieldOffset(arg1field);
      //        MethodHandles.lookup().unreflectGetter(arg1field);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public static Thread newVirtualThread(
      Executor scheduler, java.lang.String name, int characteristics, Runnable task) {
    try {
      final var thread = (Thread)
          NEW_VIRTUAL_THREAD.invoke(THREAD_BUILDERS_CLASS, scheduler, name, characteristics, task);
      //      UnsafeAccess.UNSAFE.putReferenceVolatile(thread, CONT_OFFSET, new ContinuationX(
      //        thread,
      //        VIRTUAL_THREAD_RUN,
      //        VTHREAD_SCOPE,
      //        task
      //      ));
      try {
        var runContinuation = VIRTUAL_THREAD_RUN_CONTINUATION.invoke(thread);
        var arg1 = Danger.ZONE.getReference(runContinuation, LAMBDA_ARG1_OFFSET);
        System.out.println(arg1);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }

      return thread;
    } catch (IllegalAccessException e) {
      throw new RuntimeException(e);
    } catch (InvocationTargetException e) {
      throw new RuntimeException(e);
    }
  }

  private static class SameThreadExecutor implements Executor {
    @Override
    public void execute(Runnable command) {
      //      System.out.println("EXECUTOR task -> BEFORE");
      command.run();
    }
  }

  public static void main(String[] args) throws InterruptedException {
    //    new VThreadTest().bench();
    new VThread0Test().parkUnpark();
  }

  final ReentrantLock lock = new ReentrantLock();

  private static class NanoTimed implements Runnable {
    private Runnable task;
    private long elapsed;

    @Override
    public void run() {
      final var begin = System.nanoTime();
      try {
        task.run();
      } finally {
        elapsed = begin - System.nanoTime();
      }
    }
  }

  //  @Test
  public void benchRunOnCarrierThread() throws InterruptedException {
    final AtomicLong counter = new AtomicLong(0L);
    final Callable callable = new Callable() {
      @Override
      public Object call() throws Exception {
        counter.incrementAndGet();
        return 0L;
      }
    };
    final var count = 10_000_000;
    //    final var count = 2;
    final var iterations = 5;
    final var vthread = newVirtualThread(
        //      new SameThreadExecutor(),
        new SimpleExecutor(),
        //      Executors.newSingleThreadExecutor(),
        //      Executors.newWorkStealingPool(32),
        "",
        0,
        () -> {
          for (int x = 0; x < iterations; x++) {
            final var start = System.nanoTime();
            for (int i = 0; i < count; i++) {
              try {
                counter.incrementAndGet();
              } catch (Exception e) {
                throw new RuntimeException(e);
              }
            }
            //          LockSupport.parkNanos(1L);
            final var elapsed = System.nanoTime() - start;
            System.out.println("Elapsed: " + elapsed);
            System.out.println("Nanos per Task: " + (elapsed / count));
            System.out.println("Counter: " + counter.get());
          }
        });
    vthread.start();
    vthread.join();
  }

  //  @Test
  public void bench() throws InterruptedException {
    final AtomicLong counter = new AtomicLong(0L);
    final var vthread = newVirtualThread(
        //      new SameThreadExecutor(),
        new SimpleExecutor(),
        //      Executors.newSingleThreadExecutor(),
        //      Executors.newWorkStealingPool(32),
        "",
        0,
        () -> {
          while (true) {
            //          JLA.parkVirtualThread();
            LockSupport.park();
            counter.incrementAndGet();
            //          Thread.yield();

            //          lock.lock();
            //          counter.incrementAndGet();
            //          lock.unlock();
            //        LockSupport.park(Thread.currentThread());
          }
        });
    vthread.start();

    final var count = 10_000_000;

    for (int i = 0; i < 10; i++) {
      final var start = System.nanoTime();

      while (counter.get() < count) {
        LockSupport.unpark(vthread);
        //        JLA.unparkVirtualThread(vthread);
      }
      final var elapsed = System.nanoTime() - start;
      System.out.println("Elapsed: " + elapsed);
      System.out.println("Nanos per Task: " + (elapsed / counter.get()));
      System.out.println("Count:  " + counter.get());
      counter.set(0L);
    }
  }

  private static class WrapRunnable implements Runnable {
    private final Runnable task;

    public WrapRunnable(Runnable task) {
      this.task = task;
    }

    @Override
    public void run() {
      final var begin = System.nanoTime();
      System.out.println("WRAPPED BEGIN!!!!!!!!!");
      try {
        task.run();
      } catch (Throwable e) {
        e.printStackTrace();
      } finally {
        final var elapsed = System.nanoTime() - begin;
        System.out.println("WRAPPED ELAPSED: " + elapsed);
        System.out.println("WRAPPED END!!!!!!!!!!!");
      }
    }
  }

  //  @Test
  public void parkUnpark() throws InterruptedException {
    int count = 1;
    int iterations = 10;

    final AtomicLong unparkCount = new AtomicLong();
    final var threads = new ArrayList<Thread>(count);
    final ReentrantLock lock = new ReentrantLock();
    final var executor = new SimpleExecutor();
    //    final var executor = Executors.newWorkStealingPool(8);
    for (int i = 0; i < count; i++) {
      final var thread = newVirtualThread(executor, "hi", 0, new WrapRunnable(() -> {
        for (int x = 0; x < iterations; x++) {
          int hash = 0;
          try {
            //          for (int ee = 0; ee < 10000; ee++) {
            for (; ; ) {
              //              Thread.yield();
              if (Thread.interrupted()) {
                throw new InterruptedException();
              }
              hash++;
              //            try {
              //              Thread.sleep(500);
              //            } catch (InterruptedException e) {
              //              throw new RuntimeException(e);
              //            }
            }
          } catch (Exception e) {
            System.out.println("CAUGHT EXCEPTION: " + e);
            break;
          }
          //          System.out.println(
          //            JLA.currentCarrierThread().threadId() + "
          // : " +
          // Thread.currentThread().threadId() +
          //              ": parking");
          //          LockSupport.park(this);
          //          unparkCount.incrementAndGet();
          //          System.out.println(
          //            JLA.currentCarrierThread().threadId() + "
          // : " +
          // Thread.currentThread().threadId() +
          //              ": unparked");
        }
        System.out.println("DONE!!!");
      }));
      threads.add(thread);
      thread.start();
      //      threads.add(Thread.ofVirtual().start(() -> {
      //        while (true) {
      //          System.out.println(Thread.currentThread().threadId() + ": parking");
      ////          LockSupport.park(this);
      //          JLA.parkVirtualThread();
      //          System.out.println(Thread.currentThread().threadId() + ": unparked");
      //        }
      //      }));
    }

    var runnerThread = Thread.ofPlatform().start(() -> {
      outer:
      while (unparkCount.get() < iterations) {
        final var thread = threads.get(0);
        if (!thread.isAlive() || thread.isInterrupted()) {
          break;
        }

        switch (thread.getState()) {
          case NEW -> {
            System.out.println("THREAD STATE: NEW");
            try {
              Thread.sleep(1);
            } catch (InterruptedException e) {
              throw new RuntimeException(e);
            }
          }
          case RUNNABLE -> {
            System.out.println("THREAD STATE: RUNNABLE");
            try {
              thread.interrupt();
              Thread.sleep(1);
            } catch (InterruptedException e) {
              throw new RuntimeException(e);
            }
            continue;
          }
          case BLOCKED -> {
            System.out.println("THREAD STATE: BLOCKED");
          }
          case WAITING -> {
            System.out.println("THREAD STATE: WAITING");
          }
          case TIMED_WAITING -> {
            System.out.println("THREAD STATE: TIMED_WAITING");
          }
          case TERMINATED -> {
            System.out.println("THREAD STATE: TERMINATED");
            break outer;
          }
        }

        LockSupport.unpark(thread);
        try {
          Thread.sleep(1);
        } catch (InterruptedException e) {
          throw new RuntimeException(e);
        }
      }
    });

    runnerThread.join();

    //    threads.get(0).interrupt();
    threads.get(0).join();

    System.out.flush();
  }

  private static class SimpleExecutor implements Executor {
    //    ConcurrentLinkedQueue<Runnable> queue = new ConcurrentLinkedQueue<>();
    //    ManyToOneConcurrentArrayQueue<Runnable> queue = new
    // ManyToOneConcurrentArrayQueue<>(1024 *
    // 4);
    ArrayBlockingQueue<Runnable> queue = new ArrayBlockingQueue<>(1024 * 4);

    Object2ObjectHashMap<Object, Object> vthreadTypes = new Object2ObjectHashMap<>();
    Object2ObjectHashMap<Class<?>, Object> taskTypes = new Object2ObjectHashMap<>();

    Thread thread;

    public SimpleExecutor() {
      thread = Thread.ofPlatform().unstarted(this::run);
      thread.start();
    }

    @Override
    public void execute(Runnable command) {
      while (!queue.offer(command)) {
        Thread.yield();
      }
    }

    private void run() {
      //      try (var affinityLock = AffinityLock.acquireLock(31)) {
      Runnable task = null;
      while (true) {
        task = queue.poll();
        if (task == null) {
          continue;
        }
        //        if (task.getClass().equals(VIRTUAL_THREAD_RUN_CONTINUATION_CLASS)) {
        //          System.out.println("VIRTUAL_THREAD CONTINUATION TASK");
        //        }
        try {
          //          System.out.println(task.getClass().getSimpleName());
          //          System.out.println("Executor.task BEGIN");
          task.run();
        } catch (Throwable e) {
          //          e.printStackTrace();
        }
      }
      //      }
    }
  }
}
