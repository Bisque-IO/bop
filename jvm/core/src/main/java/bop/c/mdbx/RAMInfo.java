package bop.c.mdbx;

import bop.c.Memory;
import bop.unsafe.Danger;

public class RAMInfo implements AutoCloseable {
  public long pageSize;
  public long totalPages;
  public long availPages;

  private long address;

  public static RAMInfo create() {
    final var info = new RAMInfo();
    info.update();
    return info;
  }

  @Override
  public void close() {
    final var ptr = this.address;
    if (ptr != 0L) {
      Memory.dealloc(ptr);
      this.address = 0L;
    }
  }

  public int update() {
    if (address == 0L) {
      address = Memory.zalloc(24L);
    }
    final int err;
    try {
      err = (int) CFunctions.MDBX_GET_SYSRAMINFO.invokeExact(address, address + 8L, address + 16L);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }
    if (err != Error.SUCCESS) {
      return err;
    }
    pageSize = Danger.getLong(address);
    totalPages = Danger.getLong(address + 8L);
    availPages = Danger.getLong(address + 16L);
    return err;
  }

  @Override
  public String toString() {
    return "RAMInfo{" + "pageSize="
        + pageSize + ", totalPages="
        + totalPages + ", availPages="
        + availPages + '}';
  }
}
