package bop.c.mdbx;

/// \brief Warming up options
/// \ingroup c_settings
/// \anchor warmup_flags
/// \see mdbx_env_warmup()
public interface WarmupFlags {
  /// By default \ref mdbx_env_warmup() just ask OS kernel to asynchronously
  /// prefetch database pages.
  int DEFAULT = 0;

  /// Peeking all pages of allocated portion of the database
  /// to force ones to be loaded into memory. However, the pages are just peeks
  /// sequentially, so unused pages that are in GC will be loaded in the same
  /// way as those that contain payload.
  int FORCE = 1;

  /// Using system calls to peeks pages instead of directly accessing ones,
  /// which at the cost of additional overhead avoids killing the current
  /// process by OOM-killer in a lack of memory condition.
  /// \note Has effect only on POSIX (non-Windows) systems with conjunction
  /// to \ref MDBX_warmup_force option.
  int OOM_SAFE = 2;

  /// Try to lock database pages in memory by `mlock()` on POSIX-systems
  /// or `VirtualLock()` on Windows. Please refer to description of these
  /// functions for reasonability of such locking and the information of
  /// effects, including the system as a whole.
  ///
  /// Such locking in memory requires that the corresponding resource limits
  /// (e.g. `RLIMIT_RSS`, `RLIMIT_MEMLOCK` or process working set size)
  /// and the availability of system RAM are sufficiently high.
  ///
  /// On successful, all currently allocated pages, both unused in GC and
  /// containing payload, will be locked in memory until the environment closes,
  /// or explicitly unblocked by using \ref MDBX_warmup_release, or the
  /// database geometry will changed, including its auto-shrinking.
  int LOCK = 4;

  /// Alters corresponding current resource limits to be enough for lock pages
  /// by \ref MDBX_warmup_lock. However, this option should be used in simpler
  /// applications since takes into account only current size of this environment
  /// disregarding all other factors. For real-world database application you
  /// will need full-fledged management of resources and their limits with
  /// respective engineering.
  int TOUCH_LIMIT = 8;

  /// Release the lock that was performed before by \ref MDBX_warmup_lock.
  int RELEASE = 16;
}
