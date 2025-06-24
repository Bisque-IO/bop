package bop.asm;

public class DynamicClassLoader extends ClassLoader {
  public DynamicClassLoader() {}

  public Class<?> define(String name, byte[] b) {
    return defineClass(name, b, 0, b.length);
  }
}
