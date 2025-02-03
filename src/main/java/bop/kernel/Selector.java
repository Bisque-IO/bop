package bop.kernel;

import java.security.SecureRandom;

/** */
public class Selector {
  public static final SecureRandom RND = new SecureRandom(SecureRandom.getSeed(32));
  public static final long SIGNAL_MASK = Signal.CAPACITY - 1L;

  private static final long RND_MULTIPLIER = 0x5DEECE66DL;
  private static final long RND_ADDEND = 0xBL;
  private static final long RND_MASK = (1L << 48) - 1;

  public long value;
  public long map;
  public long select;
  public long selectCount;
  public long contention;

  public long seed = RND.nextLong();

  public static Selector random() {
    return new RandomSelector();
  }

  static long unsignedLongMulXorFold(final long lhs, final long rhs) {
    final long upper = Math.multiplyHigh(lhs, rhs) + ((lhs >> 63) & rhs) + ((rhs >> 63) & lhs);
    final long lower = lhs * rhs;
    return lower ^ upper;
  }

  public long nextLongRapid() {
    seed += 0x2d358dccaa6c78a5L;
    return unsignedLongMulXorFold(seed, seed ^ 0x8bb84b93962eacc9L);
  }

  protected int next(int bits) {
    long oldseed, nextseed;
    oldseed = seed;
    nextseed = (oldseed * RND_MULTIPLIER + RND_ADDEND) & RND_MASK;
    seed = nextseed;
    return (int) (nextseed >>> (48 - bits));
  }

  public long nextLong() {
    // it's okay that the bottom word remains signed.
    return ((long) (next(32)) << 32) + next(32);
  }

  public int mapIndex(long mask) {
    return (int) Math.abs(map & mask);
  }

  public int selectIndex() {
    return (int) (Math.abs(select) & SIGNAL_MASK);
  }

  public long nextMap() {
    map++;
    selectCount = 1;
    select = 0;
    return map;
  }

  public long nextSelect() {
    if (selectCount >= Signal.CAPACITY) {
      nextMap();
      return select;
    }
    selectCount++;
    select++;
    return select & SIGNAL_MASK;
  }
}
