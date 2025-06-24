package bop.c;

import static bop.c.Loader.LINKER;
import static bop.c.Loader.LOOKUP;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

public class SysError {
  public interface Library {
    MethodHandle BOP_ENODATA = LINKER.downcallHandle(
        LOOKUP.find("bop_ENODATA").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EINVAL = LINKER.downcallHandle(
        LOOKUP.find("bop_EINVAL").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EACCESS = LINKER.downcallHandle(
        LOOKUP.find("bop_EACCESS").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_ENOMEM = LINKER.downcallHandle(
        LOOKUP.find("bop_ENOMEM").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EROFS = LINKER.downcallHandle(
        LOOKUP.find("bop_EROFS").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_ENOSYS = LINKER.downcallHandle(
        LOOKUP.find("bop_ENOSYS").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EIO = LINKER.downcallHandle(
        LOOKUP.find("bop_EIO").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EPERM = LINKER.downcallHandle(
        LOOKUP.find("bop_EPERM").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EINTR = LINKER.downcallHandle(
        LOOKUP.find("bop_EINTR").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_ENOFILE = LINKER.downcallHandle(
        LOOKUP.find("bop_ENOFILE").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EREMOTE = LINKER.downcallHandle(
        LOOKUP.find("bop_EREMOTE").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));

    MethodHandle BOP_EDEADLK = LINKER.downcallHandle(
        LOOKUP.find("bop_EDEADLK").orElseThrow(),
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT // error code
            ));
  }

  private static int getInt(MethodHandle handle) {
    try {
      return (int) handle.invokeExact();
    } catch (Throwable e) {
      throw new AssertionError(e);
    }
  }

  public static final int ENODATA = getInt(Library.BOP_ENODATA);
  public static final int EINVAL = getInt(Library.BOP_EINVAL);
  public static final int EACCESS = getInt(Library.BOP_EACCESS);
  public static final int ENOMEM = getInt(Library.BOP_ENOMEM);
  public static final int EROFS = getInt(Library.BOP_EROFS);
  public static final int ENOSYS = getInt(Library.BOP_ENOSYS);
  public static final int EIO = getInt(Library.BOP_EIO);
  public static final int EPERM = getInt(Library.BOP_EPERM);
  public static final int EINTR = getInt(Library.BOP_EINTR);
  public static final int ENOFILE = getInt(Library.BOP_ENOFILE);
  public static final int EREMOTE = getInt(Library.BOP_EREMOTE);
  public static final int EDEADLK = getInt(Library.BOP_EDEADLK);
}
