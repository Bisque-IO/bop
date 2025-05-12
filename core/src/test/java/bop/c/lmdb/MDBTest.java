package bop.c.lmdb;

import bop.bench.Bench;
import java.util.stream.IntStream;
import org.junit.jupiter.api.Test;

public class MDBTest {
  @Test
  public void testMDB() {
    final var env = Env.open("test.db", MDB.config());
    final var handle = env.newHandle();
    final var info = EnvInfo.asRecord(handle.info());
    final var stat = Stat.asRecord(handle.stat());

    System.out.println(info);
    System.out.println(stat);
    env.close();
  }

  @Test
  public void benchDowncall() throws Throwable {
    Bench.printHeader();
    IntStream.of(1, 2, 4, 8, 16).forEach(threads -> {
      try {
        Bench.threaded("bop_hello", threads, 10, 10000000, (threadId, cycle, iteration) -> {
          try {
            Library.BOP_HELLO.invokeExact();
          } catch (Throwable e) {
            throw new RuntimeException(e);
          }
        });
      } catch (InterruptedException e) {
        throw new RuntimeException(e);
      }
    });
    Bench.printFooter();
  }
}
