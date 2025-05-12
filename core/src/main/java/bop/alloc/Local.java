package bop.alloc;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.concurrent.atomic.AtomicLong;
import jdk.internal.access.SharedSecrets;
import jdk.internal.misc.CarrierThreadLocal;

public class Local {
  static final CarrierThreadLocal<Local> TLS = new CarrierThreadLocal<>();

  Thread carrierThread = SharedSecrets.getJavaLangAccess().currentCarrierThread();
  final AtomicLong size = new AtomicLong();
  final AtomicLong capacity = new AtomicLong();
  HashMap<Class<?>, Allocator<?>> allocators = new HashMap<>(1024);

  ArrayList<Object> args = new ArrayList<>(32);

  public Local() {}

  public static Local get() {
    var local = TLS.get();
    if (local == null) {
      local = new Local();
      TLS.set(local);
      Manager.INSTANCE.locals.add(local);
    }
    return local;
  }

  public <T> T alloc(Class<T> cls) {
    var allocator = (Allocator<T>) allocators.get(cls);
    if (allocator == null) {
      var descriptor = new Factory<T>(cls);
      allocator = new Allocator<T>(this, descriptor);
      allocators.put(cls, allocator);
    }
    return allocator.allocate();
  }

  public <T, A1> T alloc(Class<T> cls, A1 arg1) {
    args.clear();
    args.add(arg1);
    return alloc(cls, args);
  }

  public <T, A1, A2> T alloc(Class<T> cls, A1 arg1, A2 arg2) {
    args.clear();
    args.add(arg1);
    args.add(arg2);
    return alloc(cls, args);
  }

  public <T, A1, A2, A3> T alloc(Class<T> cls, A1 arg1, A2 arg2, A3 arg3) {
    args.clear();
    args.add(arg1);
    args.add(arg2);
    args.add(arg3);
    return alloc(cls, args);
  }

  public <T> T alloc(Class<T> cls, ArrayList<Object> args) {
    var allocator = (Allocator<T>) allocators.get(cls);
    if (allocator == null) {
      var descriptor = new Factory<T>(cls);
      allocator = new Allocator<T>(this, descriptor);
      allocators.put(cls, allocator);
    }
    return allocator.allocate(args);
  }

  public <T> void recycle(T value) {
    if (value == null) {
      return;
    }
    var allocator = (Allocator<T>) allocators.get(value.getClass());
    if (allocator == null) {
      return;
    }
    allocator.recycle(value);
  }

  <T> Allocator<T> allocator(Factory<T> factory) {
    var allocator = (Allocator<T>) allocators.get(factory.cls);
    if (allocator == null) {
      allocator = new Allocator<T>(this, factory);
      allocators.put(factory.cls, allocator);
      factory.allocator.set(allocator);
    }
    return allocator;
  }
}
