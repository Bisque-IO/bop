package bop.c.hash;

import com.dynatrace.hash4j.hashing.Hashing;
import org.junit.jupiter.api.Test;

public class XXH3Test {
  @Test
  public void testHash() {
    System.out.println(XXH3.hash("hello world"));
    System.out.println(XXH3.hash("hello world".getBytes()));
    System.out.println(Hashing.xxh3_64().hashBytesToLong("hello world".getBytes()));
  }

  @Test
  public void testStreaming() {
    try (final var digest = XXH3.Digest.create()) {
      digest.update("hello world");
      System.out.println(digest.digest());
      digest.reset();

      System.out.println(digest.updateFinal("hello world"));
      digest.reset();
      System.out.println(digest.updateFinalReset("hello world"));
    }
  }
}
