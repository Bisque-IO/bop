package bop.kernel;

///
public class CarrierThread extends Thread {
  public VCore core;
  protected Selector selector;
  protected VCpu cpu;

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
