package bop.kernel;

import jdk.internal.vm.annotation.Contended;

///
public class CarrierThread extends Thread {
  protected VCore core;
  protected Selector selector;
  protected VCpu cpu;

  /// Flag to determine whether this carrier thread is pinned because
  /// a VCore is blocking progress for too long or a VThread gets
  /// pinned via the JVM.
  @Contended
  volatile boolean pinned;

  public CarrierThread(VCpu cpu) {
    this(cpu, Selector.random());
  }

  public CarrierThread(VCpu cpu, Selector selector) {
    assert cpu != null;
    assert selector != null;
    this.cpu = cpu;
    this.selector = selector;
  }

  public void run() {
    var selector = this.selector;
    while (!Thread.interrupted()) {
      cpu.execute(selector);
    }
  }

  ///
  public static class Group {}

  public class TimerSignal {
    long[] cores;
  }
}
