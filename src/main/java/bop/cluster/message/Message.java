package bop.cluster.message;

import org.agrona.concurrent.UnsafeBuffer;

public interface Message {
  long address();

  long sentNs();

  Message sentNs(long nowNs);

  long ledgerAckNs();

  int size();

  byte type();

  UnsafeBuffer buffer();

  long id();

  Message id(long id);

  long time();

  Message time(long time);
}
