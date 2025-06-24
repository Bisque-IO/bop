package bop.concurrent;

import java.util.Collection;
import java.util.function.Consumer;

/// Many producer to many consumer concurrent queue that is array backed.
///
/// This is a Java port of Dmitry Vyukov's <a
/// href="http://www.1024cores.net/home/lock-free-algorithms/queues/bounded-mpmc-queue">MPMC
/// queue</a>.
///
/// Note: This queue breaks the contract for peek and poll in that it can return null when
/// the queue has no item available but size could be greater than zero if an offer is in progress.
/// This is due to the offer being a multiple-step process that can start and be interrupted
/// before completion, the thread will later be resumed and the offer process completes. Other
/// methods, such as peek and poll, could spin internally waiting on the offer to complete to
/// provide sequential consistency across methods, but this can have a detrimental effect in a
/// resource-starved system. This internal spinning eats up a CPU core and prevents other threads
/// making progress, resulting in latency spikes. To avoid this, a more relaxed approach is taken in
/// that an in-progress offer is not waited on to complete. The poll method has similar properties
/// for the multi-consumer implementation.
///
/// If you wish to check for empty then call {@link #isEmpty()} rather than {@link #size()}
/// checking for zero.
///
/// @param <E> type of the elements stored in the {@link java.util.Queue}.
@SuppressWarnings("removal")
public class Mpmc<E> extends AtomicArrayQueue<E> {
  private static final int SEQUENCES_ARRAY_BASE = U.arrayBaseOffset(long[].class);

  private final long[] sequences;

  /// Create a new queue with a bounded capacity. The requested capacity will be rounded up to the
  /// next positive power-of-two in size. That is if you request a capacity of 1000, then you will
  /// get 1024. If you request 1024 then that is what you will get.
  ///
  /// @param requestedCapacity of the queue which must be &gt;= 2.
  /// @throws IllegalArgumentException if the requestedCapacity &lt; 2.
  public Mpmc(final int requestedCapacity) {
    super(requestedCapacity);

    if (requestedCapacity < 2) {
      throw new IllegalArgumentException(
          "requestedCapacity must be >= 2: requestedCapacity=" + requestedCapacity);
    }

    final long[] sequences = new long[capacity];

    for (int i = 0; i < capacity; i++) {
      sequences[i] = i;
    }

    U.putLongVolatile(sequences, sequenceArrayOffset(0, sequences.length - 1), 0);
    this.sequences = sequences;
  }

  /// {@inheritDoc}
  public boolean offer(final E e) {
    if (null == e) {
      throw new NullPointerException("element cannot be null");
    }

    final long mask = this.capacity - 1;
    final long[] sequences = this.sequences;
    final E[] buffer = this.buffer;

    while (true) {
      final long currentTail = tail;
      final long sequenceOffset = sequenceArrayOffset(currentTail, mask);
      final long sequence = U.getLongVolatile(sequences, sequenceOffset);

      if (sequence < currentTail) {
        return false;
      }

      if (U.compareAndSetLong(this, TAIL_OFFSET, currentTail, currentTail + 1L)) {
        U.putReference(buffer, sequenceToBufferOffset(currentTail, mask), e);
        U.putLongRelease(sequences, sequenceOffset, currentTail + 1L);

        return true;
      }

      Thread.onSpinWait();
    }
  }

  /// {@inheritDoc}
  @SuppressWarnings("unchecked")
  public E poll() {
    final long[] sequences = this.sequences;
    final E[] buffer = this.buffer;
    final long mask = this.capacity - 1;

    while (true) {
      final long currentHead = head;
      final long sequenceOffset = sequenceArrayOffset(currentHead, mask);
      final long sequence = U.getLongVolatile(sequences, sequenceOffset);
      final long attemptedHead = currentHead + 1L;

      if (sequence < attemptedHead) {
        return null;
      }

      if (U.compareAndSetLong(this, HEAD_OFFSET, currentHead, attemptedHead)) {
        final long elementOffset = sequenceToBufferOffset(currentHead, mask);
        final Object e = U.getReference(buffer, elementOffset);
        U.putReference(buffer, elementOffset, null);
        U.putLongRelease(sequences, sequenceOffset, attemptedHead + mask);

        return (E) e;
      }

      Thread.onSpinWait();
    }
  }

  /// {@inheritDoc}
  @SuppressWarnings("unchecked")
  public E peek() {
    final long[] sequences = this.sequences;
    final E[] buffer = this.buffer;
    final long mask = this.capacity - 1;

    while (true) {
      final long currentHead = head;
      final long sequenceOffset = sequenceArrayOffset(currentHead, mask);
      final long sequence = U.getLongVolatile(sequences, sequenceOffset);
      final long attemptedHead = currentHead + 1L;

      if (sequence < attemptedHead) {
        return null;
      }

      if (sequence == attemptedHead) {
        final long elementOffset = sequenceToBufferOffset(currentHead, mask);
        final Object e = U.getReference(buffer, elementOffset);

        if (currentHead == head) {
          return (E) e;
        }
      }

      Thread.onSpinWait();
    }
  }

  /// {@inheritDoc}
  public int drain(final Consumer<E> elementConsumer) {
    return drain(elementConsumer, size());
  }

  /// {@inheritDoc}
  public int drain(final Consumer<E> elementConsumer, final int limit) {
    int count = 0;

    E e;
    while (count < limit && null != (e = poll())) {
      elementConsumer.accept(e);
      ++count;
    }

    return count;
  }

  /// {@inheritDoc}
  public int drainTo(final Collection<? super E> target, final int limit) {
    int count = 0;

    while (count < limit) {
      final E e = poll();
      if (null == e) {
        break;
      }

      target.add(e);
      ++count;
    }

    return count;
  }

  private static long sequenceArrayOffset(final long sequence, final long mask) {
    return SEQUENCES_ARRAY_BASE + ((sequence & mask) << 3);
  }
}
