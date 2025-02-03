package bop.alloc;

import java.util.ArrayList;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.atomic.AtomicLong;

public class Manager {
  public static final Manager INSTANCE = new Manager();

  final AtomicLong maxBytes = new AtomicLong();
  final CopyOnWriteArrayList<Local> locals = new CopyOnWriteArrayList<>();

  final ConcurrentHashMap<Class<?>, Factory<?>> map = new ConcurrentHashMap<>();
  Thread thread;

  Stats stats = new Stats();

  public static class Stats {
    public long estimatedBytes;
    public long totalTypes;
    public long totalLocals;
    public long totalThreads;
  }

  public Manager() {
    thread = new Thread(this::run);
    thread.setName("bop-alloc-global");
    thread.setDaemon(true);
    thread.start();
  }

  private void run() {
    final ArrayList<Local> died = new ArrayList<>();
    while (!Thread.interrupted()) {
      try {
        Thread.sleep(10L);
      } catch (InterruptedException e) {
        break;
      }

      Local local;
      // Check for threads that recently died
      for (int i = 0; i < locals.size(); i++) {
        try {
          local = locals.get(i);
        } catch (Throwable t) {
          break;
        }

        if (!local.carrierThread.isAlive()) {
          died.add(local);
        }
      }

      if (!died.isEmpty()) {
        for (int i = 0; i < died.size(); i++) {}

        locals.removeAll(died);
        died.clear();
      }
    }
  }
}
