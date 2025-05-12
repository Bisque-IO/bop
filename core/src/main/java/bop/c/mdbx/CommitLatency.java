package bop.c.mdbx;

import bop.unsafe.Danger;

/// Latency of commit stages in 1/65536 of seconds units.
///
/// \warning This structure may be changed in future releases.
///
/// \see mdbx_txn_commit_ex()
public class CommitLatency {
  public static final long SIZE = 132L;

  /// \brief Duration of preparation (commit child transactions, update
  /// table's records and cursors destroying).
  public int preparation;

  /// Duration of GC update by wall clock.
  public int gcWallclock;

  /// Duration of internal audit if enabled.
  public int audit;

  /// \brief Duration of writing dirty/modified data pages to a filesystem,
  /// i.e. the summary duration of a `write()` syscalls during commit.
  public int write;

  /// \brief Duration of syncing written data to the disk/storage, i.e.
  /// the duration of a `fdatasync()` or a `msync()` syscall during commit.
  public int sync;

  /// \brief Duration of transaction ending (releasing resources).
  public int ending;

  // The total duration of a commit.
  public int whole;

  /// User-mode CPU time spent on GC update.
  public int gcCputime;

  /// Information for profiling GC work.
  /// Statistics are common for all processes working with one
  /// DB file and are stored in the LCK file. Data is accumulated when all
  /// transactions are committed, but only in libmdbx builds with the
  /// \ref MDBX_ENABLE_PROFGC option set. Collected statistics are returned to any process
  /// when using \ref mdbx_txn_commit_ex() and are simultaneously reset
  /// when top-level transactions (not nested) are completed.

  /// Number of GC update iterations, greater than 1 if there were retries/restarts.
  public int gcProfWloops;

  /// Number of GC record merge iterations.
  public int gcProfCoalescences;

  /// The number of previous reliable/stable commit points destroyed when operating in
  /// \ref MDBX_UTTERLY_NOSYNC mode.
  public int gcProfWipes;

  /// Number of forced commits to disk to avoid database growth when
  /// working outside the \ref MDBX_UTTERLY_NOSYNC mode.
  public int gcProfFlushes;

  /// Number of calls to the Handle-Slow-Readers mechanism to avoid DB growth.
  /// \see MDBX_hsr_func
  public int gcProfKicks;

  /// Slow path execution count of GC for user data.
  public int gcProfWorkCounter;

  /// The "wall clock" time spent reading and searching within the GC for user data.
  public int gcProfWorkRtimeMonotonic;

  /// CPU time in user mode spent preparing pages fetched from the GC for user data,
  /// including swapping from disk.
  public int gcProfWorkXtimeCpu;

  /// Number of iterations of search inside GC when allocating pages for user data.
  public int gcProfWorkRsteps;

  /// Number of requests to allocate page sequences for user data.
  public int gcProfWorkXpages;

  /// The number of page faults inside the GC when allocating and preparing pages for user data.
  public int gcProfWorkMajflt;

  /// Slow path execution count
  /// GC for the purpose of maintaining and updating the GC itself.
  public int gcProfSelfCounter;

  /// Time "by the wall clock" spent reading and searching inside the GC for the purposes of
  /// maintaining and updating the GC itself.
  public int gcProfSelfRtimeMonotonic;

  /// CPU time in user mode spent preparing pages fetched from the GC for the purposes of
  /// maintaining and updating the GC itself, including swapping from disk.
  public int gcProfSelfXtimeCpu;

  /// The number of iterations of search inside the GC when allocating pages
  /// for the purposes of maintaining and updating the GC itself.
  public int gcProfSelfRsteps;

  /// Number of requests for allocation of page sequences
  /// for the GC itself.
  public int gcProfSelfXpages;

  /// The number of page faults inside the GC when allocating and preparing pages
  /// for the GC itself.
  public int gcProfSelfMajflt;

  // For disassembling with pnl_merge()

  public int gcProfSelfPnlMergeWorkTime;
  public long gcProfSelfPnlMergeWorkVolume;
  public int gcProfSelfPnlMergeWorkCalls;
  public int gcProfSelfPnlMergeSelfTime;
  public long gcProfSelfPnlMergeSelfVolume;
  public int gcProfSelfPnlMergeSelfCalls;

  void update(long address) {
    preparation = Danger.getInt(address);
    gcWallclock = Danger.getInt(address + 4L);
    audit = Danger.getInt(address + 8L);
    write = Danger.getInt(address + 12L);
    sync = Danger.getInt(address + 16L);
    ending = Danger.getInt(address + 20L);
    whole = Danger.getInt(address + 24L);
    gcCputime = Danger.getInt(address + 28L);
    gcProfWloops = Danger.getInt(address + 32L);
    gcProfCoalescences = Danger.getInt(address + 36L);
    gcProfWipes = Danger.getInt(address + 40L);
    gcProfFlushes = Danger.getInt(address + 44L);
    gcProfKicks = Danger.getInt(address + 48L);
    gcProfWorkCounter = Danger.getInt(address + 52L);
    gcProfWorkRtimeMonotonic = Danger.getInt(address + 56L);
    gcProfWorkXtimeCpu = Danger.getInt(address + 60L);
    gcProfWorkRsteps = Danger.getInt(address + 64L);
    gcProfWorkXpages = Danger.getInt(address + 68L);
    gcProfWorkMajflt = Danger.getInt(address + 72L);
    gcProfSelfCounter = Danger.getInt(address + 76L);
    gcProfSelfRtimeMonotonic = Danger.getInt(address + 80L);
    gcProfSelfXtimeCpu = Danger.getInt(address + 84L);
    gcProfSelfRsteps = Danger.getInt(address + 88L);
    gcProfSelfXpages = Danger.getInt(address + 92L);
    gcProfSelfMajflt = Danger.getInt(address + 96L);
    gcProfSelfPnlMergeWorkTime = Danger.getInt(address + 100L);
    gcProfSelfPnlMergeWorkVolume = Danger.getLong(address + 104L);
    gcProfSelfPnlMergeWorkCalls = Danger.getInt(address + 112L);
    gcProfSelfPnlMergeSelfTime = Danger.getInt(address + 116L);
    gcProfSelfPnlMergeSelfVolume = Danger.getLong(address + 120L);
    gcProfSelfPnlMergeSelfCalls = Danger.getInt(address + 128L);
  }

  @Override
  public String toString() {
    return "CommitLatency{" + "preparation="
        + preparation + ", gcWallclock="
        + gcWallclock + ", audit="
        + audit + ", write="
        + write + ", sync="
        + sync + ", ending="
        + ending + ", whole="
        + whole + ", gcCputime="
        + gcCputime + ", gcProfWloops="
        + gcProfWloops + ", gcProfCoalescences="
        + gcProfCoalescences + ", gcProfWipes="
        + gcProfWipes + ", gcProfFlushes="
        + gcProfFlushes + ", gcProfKicks="
        + gcProfKicks + ", gcProfWorkCounter="
        + gcProfWorkCounter + ", gcProfWorkRtimeMonotonic="
        + gcProfWorkRtimeMonotonic + ", gcProfWorkXtimeCpu="
        + gcProfWorkXtimeCpu + ", gcProfWorkRsteps="
        + gcProfWorkRsteps + ", gcProfWorkXpages="
        + gcProfWorkXpages + ", gcProfWorkMajflt="
        + gcProfWorkMajflt + ", gcProfSelfCounter="
        + gcProfSelfCounter + ", gcProfSelfRtimeMonotonic="
        + gcProfSelfRtimeMonotonic + ", gcProfSelfXtimeCpu="
        + gcProfSelfXtimeCpu + ", gcProfSelfRsteps="
        + gcProfSelfRsteps + ", gcProfSelfXpages="
        + gcProfSelfXpages + ", gcProfSelfMajflt="
        + gcProfSelfMajflt + ", gcProfSelfPnlMergeWorkTime="
        + gcProfSelfPnlMergeWorkTime + ", gcProfSelfPnlMergeWorkVolume="
        + gcProfSelfPnlMergeWorkVolume + ", gcProfSelfPnlMergeWorkCalls="
        + gcProfSelfPnlMergeWorkCalls + ", gcProfSelfPnlMergeSelfTime="
        + gcProfSelfPnlMergeSelfTime + ", gcProfSelfPnlMergeSelfVolume="
        + gcProfSelfPnlMergeSelfVolume + ", gcProfSelfPnlMergeSelfCalls="
        + gcProfSelfPnlMergeSelfCalls + '}';
  }
}
