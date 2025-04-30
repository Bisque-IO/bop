package bop.c.mdbx;

import bop.unsafe.Danger;

public class Stat {
  public static final long SIZE = 48L;
  public int pageSize;
  public int depth;
  public long branchPages;
  public long leafPages;
  public long overflowPages;
  public long entries;
  public long modTxnid;

  void update(long ptr) {
    pageSize = Danger.UNSAFE.getInt(ptr);
    depth = Danger.UNSAFE.getInt(ptr + 4L);
    branchPages = Danger.UNSAFE.getLong(ptr + 8L);
    leafPages = Danger.UNSAFE.getLong(ptr + 16L);
    overflowPages = Danger.UNSAFE.getLong(ptr + 24L);
    entries = Danger.UNSAFE.getLong(ptr + 32L);
    modTxnid = Danger.UNSAFE.getLong(ptr + 40L);
  }

  @Override
  public String toString() {
    return "Stat{"
        + "pageSize="
        + pageSize
        + ", depth="
        + depth
        + ", branchPages="
        + branchPages
        + ", leafPages="
        + leafPages
        + ", overflowPages="
        + overflowPages
        + ", entries="
        + entries
        + ", modTxnid="
        + modTxnid
        + '}';
  }
}
