package bop.c.mdbx;

public interface CopyFlags {
  int DEFAULTS = 0;

  /// Copy with compactification: Omit free space from copy and renumber all
  /// pages sequentially
  int COMPACT = 1;

  /// Force to make resizable copy, i.e. dynamic size instead of fixed
  int FORCE_DYNAMIC_SIZE = 2;

  /// Don't explicitly flush the written data to an output media
  int DONT_FLUSH = 4;

  /// Use read transaction parking during copying MVCC-snapshot
  /// \see mdbx_txn_park()
  int THROTTLE_MVCC = 8;

  /// Abort/dispose passed transaction after copy
  /// \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname()
  int DISPOSE_TXN = 16;

  /// Enable renew/restart read transaction in case it use outdated
  /// MVCC shapshot, otherwise the \ref MDBX_MVCC_RETARDED will be returned
  /// \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname()
  int RENEW_TXN = 32;
}
