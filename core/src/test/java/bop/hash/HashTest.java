package bop.hash;

import bop.c.Memory;
import bop.c.hash.XXH3;
import bop.unsafe.Danger;
import org.junit.jupiter.api.Test;

public class HashTest {
  final XXH3_64 xxh3 = XXH3_64.create();

  @Test
  public void testXXH3() {
    final var address = Memory.allocCString("hello world");
    System.out.println(XXH3.hash("hello world"));
    System.out.println(xxh3.hash(Danger.getBytes("hello world")));
    System.out.println(xxh3.hash(address, 0, "hello world".length()));
    System.out.println(Hash.xxh3(address, "hello world".length()));
  }
}
