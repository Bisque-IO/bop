package bop.bench;

@FunctionalInterface
public interface Operation {
  void run(int threadId, int cycle, int iteration);
}
