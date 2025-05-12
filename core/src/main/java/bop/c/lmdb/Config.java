package bop.c.lmdb;

public class Config {
  /// Open an environment handle. Flags set by mdb_env_set_flags() are also used.
  ///
  ///   - #MDB_FIXEDMAP use a fixed address for the mmap region. This flag must be specified when
  ///     creating the environment, and is stored persistently in the environment. If successful,
  ///     the memory map will always reside at the same virtual address and pointers used to
  ///     reference data items in the database will be constant across multiple invocations. This
  ///     option may not always work, depending on how the operating system has allocated memory
  ///     to shared libraries and other uses. The feature is highly experimental.
  ///   - #MDB_NOSUBDIR By default, LMDB creates its environment in a directory whose pathname is
  ///     given in \b path, and creates its data and lock files under that directory. With this
  ///     option, \b path is used as-is for the database main data file. The database lock file
  ///     is the \b path with "-lock" appended.
  ///   - #MDB_RDONLY Open the environment in read-only mode. No write operations will be
  ///     allowed. LMDB will still modify the lock file - except on read-only filesystems, where
  ///     LMDB does not use locks.
  ///   - #MDB_WRITEMAP Use a writeable memory map unless MDB_RDONLY is set. This uses fewer
  ///     mallocs but loses protection from application bugs like wild pointer writes and other
  ///     bad updates into the database. This may be slightly faster for DBs that fit entirely in
  ///     RAM, but is slower for DBs larger than RAM. Incompatible with nested transactions. Do
  ///     not mix processes with and without MDB_WRITEMAP on the same environment. This can
  ///     defeat durability (#mdb_env_sync etc).
  ///   - #MDB_NOMETASYNC Flush system buffers to disk only once per transaction, omit the
  ///     metadata flush. Defer that until the system flushes files to disk, or next
  ///     non-MDB_RDONLY commit or #mdb_env_sync(). This optimization maintains database
  ///     integrity, but a system crash may undo the last committed transaction. I.e. it
  ///     preserves the ACI (atomicity, consistency, isolation) but not D (durability) database
  ///     property. This flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_NOSYNC Don't flush system buffers to disk when committing a transaction. This
  ///     optimization means a system crash can corrupt the database or lose the last
  ///     transactions if buffers are not yet flushed to disk. The risk is governed by how often
  ///     the system flushes dirty buffers to disk and how often #mdb_env_sync() is called.
  ///     However, if the filesystem preserves write order and the #MDB_WRITEMAP flag is not
  ///     used, transactions exhibit ACI (atomicity, consistency, isolation) properties and only
  ///     lose D (durability). I.e. database integrity is maintained, but a system crash may undo
  ///     the final transactions. Note that (#MDB_NOSYNC | #MDB_WRITEMAP) leaves the system with
  ///     no hint for when to write transactions to disk, unless #mdb_env_sync() is called.
  ///     (#MDB_MAPASYNC | #MDB_WRITEMAP) may be preferable. This flag may be changed at any time
  ///     using #mdb_env_set_flags().
  ///   - #MDB_MAPASYNC When using #MDB_WRITEMAP, use asynchronous flushes to disk. As with
  ///     #MDB_NOSYNC, a system crash can then corrupt the database or lose the last
  ///     transactions. Calling #mdb_env_sync() ensures on-disk database integrity until next
  ///     commit. This flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_NOTLS Don't use Thread-Local Storage. Tie reader locktable slots to #MDB_txn
  ///     objects instead of to threads. I.e. #mdb_txn_reset() keeps the slot reserved for the
  ///     #MDB_txn object. A thread may use parallel read-only transactions. A read-only
  ///     transaction may span threads if the user synchronizes its use. Applications that
  ///     multiplex many user threads over individual OS threads need this option. Such an
  ///     application must also serialize the write transactions in an OS thread, since LMDB's
  ///     write locking is unaware of the user threads.
  ///   - #MDB_NOLOCK Don't do any locking. If concurrent access is anticipated, the caller must
  ///     manage all concurrency itself. For proper operation the caller must enforce
  ///     single-writer semantics, and must ensure that no readers are using old transactions
  ///     while a writer is active. The simplest approach is to use an exclusive lock so that no
  ///     readers may be active at all when a writer begins.
  ///   - #MDB_NORDAHEAD Turn off readahead. Most operating systems perform readahead on read
  ///     requests by default. This option turns it off if the OS supports it. Turning it off may
  ///     help random read performance when the DB is larger than RAM and system RAM is full. The
  ///     option is not implemented on Windows.
  ///   - #MDB_NOMEMINIT Don't initialize malloc'd memory before writing to unused spaces in the
  ///     data file. By default, memory for pages written to the data file is obtained using
  ///     malloc. While these pages may be reused in subsequent transactions, freshly malloc'd
  ///     pages will be initialized to zeroes before use. This avoids persisting leftover data
  ///     from other code (that used the heap and subsequently freed the memory) into the data
  ///     file. Note that many other system libraries may allocate and free memory from the heap
  ///     for arbitrary uses. E.g., stdio may use the heap for file I/O buffers. This
  ///     initialization step has a modest performance cost so some applications may want to
  ///     disable it using this flag. This option can be a problem for applications which handle
  ///     sensitive data like passwords, and it makes memory checkers like Valgrind noisy. This
  ///     flag is not needed with #MDB_WRITEMAP, which writes directly to the mmap instead of
  ///     using malloc for pages. The initialization is also skipped if #MDB_RESERVE is used; the
  ///     caller is expected to overwrite all of the memory that was reserved in that case. This
  ///     flag may be changed at any time using #mdb_env_set_flags().
  ///   - #MDB_PREVSNAPSHOT Open the environment with the previous snapshot rather than the
  ///     latest one. This loses the latest transaction, but may help work around some types of
  ///     corruption. If opened with write access, this must be the only process using the
  ///     environment. This flag is automatically reset after a write transaction is successfully
  ///     committed.
  ///
  ///
  ///   - #MDB_VERSION_MISMATCH - the version of the LMDB library doesn't match the version that
  ///     created the database environment.
  ///   - #MDB_INVALID - the environment file headers are corrupted.
  ///   - ENOENT - the directory specified by the path parameter doesn't exist.
  ///   - EACCES - the user didn't have permission to access the environment files.
  ///   - EAGAIN - the environment was locked by another process.
  ///
  public int flags = EnvFlags.MDB_WRITEMAP | EnvFlags.MDB_NOSUBDIR;

  /// Set the maximum number of threads/reader slots for the environment.
  ///
  /// This defines the number of slots in the lock table that is used to track readers in the
  /// environment. The default is 126. Starting a read-only transaction normally ties a lock table
  /// slot to the current thread until the environment closes or the thread exits. If MDB_NOTLS is
  /// in use, #mdb_txn_begin() instead ties the slot to the MDB_txn object until it or the
  /// #MDB_env object is destroyed. This function may only be called after #mdb_env_create()
  /// and before #mdb_env_open().
  public int maxReaders;

  /// Set the maximum number of named databases for the environment.
  ///
  /// This function is only needed if multiple databases will be used in the environment.
  /// Simpler applications that use the environment as a single unnamed database can ignore this
  /// option. This function may only be called after #mdb_env_create() and before #mdb_env_open().
  ///
  /// Currently, a moderate number of slots are cheap but a huge value gets expensive: 7-120
  /// words per transaction, and every #mdb_dbi_open() does a linear search of the opened
  // slots.
  public int maxDbs = 4;

  /// Set the size of the memory map to use for this environment in bytes.
  ///
  /// The size should be a multiple of the OS page size. The default is 10485760 bytes. The size
  /// of the memory map is also the maximum size of the database. The value should be chosen as
  /// large as possible, to accommodate future growth of the database. This function should be
  /// called after #mdb_env_create() and before #mdb_env_open(). It may be called at later times
  /// if no transactions are active in this process. Note that the library does not check for
  /// this condition, the caller must ensure it explicitly.
  ///
  /// The new size takes effect immediately for the current process but will not be persisted to
  /// any others until a write transaction has been committed by the current process. Also, only
  /// mapsize increases are persisted into the environment.
  ///
  /// If the mapsize is increased by another process, and data has grown beyond the range of the
  /// current mapsize, #mdb_txn_begin() will return #MDB_MAP_RESIZED. This function may be called
  /// with a size of zero to adopt the new size.
  ///
  /// Any attempt to set a size smaller than the space already consumed by the environment will
  /// be silently changed to the current size of the used space.
  ///
  ///   - EINVAL - an invalid parameter was specified, or the environment has an active write
  ///     transaction.
  ///
  public long mapSize = 1024 * 1024 * 32;

  /** File permissions mode. */
  public int mode = 755;

  public Config flags(int flags) {
    this.flags = flags;
    return this;
  }

  public Config maxReaders(int maxReaders) {
    this.maxReaders = maxReaders;
    return this;
  }

  public Config maxDbs(int maxDbs) {
    this.maxDbs = maxDbs;
    return this;
  }

  public Config mapSize(long mapSize) {
    this.mapSize = mapSize;
    return this;
  }

  public Config mode(int mode) {
    this.mode = mode;
    return this;
  }
}
