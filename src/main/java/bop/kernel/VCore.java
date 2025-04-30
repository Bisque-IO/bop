package bop.kernel;

import bop.unsafe.Danger;
import jdk.internal.misc.Unsafe;
import jdk.internal.vm.annotation.Contended;

/// Thread-safe container for arbitrary code execution.
public class VCore {
  public static final byte EMPTY = 0;
  public static final byte SCHEDULE = 0x00000001;
  public static final byte EXECUTE = 0x00000002;
  static final long FLAGS_OFFSET;
  static final long COUNTER_OFFSET;
  static final long CPU_TIME_OFFSET;
  static final long CONTENTION_OFFSET;
  static final long EXCEPTIONS_OFFSET;
  private static final Unsafe U = Danger.UNSAFE;

  static {
    try {
      FLAGS_OFFSET = U.objectFieldOffset(VCore.class.getDeclaredField("flags"));
      CPU_TIME_OFFSET = U.objectFieldOffset(VCore.class.getDeclaredField("cpuTime"));
      COUNTER_OFFSET = U.objectFieldOffset(VCore.class.getDeclaredField("counter"));
      CONTENTION_OFFSET = U.objectFieldOffset(VCore.class.getDeclaredField("contention"));
      EXCEPTIONS_OFFSET = U.objectFieldOffset(VCore.class.getDeclaredField("exceptions"));
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
  }

  public final int signalIndex;
  public final int select;
  public final VCpu owner;
  public final Signal signal;
  public final long id;

  @Contended
  volatile byte flags;

  @Contended
  volatile long counter;

  @Contended
  volatile long cpuTime;

  long cpuTime2;

  @Contended
  volatile long contention;

  @Contended
  volatile long exceptions;

  Throwable exception;

  public VCore(VCpu owner, long id, int signalIndex, Signal signal, int selectIndex) {
    this.owner = owner;
    this.id = id;
    this.signalIndex = signalIndex;
    this.signal = signal;
    this.select = selectIndex;
  }

  public static Builder<WithStep> of(final Step step) {
    return (VCpu owner, long id, int signalIndex, Signal signal, int selectIndex) ->
        new WithStep(owner, id, signalIndex, signal, selectIndex, step);
  }

  long cpuTimeAdd(long delta) {
    return U.getAndAddLong(this, CPU_TIME_OFFSET, delta);
  }

  public long cpuTime() {
    return cpuTime;
  }

  public long cpuTimeReset() {
    return U.getAndSetLong(this, CPU_TIME_OFFSET, 0);
  }

  long incrCounter() {
    return U.getAndAddLong(this, COUNTER_OFFSET, 1);
  }

  long incrContention() {
    return U.getAndAddLong(this, CONTENTION_OFFSET, 1);
  }

  long incrExceptions() {
    return U.getAndAddLong(this, EXCEPTIONS_OFFSET, 1);
  }

  /// The number of resumes since last reset.
  public long count() {
    return exceptions;
  }

  public long countReset() {
    return U.getAndSetLong(this, COUNTER_OFFSET, 0);
  }

  public long exceptionsCount() {
    return exceptions;
  }

  public long exceptionsReset() {
    return U.getAndSetLong(this, EXCEPTIONS_OFFSET, 0);
  }

  public long contentionCount() {
    return contention;
  }

  public long contentionReset() {
    return U.getAndSetLong(this, CONTENTION_OFFSET, 0);
  }

  public boolean isScheduled() {
    return (flags & SCHEDULE) != 0;
  }

  /// Schedule this VCore to resume processing.
  public void schedule() {
    if ((flags & SCHEDULE) != 0) return;
    final var previousFlags = U.getAndBitwiseOrByte(this, FLAGS_OFFSET, SCHEDULE);
    final var notScheduledNorExecuting = (previousFlags & (SCHEDULE | EXECUTE)) == 0;
    if (notScheduledNorExecuting) {
      final var bit = 1L << select;
      final var prev = U.getAndBitwiseOrLong(signal, Signal.VALUE_OFFSET, bit);
      if (prev == 0L) {
        owner.incrNonZeroCounter();
      }
    }
  }

  /// step is the primary method where more CPU bound work is performed.
  protected byte step() {
    return 0;
  }

  protected void onException(Throwable e) {}

  /// Resumes execution.
  byte resume() {
    //    final var start = Epoch.nanos();
    byte result = 0;
    try {
      incrCounter();
      flags = EXECUTE;
      result = step();
    } catch (Throwable e) {
      exception = e;
      incrExceptions();
      try {
        onException(e);
      } catch (Throwable e2) {
        // ignore
      }
    } finally {
      //      U.getAndAddLong(this, CPU_TIME_OFFSET, System.nanoTime() - start);
      //      U.getAndAddLong(this, CPU_TIME_OFFSET, Epoch.nanos() - start);
    }
    return result;
  }

  @FunctionalInterface
  public interface Builder<T extends VCore> {
    T build(VCpu owner, long id, int signalIndex, Signal signal, int selectIndex);
  }

  @FunctionalInterface
  public interface Step {
    byte run();
  }

  public static class WithStep extends VCore {
    final Step step;

    public WithStep(
        VCpu owner, long id, int signalIndex, Signal signal, int selectIndex, Step step) {
      super(owner, id, signalIndex, signal, selectIndex);
      this.step = step;
    }

    @Override
    protected byte step() {
      return step.run();
    }
  }
}
