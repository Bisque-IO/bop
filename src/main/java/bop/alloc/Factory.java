package bop.alloc;

import bop.unsafe.Danger;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.reflect.AccessFlag;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArraySet;
import java.util.concurrent.locks.ReentrantLock;
import jdk.internal.misc.CarrierThreadLocal;
import org.agrona.UnsafeApi;

public class Factory<T> {
  static final ConcurrentHashMap<Class<?>, Factory<?>> MAP = new ConcurrentHashMap<>();
  static final MethodHandles.Lookup LOOKUP = MethodHandles.lookup();

  public final Class<T> cls;
  final CarrierThreadLocal<Allocator<T>> allocator = new CarrierThreadLocal<>();
  final long refOffset;
  final long allocatorOffset;
  final CopyOnWriteArraySet<Allocator<T>> allocators = new CopyOnWriteArraySet<>();
  final ReentrantLock lock = new ReentrantLock();
  final long sizeOf;

  public final Ctor[] ctors;
  static final Ctor[] EMPTY_CTORS = new Ctor[0];

  public record Ctor<T>(Constructor<T> method, MethodHandle handle, int argCount) {}

  Factory(Class<T> cls) {
    this.cls = cls;
    //    this.layout = ClassLayout.parseClass(cls);
    //    this.sizeOf = layout.instanceSize();
    this.sizeOf = estimateSizeOf(cls);

    if (cls.isPrimitive()) {
      this.refOffset = 0;
      this.allocatorOffset = 0;
      this.ctors = EMPTY_CTORS;
      return;
    }

    if (cls.isArray()) {
      this.refOffset = 0;
      this.allocatorOffset = 0;
      this.ctors = EMPTY_CTORS;
      return;
    }

    long refOffset;
    try {
      var refField = cls.getDeclaredField("$$ref$$");
      refField.setAccessible(true);
      refOffset = UnsafeApi.objectFieldOffset(refField);
    } catch (Throwable e) {
      refOffset = 0;
    }
    this.refOffset = refOffset;

    long allocatorOffset;
    try {
      var allocatorField = cls.getDeclaredField("$$allocator$$");
      allocatorField.setAccessible(true);
      allocatorOffset = UnsafeApi.objectFieldOffset(allocatorField);
    } catch (Throwable e) {
      allocatorOffset = 0;
    }
    this.allocatorOffset = allocatorOffset;

    var ctors = cls.getDeclaredConstructors();
    this.ctors = new Ctor[ctors.length];
    for (int i = 0; i < ctors.length; i++) {
      ctors[i].setAccessible(true);
      try {
        this.ctors[i] = new Ctor(
            ctors[i], LOOKUP.unreflectConstructor(ctors[i]), ctors[i].getParameterTypes().length);
      } catch (IllegalAccessException e) {
        throw new RuntimeException(e);
      }
    }
  }

  @SuppressWarnings("unchecked")
  public static <T> Factory<T> of(Class<T> cls) {
    var g = MAP.get(cls);
    if (g == null) {
      g = MAP.computeIfAbsent(cls, Factory::new);
    }
    return (Factory<T>) g;
  }

  public boolean hasRefCounting() {
    return refOffset != 0;
  }

  public int refCount(T obj) {
    if (refOffset == 0L) return -1;
    return UnsafeApi.getIntVolatile(obj, refOffset);
  }

  public int refIncr(T obj) {
    return refIncr(obj, 1);
  }

  public int refIncr(T obj, int by) {
    if (!hasRefCounting()) return -1;
    return UnsafeApi.getAndAddInt(obj, refOffset, by) + 1;
  }

  public int refDecr(T obj) {
    return refDecr(obj, -1);
  }

  public int refDecr(T obj, int by) {
    if (!hasRefCounting()) return -1;

    final int count = UnsafeApi.getAndAddInt(obj, refOffset, by) - 1;

    if (count == 0) {
      Allocator<T> allocator;
      if (allocatorOffset != 0L) {
        allocator = (Allocator<T>) UnsafeApi.getReference(obj, allocatorOffset);
      } else {
        allocator = allocator();
      }
      if (allocator != null) {
        allocator.recycle(obj);
      }
    }
    return count;
  }

  public T alloc() {
    return allocator().allocate();
  }

  public <A1> T alloc(A1 arg1) {
    return allocator().allocate(arg1);
  }

  public Allocator<T> allocator() {
    var allocator = this.allocator.get();
    if (allocator == null) {
      allocator = Local.get().allocator(this);
      this.allocator.set(allocator);
    }
    return allocator;
  }

  public void recycle(T value) {
    if (value == null) {
      return;
    }
    allocator().recycle(value);
  }

  public T newInstance(ArrayList<Object> args) {
    for (int i = 0; i < ctors.length; i++) {
      if (ctors[i].argCount == args.size()) {
        try {
          if (args.isEmpty()) {
            return (T) ctors[i].handle.invoke();
          }
          return (T) ctors[i].handle.invokeWithArguments(args);
        } catch (Throwable e) {
          throw new RuntimeException(e);
        }
      }
    }
    return null;
  }

  public static final Factory<byte[]> BYTE_ARRAY = new Factory<>(byte[].class);
  public static final Factory<char[]> CHAR_ARRAY = new Factory<>(char[].class);
  public static final Factory<short[]> SHORT_ARRAY = new Factory<>(short[].class);
  public static final Factory<int[]> INT_ARRAY = new Factory<>(int[].class);
  public static final Factory<long[]> LONG_ARRAY = new Factory<>(long[].class);
  public static final Factory<float[]> FLOAT_ARRAY = new Factory<>(float[].class);
  public static final Factory<double[]> DOUBLE_ARRAY = new Factory<>(double[].class);
  public static final Factory<Object[]> OBJECT_ARRAY = new Factory<>(Object[].class);

  public static final Factory<byte[][]> BYTE_ARRAY2 = new Factory<>(byte[][].class);
  public static final Factory<char[][]> CHAR_ARRAY2 = new Factory<>(char[][].class);
  public static final Factory<short[][]> SHORT_ARRAY2 = new Factory<>(short[][].class);
  public static final Factory<int[][]> INT_ARRAY2 = new Factory<>(int[][].class);
  public static final Factory<long[][]> LONG_ARRAY2 = new Factory<>(long[][].class);
  public static final Factory<float[][]> FLOAT_ARRAY2 = new Factory<>(float[][].class);
  public static final Factory<double[][]> DOUBLE_ARRAY2 = new Factory<>(double[][].class);
  public static final Factory<Object[][]> OBJECT_ARRAY2 = new Factory<>(Object[][].class);

  static void allFields(Class<?> cls, ArrayList<Field> fields) {
    if (cls == null || cls.equals(Object.class)) return;
    var declared = cls.getDeclaredFields();
    for (int i = 0; i < declared.length; i++) {
      if (declared[i].accessFlags().contains(AccessFlag.STATIC)) continue;
      declared[i].setAccessible(true);
      fields.add(declared[i]);
    }
    allFields(cls.getSuperclass(), fields);
  }

  public static long estimateSizeOf(Class<?> cls) {
    var fields = new ArrayList<Field>(64);
    allFields(cls, fields);
    long max = 0;
    for (int i = 0; i < fields.size(); i++) {
      fields.get(i).setAccessible(true);
      Math.max(max, Danger.UNSAFE.objectFieldOffset(fields.get(i)));
    }
    return max + 8L;
  }
}
