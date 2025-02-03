package bop.kernel;

public class RandomSelector extends Selector {
  public RandomSelector() {
    nextMap();
  }

  public long nextMap() {
    map = Math.abs(nextLong());
    select = Math.abs(nextLong()) & SIGNAL_MASK;
    selectCount = 1;
    return map;
  }

  public long nextSelect() {
    // Iterate a max of signal map capacity before moving to next signal map.
    if (selectCount >= Signal.CAPACITY) {
      nextMap();
      return select;
    }
    select++;
    selectCount++;
    return select & SIGNAL_MASK;
  }
}
