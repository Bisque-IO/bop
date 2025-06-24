package bop.kernel;

import bop.concurrent.MpscSharded;
import bop.concurrent.SpinLock;
import java.util.ArrayList;
import java.util.concurrent.Executor;
import org.agrona.collections.ObjectHashSet;

///
public class Isolate extends VCore implements Executor {
  private final SpinLock lock = new SpinLock();
  private final ObjectHashSet<VThread> threads = new ObjectHashSet<>();
  private final MpscSharded<Object> queue;
  private final Timers timers = new Timers();
  private final ArrayList<Object> taskList = new ArrayList<>(4096);

  private Isolate(
      VCpu owner, long id, int signalIndex, Signal signal, int selectIndex, Options options) {
    super(owner, id, signalIndex, signal, selectIndex);
    queue = new MpscSharded<>(options.partitions, options.partitionSize);
  }

  public static VCore.Builder<Isolate> builder(final Options options) {
    return (owner, id, signalIndex, signal, selectIndex) ->
        new Isolate(owner, id, signalIndex, signal, selectIndex, options);
  }

  @Override
  public byte step() {
    processTimers();

    flushTasks();

    dequeueTasks();

    flushTasks();

    return 0;
  }

  int processTimers() {
    timers.poll(1024);
    return 0;
  }

  int dequeueTasks() {
    queue.drain(taskList, 4096);
    return flushTasks();
  }

  int flushTasks() {
    for (int i = 0; i < taskList.size(); i++) {
      switch (taskList.get(i)) {
        case null -> {}
        case Timer t -> {}
        case Runnable r -> {
          r.run();
        }
        default -> {}
      }
    }
    int count = taskList.size();
    taskList.clear();
    return count;
  }

  @Override
  public void execute(Runnable command) {
    queue.offer(command);
    schedule();
  }

  public record Options(int partitions, int partitionSize) {}
}
