package bop.alloc;

import java.util.ArrayList;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;

public class Allocator<T> {
  public static final long PAGE_BYTES = Factory.estimateSizeOf(Block.class);
  public static final int PAGE_SIZE = 16;

  static final ArrayList<Object> EMPTY_ARGS_LIST = new ArrayList<>(0);
  final Local local;
  final Factory<T> factory;
  final long itemSize;
  final ReentrantLock lock = new ReentrantLock();
  final AtomicLong counter = new AtomicLong(0L);

  Block<Block<T>> full;
  Block<Block<T>> empty;
  Block<T> current;
  Block<T> overflow;

  long size = 0;
  long allocs = 0;
  long constructed = 0;
  long recycles = 0;
  long recycleCalled = 0;
  long pages = 0;
  long pageAllocs = 0;
  long count = 0;
  long reduceSize = 0;
  long reducePages = 0;

  //  volatile Stats stats = new Stats();
  //
  //  public static class Stats {
  //    long size = 0;
  //    long allocs = 0;
  //    long constructed = 0;
  //    long recycles = 0;
  //    long pages = 0;
  //    long pageAllocs = 0;
  //    long count = 0;
  //  }
  long reduceCount = 0;

  public Allocator(Local local, Factory<T> factory) {
    this.local = local;
    this.factory = factory;
    this.full = new Block<>(PAGE_SIZE);
    this.itemSize = factory.sizeOf;
  }

  public long gc(long targetBytes) {
    var pagesFreed = 0L;
    var itemsFreed = 0L;
    var bytesFreed = 0L;
    var itemSize = this.itemSize;

    for (var i = 0; i < PAGE_SIZE; i++) {
      var page = empty.poll();
      if (page == null) {
        break;
      }
      pagesFreed++;
      bytesFreed += PAGE_BYTES;
      if (targetBytes <= bytesFreed) {
        return bytesFreed;
      }
    }
    for (int p = 0; p < PAGE_SIZE; p++) {
      var page = full.poll();
      if (page == null) {
        break;
      }
      pagesFreed++;
      for (int i = 0; i < PAGE_SIZE; i++) {
        if (page.poll() == null) {
          break;
        }
        bytesFreed += PAGE_BYTES + itemSize;
        itemsFreed++;
      }
      if (targetBytes <= bytesFreed) {
        return bytesFreed;
      }
    }

    return bytesFreed;
  }

  public T allocate() {
    return allocate(EMPTY_ARGS_LIST);
  }

  public <A1> T allocate(A1 arg1) {
    local.args.clear();
    local.args.add(arg1);
    return allocate(local.args);
  }

  public T allocate(Object arg1, Object arg2) {
    local.args.clear();
    local.args.add(arg1);
    local.args.add(arg2);
    return allocate(local.args);
  }

  public T allocate(Object arg1, Object arg2, Object arg3) {
    local.args.clear();
    local.args.add(arg1);
    local.args.add(arg2);
    local.args.add(arg3);
    return allocate(local.args);
  }

  public T allocate(Object arg1, Object arg2, Object arg3, Object arg4) {
    local.args.clear();
    local.args.add(arg1);
    local.args.add(arg2);
    local.args.add(arg3);
    local.args.add(arg4);
    return allocate(local.args);
  }

  public T allocate(ArrayList<Object> args) {
    allocs++;
    if (current == null) {
      constructed++;
      return factory.newInstance(args);
    }
    T value = current.poll();
    if (value != null) {
      size -= itemSize;
      count--;
      return value;
    }
    if (empty == null) {
      pageAllocs++;
      empty = new Block<>(PAGE_SIZE);
    }

    if (overflow != null) {
      value = overflow.poll();
      if (value != null) {
        size -= itemSize;
        count--;
        var original = current;
        current = overflow;
        overflow = original;
        return value;
      }

      var next = full.poll();
      if (next != null) {
        value = next.poll();

        if (value != null) {
          size -= itemSize;
          count--;
          empty.offer(overflow);
          overflow = current;
          current = next;
          return value;
        }

        empty.offer(next);
        constructed++;
        return factory.newInstance(args);
      }

      constructed++;
      return factory.newInstance(args);
    }

    var next = full.poll();
    if (next != null) {
      value = next.poll();
      if (value != null) {
        overflow = current;
        current = next;
        return value;
      }
      overflow = next;
      constructed++;
      return factory.newInstance(args);
    }

    constructed++;
    return factory.newInstance(args);
  }

  public void recycle(T value) {
    recycleCalled++;
    if (current == null) {
      pageAllocs++;
      current = new Block<>(PAGE_SIZE);
    }
    if (current.offer(value)) {
      count++;
      size += itemSize;
      recycles++;
      return;
    }
    if (overflow == null) {
      pageAllocs++;
      overflow = new Block<>(PAGE_SIZE);
    }
    if (overflow.offer(value)) {
      count++;
      size += itemSize;
      recycles++;
      return;
    }
    if (full == null) {
      pageAllocs++;
      full = new Block<>(PAGE_SIZE);
    }
    full.offer(overflow);
    if (empty == null) {
      pageAllocs++;
      empty = new Block<>(PAGE_SIZE);
    }
    overflow = empty.poll();
    if (overflow == null) {
      pageAllocs++;
      overflow = new Block<>(PAGE_SIZE);
    }
    if (overflow.offer(value)) {
      count++;
      size += itemSize;
      recycles++;
    }
  }
}
