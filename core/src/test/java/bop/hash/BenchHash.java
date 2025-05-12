package bop.hash;

import bop.bench.Bench;
import bop.io.DirectBytes;
import bop.c.hash.RapidHash;
import bop.c.hash.XXH3;
import bop.unsafe.Danger;
import org.junit.jupiter.api.Test;

public class BenchHash {
  static final String STR_15 = "hello world!!!!";
  static final String STR_16 = "hello world!!!!!";
  static final String STR_64 = STR_16 + STR_16 + STR_16 + STR_16;

  static final XXH3_64 xxh3 = XXH3_64.create();

  @Test
  public void bench() throws Throwable {
//    rounds(1, 8, 12, 16, 24, 25, 32, 48, 64, 65, 127, 128, 256, 512);
//    rounds(128);
    rounds(128, 256, 512);
//    rounds(1024, 4096);
  }

  static final int CYCLES = 10;
  static final int ITERS = 5_000_000;
  private void rounds(int... lengths) throws Throwable {
    Bench.printHeader();
    for (int i = 0; i < lengths.length; i++) {
      if (i > 0) {
        Bench.printSeparator();
      }
//      round(lengths[i]);
      roundNative(lengths[i]);
    }
    Bench.printFooter();
  }

  private void round(int length) throws Throwable {
    StringBuilder sb = new StringBuilder(length);
    for (int i = 0; i < length; i++) {
      sb.append('a');
    }
    final String str = sb.toString();

    Bench.threaded(
      "rapid - String(" + length + ")", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> RapidHash.hash(str));

//    Bench.printSeparator();
//    Bench.threaded(
//      "XXH3 - String(" + length + ")", 1, CYCLES, 10, (threadId, cycle, iteration) -> XXH3.hash(str));
  }

  private void roundNative(int length) throws Throwable {
    StringBuilder sb = new StringBuilder(length);
    for (int i = 0; i < length; i++) {
      sb.append('a');
    }
    final String str = sb.toString();
    final var buf = DirectBytes.allocate(str.length());
    buf.writeString(str);
    final var bytes = Danger.getBytes(str);

    final long[] result = new long[1];

    Bench.threaded(
      "rapid - native(" + length + ")", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> result[0] = RapidHash.hash(buf.address(), length));
//    Bench.threaded(
//      "crc32 - NativeBuffer(" + length + ")", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> Crc32.INSTANCE.compute(buf.address(), 0, length));
//    Bench.printSeparator();
    Bench.threaded(
      "XXH3 (j) - byte[" + length + "]", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
        result[0] = xxh3.hash(bytes);
      });
    Bench.threaded(
      "XXH3 (j) - native[" + length + "]", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> result[0] = xxh3.hash(buf.address(), 0, length));
    Bench.threaded(
      "XXH3 - native(" + length + ")", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> result[0] = XXH3.hash(buf.address(), length));

    System.out.println(result[0]);
  }
}
