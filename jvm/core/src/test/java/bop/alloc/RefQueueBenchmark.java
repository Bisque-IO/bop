package bop.alloc;

import bop.concurrent.SpinLock;
import java.lang.ref.*;
import java.util.concurrent.atomic.AtomicInteger;

public class RefQueueBenchmark {

  static final ThreadLocal<PhantomRef> TLS = new ThreadLocal<>();
  private static final int NUM_OBJECTS = 1_000_000;

  public static void main(String[] args) throws Exception {
    var bg = new Thread(() -> {
      Object o = null;
      for (; ; ) {
        try {
          Thread.sleep(1000);
        } catch (InterruptedException e) {
          throw new RuntimeException(e);
        }
        System.gc();

        //        for (int i = 0; i < 10_000_000; i++) {
        //          o = new Object();
        //        }
        //        System.out.println(o);
      }
    });
    bg.setDaemon(true);
    //    bg.start();
    System.out.println("Warming up JVM...");
    benchmarkCleaner();
    benchmarkReferenceQueue();

    System.out.println("\nBenchmarking Cleaner:");
    for (int i = 0; i < 10; i++) {
      benchmarkCleaner();
    }

    System.out.println("\nBenchmarking ReferenceQueue:");
    for (int i = 0; i < 10; i++) {
      benchmarkReferenceQueue();
    }
  }

  private static void benchmarkCleaner() throws Exception {
    Cleaner cleaner = Cleaner.create();
    AtomicInteger counter = new AtomicInteger();
    long start = System.nanoTime();

    for (int i = 0; i < NUM_OBJECTS; i++) {
      Object obj = new Object();
      cleaner.register(obj, counter::incrementAndGet);
    }

    System.gc();
    waitForCount(counter);

    long duration = System.nanoTime() - start;
    printResult(duration, counter.get());
  }

  private static void benchmarkReferenceQueue() throws Exception {
    final ReferenceQueue<Object> queue = new ReferenceQueue<>();
    AtomicInteger counter = new AtomicInteger();
    final RefList refs = new RefList();
    Thread processor = new Thread(() -> {
      try {
        while (counter.get() < (NUM_OBJECTS)) {
          Reference<?> ref = queue.remove();
          if (ref != null) {
            refs.remove((PhantomRef) ref);
            counter.incrementAndGet();
          }
        }
      } catch (InterruptedException e) {
        Thread.currentThread().interrupt();
      }
    });
    processor.setDaemon(true);
    processor.start();

    long start = System.nanoTime();

    {
      PhantomRef r = null;

      for (int i = 0; i < NUM_OBJECTS; i++) {
        r = new PhantomRef(refs, new Object(), queue);
      }

      r = null;
      //      TLS.remove();
    }

    System.gc();

    processor.join();

    long duration = System.nanoTime() - start;
    printResult(duration, counter.get());
  }

  private static void waitForCount(AtomicInteger counter) throws InterruptedException {
    while (counter.get() < NUM_OBJECTS) {
      Thread.sleep(1);
    }
  }

  private static void printResult(long durationNano, int total) {
    double durationSec = durationNano / 1_000_000_000.0;
    double opsPerSec = total / durationSec;
    System.out.printf(
        "Processed %d cleanups in %.2f seconds → ~%.2f ops/sec%n", total, durationSec, opsPerSec);
  }

  static class MyObject extends PhantomRef {
    public MyObject(RefList list, Object referent, ReferenceQueue<? super Object> q) {
      super(list, referent, q);
    }
  }

  static class PhantomRef extends PhantomReference<Object> {
    public final long tid = Thread.currentThread().threadId();
    public final Thread thread = Thread.currentThread();
    /** The list of PhantomCleanable; synchronizes insert and remove. */
    private final RefList list;

    /**
     * Index of this PhantomCleanable in the list node. Synchronized by the same lock as the list
     * itself.
     */
    int index;

    /**
     * Node for this PhantomCleanable in the list. Synchronized by the same lock as the list itself.
     */
    RefList.Node node;

    public PhantomRef(RefList list, Object referent, ReferenceQueue<? super Object> q) {
      super(referent, q);
      this.list = list;
      list.insert(this);
      Reference.reachabilityFence(referent);
      Reference.reachabilityFence(q);
    }
  }

  /** A specialized implementation that tracks phantom cleanables. */
  static final class RefList {
    /**
     * Capacity for a single node in the list. This balances memory overheads vs locality vs GC
     * walking costs.
     */
    static final int NODE_CAPACITY = 2048;

    private final SpinLock lock = new SpinLock();
    /**
     * Head node. This is the only node where PhantomCleanables are added to or removed from. This
     * is the only node with variable size, all other nodes linked from the head are always at full
     * capacity.
     */
    private Node head;
    /**
     * Cached node instance to provide better behavior near NODE_CAPACITY threshold: if list size
     * flips around NODE_CAPACITY, it would reuse the cached node instead of wasting and
     * re-allocating a new node all the time.
     */
    private Node cache;

    public RefList() {
      reset();
    }

    /** Testing support: reset list to initial state. */
    void reset() {
      lock.lock();
      try {
        this.head = new Node();
      } finally {
        lock.unlock();
      }
    }

    /**
     * Returns true if cleanable list is empty.
     *
     * @return true if the list is empty
     */
    public boolean isEmpty() {
      lock.lock();
      try {
        // Head node size is zero only when the entire list is empty.
        return head.size == 0;
      } finally {
        lock.unlock();
      }
    }

    /** Insert this PhantomCleanable in the list. */
    public void insert(PhantomRef phc) {
      lock.lock();
      try {
        if (head.size == NODE_CAPACITY) {
          // Head node is full, insert new one.
          // If possible, pick a pre-allocated node from cache.
          Node newHead;
          if (cache != null) {
            newHead = cache;
            cache = null;
          } else {
            newHead = new Node();
          }
          newHead.next = head;
          head = newHead;
        }
        assert head.size < NODE_CAPACITY;

        // Put the incoming object in head node and record indexes.
        final int lastIndex = head.size;
        phc.node = head;
        phc.index = lastIndex;
        head.arr[lastIndex] = phc;
        head.size++;
      } finally {
        lock.unlock();
      }
    }

    /**
     * Remove this PhantomCleanable from the list.
     *
     * @return true if Cleanable was removed or false if not because it had already been removed
     *     before
     */
    public boolean remove(PhantomRef phc) {
      lock.lock();
      try {
        if (phc.node == null) {
          // Not in the list.
          return false;
        }
        assert phc.node.arr[phc.index] == phc;

        // Replace with another element from the head node, as long
        // as it is not the same element. This keeps all non-head
        // nodes at full capacity.
        final int lastIndex = head.size - 1;
        assert lastIndex >= 0;
        if (head != phc.node || (phc.index != lastIndex)) {
          PhantomRef mover = head.arr[lastIndex];
          mover.node = phc.node;
          mover.index = phc.index;
          phc.node.arr[phc.index] = mover;
        }

        // Now we can unlink the removed element.
        phc.node = null;

        // Remove the last element from the head node.
        head.arr[lastIndex] = null;
        head.size--;

        // If head node becomes empty after this, and there are
        // nodes that follow it, replace the head node with another
        // full one. If needed, stash the now free node in cache.
        if (head.size == 0 && head.next != null) {
          Node newHead = head.next;
          if (cache == null) {
            cache = head;
            cache.next = null;
          }
          head = newHead;
        }

        return true;
      } finally {
        lock.unlock();
      }
    }

    /** Segment node. */
    static class Node {
      // Array of tracked cleanables, and the amount of elements in it.
      final PhantomRef[] arr = new PhantomRef[NODE_CAPACITY];
      int size;

      // Linked list structure.
      Node next;
    }
  }
}
