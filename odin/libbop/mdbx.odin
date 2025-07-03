#+feature dynamic-literals

package libbop

import c "core:c/libc"

dd:: c.size_t

Mdbx_Max_DBI :: 32765
Mdbx_Max_Data_Size :: 0x7fff0000
Mdbx_Min_Page_Size :: 256
Mdbx_Max_Page_Size :: 65536

Mdbx_Env :: struct {}
Mdbx_DBI :: u32
Mdbx_Cursor :: struct {}
Mdbx_Txn :: struct {}

Mdbx_Val :: IO_Vec

Mdbx_Assert_Func :: #type proc "c" (env: ^Mdbx_Env, msg: cstring, function: cstring, line: u32)

/*
Callback function for enumerating user-defined named tables.

\see mdbx_enumerate_tables()

\param [in] ctx       Pointer to the context passed by the same parameter in
					  \ref mdbx_enumerate_tables().
\param [in] txn       Transaction.
\param [in] name      Table name.
\param [in] flags     Flags \ref MDBX_db_flags_t.
\param [in] stat      Basic information \ref MDBX_stat о таблице.
\param [in] dbi       A non-0 DBI handle,
					  if one was opened for this table.
					  Or 0 if no such handle was open.

\returns Zero on success and enumeration continues, if another value is returned
		 it will be immediately returned to the caller without enumeration continuing.
*/
Mdbx_Table_Enum_Func :: #type proc "c" (
	ctx: rawptr,
	txn: ^Mdbx_Txn,
	name: ^Mdbx_Val,
	flags: Mdbx_DB_Flags,
	stat: ^Mdbx_Stat,
	dbi: Mdbx_DBI,
) -> Mdbx_Error

Mdbx_Preserve_Func :: #type proc "c" (
	ctx: rawptr,
	target: ^Mdbx_Val,
	src: [^]byte,
	bytes: uintptr,
) -> Mdbx_Error

/*
The type of predictive callback functions used by
\ref mdbx_cursor_scan() and \ref mdbx_cursor_scan_from() to probe
key-value pairs.

\param [in,out] context  A pointer to a context with the information needed
						 for the assessment, which is entirely prepared and
						 controlled by you.
\param [in] key          Key for evaluating a custom function.
\param [in] value        Value to evaluate by user function.
\param [in,out] arg      Additional argument of the predicative function,
                         which is completely prepared and controlled by you.

\returns The result of the check of the passed key-value pair to the target
sought. Otherwise, an error code that aborts the scan and is returned unchanged
as the result from the functions \ref mdbx_cursor_scan() or
\ref mdbx_cursor_scan_from().

\retval MDBX_RESULT_TRUE if the passed key-value pair matches the one sought,
						 the scan should be terminated.
\retval MDBX_RESULT_FALSE if the passed key-value pair does NOT match the one
						  sought and scanning should continue.
\retval ELSE any value other than \ref MDBX_RESULT_TRUE and
		\ref MDBX_RESULT_FALSE is considered an error indicator
		and is returned unchanged as the scan result.

\see mdbx_cursor_scan()
\see mdbx_cursor_scan_from()
*/
Mdbx_Predicate_Func :: #type proc "c" (
	ctx: rawptr,
	key, value: ^Mdbx_Val,
	arg: rawptr,
) -> Mdbx_Error

/*
A callback function used to enumerate the reader lock table.

\ingroup c_statinfo

\param [in] ctx            An arbitrary context pointer for the callback.
\param [in] num            The serial number during enumeration,
                           starting from 1.
\param [in] slot           The reader lock table slot number.
\param [in] txnid          The ID of the transaction being read,
                           i.e. the MVCC-snapshot number.
\param [in] lag            The lag from a recent MVCC-snapshot,
                           i.e. the number of committed write transactions
                           since the current read transaction started.
\param [in] pid            The reader process ID.
\param [in] thread         The reader thread ID.
\param [in] bytes_used     The number of last used page
                           in the MVCC-snapshot which being read,
                           i.e. database file can't be shrunk beyond this.
\param [in] bytes_retained The total size of the database pages that were
                           retired by committed write transactions after
                           the reader's MVCC-snapshot,
                           i.e. the space which would be freed after
                           the Reader releases the MVCC-snapshot
                           for reuse by completion read transaction.

\returns < 0 on failure, >= 0 on success. \see mdbx_reader_list()
*/
Mdbx_Reader_List_Func :: #type proc "c" (
	ctx: rawptr,
	num: c.int,
	slot: c.int,
	pid: PID,
	thread: TID,
	txnid: u64,
	lag: u64,
	bytes_used: c.size_t,
	bytes_retained: c.size_t,
) -> c.int

/*
A Handle-Slow-Readers callback function to resolve database
full/overflow issue due to a reader(s) which prevents the old data from being
recycled.

Read transactions prevent reuse of pages freed by newer write transactions,
thus the database can grow quickly. This callback will be called when there
is not enough space in the database (i.e. before increasing the database size
or before \ref MDBX_MAP_FULL error) and thus can be used to resolve issues
with a "long-lived" read transactions.
\see mdbx_env_set_hsr()
\see mdbx_env_get_hsr()
\see mdbx_txn_park()
\see <a href="intro.html#long-lived-read">Long-lived read transactions</a>

Using this callback you can choose how to resolve the situation:
  - abort the write transaction with an error;
  - wait for the read transaction(s) to complete;
  - notify a thread performing a long-lived read transaction
    and wait for an effect;
  - kill the thread or whole process that performs the long-lived read
    transaction;

Depending on the arguments and needs, your implementation may wait,
terminate a process or thread that is performing a long read, or perform
some other action. In doing so it is important that the returned code always
corresponds to the performed action.

\param [in] env     An environment handle returned by \ref mdbx_env_create().
\param [in] txn     The current write transaction which internally at
                    the \ref MDBX_MAP_FULL condition.
\param [in] pid     A pid of the reader process.
\param [in] tid     A thread_id of the reader thread.
\param [in] laggard An oldest read transaction number on which stalled.
\param [in] gap     A lag from the last committed txn.
\param [in] space   A space that actually become available for reuse after
                    this reader finished. The callback function can take
                    this value into account to evaluate the impact that
                    a long-running transaction has.
\param [in] retry   A retry number starting from 0.
                    If callback has returned 0 at least once, then at end of
                    current handling loop the callback function will be
                    called additionally with negative `retry` value to notify
                    about the end of loop. The callback function can use this
                    fact to implement timeout reset logic while waiting for
                    a readers.

\returns The RETURN CODE determines the further actions libmdbx and must
         match the action which was executed by the callback:

\retval -2 or less  An error condition and the reader was not killed.

\retval -1          The callback was unable to solve the problem and
                    agreed on \ref MDBX_MAP_FULL error;
                    libmdbx should increase the database size or
                    return \ref MDBX_MAP_FULL error.

\retval 0 (zero)    The callback solved the problem or just waited for
                    a while, libmdbx should rescan the reader lock table and
                    retry. This also includes a situation when corresponding
                    transaction terminated in normal way by
                    \ref mdbx_txn_abort() or \ref mdbx_txn_reset(),
                    and my be restarted. I.e. reader slot don't needed
                    to be cleaned from transaction.

\retval 1           Transaction aborted asynchronous and reader slot
                    should be cleared immediately, i.e. read transaction
                    will not continue but \ref mdbx_txn_abort()
                    nor \ref mdbx_txn_reset() will be called later.

\retval 2 or great  The reader process was terminated or killed,
                    and libmdbx should entirely reset reader registration.
 */
Mdbx_Hsr_Func :: #type proc "c" (
	env: ^Mdbx_Env,
	txn: ^Mdbx_Txn,
	pid: PID,
	tid: TID,
	laggard: u64,
	gap: c.uint,
	space: c.size_t,
	retry: c.int,
) -> c.int

/*
The fours integers markers (aka "canary") associated with the
environment.
\ingroup c_crud
\see mdbx_canary_put()
\see mdbx_canary_get()

The `x`, `y` and `z` values could be set by \ref mdbx_canary_put(), while the
'v' will be always set to the transaction number. Updated values becomes
visible outside the current transaction only after it was committed. Current
values could be retrieved by \ref mdbx_canary_get().
*/
Mdbx_Canary :: struct {
	x, y, z, v: u64,
}

Mdbx_Env_Flags :: enum u32 {
	Env_Defaults      = 0,

	/*
    Extra validation of DB structure and pages content.
   
    The `MDBX_VALIDATION` enabled the simple safe/careful mode for working
    with damaged or untrusted DB. However, a notable performance
    degradation should be expected. */
	Validation        = 0x00002000,

	/*
    No environment directory.

    By default, MDBX creates its environment in a directory whose pathname is
    given in path, and creates its data and lock files under that directory.
    With this option, path is used as-is for the database main data file.
    The database lock file is the path with "-lck" appended.

    - with `MDBX_NOSUBDIR` = in a filesystem we have the pair of MDBX-files
      which names derived from given pathname by appending predefined suffixes.

    - without `MDBX_NOSUBDIR` = in a filesystem we have the MDBX-directory with
      given pathname, within that a pair of MDBX-files with predefined names.

    This flag affects only at new environment creating by \ref mdbx_env_open(),
    otherwise at opening an existing environment libmdbx will choice this
    automatically. */
	No_Sub_Dir        = 0x4000,

	/*
    Read only mode.

    Open the environment in read-only mode. No write operations will be
    allowed. MDBX will still modify the lock file - except on read-only
    filesystems, where MDBX does not use locks.

    - with `MDBX_RDONLY` = open environment in read-only mode.
      MDBX supports pure read-only mode (i.e. without opening LCK-file) only
      when environment directory and/or both files are not writable (and the
      LCK-file may be missing). In such case allowing file(s) to be placed
      on a network read-only share.

    - without `MDBX_RDONLY` = open environment in read-write mode.

    This flag affects only at environment opening but can't be changed after. */
	Read_Only         = 0x20000,

	/*
    Open environment in exclusive/monopolistic mode.

    `MDBX_EXCLUSIVE` flag can be used as a replacement for `MDB_NOLOCK`,
    which don't supported by MDBX.
    In this way, you can get the minimal overhead, but with the correct
    multi-process and multi-thread locking.

    - with `MDBX_EXCLUSIVE` = open environment in exclusive/monopolistic mode
      or return \ref MDBX_BUSY if environment already used by other process.
      The main feature of the exclusive mode is the ability to open the
      environment placed on a network share.

    - without `MDBX_EXCLUSIVE` = open environment in cooperative mode,
      i.e. for multi-process access/interaction/cooperation.
      The main requirements of the cooperative mode are:

      1. data files MUST be placed in the LOCAL file system,
         but NOT on a network share.
      2. environment MUST be opened only by LOCAL processes,
         but NOT over a network.
      3. OS kernel (i.e. file system and memory mapping implementation) and
         all processes that open the given environment MUST be running
         in the physically single RAM with cache-coherency. The only
         exception for cache-consistency requirement is Linux on MIPS
         architecture, but this case has not been tested for a long time).

    This flag affects only at environment opening but can't be changed after. */
	Exclusive         = 0x400000,

	/*
    Using database/environment which already opened by another process(es).

    The `MDBX_ACCEDE` flag is useful to avoid \ref MDBX_INCOMPATIBLE error
    while opening the database/environment which is already used by another
    process(es) with unknown mode/flags. In such cases, if there is a
    difference in the specified flags (\ref MDBX_NOMETASYNC,
    \ref MDBX_SAFE_NOSYNC, \ref MDBX_UTTERLY_NOSYNC, \ref MDBX_LIFORECLAIM
    and \ref MDBX_NORDAHEAD), instead of returning an error,
    the database will be opened in a compatibility with the already used mode.

    `MDBX_ACCEDE` has no effect if the current process is the only one either
    opening the DB in read-only mode or other process(es) uses the DB in
    read-only mode. */
	Accede            = 0x40000000,

	/*
    Map data into memory with write permission.

    Use a writeable memory map unless \ref MDBX_RDONLY is set. This uses fewer
    mallocs and requires much less work for tracking database pages, but
    loses protection from application bugs like wild pointer writes and other
    bad updates into the database. This may be slightly faster for DBs that
    fit entirely in RAM, but is slower for DBs larger than RAM. Also adds the
    possibility for stray application writes thru pointers to silently
    corrupt the database.

    - with `MDBX_WRITEMAP` = all data will be mapped into memory in the
      read-write mode. This offers a significant performance benefit, since the
      data will be modified directly in mapped memory and then flushed to disk
      by single system call, without any memory management nor copying.

    - without `MDBX_WRITEMAP` = data will be mapped into memory in the
      read-only mode. This requires stocking all modified database pages in
      memory and then writing them to disk through file operations.

    \warning On the other hand, `MDBX_WRITEMAP` adds the possibility for stray
    application writes thru pointers to silently corrupt the database.

    \note The `MDBX_WRITEMAP` mode is incompatible with nested transactions,
    since this is unreasonable. I.e. nested transactions requires mallocation
    of database pages and more work for tracking ones, which neuters a
    performance boost caused by the `MDBX_WRITEMAP` mode.

    This flag affects only at environment opening but can't be changed after. */
	Write_Map         = 0x80000,

	/*
    Decouples transactions from threads as much as possible.

    This option is intended for applications that multiplex multiple
    user-defined lightweight threads of execution over separate operating
    system threads, such as in the GoLang and Rust runtimes. Such applications
    are also recommended to serialize write transactions on a single operating
    system thread, since the MDBX write lock uses the underlying system synchronization
    primitives and knows nothing about user threads and/or lightweight threads
    of the runtime.

    At a minimum, it is mandatory to ensure that each write transaction completes
    strictly in the same operating system thread where it was started.

    \note Starting with v0.13, the `MDBX_NOSTICKYTHREADS` option completely
    replaces the \ref MDBX_NOTLS option.

    When using `MDBX_NOSTICKYTHREADS`, transactions become unassociated with the
    threads of execution that created them. Therefore, the API functions do not
    check whether the transaction corresponds to the current thread of execution.
    Most functions working with transactions and cursors can be called from any
    thread. However, it also becomes impossible to detect errors of simultaneous
    use of transactions and/or cursors in different threads.

    Using `MDBX_NOSTICKYTHREADS` also limits the ability to change the size of the
    database, since it is no longer possible to track the threads of execution working
    with the database and suspend them while the database is being removed from RAM.
    In particular, for this reason, on Windows, it is not possible to reduce the size
    of the database file until the last process working with it closes the database
    or until the database is subsequently opened in read-write mode.

    \warning Regardless of \ref MDBX_NOSTICKYTHREADS and \ref MDBX_NOTLS, it is not
    allowed to use API objects from different threads of execution at the same time!

    It is entirely your responsibility to ensure that all measures are taken to exclude
    the simultaneous use of API objects from different threads of execution!

    \warning Write transactions can only be committed in the same thread... Reading
    transactions, when using `MDBX_NOSTICKYTHREADS`, stop using TLS (Thread Local Storage),
    and MVCC snapshot lock slots in the reader table are bound only to transactions.
    Termination of any threads does not release MVCC snapshot locks until explicit
    termination of the transactions, or until the corresponding process as a whole is
    terminated.

    For writing transactions, no check is performed to ensure that the current thread of
    execution matches the thread that created the transaction. However, committing or
    aborting writing transactions must be performed strictly in the thread that started
    the transaction, since these operations are associated with the acquisition and release
    of synchronization primitives (mutexes, critical sections), for which most operating
    systems require that they be released only by the thread that acquired the resource.

    This flag takes effect when the environment is opened and cannot be changed after that.*/
	No_Sticky_Threads = 0x200000,

	/*
    Don't do readahead.

    Turn off readahead. Most operating systems perform readahead on read
    requests by default. This option turns it off if the OS supports it.
    Turning it off may help random read performance when the DB is larger
    than RAM and system RAM is full.

    By default libmdbx dynamically enables/disables readahead depending on
    the actual database size and currently available memory. On the other
    hand, such automation has some limitation, i.e. could be performed only
    when DB size changing but can't tracks and reacts changing a free RAM
    availability, since it changes independently and asynchronously.

    \note The mdbx_is_readahead_reasonable() function allows to quickly find
    out whether to use readahead or not based on the size of the data and the
    amount of available memory.

    This flag affects only at environment opening and can't be changed after.*/
	No_Read_Ahead     = 0x800000,

	/*
    Don't initialize malloc'ed memory before writing to datafile.

    Don't initialize malloc'ed memory before writing to unused spaces in the
    data file. By default, memory for pages written to the data file is
    obtained using malloc. While these pages may be reused in subsequent
    transactions, freshly malloc'ed pages will be initialized to zeroes before
    use. This avoids persisting leftover data from other code (that used the
    heap and subsequently freed the memory) into the data file.

    Note that many other system libraries may allocate and free memory from
    the heap for arbitrary uses. E.g., stdio may use the heap for file I/O
    buffers. This initialization step has a modest performance cost so some
    applications may want to disable it using this flag. This option can be a
    problem for applications which handle sensitive data like passwords, and
    it makes memory checkers like Valgrind noisy. This flag is not needed
    with \ref MDBX_WRITEMAP, which writes directly to the mmap instead of using
    malloc for pages. The initialization is also skipped if \ref MDBX_RESERVE
    is used; the caller is expected to overwrite all of the memory that was
    reserved in that case.

    This flag may be changed at any time using `mdbx_env_set_flags()`. */
	No_Mem_Init       = 0x1000000,

	/*
    LIFO policy for recycling a Garbage Collection items.

    `MDBX_LIFORECLAIM` flag turns on LIFO policy for recycling a Garbage
    Collection items, instead of FIFO by default. On systems with a disk
    write-back cache, this can significantly increase write performance, up
    to several times in a best case scenario.

    LIFO recycling policy means that for reuse pages will be taken which became
    unused the lastest (i.e. just now or most recently). Therefore the loop of
    database pages circulation becomes as short as possible. In other words,
    the number of pages, that are overwritten in memory and on disk during a
    series of write transactions, will be as small as possible. Thus creates
    ideal conditions for the efficient operation of the disk write-back cache.

    \ref MDBX_LIFORECLAIM is compatible with all no-sync flags, but gives NO
    noticeable impact in combination with \ref MDBX_SAFE_NOSYNC or
    \ref MDBX_UTTERLY_NOSYNC. Because MDBX will reused pages only before the
    last "steady" MVCC-snapshot, i.e. the loop length of database pages
    circulation will be mostly defined by frequency of calling
    \ref mdbx_env_sync() rather than LIFO and FIFO difference.

    This flag may be changed at any time using mdbx_env_set_flags(). */
	Lifo_Reclaim      = 0x4000000,

	/*
    Debugging option, fill/perturb released pages. */
	Page_Perturb      = 0x8000000,

	// Sync Modes

	/*
    SYNC MODES

    Using any combination of \ref MDBX_SAFE_NOSYNC, \ref
    MDBX_NOMETASYNC and especially \ref MDBX_UTTERLY_NOSYNC is always a deal to
    reduce durability for gain write performance. You must know exactly what
    you are doing and what risks you are taking!

    @note for LMDB users: \ref MDBX_SAFE_NOSYNC is NOT similar to LMDB_NOSYNC,
    but \ref MDBX_UTTERLY_NOSYNC is exactly match LMDB_NOSYNC. See details
    below.

    THE SCENE:
    - The DAT-file contains several MVCC-snapshots of B-tree at same time,
      each of those B-tree has its own root page.
    - Each of meta pages at the beginning of the DAT file contains a
      pointer to the root page of B-tree which is the result of the particular
      transaction, and a number of this transaction.
    - For data durability, MDBX must first write all MVCC-snapshot data
      pages and ensure that are written to the disk, then update a meta page
      with the new transaction number and a pointer to the corresponding new
      root page, and flush any buffers yet again.
    - Thus during commit a I/O buffers should be flushed to the disk twice;
      i.e. fdatasync(), FlushFileBuffers() or similar syscall should be
      called twice for each commit. This is very expensive for performance,
      but guaranteed durability even on unexpected system failure or power
      outage. Of course, provided that the operating system and the
      underlying hardware (e.g. disk) work correctly.

    TRADE-OFF:
    By skipping some stages described above, you can significantly benefit in
    speed, while partially or completely losing in the guarantee of data
    durability and/or consistency in the event of system or power failure.
    Moreover, if for any reason disk write order is not preserved, then at
    moment of a system crash, a meta-page with a pointer to the new B-tree may
    be written to disk, while the itself B-tree not yet. In that case, the
    database will be corrupted!

    @see Sync_Durable
    @see No_Meta_Sync
    @see Safe_No_Sync
    @see Utterly_No_Sync
    */

	/*
    Default robust and durable sync mode.
  
      Metadata is written and flushed to disk after a data is written and
      flushed, which guarantees the integrity of the database in the event
      of a crash at any time.
  
      \attention Please do not use other modes until you have studied all the
      details and are sure. Otherwise, you may lose your users' data, as happens
      in [Miranda NG](https://www.miranda-ng.org/) messenger. */
	Sync_Durable      = 0,

	/*
    Don't sync the meta-page after commit.

    Flush system buffers to disk only once per transaction commit, omit the
    metadata flush. Defer that until the system flushes files to disk,
    or next non-\ref MDBX_RDONLY commit or \ref mdbx_env_sync(). Depending on
    the platform and hardware, with \ref MDBX_NOMETASYNC you may get a doubling
    of write performance.

    This trade-off maintains database integrity, but a system crash may
    undo the last committed transaction. I.e. it preserves the ACI
    (atomicity, consistency, isolation) but not D (durability) database
    property.

    `MDBX_NOMETASYNC` flag may be changed at any time using
    \ref mdbx_env_set_flags() or by passing to \ref mdbx_txn_begin() for
    particular write transaction. \see sync_modes */
	No_Meta_Sync      = 0x40000,

	/*
    Don't sync anything but keep previous steady commits.

    Like \ref MDBX_UTTERLY_NOSYNC the `MDBX_SAFE_NOSYNC` flag disable similarly
    flush system buffers to disk when committing a transaction. But there is a
    huge difference in how are recycled the MVCC snapshots corresponding to
    previous "steady" transactions (see below).

    With \ref MDBX_WRITEMAP the `MDBX_SAFE_NOSYNC` instructs MDBX to use
    asynchronous mmap-flushes to disk. Asynchronous mmap-flushes means that
    actually all writes will scheduled and performed by operation system on it
    own manner, i.e. unordered. MDBX itself just notify operating system that
    it would be nice to write data to disk, but no more.

    Depending on the platform and hardware, with `MDBX_SAFE_NOSYNC` you may get
    a multiple increase of write performance, even 10 times or more.

    In contrast to \ref MDBX_UTTERLY_NOSYNC mode, with `MDBX_SAFE_NOSYNC` flag
    MDBX will keeps untouched pages within B-tree of the last transaction
    "steady" which was synced to disk completely. This has big implications for
    both data durability and (unfortunately) performance:
     - a system crash can't corrupt the database, but you will lose the last
       transactions; because MDBX will rollback to last steady commit since it
       kept explicitly.
     - the last steady transaction makes an effect similar to "long-lived" read
       transaction (see above in the \ref restrictions section) since prevents
       reuse of pages freed by newer write transactions, thus the any data
       changes will be placed in newly allocated pages.
     - to avoid rapid database growth, the system will sync data and issue
       a steady commit-point to resume reuse pages, each time there is
       insufficient space and before increasing the size of the file on disk.

    In other words, with `MDBX_SAFE_NOSYNC` flag MDBX insures you from the
    whole database corruption, at the cost increasing database size and/or
    number of disk IOPs. So, `MDBX_SAFE_NOSYNC` flag could be used with
    \ref mdbx_env_sync() as alternatively for batch committing or nested
    transaction (in some cases). As well, auto-sync feature exposed by
    \ref mdbx_env_set_syncbytes() and \ref mdbx_env_set_syncperiod() functions
    could be very useful with `MDBX_SAFE_NOSYNC` flag.

    The number and volume of disk IOPs with MDBX_SAFE_NOSYNC flag will
    exactly the as without any no-sync flags. However, you should expect a
    larger process's [work set](https://bit.ly/2kA2tFX) and significantly worse
    a [locality of reference](https://bit.ly/2mbYq2J), due to the more
    intensive allocation of previously unused pages and increase the size of
    the database.

    `MDBX_SAFE_NOSYNC` flag may be changed at any time using
    \ref mdbx_env_set_flags() or by passing to \ref mdbx_txn_begin() for
    particular write transaction. */
	Safe_No_Sync      = 0x10000,
	/*
    \deprecated Please use \ref MDBX_SAFE_NOSYNC instead of `MDBX_MAPASYNC`.

    Since version 0.9.x the `MDBX_MAPASYNC` is deprecated and has the same
    effect as \ref MDBX_SAFE_NOSYNC with \ref MDBX_WRITEMAP. This just API
    simplification is for convenience and clarity. */
	Map_Async         = Safe_No_Sync,

	/*
    Don't sync anything and wipe previous steady commits.

    Don't flush system buffers to disk when committing a transaction. This
    optimization means a system crash can corrupt the database, if buffers are
    not yet flushed to disk. Depending on the platform and hardware, with
    `MDBX_UTTERLY_NOSYNC` you may get a multiple increase of write performance,
    even 100 times or more.

    If the filesystem preserves write order (which is rare and never provided
    unless explicitly noted) and the \ref MDBX_WRITEMAP and \ref
    MDBX_LIFORECLAIM flags are not used, then a system crash can't corrupt the
    database, but you can lose the last transactions, if at least one buffer is
    not yet flushed to disk. The risk is governed by how often the system
    flushes dirty buffers to disk and how often \ref mdbx_env_sync() is called.
    So, transactions exhibit ACI (atomicity, consistency, isolation) properties
    and only lose `D` (durability). I.e. database integrity is maintained, but
    a system crash may undo the final transactions.

    Otherwise, if the filesystem not preserves write order (which is
    typically) or \ref MDBX_WRITEMAP or \ref MDBX_LIFORECLAIM flags are used,
    you should expect the corrupted database after a system crash.

    So, most important thing about `MDBX_UTTERLY_NOSYNC`:
     - a system crash immediately after commit the write transaction
       high likely lead to database corruption.
     - successful completion of mdbx_env_sync(force = true) after one or
       more committed transactions guarantees consistency and durability.
     - BUT by committing two or more transactions you back database into
       a weak state, in which a system crash may lead to database corruption!
       In case single transaction after mdbx_env_sync, you may lose transaction
       itself, but not a whole database.

    Nevertheless, `MDBX_UTTERLY_NOSYNC` provides "weak" durability in case
    of an application crash (but no durability on system failure), and
    therefore may be very useful in scenarios where data durability is
    not required over a system failure (e.g for short-lived data), or if you
    can take such risk.

    `MDBX_UTTERLY_NOSYNC` flag may be changed at any time using
    \ref mdbx_env_set_flags(), but don't has effect if passed to
    \ref mdbx_txn_begin() for particular write transaction. \see sync_modes */
	Utterly_No_Sync   = Safe_No_Sync | 0x100000,
}

Mdbx_Txn_Flags :: enum u32 {
	/*
	Start read-write transaction.
	Only one write transaction may be active at a time. Writes are fully
	serialized, which guarantees that writers can never deadlock. */
	Read_Write        = 0,

	/*
	Start read-only transaction.

    There can be multiple read-only transactions simultaneously that do not
    block each other and a write transactions. */
	Read_Only         = u32(Mdbx_Env_Flags.Read_Only),

	/* Prepare but not start read-only transaction.
	Transaction will not be started immediately, but created transaction handle
	will be ready for use with \ref mdbx_txn_renew(). This flag allows to
	preallocate memory and assign a reader slot, thus avoiding these operations
	at the next start of the transaction. */
	Read_Only_Prepare = u32(Mdbx_Env_Flags.Read_Only | Mdbx_Env_Flags.No_Mem_Init),

	/* Do not block when starting a write transaction. */
	Try               = 0x10000000,

	/*
	Exactly the same as \ref MDBX_NOMETASYNC,
    but for this transaction only. */
	No_Meta_Sync      = u32(Mdbx_Env_Flags.No_Meta_Sync),

	/*
	Exactly the same as \ref MDBX_SAFE_NOSYNC,
    but for this transaction only. */
	No_Sync           = u32(Mdbx_Env_Flags.Safe_No_Sync),

	/*
	Exactly the same as \ref MDBX_SAFE_NOSYNC,
    but for this transaction only. */

	/*
	Transaction is invalid.
    \note Transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Invalid           = 2147483648,

	/*
	Transaction is finished or never began.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Finished          = 0x01,

	/*
	Transaction is unusable after an error.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Error             = 0x02,

	/*
	Transaction must write, even if dirty list is empty.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Dirty             = 0x04,

	/*
	Transaction or a parent has spilled pages.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Spills            = 0x08,

	/*
	Transaction has a nested child transaction.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Has_Child         = 0x10,

	/*
	Transaction is parked by \ref mdbx_txn_park().
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Parked            = 0x20,

	/*
	Transaction is parked by \ref mdbx_txn_park() with `autounpark=true`,
    and therefore it can be used without explicitly calling
    \ref mdbx_txn_unpark() first.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Auto_Unpark       = 0x40,

	/*
	The transaction was blocked using the \ref mdbx_txn_park() function,
    and then ousted by a write transaction because
    this transaction was interfered with garbage recycling.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Ousted            = 0x80,

	/*
	Most operations on the transaction are currently illegal.
    \note This is a transaction state flag. Returned from \ref mdbx_txn_flags()
    but can't be used with \ref mdbx_txn_begin(). */
	Blocked           = Finished | Error | Has_Child | Parked,
}

Mdbx_DB_Flags :: enum u32 {
	/*
	Variable length unique keys with usual byte-by-byte string comparison. */
	Defaults    = 0,

	/*
	Use reverse string comparison for keys. */
	Reverse_Key = 0x02,

	/*
	Use sorted duplicates, i.e. allow multi-values for a keys. */
	Dup_Sort    = 0x04,

	/*
	Numeric keys in native byte order either uint32_t or uint64_t
    (must be one of uint32_t or uint64_t, other integer types, for example,
    signed integer or uint16_t will not work).
    The keys must all be of the same size and must be aligned while passing as
    arguments. */
	Integer_Key = 0x08,

	/*
	With \ref MDBX_DUPSORT; sorted dup items have fixed size. The data values
    must all be of the same size. */
	Dup_Fixed   = 0x10,

	/*
	With \ref MDBX_DUPSORT and with \ref MDBX_DUPFIXED; dups are fixed size
    like \ref MDBX_INTEGERKEY -style integers. The data values must all be of
    the same size and must be aligned while passing as arguments. */
	Integer_Dup = 0x20,

	/*
	With \ref MDBX_DUPSORT; use reverse string comparison for data values. */
	Reverse_Dup = 0x40,

	/*
	Create DB if not already existing. */
	Create      = 0x40000,

	/*
	Opens an existing table created with unknown flags.

    The `MDBX_DB_ACCEDE` flag is intend to open a existing table which
    was created with unknown flags (\ref MDBX_REVERSEKEY, \ref MDBX_DUPSORT,
    \ref MDBX_INTEGERKEY, \ref MDBX_DUPFIXED, \ref MDBX_INTEGERDUP and
    \ref MDBX_REVERSEDUP).

    In such cases, instead of returning the \ref MDBX_INCOMPATIBLE error, the
    table will be opened with flags which it was created, and then an
    application could determine the actual flags by \ref mdbx_dbi_flags(). */
	Accede      = u32(Mdbx_Env_Flags.Accede),
}

Mdbx_Put_Flags :: enum u32 {
	/*
	Upsertion by default (without any other flags) */
	Upsert       = 0,

	/*
	For insertion: Don't write if the key already exists. */
	No_Overwrite = 0x10,

	/*
	Has effect only for \ref MDBX_DUPSORT tables.
    For upsertion: don't write if the key-value pair already exist. */
	No_Dup_Data  = 0x20,

	/*
	For upsertion: overwrite the current key/data pair.
    MDBX allows this flag for \ref mdbx_put() for explicit overwrite/update
    without insertion.

    For deletion: remove only single entry at the current cursor position. */
	Current      = 0x40,

	/*
	Has effect only for \ref MDBX_DUPSORT tables.
    For deletion: remove all multi-values (aka duplicates) for given key.
    For upsertion: replace all multi-values for given key with a new one. */
	All_Dups     = 0x80,

	/*
	For upsertion: Just reserve space for data, don't copy it.
    Return a pointer to the reserved space. */
	Reserve      = 0x10000,

	/*
	Data is being appended.
    Don't split full pages, continue on a new instead. */
	Append       = 0x20000,

	/*
	Has effect only for \ref MDBX_DUPSORT tables.
    Duplicate data is being appended.
    Don't split full pages, continue on a new instead. */
	Append_Dup   = 0x40000,

	/*
	Only for \ref MDBX_DUPFIXED.
    Store multiple data items in one call. */
	Multiple     = 0x80000,
}

Mdbx_Copy_Flags :: enum u32 {
	Defaults           = 0,

	/*
	Copy with compactification: Omit free space from copy and renumber all
    pages sequentially */
	Compact            = 1,

	/*
	Force to make resizable copy, i.e. dynamic size instead of fixed */
	Force_Dynamic_Size = 2,

	/*
	Don't explicitly flush the written data to an output media */
	Dont_Flush         = 4,

	/*
	Use read transaction parking during copying MVCC-snapshot
    \see mdbx_txn_park() */
	Throttle_MVCC      = 8,

	/*
	Abort/dispose passed transaction after copy
    \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname() */
	Dispose_Txn        = 16,

	/*
	Enable renew/restart read transaction in case it use outdated
    MVCC shapshot, otherwise the \ref MDBX_MVCC_RETARDED will be returned
    \see mdbx_txn_copy2fd() \see mdbx_txn_copy2pathname() */
	Renew_Txn          = 32,
}

/*
Cursor operations

This is the set of all operations for retrieving data using a cursor.
@see mdbx_cursor_get()
*/
Mdbx_Cursor_Op :: enum i32 {
	/* Position at first key/data item */
	First,

	/* \ref MDBX_DUPSORT -only: Position at first data item of current key. */
	First_Dup,

	/* \ref MDBX_DUPSORT -only: Position at key/data pair. */
	Get_Both,

	/* \ref MDBX_DUPSORT -only: Position at given key and at first data greater
    than or equal to specified data. */
	Get_Both_Range,

	/* Return key/data at current cursor position */
	Get_Current,

	/* \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    from current cursor position. Move cursor to prepare
    for \ref MDBX_NEXT_MULTIPLE. */
	Get_Multiple,

	/* Position at last key/data item */
	Last,

	/* \ref MDBX_DUPSORT -only: Position at last data item of current key. */
	Last_Dup,

	/* Position at next data item */
	Next,

	/* \ref MDBX_DUPSORT -only: Position at next data item of current key. */
	Next_Dup,

	/* \ref MDBX_DUPFIXED -only: Return up to a page of duplicate data items
    from next cursor position. Move cursor to prepare
    for `MDBX_NEXT_MULTIPLE`. */
	Next_Multiple,

	/* Position at first data item of next key */
	Next_No_Dup,

	/* Position at previous data item */
	Prev,

	/* \ref MDBX_DUPSORT -only: Position at previous data item of current key. */
	Prev_Dup,

	/* Position at last data item of previous key */
	Prev_No_Dup,

	/* Position at specified key */
	Set,

	/* Position at specified key, return both key and data */
	Set_Key,

	/* Position at first key greater than or equal to specified key. */
	Set_Range,

	/*
	\ref MDBX_DUPFIXED -only: Position at previous page and return up to
    a page of duplicate data items. */
	Prev_Multiple,

	/*
	Positions cursor at first key-value pair greater than or equal to
    specified, return both key and data, and the return code depends on whether
    a exact match.

    For non DUPSORT-ed collections this work the same to \ref MDBX_SET_RANGE,
    but returns \ref MDBX_SUCCESS if key found exactly or
    \ref MDBX_RESULT_TRUE if greater key was found.

    For DUPSORT-ed a data value is taken into account for duplicates,
    i.e. for a pairs/tuples of a key and an each data value of duplicates.
    Returns \ref MDBX_SUCCESS if key-value pair found exactly or
    \ref MDBX_RESULT_TRUE if the next pair was returned. */
	Set_Lowerbound,

	/*
	Positions cursor at first key-value pair greater than specified,
    return both key and data, and the return code depends on whether a
    upper-bound was found.

    For non DUPSORT-ed collections this work like \ref MDBX_SET_RANGE,
    but returns \ref MDBX_SUCCESS if the greater key was found or
    \ref MDBX_NOTFOUND otherwise.

    For DUPSORT-ed a data value is taken into account for duplicates,
    i.e. for a pairs/tuples of a key and an each data value of duplicates.
    Returns \ref MDBX_SUCCESS if the greater pair was returned or
    \ref MDBX_NOTFOUND otherwise. */
	Set_Upperbound,

	/* Doubtless cursor positioning at a specified key. */
	To_Key_Lesser_Than,
	To_Key_Lesser_Than_Or_Equal,
	To_Key_Equal,
	To_Key_Greater_Or_Equal,
	To_Key_Greater_Than,

	/*
	Doubtless cursor positioning at a specified key-value pair
    for dupsort/multi-value hives.
    */
	To_Exact_Key_Value_Lesser_Than,
	To_Exact_Key_Value_Lesser_Or_Equal,
	To_Exact_Key_Valyue_Equal,
	To_Exact_Key_Value_Greater_Or_Equal,
	To_Exact_Key_Value_Greater_Than,
	To_Pair_Lesser_Than,
	To_Pair_Lesser_Or_Equal,
	To_Pair_Equal,
	To_Pair_Greater_Or_Equal,
	To_Pair_Greater_Than,
}

/*
Errors and return codes

BerkeleyDB uses -30800 to -30999, we'll go under them
\see mdbx_strerror() \see mdbx_strerror_r() \see mdbx_liberr2str()
*/
Mdbx_Error :: enum c.int {
	/* Successful result */
	Success                = 0,

	/*
	Alias for \ref MDBX_SUCCESS */
	Result_False           = Success,

	/*
	Successful result with special meaning or a flag */
	Result_True            = -1,

	/*
	key/data pair already exists */
	Key_Exist              = -30799,

	/*
	key/data pair not found (EOF) */
	Not_Found              = -30798,

	/*
	Requested page not found - this usually indicates corruption */
	Page_Not_Found         = -30797,

	/*
	Database is corrupted (page was wrong type and so on) */
	Corrupted              = -30796,

	/*
	Environment had fatal error,
    i.e. update of meta page failed and so on. */
	Panic                  = -30795,

	/*
	DB file version mismatch with libmdbx */
	Version_Mismatch       = -30794,

	/*
	File is not a valid MDBX file */
	Invalid                = -30793,

	/*
	Environment mapsize reached */
	Map_Full               = -30792,

	/*
	Environment maxdbs reached */
	DBS_Full               = -30791,

	/*
	Environment maxreaders reached */
	Readers_Full           = -30790,

	/*
	Transaction has too many dirty pages, i.e transaction too big */
	Txn_Full               = -30788,

	/*
	Cursor stack too deep - this usually indicates corruption,
    i.e branch-pages loop */
	Cursor_Full            = -30787,

	/*
	Page has not enough space - internal error */
	Page_Full              = -30786,

	/*
	Database engine was unable to extend mapping, e.g. since address space
    is unavailable or busy. This can mean:
     - Database size extended by other process beyond to environment mapsize
       and engine was unable to extend mapping while starting read
       transaction. Environment should be reopened to continue.
     - Engine was unable to extend mapping during write transaction
       or explicit call of \ref mdbx_env_set_geometry(). */
	Unable_Extend_Map_Size = -30785,

	/*
	Environment or table is not compatible with the requested operation
    or the specified flags. This can mean:
     - The operation expects an \ref MDBX_DUPSORT / \ref MDBX_DUPFIXED
       table.
     - Opening a named DB when the unnamed DB has \ref MDBX_DUPSORT /
       \ref MDBX_INTEGERKEY.
     - Accessing a data record as a named table, or vice versa.
     - The table was dropped and recreated with different flags. */
	Incompatible           = -30784,

	/*
	Invalid reuse of reader locktable slot,
    e.g. read-transaction already run for current thread */
	Bad_RSlot              = -30783,

	/*
	Transaction is not valid for requested operation,
    e.g. had errored and be must aborted, has a child/nested transaction,
    or is invalid */
	Bad_Txn                = -30782,

	/*
	Invalid size or alignment of key or data for target table,
    either invalid table name */
	Bad_Val_Size           = -30781,

	/*
	The specified DBI-handle is invalid
    or changed by another thread/transaction */
	Bad_DBI                = -30780,

	/*
	Unexpected internal error, transaction should be aborted */
	Problem                = -30779,

	/*
	Another write transaction is running or environment is already used while
    opening with \ref MDBX_EXCLUSIVE flag */
	Busy                   = -30778,

	/*
	The first of MDBX-added error codes */
	First_Added_Err_Code   = Busy,

	/*
	The specified key has more than one associated value */
	E_Multi_Val            = -30421,

	/*
	Bad signature of a runtime object(s), this can mean:
     - memory corruption or double-free;
     - ABI version mismatch (rare case); */
	E_Bad_Sign             = -30420,

	/*
	Database should be recovered, but this could NOT be done for now
    since it opened in read-only mode */
	Wanna_Recovery         = -30419,

	/*
	The given key value is mismatched to the current cursor position */
	E_Key_Mismatch         = -30418,

	/*
	Database is too large for current system,
    e.g. could NOT be mapped into RAM. */
	Too_Large              = -30417,

	/*
	A thread has attempted to use a not owned object,
    e.g. a transaction that started by another thread */
	Thread_Mismatch        = -30416,

	/*
	Overlapping read and write transactions for the current thread */
	Txn_Overlapping        = -30415,

	/*
	Internal error returned when there is insufficient free pages
	when updating the GC. Used as a debugging aid. From the user's
	point of view, it is semantically equivalent \ref MDBX_PROBLEM. */
	Backlog_Depleted       = -30414,

	/*
	Alternative/Duplicate LCK-file is exists and should be removed manually */
	Duplicated_CLK         = -30413,

	/*
	Some cursors and/or other resources should be closed before table or
     corresponding DBI-handle could be (re)used and/or closed. */
	Dangling_DBI           = -30412,

	/*
	The parked read transaction was outed for the sake of
    recycling old MVCC snapshots. */
	Ousted                 = -30411,

	/*
	MVCC snapshot used by read transaction is outdated and could not be
     copied since corresponding meta-pages was overwritten. */
	MVCC_Retarded          = -30410,

	/*
	The last of MDBX-added error codes */
	Last_Added_Err_Code    = MVCC_Retarded,
	ENODATA                = i32(ENODATA),
	EINVAL                 = i32(EINVAL),
	EACCESS                = i32(EACCESS),
	ENOMEM                 = i32(ENOMEM),
	EROFS                  = i32(EROFS),
	ENOSYS                 = i32(ENOSYS),
	EIO                    = i32(EIO),
	EPERM                  = i32(EPERM),
	EINTR                  = i32(EINTR),
	ENOFILE                = i32(ENOFILE),
	EREMOTE                = i32(EREMOTE),
	EDEADLK                = i32(EDEADLK),
}

Mdbx_Error_Message := map[Mdbx_Error]string{
	.Result_True = "RESULT_TRUE"
}

/*
MDBX environment extra runtime options.

\see mdbx_env_set_option() \see mdbx_env_get_option()
*/
Mdbx_Option :: enum i32 {
	/*
	Controls the maximum number of named tables for the environment.

    \details By default only unnamed key-value table could used and
    appropriate value should set by `MDBX_opt_max_db` to using any more named
    table(s). To reduce overhead, use the minimum sufficient value. This option
    may only set after \ref mdbx_env_create() and before \ref mdbx_env_open().

    \see mdbx_env_set_maxdbs() \see mdbx_env_get_maxdbs() */
	Max_DB,

	/*
	Defines the maximum number of threads/reader slots
    for all processes interacting with the database.

    \details This defines the number of slots in the lock table that is used to
    track readers in the environment. The default is about 100 for 4K
    system page size. Starting a read-only transaction normally ties a lock
    table slot to the current thread until the environment closes or the thread
    exits. If \ref MDBX_NOSTICKYTHREADS is in use, \ref mdbx_txn_begin()
    instead ties the slot to the \ref MDBX_txn object until it or the \ref
    MDBX_env object is destroyed. This option may only set after \ref
    mdbx_env_create() and before \ref mdbx_env_open(), and has an effect only
    when the database is opened by the first process interacts with the
    database.

    \see mdbx_env_set_maxreaders() \see mdbx_env_get_maxreaders() */
	Max_Readers,

	/*
	Controls interprocess/shared threshold to force flush the data
    buffers to disk, if \ref MDBX_SAFE_NOSYNC is used.

    \see mdbx_env_set_syncbytes() \see mdbx_env_get_syncbytes() */
	Sync_Bytes,

	/*
	Controls interprocess/shared relative period since the last
    unsteady commit to force flush the data buffers to disk,
    if \ref MDBX_SAFE_NOSYNC is used.
    \see mdbx_env_set_syncperiod() \see mdbx_env_get_syncperiod() */
	Sync_Period,

	/*
	Controls the in-process limit to grow a list of reclaimed/recycled
    page's numbers for finding a sequence of contiguous pages for large data
    items.
    \see MDBX_opt_gc_time_limit

    \details A long values requires allocation of contiguous database pages.
    To find such sequences, it may be necessary to accumulate very large lists,
    especially when placing very long values (more than a megabyte) in a large
    databases (several tens of gigabytes), which is much expensive in extreme
    cases. This threshold allows you to avoid such costs by allocating new
    pages at the end of the database (with its possible growth on disk),
    instead of further accumulating/reclaiming Garbage Collection records.

    On the other hand, too small threshold will lead to unreasonable database
    growth, or/and to the inability of put long values.

    The `MDBX_opt_rp_augment_limit` controls described limit for the current
    process. By default this limit adjusted dynamically to 1/3 of current
    quantity of DB pages, which is usually enough for most cases. */
	RP_Augment_Limit,

	/*
	Controls the in-process limit to grow a cache of dirty
    pages for reuse in the current transaction.

    \details A 'dirty page' refers to a page that has been updated in memory
    only, the changes to a dirty page are not yet stored on disk.
    To reduce overhead, it is reasonable to release not all such pages
    immediately, but to leave some ones in cache for reuse in the current
    transaction.

    The `MDBX_opt_loose_limit` allows you to set a limit for such cache inside
    the current process. Should be in the range 0..255, default is 64. */
	Loose_Limit,

	/*
	Controls the in-process limit of a pre-allocated memory items
    for dirty pages.

    \details A 'dirty page' refers to a page that has been updated in memory
    only, the changes to a dirty page are not yet stored on disk.
    Without \ref MDBX_WRITEMAP dirty pages are allocated from memory and
    released when a transaction is committed. To reduce overhead, it is
    reasonable to release not all ones, but to leave some allocations in
    reserve for reuse in the next transaction(s).

    The `MDBX_opt_dp_reserve_limit` allows you to set a limit for such reserve
    inside the current process. Default is 1024. */
	DP_Reserve_Limit,

	/*
	Controls the in-process limit of dirty pages
    for a write transaction.

    \details A 'dirty page' refers to a page that has been updated in memory
    only, the changes to a dirty page are not yet stored on disk.
    Without \ref MDBX_WRITEMAP dirty pages are allocated from memory and will
    be busy until are written to disk. Therefore for a large transactions is
    reasonable to limit dirty pages collecting above an some threshold but
    spill to disk instead.

    The `MDBX_opt_txn_dp_limit` controls described threshold for the current
    process. Default is 65536, it is usually enough for most cases. */
	Txn_DP_Limit,

	/* \brief Controls the in-process initial allocation size for dirty pages
    list of a write transaction. Default is 1024. */
	Txn_DP_Initial,

	/*
	Controls the in-process how maximal part of the dirty pages may be
    spilled when necessary.

    \details The `MDBX_opt_spill_max_denominator` defines the denominator for
    limiting from the top for part of the current dirty pages may be spilled
    when the free room for a new dirty pages (i.e. distance to the
    `MDBX_opt_txn_dp_limit` threshold) is not enough to perform requested
    operation.
    Exactly `max_pages_to_spill = dirty_pages - dirty_pages / N`,
    where `N` is the value set by `MDBX_opt_spill_max_denominator`.

    Should be in the range 0..255, where zero means no limit, i.e. all dirty
    pages could be spilled. Default is 8, i.e. no more than 7/8 of the current
    dirty pages may be spilled when reached the condition described above. */
	Spill_Max_Denominator,

	/*
	Controls the in-process how minimal part of the dirty pages should
    be spilled when necessary.

    \details The `MDBX_opt_spill_min_denominator` defines the denominator for
    limiting from the bottom for part of the current dirty pages should be
    spilled when the free room for a new dirty pages (i.e. distance to the
    `MDBX_opt_txn_dp_limit` threshold) is not enough to perform requested
    operation.
    Exactly `min_pages_to_spill = dirty_pages / N`,
    where `N` is the value set by `MDBX_opt_spill_min_denominator`.

    Should be in the range 0..255, where zero means no restriction at the
    bottom. Default is 8, i.e. at least the 1/8 of the current dirty pages
    should be spilled when reached the condition described above. */
	Spill_Min_Denominator,

	/*
	Controls the in-process how much of the parent transaction dirty
    pages will be spilled while start each child transaction.

    \details The `MDBX_opt_spill_parent4child_denominator` defines the
    denominator to determine how much of parent transaction dirty pages will be
    spilled explicitly while start each child transaction.
    Exactly `pages_to_spill = dirty_pages / N`,
    where `N` is the value set by `MDBX_opt_spill_parent4child_denominator`.

    For a stack of nested transactions each dirty page could be spilled only
    once, and parent's dirty pages couldn't be spilled while child
    transaction(s) are running. Therefore a child transaction could reach
    \ref MDBX_TXN_FULL when parent(s) transaction has  spilled too less (and
    child reach the limit of dirty pages), either when parent(s) has spilled
    too more (since child can't spill already spilled pages). So there is no
    universal golden ratio.

    Should be in the range 0..255, where zero means no explicit spilling will
    be performed during starting nested transactions.
    Default is 0, i.e. by default no spilling performed during starting nested
    transactions, that correspond historically behaviour. */
	Spill_Parent4Child_Denominator,

	/*
	Controls the in-process threshold of semi-empty pages merge.
    \details This option controls the in-process threshold of minimum page
    fill, as used space of percentage of a page. Neighbour pages emptier than
    this value are candidates for merging. The threshold value is specified
    in 1/65536 points of a whole page, which is equivalent to the 16-dot-16
    fixed point format.
    The specified value must be in the range from 12.5% (almost empty page)
    to 50% (half empty page) which corresponds to the range from 8192 and
    to 32768 in units respectively.
    \see MDBX_opt_prefer_waf_insteadof_balance */
	Merge_Threshold_16dot16_Percent,

	/*
	Controls the choosing between use write-through disk writes and
    usual ones with followed flush by the `fdatasync()` syscall.
    \details Depending on the operating system, storage subsystem
    characteristics and the use case, higher performance can be achieved by
    either using write-through or a serie of usual/lazy writes followed by
    the flush-to-disk.

    Basically for N chunks the latency/cost of write-through is:
     latency = N(emit + round-trip-to-storage + storage-execution);
    And for serie of lazy writes with flush is:
     latency = N(emit + storage-execution) + flush + round-trip-to-storage.

    So, for large N and/or noteable round-trip-to-storage the write+flush
    approach is win. But for small N and/or near-zero NVMe-like latency
    the write-through is better.

    To solve this issue libmdbx provide `MDBX_opt_writethrough_threshold`:
     - when N described above less or equal specified threshold,
       a write-through approach will be used;
     - otherwise, when N great than specified threshold,
       a write-and-flush approach will be used.
	*
    \note MDBX_opt_writethrough_threshold affects only \ref MDBX_SYNC_DURABLE
    mode without \ref MDBX_WRITEMAP, and not supported on Windows.
    On Windows a write-through is used always but \ref MDBX_NOMETASYNC could
    be used for switching to write-and-flush. */
	Writethrough_Threshold,

	/*
	Controls prevention of page-faults of reclaimed and allocated pages
    in the \ref MDBX_WRITEMAP mode by clearing ones through file handle before
    touching. */
	Prefault_Write_Enable,

	/*
	Controls the in-process spending time limit of searching consecutive pages
	inside GC.

    \see MDBX_opt_rp_augment_limit

    Specifies a time limit in 1/65536ths of a second that can be spent during a
    write transaction searching for page sequences within the GC/freelist after
    reaching the limit specified by the \ref MDBX_opt_rp_augment_limit option.
    Time control is not performed when searching/allocating single pages and when
    allocating pages for GC needs (when updating the GC during a transaction commit).

    The specified time limit is calculated by the "wall clock" and is controlled
    within the transaction, inherited for nested transactions and with accumulation
    in the parent when they are committed. Time control is performed only when the
    limit specified by the \ref MDBX_opt_rp_augment_limit option is reached. This
    allows flexible behavior control using both options.

    By default, the limit is set to 0, which causes GC search to stop immediately
    when \ref MDBX_opt_rp_augment_limit is reached in the internal transaction state,
    and matches the behavior before the `MDBX_opt_gc_time_limit` option was introduced.
    On the other hand, with the minimum value (including 0) of `MDBX_opt_rp_augment_limit`,
    GC processing will be limited primarily by elapsed time.
    */
	GC_Time_Limit,

	/*
	Controls the choice between striving for uniformity of page filling or
	reducing the number of modified and written pages.

    After deletion operations, pages containing less than the minimum number
    of keys or emptied to \ref MDBX_opt_merge_threshold_16dot16_percent are
    subject to merging with one of the neighboring pages. If the pages to the
    right and left of the current page are both "dirty" (were modified during
    the transaction and must be written to disk), or both are "clean" (were
    not modified during the current transaction), then the less filled page is
    always chosen as the target for merging. When only one of the neighboring
    pages is "dirty" and the other is "clean", then two tactics for choosing
    the target for merging are possible:

     - If `MDBX_opt_prefer_waf_insteadof_balance = True`, then an already
       modified page will be selected, which will NOT INCREASE the number of
       modified pages and the amount of disk writes when the current transaction
       is committed, but on average will INCREASE the unevenness of page filling.

     - If `MDBX_opt_prefer_waf_insteadof_balance = False`, then the less filled
       page will be chosen, which will INCREASE the number of modified pages and
       the amount of disk writes when the current transaction commits, but on
       average will DECREASE the unevenness of page filling.

    \see MDBX_opt_merge_threshold_16dot16_percent */
	Prefer_WAF_Instead_Of_Balance,

	/*
	Specifies in % the maximum size of nested pages used to accommodate a small
	number of multi-values associated with a single key.

    Using nested pages, instead of moving values to separate pages of the nested
    tree, allows you to reduce the amount of unused space and thereby increase
    the density of data placement.

    But as the nested pages increase in size, more leaf pages of the main tree are
    required, which also increases the height of the main tree. In addition, changing
    data in nested pages requires additional copies, so the cost can be higher in
    many scenarios.

    min 12.5% (8192), max 100% (65535), default = 100% */
	Subpage_Limit,

	/*
	Задаёт в % минимальный объём свободного места на основной странице,
    при отсутствии которого вложенные страницы выносятся в отдельное дерево.

    min 0, max 100% (65535), default = 0 */
	Subpage_Room_Threshold,

	/*
	Specifies in % the minimum amount of free space on the main page, if
	available, space is reserved in the nested page.

    If there is not enough free space on the main page, then the nested page
    will be of the minimum size. In turn, if there is no reserve in the nested
    page, each addition of elements to it will require re-forming the main page
    with the transfer of all data nodes.

    Therefore, reserving space is usually beneficial in scenarios with intensive
    addition of short multi-values, such as indexing. But it reduces the data
    density, and accordingly increases the volume of the database and I/O operations.

    min 0, max 100% (65535), default = 42% (27525) */
	Subpage_Reserve_Prereq,

	/*
	Specifies the limit in % for reserving space on nested pages.

    min 0, max 100% (65535), default = 4.2% (2753) */
	Subpage_Reserve_Limit,
}

/*
Deletion modes for \ref mdbx_env_delete().

\see mdbx_env_delete()
*/
Mdbx_Env_Delete_Mode :: enum i32 {
	/*
	Just delete the environment's files and directory if any.
    \note On POSIX systems, processes already working with the database will
    continue to work without interference until it close the environment.
    \note On Windows, the behavior of `MDBX_ENV_JUST_DELETE` is different
    because the system does not support deleting files that are currently
    memory mapped.
    */
	Just_Delete     = 0,

	/*
	Make sure that the environment is not being used by other
    processes, or return an error otherwise.
    */
	Ensure_Unused   = 1,

	/*
	Wait until other processes closes the environment before deletion.
    */
	Wait_For_Unused = 2,
}

/*
Statistics for a table in the environment

mdbx_env_stat_ex() \see mdbx_dbi_stat()
*/
Mdbx_Stat :: struct {
	/*
	Size of a table page. This is the same for all tables in a database.
	*/
	psize:          u32,

	/*
	Depth (height) of the B-tree
	*/
	depth:          u32,

	/*
	Number of internal (non-leaf) pages
	*/
	branch_pages:   u64,

	/*
	Number of leaf pages
	*/
	leaf_pages:     u64,

	/*
	Number of large/overflow pages
	*/
	overflow_pages: u64,

	/*
	Number of data items
	*/
	entries:        u64,

	/*
	Transaction ID of committed last modification
	*/
	mod_txnid:      u64,
}

Mdbx_Env_Info :: struct {
	geo:                                struct {
		/* Lower limit for datafile size */
		lower:   u64,
		/* Upper limit for datafile size */
		upper:   u64,
		/* Current datafile size */
		current: u64,
		/* Shrink threshold for datafile */
		shrink:  u64,
		/* Growth step for datafile */
		grow:    u64,
	},

	/* Size of the data memory map */
	map_size:                           u64,

	/* Number of the last used page */
	last_pgno:                          u64,

	/* ID of the last committed transaction */
	recent_txnid:                       u64,

	/* ID of the last reader transaction */
	latter_reader_txnid:                u64,

	/* ID of the last reader transaction of caller process */
	self_latter_reader_txnid:           u64,
	meta_txnid:                         [3]u64,
	meta_sign:                          [3]u64,

	/* Total reader slots in the environment */
	max_readers:                        u32,

	/* Max reader slots used in the environment */
	num_readers:                        u32,

	/* Database pagesize */
	dxb_page_size:                      u32,

	/* System pagesize */
	sys_page_size:                      u32,

	/*
	A mostly unique ID that is regenerated on each boot.

	As such it can be used to identify the local machine's current boot. MDBX
	uses such when open the database to determine whether rollback required to
	the last steady sync point or not. I.e. if current bootid is differ from the
	value within a database then the system was rebooted and all changes since
	last steady sync must be reverted for data integrity. Zeros mean that no
	relevant information is available from the system.
	*/
	boot_id:                            struct {
		current: struct {
			x, y: u64,
		},
		meta:    [3]struct {
			x, y: u64,
		},
	},

	/* Bytes not explicitly synchronized to disk */
	unsync_volume:                      u64,

	/* Current auto-sync threshold, see \ref mdbx_env_set_syncbytes(). */
	auto_sync_threshold:                u64,

	/*
	Time since entering to a "dirty" out-of-sync state in units of 1/65536 of
	second. In other words, this is the time since the last non-steady commit
	or zero if it was steady.
    */
	since_sync_seconds_16dot16:         u32,

	/*
	Current auto-sync period in 1/65536 of second,
    see \ref mdbx_env_set_syncperiod().
    */
	auto_sync_period_seconds_16dot16:   u32,

	/*
	Time since the last readers check in 1/65536 of second,
    see \ref mdbx_reader_check().
    */
	since_reader_check_seconds_16dot16: u32,

	/*
	Current environment mode.
    The same as \ref mdbx_env_get_flags() returns.
    */
	mode:                               u32,

	/*
	Statistics of page operations.

    Overall statistics of page operations of all (running, completed
    and aborted) transactions in the current multi-process session (since the
    first process opened the database after everyone had previously closed it).
	*/
	pgop_stat:                          struct {
		/* Quantity of a new pages added */
		newly:    u64,

		/* Quantity of pages copied for update */
		cow:      u64,

		/* Quantity of parent's dirty pages clones for nested transactions */
		clone:    u64,

		/* Page splits */
		split:    u64,

		/* Page merges */
		merge:    u64,

		/* Quantity of spilled dirty pages */
		spill:    u64,

		/* Quantity of unspilled/reloaded pages */
		unspill:  u64,

		/* Number of explicit write operations (not a pages) to a disk */
		wops:     u64,

		/* Number of prefault write operations (not a pages) */
		prefault: u64,

		/* Number of mincore() calls */
		mincore:  u64,

		/* Number of explicit msync-to-disk operations (not a pages) */
		msync:    u64,

		/* Number of explicit fsync-to-disk operations (not a pages) */
		fsync:    u64,
	},

	/* GUID of the database DXB file. */
	dxbid:                              struct {
		x, y: u64,
	},
}

/*
Warming up options

\see mdbx_env_warmup()
*/
Mdbx_Warmup_Flags :: enum i32 {
	/*
	By default \ref mdbx_env_warmup() just ask OS kernel to asynchronously
	prefetch database pages.
	*/
	Default     = 0,
	/*
	Peeking all pages of allocated portion of the database
	to force ones to be loaded into memory. However, the pages are just peeks
	sequentially, so unused pages that are in GC will be loaded in the same
	way as those that contain payload.
	*/
	Force       = 1,

	/*
	Using system calls to peeks pages instead of directly accessing ones,
	which at the cost of additional overhead avoids killing the current
	process by OOM-killer in a lack of memory condition.
	\note Has effect only on POSIX (non-Windows) systems with conjunction
	to \ref MDBX_warmup_force option.
	*/
	OOM_Safe    = 2,

	/*
	Try to lock database pages in memory by `mlock()` on POSIX-systems
	or `VirtualLock()` on Windows. Please refer to description of these
	functions for reasonability of such locking and the information of
	effects, including the system as a whole.

	Such locking in memory requires that the corresponding resource limits
	(e.g. `RLIMIT_RSS`, `RLIMIT_MEMLOCK` or process working set size)
	and the availability of system RAM are sufficiently high.

	On successful, all currently allocated pages, both unused in GC and
	containing payload, will be locked in memory until the environment closes,
	or explicitly unblocked by using \ref MDBX_warmup_release, or the
	database geometry will changed, including its auto-shrinking.
	*/
	Lock        = 4,

	/*
	Alters corresponding current resource limits to be enough for lock pages
	by \ref MDBX_warmup_lock. However, this option should be used in simpler
	applications since takes into account only current size of this environment
	disregarding all other factors. For real-world database application you
	will need full-fledged management of resources and their limits with
	respective engineering.
	*/
	Touch_Limit = 8,

	/* Release the lock that was performed before by \ref MDBX_warmup_lock. */
	Release     = 16,
}

/*
Information about the transaction

@see mdbx_txn_info
*/
Mdbx_Txn_Info :: struct {
	/*
	The ID of the transaction. For a READ-ONLY transaction, this corresponds
    to the snapshot being read.
    */
	id:               u64,

	/*
	For READ-ONLY transaction: the lag from a recent MVCC-snapshot, i.e. the
    number of committed transaction since read transaction started.
    For WRITE transaction (provided if `scan_rlt=true`): the lag of the oldest
    reader from current transaction (i.e. at least 1 if any reader running).
    */
	reader_lag:       u64,

	/*
	Used space by this transaction, i.e. corresponding to the last used database page.
	*/
	space_used:       u64,

	/* Current size of database file. */
	space_limit_soft: u64,

	/*
	Upper bound for size the database file, i.e. the value `size_upper`
    argument of the appropriate call of \ref mdbx_env_set_geometry().
    */
	space_limit_hard: u64,

	/*
	For READ-ONLY transaction: The total size of the database pages that were
    retired by committed write transactions after the reader's MVCC-snapshot,
    i.e. the space which would be freed after the Reader releases the
    MVCC-snapshot for reuse by completion read transaction.
    For WRITE transaction: The summarized size of the database pages that were
    retired for now due Copy-On-Write during this transaction.
    */
	space_retired:    u64,

	/*
	For READ-ONLY transaction: the space available for writer(s) and that
    must be exhausted for reason to call the Handle-Slow-Readers callback for
    this read transaction.
    For WRITE transaction: the space inside transaction
    that left to `MDBX_TXN_FULL` error.
    */
	space_leftover:   u64,

	/*
	For READ-ONLY transaction (provided if `scan_rlt=true`): The space that
    actually become available for reuse when only this transaction will be
    finished.
    For WRITE transaction: The summarized size of the dirty database
    pages that generated during this transaction.
    */
	space_dirty:      u64,
}

/*
Latency of commit stages in 1/65536 of seconds units.

@see mdbx_txn_commit_ex()
*/
Mdbx_Commit_Latency :: struct {
	/*
	Duration of preparation (commit child transactions, update
	table's records and cursors destroying).
	*/
	preparation:   u32,

	/*
	Duration of GC update by wall clock.
	*/
	gc_wall_clock: u32,

	/*
	Duration of internal audit if enabled.
	*/
	audit:         u32,

	/*
	Duration of writing dirty/modified data pages to a filesystem,
    i.e. the summary duration of a `write()` syscalls during commit.
    */
	write:         u32,

	/*
	Duration of syncing written data to the disk/storage, i.e.
	the duration of a `fdatasync()` or a `msync()` syscall during commit.
	*/
	sync:          u32,

	/*
	Duration of transaction ending (releasing resources).
	*/
	ending:        u32,

	/*
	The total duration of a commit.
	*/
	whole:         u32,

	/*
	User-mode CPU time spent on GC update.
	*/
	gc_cpu_time:   u32,

	/*
	Information for profiling GC work. \note Statistics are common for all
	processes working with one DB file and are stored in the LCK file. Data
	is accumulated when all transactions are committed, but only in libmdbx
	builds with the \ref MDBX_ENABLE_PROFGC option set. Collected statistics
	are returned to any process when using \ref mdbx_txn_commit_ex() and are
	simultaneously reset when top-level transactions (not nested) are
	completed.
	*/
	gc_prof:       struct {
		/*
		Number of GC update iterations, greater than 1 if there
		were retries/restarts.
	 	*/
		wloops:               u32,

		/*
		Number of GC record merge iterations.
		*/
		coalescences:         u32,

		/*
		The number of previous reliable/stable commit points destroyed
		when operating in \ref MDBX_UTTERLY_NOSYNC mode.
    	*/
		wipes:                u32,

		/*
		The number of forced commits to disk to avoid database growth when
		working outside the mode
		\ref MDBX_UTTERLY_NOSYNC.
		*/
		flushes:              u32,

		/*
		Number of calls to the Handle-Slow-Readers mechanism to avoid
		database growth.
		\see MDBX_hsr_func
		*/
		kicks:                u32,

		/*
		Slow path execution count of GC for user data.
        */
		work_counter:         u32,

		/*
		The "wall clock" time spent reading and searching within the GC for
		user data.
        */
		work_rtime_monotonic: u32,

		/*
		CPU time in user mode spent preparing pages fetched from the GC for
		user data, including swapping from disk.
        */
		work_xtime_cpu:       u32,

		/*
		The number of iterations of search inside the GC when allocating
		pages for user data.
        */
		work_rsteps:          u32,

		/*
		The number of requests to allocate page sequences for user data.
        */
		work_xpages:          u32,

		/*
		The number of page faults within the GC when allocating and preparing pages
		for user data.
        */
		work_majflt:          u32,

		/*
		The slow path execution count of the GC for the purposes of maintaining
		and updating the GC itself.
		*/
		self_counter:         u32,

		/*
		The "wall clock" time spent reading and searching within the GC for the
		purposes of maintaining and updating the GC itself.
        */
		self_rtime_monotonic: u32,

		/*
		CPU time in user mode spent preparing pages fetched from the GC for the
		purpose of maintaining and updating the GC itself, including swapping
		from disk.
        */
		self_xtime_cpu:       u32,

		/*
		The number of iterations of searches inside the GC when allocating pages
		for the purposes of maintaining and updating the GC itself.
        */
		self_rsteps:          u32,

		/*
		The number of requests to allocate page sequences for the GC itself.
        */
		self_xpages:          u32,

		/*
		The number of page faults inside the GC when allocating and preparing
		pages for the GC itself.
		*/
		self_majflt:          u32,
	},
}

/*
DBI state bits reported by \ref mdbx_dbi_flags_ex()

\see mdbx_dbi_flags_ex()
*/
Mdbx_DBI_State :: enum u32 {
	/* DB was written in this txn */
	Dirty = 0x01,
	/* Cached Named-DB record is older than txnID */
	Stale = 0x02,
	/* Named-DB handle opened in this txn */
	Fresh = 0x04,
	/* Named-DB handle created in this txn */
	Creat = 0x08,
}

