package bop.cluster;

import bop.unsafe.Danger;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.io.OutputStream;

public class MessageTest {
  @Test
  public void testMessage() {

  }

  interface Block {
    int size();
  }

  interface Array<T> {
    T get(int index);
  }

  interface Dictionary<K, V> {

  }

  interface Header {
    long checksum();

    int size();
  }

  interface Root<T> {
    Header header();

    T root();
  }

  interface SavePrices {
    Array<Price> prices();
  }

  interface Price {
    double open();
    double high();
    double low();
    double last();

    Array<Price> prices();

    PriceMut asMut();
  }

  interface PriceMut extends Price {
    PriceMut open(double open);
    PriceMut high(double high);
    PriceMut low(double low);
    PriceMut last(double last);
  }

  static class MessageBuffer<T> implements Root<T> {
    @Override public Header header() {
      return null;
    }

    @Override public T root() {
      return null;
    }
  }

  /* Header

  -8   u64      hash

   0   i32      size
   4   u8       type
   5   u8       flags
   6   u8       topicOffset
   7   u8       topicSize

   8   i64      topicHash

   16  i64      id

   24  i64      time

   32  u32      sourceIp
   36  u32      sourcePort

   40  i64      sourceId

   48  u32      sourceStarted
   52  u16      headerOffset
   54  u16      headerSize

   56  u8       format
   57  u8       reserved
   58  u16      bodyOffset
   60  i32      bodySize

  */

  public static class Msg {
    private byte[] hb;
    private long addressOffset;
    private boolean owned;

    public boolean isOffHeap() {
      return hb == null;
    }

    public boolean isOnHeap() {
      return hb != null;
    }

    public int size() {
      return Danger.getIntUnaligned(hb, addressOffset, false);
    }

    public Msg size(int size) {
      Danger.putIntUnaligned(hb, addressOffset, size, false);
      return this;
    }
  }
}
