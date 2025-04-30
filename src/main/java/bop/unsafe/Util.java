package bop.unsafe;

import java.nio.ByteBuffer;
import jdk.internal.access.JavaNioAccess;
import jdk.internal.access.SharedSecrets;

public final class Util {
  public static final JavaNioAccess NIO_ACCESS = SharedSecrets.getJavaNioAccess();

  public static void checkArrayOffs(final int arrayLength, final int off, final int len) {
    if (len < 0 || off < 0 || off + len > arrayLength || off + len < 0)
      throw new IndexOutOfBoundsException();
  }

  public static long getDirectBufferAddress(final ByteBuffer buff) {
    return NIO_ACCESS.getBufferAddress(buff);
  }
}
