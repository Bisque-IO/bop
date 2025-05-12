package bop.kernel;

import bop.unsafe.Danger;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import jdk.internal.misc.Unsafe;

public class InvokeUtils {
  public static final Unsafe UNSAFE = Danger.ZONE;
  public static final MethodHandles.Lookup LOOKUP = MethodHandles.lookup();

  public static long fieldOffset(Class<?> clazz, String fieldName) {
    try {
      var field = clazz.getDeclaredField(fieldName);
      field.setAccessible(true);
      return Danger.ZONE.objectFieldOffset(field);
    } catch (NoSuchFieldException e) {
      throw new RuntimeException(e);
    }
  }

  public static VarHandle findVarHandle(Class<?> clazz, String name, Class<?> type) {
    try {
      var field = clazz.getDeclaredField(name);
      field.setAccessible(true);
      return LOOKUP.unreflectVarHandle(field);
    } catch (IllegalAccessException | NoSuchFieldException e) {
      throw new RuntimeException(e);
    }
  }

  public static MethodHandle findMethodHandle(
      Class<?> clazz, String name, Class<?>... parameterTypes) {
    try {
      var method = clazz.getDeclaredMethod(name, parameterTypes);
      method.setAccessible(true);
      return LOOKUP.unreflect(method);
    } catch (NoSuchMethodException | IllegalAccessException e) {
      throw new RuntimeException(e);
    }
  }
}
