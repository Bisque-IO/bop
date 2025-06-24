package bop.alloc;

import bop.concurrent.SpinLock;
import java.lang.ref.PhantomReference;
import java.lang.ref.Reference;
import java.lang.ref.ReferenceQueue;
import java.util.concurrent.atomic.AtomicLong;

///
public class Finalizer {
  public static class Queue {
    private final AtomicLong count = new AtomicLong();
    private final ReferenceQueue<Object> queue = new ReferenceQueue<>();
    private final List list = new List();

    public boolean step() {
      final var handle = (Handle) queue.poll();
      if (handle == null) return false;
      count.incrementAndGet();
      try {
        handle.clean();
      } catch (Throwable e) {
        // TODO: Log
      }
      return true;
    }
  }

  public static class Handle extends PhantomReference<Object> {
    public final Thread thread = Thread.currentThread();
    /** The list of PhantomCleanable; synchronizes insert and remove. */
    private final List list;

    /**
     * Index of this PhantomCleanable in the list node. Synchronized by the same lock as the list
     * itself.
     */
    int index;

    /**
     * Node for this PhantomCleanable in the list. Synchronized by the same lock as the list itself.
     */
    List.Node node;

    public Handle(List list, Object referent, ReferenceQueue<? super Object> q) {
      super(referent, q);
      this.list = list;
      list.insert(this);
      Reference.reachabilityFence(referent);
      Reference.reachabilityFence(q);
    }

    /**
     * Unregister this PhantomCleanable and invoke {@link #performCleanup()}, ensuring at-most-once
     * semantics.
     */
    public final void clean() {
      if (list.remove(this)) {
        super.clear();
        performCleanup();
      }
    }

    /**
     * Unregister this PhantomCleanable and clear the reference. Due to inherent concurrency,
     * {@link #performCleanup()} may still be invoked.
     */
    public void clear() {
      if (list.remove(this)) {
        super.clear();
      }
    }

    protected void performCleanup() {}
  }

  /** A specialized implementation that tracks phantom cleanables. */
  public static final class List {
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

    public List() {
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
    public void insert(Handle phc) {
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
    public boolean remove(Handle phc) {
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
          Handle mover = head.arr[lastIndex];
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
      final Handle[] arr = new Handle[NODE_CAPACITY];
      int size;

      // Linked list structure.
      Node next;
    }
  }
}
