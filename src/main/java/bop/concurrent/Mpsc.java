package bop.concurrent;

import java.util.Collection;
import java.util.function.Consumer;

/// Many producer to one consumer concurrent queue that is array backed. The algorithm is a
/// variation of Fast Flow consumer adapted to work with the Java Memory Model on arrays by
/// using {@link jdk.internal.misc.Unsafe}.
///
/// Note: This queue breaks the contract for peek and poll in that it can return null when the
/// queue has no item available but size could be greater than zero if an offer is in progress.
/// This is due to the offer being a multiple step process which can start and be interrupted
/// before completion, the thread will later be resumed and the offer process completes. Other
/// methods, such as peek and poll, could spin internally waiting on the offer to complete to
/// provide sequentially consistency across methods but this can have a detrimental effect in
/// a resource starved system. This internal spinning eats up a CPU core and prevents other
/// threads making progress resulting in latency spikes. To avoid this a more relaxed approach
/// is taken in that an in-progress offer is not waited on to complete.
///
/// If you wish to check for empty then call {@link #isEmpty()} rather than {@link #size()}
/// checking for zero.
///
/// @param <E> type of the elements stored in the {@link java.util.Queue}.
@SuppressWarnings("removal")
public class Mpsc<E> extends AbstractConcurrentArrayQueue<E> {
  /// Constructs a queue with the requested capacity.
  ///
  /// @param requestedCapacity of the queue.
  public Mpsc(final int requestedCapacity) {
    super(requestedCapacity);
  }

  /// {@inheritDoc}
  public boolean offer(final E e) {
    if (null == e) {
      throw new NullPointerException("element cannot be null");
    }

    final int capacity = this.capacity;
    long currentHead = sharedHeadCache;
    long bufferLimit = currentHead + capacity;
    long currentTail;
    do {
      currentTail = tail;
      if (currentTail >= bufferLimit) {
        currentHead = head;
        bufferLimit = currentHead + capacity;
        if (currentTail >= bufferLimit) {
          return false;
        }

        U.putLongRelease(this, SHARED_HEAD_CACHE_OFFSET, currentHead);
      }
    } while (!U.compareAndSetLong(this, TAIL_OFFSET, currentTail, currentTail + 1));

    U.putReferenceRelease(buffer, sequenceToBufferOffset(currentTail, capacity - 1), e);

    return true;
  }

  /// {@inheritDoc}
  @SuppressWarnings("unchecked")
  public E poll() {
    final long currentHead = head;
    final long elementOffset = sequenceToBufferOffset(currentHead, capacity - 1);
    final Object[] buffer = this.buffer;
    final Object e = U.getReferenceVolatile(buffer, elementOffset);

    if (null != e) {
      U.putReference(buffer, elementOffset, null);
      U.putLongRelease(this, HEAD_OFFSET, currentHead + 1);
    }

    return (E) e;
  }

  /// {@inheritDoc}
  public int drain(final Consumer<E> elementConsumer) {
    return drain(elementConsumer, (int) (tail - head));
  }

  /// {@inheritDoc}
  @SuppressWarnings("unchecked")
  public int drain(final Consumer<E> elementConsumer, final int limit) {
    final Object[] buffer = this.buffer;
    final long mask = this.capacity - 1;
    final long currentHead = head;
    long nextSequence = currentHead;
    final long limitSequence = nextSequence + limit;

    while (nextSequence < limitSequence) {
      final long elementOffset = sequenceToBufferOffset(nextSequence, mask);
      final Object item = U.getReferenceVolatile(buffer, elementOffset);

      if (null == item) {
        break;
      }

      U.putReferenceRelease(buffer, elementOffset, null);
      nextSequence++;
      U.putLongRelease(this, HEAD_OFFSET, nextSequence);
      elementConsumer.accept((E) item);
    }

    return (int) (nextSequence - currentHead);
  }

  /// {@inheritDoc}
  @SuppressWarnings("unchecked")
  public int drainTo(final Collection<? super E> target, final int limit) {
    final Object[] buffer = this.buffer;
    final long mask = this.capacity - 1;
    long nextSequence = head;
    int count = 0;

    while (count < limit) {
      final long elementOffset = sequenceToBufferOffset(nextSequence, mask);
      final Object e = U.getReferenceVolatile(buffer, elementOffset);
      if (null == e) {
        break;
      }

      U.putReferenceRelease(buffer, elementOffset, null);
      nextSequence++;
      U.putLongRelease(this, HEAD_OFFSET, nextSequence);
      count++;
      target.add((E) e);
    }

    return count;
  }
}
