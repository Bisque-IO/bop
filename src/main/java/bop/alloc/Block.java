package bop.alloc;

import bop.concurrent.Mpsc;

public class Block<E> extends Mpsc<E> {
  public Block(int requestedCapacity) {
    super(requestedCapacity);
  }
}

// public class Block<T> {
//  T[] values;
//  int ridx;
//  int widx;
//  int mask;
//
//  public Block(int capacity) {
//    capacity = BitUtil.findNextPositivePowerOfTwo(capacity);
//    values = (T[]) new Object[capacity];
//    ridx = 0;
//    widx = 0;
//    mask = capacity-1;
//  }
//
//  public int size() {
//    return widx-ridx;
//  }
//
//  public boolean offer(T value) {
//    var widx = this.widx;
//    var ridx = this.ridx;
//    if (widx-ridx >= values.length) {
//      return false;
//    }
//    this.widx++;
//    values[widx & mask] = value;
//    return true;
//  }
//
//  public T poll() {
//    var widx = this.widx;
//    var ridx = this.ridx;
//    if (widx-ridx <= 0) {
//      return null;
//    }
//    this.ridx++;
//    return values[ridx & mask];
//  }
// }
