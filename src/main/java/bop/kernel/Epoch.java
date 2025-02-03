package bop.kernel;

import jdk.internal.misc.VM;

public class Epoch {
  public static long millis() {
    return System.currentTimeMillis();
  }

  public static long micros() {
    return nanos() / 1000;
  }

  static final long NANOS_PER_SECOND = 1_000_000_000L;
  // initial offset
  private static final long OFFSET_SEED = System.currentTimeMillis() / 1000 - 1024;
  // We don't actually need a volatile here.
  // We don't care if offset is set or read concurrently by multiple
  // threads - we just need a value which is 'recent enough' - in other
  // words something that has been updated at least once in the last
  // 2^32 secs (~136 years). And even if we by chance see an invalid
  // offset, the worst that can happen is that we will get a -1 value
  // from getNanoTimeAdjustment, forcing us to update the offset
  // once again.
  private static long offset = OFFSET_SEED;

  public static long nanos() {
    // Take a local copy of offset. offset can be updated concurrently
    // by other threads (even if we haven't made it volatile) so we will
    // work with a local copy.
    long localOffset = offset;
    long adjustment = VM.getNanoTimeAdjustment(localOffset);

    if (adjustment == -1) {
      // -1 is a sentinel value returned by VM.getNanoTimeAdjustment
      // when the offset it is given is too far off the current UTC
      // time. In principle, this should not happen unless the
      // JVM has run for more than ~136 years (not likely) or
      // someone is fiddling with the system time, or the offset is
      // by chance at 1ns in the future (very unlikely).
      // We can easily recover from all these conditions by bringing
      // back the offset in range and retry.

      // bring back the offset in range. We use -1024 to make
      // it more unlikely to hit the 1ns in the future condition.
      localOffset = System.currentTimeMillis() / 1000 - 1024;

      // retry
      adjustment = VM.getNanoTimeAdjustment(localOffset);

      if (adjustment == -1) {
        // Should not happen: we just recomputed a new offset.
        // It should have fixed the issue.
        throw new InternalError("Offset " + localOffset + " is not in range");
      } else {
        // OK - recovery succeeded. Update the offset for the
        // next call...
        offset = localOffset;
      }
    }
    //        long secs = Math.addExact(localOffset, Math.floorDiv(adjustment, NANOS_PER_SECOND));
    //        long nos = Math.floorMod(adjustment, NANOS_PER_SECOND);
    //        return secs * NANOS_PER_SECOND + nos;

    return localOffset + adjustment;
  }
}
