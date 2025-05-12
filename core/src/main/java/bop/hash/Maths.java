package bop.hash;

class Maths {
  public static long unsignedLongMulXorFold(final long lhs, final long rhs) {
    final long upper = Math.multiplyHigh(lhs, rhs) + ((lhs >> 63) & rhs) + ((rhs >> 63) & lhs);
    final long lower = lhs * rhs;
    return lower ^ upper;
  }

  public static long unsignedLongMulHigh(final long lhs, final long rhs) {
    return Math.multiplyHigh(lhs, rhs) + ((lhs >> 63) & rhs) + ((rhs >> 63) & lhs);
  }

  public static long unsignedMultiplyHigh(long x, long y) {
    return Math.unsignedMultiplyHigh(x, y);
  }
}
