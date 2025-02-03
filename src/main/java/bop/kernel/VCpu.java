package bop.kernel;

import bop.concurrent.SpinLock;
import java.util.ArrayList;
import jdk.internal.misc.Unsafe;
import jdk.internal.vm.annotation.Contended;
import org.agrona.BitUtil;

public abstract class VCpu {
  //      static final int CAPACITY = 4096 * 8;
  public static final int DEFAULT_CAPACITY = 8 * 8 * (8 * 8);

  private static final Unsafe UNSAFE = InvokeUtils.UNSAFE;
  private static final long NEXT_SIGNAL_INDEX_OFFSET;
  private static final long NON_ZERO_COUNTER_OFFSET;

  static {
    try {
      {
        var field = VCpu.class.getDeclaredField("signalIndexCounter");
        field.setAccessible(true);
        NEXT_SIGNAL_INDEX_OFFSET = UNSAFE.objectFieldOffset(field);
      }
      {
        var field = VCpu.class.getDeclaredField("nonZeroCounter");
        field.setAccessible(true);
        NON_ZERO_COUNTER_OFFSET = UNSAFE.objectFieldOffset(field);
      }
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  final long signalMapCount;
  final long signalMask;
  final Signal[] signals;
  final Signal[] assigned;
  final VCore[] cores;
  final long capacity;

  @Contended
  volatile long signalIndexCounter;

  @Contended
  volatile long nonZeroCounter;

  public VCpu(long capacity) {
    if (capacity < 64L) {
      capacity = 64L;
    }
    capacity = BitUtil.findNextPositivePowerOfTwo(capacity);
    this.capacity = capacity;
    this.signalMapCount = capacity / Signal.CAPACITY;
    this.signalMask = signalMapCount - 1L;
    this.signals = new Signal[(int) signalMapCount];
    this.assigned = new Signal[(int) signalMapCount];
    this.cores = new VCore[(int) (signalMapCount * Signal.CAPACITY)];
    this.signalIndexCounter = 0;
    this.nonZeroCounter = 0;

    for (int i = 0; i < assigned.length; i++) {
      signals[i] = new Signal();
      assigned[i] = new Signal();
    }
  }

  /// Select the next available VCore index if available and construct
  /// a new VCore from supplied builder.
  public <T extends VCore> T createCore(VCore.Builder<T> builder) {
    int signalIndex = -1;
    int coreId = -1;
    int selectIndex = -1;

    outer:
    for (int i = 0; i < assigned.length; i++) {
      signalIndex = (int) (UNSAFE.getAndAddLong(this, NEXT_SIGNAL_INDEX_OFFSET, 1) & signalMask);
      var signal = assigned[signalIndex];
      var count = signal.size();
      if (count == Signal.CAPACITY) {
        continue;
      }
      selectIndex = Long.numberOfLeadingZeros(signal.value);
      if (selectIndex > 0) {
        selectIndex = 64 - selectIndex;
        if (!signal.set(selectIndex)) {
          continue;
        }
        coreId = signalIndex * (int) Signal.CAPACITY + selectIndex;
        break;
      }

      for (selectIndex = 0; selectIndex < Signal.CAPACITY; selectIndex++) {
        if (!signal.isSet(selectIndex) && signal.set(selectIndex)) {
          coreId = signalIndex * (int) Signal.CAPACITY + selectIndex;
          break outer;
        }
      }
    }

    if (coreId == -1) {
      return null;
    }

    var signal = signals[signalIndex];
    var core = builder.build(this, coreId, signalIndex, signal, selectIndex);
    this.cores[coreId] = core;
    return core;
  }

  /// Increment the number of non-zero signals.
  abstract void incrNonZeroCounter();

  /// Decrement the number of non-zero signals.
  abstract void decrNonZeroCounter();

  /// select a signal (a set signal) from the array of signal trees and, if found,
  /// (which clears the signal) then process the pending action on that contract
  /// based on the flags associated with that contract.
  ///
  /// @param selector control iteration order for fairness
  /// @return selected index
  public long execute(Selector selector) {
    final var signalIndex = selector.nextSelect();
//    var index = (int)(Thread.currentThread().threadId() & 31);
    final var index = (int) (selector.map & signalMask);
    final var signal = signals[index];

    // Cache signal value.
    final var signalValue = signal.value;

    // Any signals?
    if (signalValue == 0) {
      // Go to the next map.
      selector.nextMap();
      return -1L;
    }

    // Select nearest index. This is guaranteed to succeed.
    final var selected = Signal.nearest(signalValue, signalIndex);

    final var bit = 1L << selected;
    final var expected = UNSAFE.getAndBitwiseAndLong(signal, Signal.VALUE_OFFSET, ~bit);
    final var acquired = (expected & bit) == bit;

    // Select contract.
    final var core = cores[index * 64 + (int) selected];
    if (core == null) {
      return -2L;
    }

    // Atomically acquire index
    if (!acquired) {
      // Select contract.
      core.incrContention();
      return -1L;
    }

    // Is the signal empty?
    if (expected == bit) {
      decrNonZeroCounter();
    }

    // Execute contract.
    core.resume();

    // Return selected index.
    return selected;
  }

  public static class NonBlocking extends VCpu {
    public NonBlocking(long capacity) {
      super(capacity);
    }

    final void incrNonZeroCounter() {}

    final void decrNonZeroCounter() {}
  }

  /// Blocking version of VCpu that blocks the current thread until
  /// there is a signal to process. This should the default since,
  /// the performance is almost identical and idle threads consume
  /// no additional CPU resources.
  public static class Blocking extends VCpu {
    final SpinLock lock = new SpinLock();
    // SpinLock condition objects do not spin when waiting for a signal.
    // No CPU cycles are wasted when waiting for any amount of time.
    final SpinLock.Cond condition = lock.newCondition();
    final ArrayList<Thread> threads = new ArrayList<>(256);

    public Blocking(long capacity) {
      super(capacity);
    }

    public void waitingThreads() {}

    public void signalAll() {
      lock.lock();
      try {
        condition.signalAll();
      } finally {
        lock.unlock();
      }
    }

    public void signal() {
      lock.lock();
      try {
        condition.signal();
      } finally {
        lock.unlock();
      }
    }

    public void await() throws InterruptedException {
      lock.lock();
      try {
        condition.await();
      } finally {
        lock.unlock();
      }
    }

    final void incrNonZeroCounter() {
      if (UNSAFE.getAndAddLong(this, NON_ZERO_COUNTER_OFFSET, 1) == 0) {
        signalAll();
      }
    }

    final void decrNonZeroCounter() {
      UNSAFE.getAndAddLong(this, NON_ZERO_COUNTER_OFFSET, -1);
    }

    /// Try to execute the next VCore as selected by supplied selector,
    /// optionally blocking current thread until at least 1 is available.
    public final long execute(Selector selector) {
      if (nonZeroCounter == 0L) {
        try {
          await();
        } catch (InterruptedException e) {
          throw new RuntimeException(e);
        }
      }
      return super.execute(selector);
    }
  }
}
