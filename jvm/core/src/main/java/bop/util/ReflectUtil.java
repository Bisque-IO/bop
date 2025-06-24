package bop.util;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

public class ReflectUtil {
  public static Method setAccessible(Method method) {
    method.setAccessible(true);
    return method;
  }

  public static Field setAccessible(Field field) {
    field.setAccessible(true);
    return field;
  }

  public static <T> Constructor<T> setAccessible(Constructor<T> ctor) {
    ctor.setAccessible(true);
    return ctor;
  }
}
