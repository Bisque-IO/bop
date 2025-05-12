package bop.concurrent;

import bop.bit.Bits;
import java.util.ArrayList;
import jdk.internal.vm.annotation.Contended;

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
/// @param <E> type of the elements stored in the {@link java.util.Queue}.
@SuppressWarnings("removal")
public class MpscSharded<E> {
  final Mpsc<E>[] queues;
  final int mask;

  @Contended
  volatile long signal;

  private int lastIndex;

  @SuppressWarnings("unchecked")
  public MpscSharded(int partitions, int capacity) {
    if (partitions <= 0) {
      partitions = 1;
    }
    partitions = (int) Bits.findNextPositivePowerOfTwo(partitions);
    this.mask = partitions - 1;
    this.signal = 0;
    this.queues = (Mpsc<E>[]) new Mpsc[partitions];
    for (int i = 0; i < partitions; i++) {
      queues[i] = new Mpsc<>(capacity);
    }
  }

  public boolean offer(E value) {
    if (queues[(int) (Thread.currentThread().threadId() & mask)].offer(value)) {
      return true;
    }
    for (int i = 0; i < queues.length; i++) {
      if (queues[i].offer(value)) {
        return true;
      }
    }
    return false;
  }

  public E poll(int cursor) {
    return queues[Math.abs(cursor) & mask].poll();
  }

  public int drain(ArrayList<E> list, int limit) {
    int count = 0;
    int remaining = limit;
    for (; lastIndex < queues.length; lastIndex++, count++) {
      remaining -= queues[lastIndex].drainTo(list, remaining);
      if (remaining == 0) {
        lastIndex++;
        return limit;
      }
    }
    for (lastIndex = 0; lastIndex < queues.length; lastIndex++) {
      remaining -= queues[lastIndex].drainTo(list, remaining);
      if (remaining == 0) {
        lastIndex++;
        return limit;
      }
    }
    return list.size();
  }
}
