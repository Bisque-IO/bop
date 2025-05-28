package bop.c;

import bop.io.Bytes;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class BytesTest {
  @Test
  public void testBuffer() {
    try (final var buf = Bytes.allocate(64).asMut()) {
      buf.putShortLE(0, 35125);
      var b1 = buf.getByte(0);
      var b2 = buf.getByte(1);
      var b = buf.getShortLE(0);
      buf.putShortBE(0, 35125);
      var c1 = buf.getByte(0);
      var c2 = buf.getByte(1);
      var c = buf.getShortBE(0);
      Assertions.assertEquals(b1, c2);
      Assertions.assertEquals(b2, c1);
      Assertions.assertEquals(b, c);

      try (final var slice = buf.slice(0, 2)) {
        var d = slice.getShortBE(0);
        Assertions.assertEquals(c, d);
        Assertions.assertEquals(b, d);
      }

      buf.writeInt(2).writeLong(3L).writeChar(4).writeShort(5).writeByte(6);

      final var strOffset = buf.getSize();
      buf.writeString("hello everybody in the world");
      buf.writeShort(10);
      System.out.println(buf.getCapacity());
      System.out.println(buf.getString(strOffset, buf.getSize() - 2 - strOffset));
      System.out.println(buf.getShort(buf.getSize() - 2));
    }
  }
}
