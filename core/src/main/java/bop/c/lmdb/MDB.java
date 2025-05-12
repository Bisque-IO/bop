package bop.c.lmdb;

import bop.c.Loader;
import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;

/// Lightning Memory-Mapped Database Manager (LMDB)
///
/// Introduction
///
/// LMDB is a Btree-based database management library modeled loosely on the BerkeleyDB API, but
/// much simplified. The entire database is exposed in a memory map, and all data fetches return
// data
/// directly from the mapped memory, so no malloc's or memcpy's occur during data fetches. As such,
/// the library is extremely simple because it requires no page caching layer of its own, and it is
/// extremely high performance and memory-efficient. It is also fully transactional with full ACID
/// semantics, and when the memory map is read-only, the database integrity cannot be corrupted by
/// stray pointer writes from application code.
///
/// The library is fully thread-aware and supports concurrent read/write access from multiple
/// processes and threads. Data pages use a copy-on- write strategy so no active data pages are ever
/// overwritten, which also provides resistance to corruption and eliminates the need of any special
/// recovery procedures after a system crash. Writes are fully serialized; only one write
// transaction
/// may be active at a time, which guarantees that writers can never deadlock. The database
// structure
/// is multi-versioned so readers run with no locks; writers cannot block readers, and readers don't
/// block writers.
///
/// Unlike other well-known database mechanisms which use either write-ahead transaction logs or
/// append-only data writes, LMDB requires no maintenance during operation. Both write-ahead loggers
/// and append-only databases require periodic checkpointing and/or compaction of their log or
/// database files otherwise they grow without bound. LMDB tracks free pages within the database and
/// re-uses them for new write operations, so the database size does not grow without bound in
// normal
/// use.
///
/// The memory map can be used as a read-only or read-write map. It is read-only by default as
/// this provides total immunity to corruption. Using read-write mode offers much higher write
/// performance, but adds the possibility for stray application writes thru pointers to silently
/// corrupt the database. Of course if your application code is known to be bug-free (...) then this
/// is not an issue.
///
/// If this is your first time using a transactional embedded key/value store, you may find the
/// \ref starting page to be helpful.
///
/// @author Howard Chu, Symas Corporation.
/// @section caveats_sec Caveats Troubleshooting the lock file, plus semaphores on BSD systems:
///
/// - A broken lockfile can cause sync issues. Stale reader transactions left behind by an
///     aborted program cause further writes to grow the database quickly, and stale locks can block
///     further operation.
///
/// Fix: Check for stale readers periodically, using the #mdb_reader_check function or the
///     \ref mdb_stat_1 "mdb_stat" tool. Stale writers will be cleared automatically on most
// systems:
///     - Windows - automatic - BSD, systems using SysV semaphores - automatic - Linux, systems
// using
///     POSIX mutexes with Robust option - automatic Otherwise just make all programs using the
///     database close it; the lockfile is always reset on first open of the environment.
///
/// - On BSD systems or others configured with MDB_USE_SYSV_SEM or MDB_USE_POSIX_SEM, startup
///     can fail due to semaphores owned by another userid.
///
/// Fix: Open and close the database as the user which owns the semaphores (likely last user)
///     or as root, while no other process is using the database.
///
/// Restrictions/caveats (in addition to those listed for some functions):
///
/// - Only the database owner should normally use the database on BSD systems or when
///     otherwise configured with MDB_USE_POSIX_SEM. Multiple users can cause startup to fail later,
///     as noted above.
///
/// - There is normally no pure read-only mode, since readers need write access to locks and
///     lock file. Exceptions: On read-only filesystems or with the #MDB_NOLOCK flag described under
///     #mdb_env_open().
///
/// - An LMDB configuration will often reserve considerable \b unused memory address space and
///     maybe file size for future growth. This does not use actual memory or disk space, but users
///     may need to understand the difference so they won't be scared off.
///
/// - By default, in versions before 0.9.10, unused portions of the data file might receive
///     garbage data from memory freed by other code. (This does not happen when using the
///     #MDB_WRITEMAP flag.) As of 0.9.10 the default behavior is to initialize such memory before
///     writing to the data file. Since there may be a slight performance cost due to this
///     initialization, applications may disable it using the #MDB_NOMEMINIT flag. Applications
///     handling sensitive data which must not be written should not use this flag. This flag is
///     irrelevant when using #MDB_WRITEMAP.
///
/// - A thread can only use one transaction at a time, plus any child transactions. Each
///     transaction belongs to one thread. See below. The #MDB_NOTLS flag changes this for read-only
///     transactions.
///
/// - Use an MDB_env* in the process which opened it, not after fork().
///
/// - Do not have open an LMDB database twice in the same process at the same time. Not even
///     from a plain open() call - close()ing it breaks fcntl() advisory locking. (It is OK to
// reopen
///     it after fork() - exec*(), since the lockfile has FD_CLOEXEC set.)
///
/// - Avoid long-lived transactions. Read transactions prevent reuse of pages freed by newer
///     write transactions, thus the database can grow quickly. Write transactions prevent other
///     write transactions, since writes are serialized.
///
/// - Avoid suspending a process with active transactions. These would then be "long-lived" as
///     above. Also read transactions suspended when writers commit could sometimes see wrong data.
///
/// ...when several processes can use a database concurrently:
///
/// - Avoid aborting a process with an active transaction. The transaction becomes
///     "long-lived" as above until a check for stale readers is performed or the lockfile is reset,
///     since the process may not remove it from the lockfile.
///
/// This does not apply to write transactions if the system clears stale writers, see above.
///
/// - If you do that anyway, do a periodic check for stale readers. Or close the environment
///     once in a while, so the lockfile can get reset.
///
/// - Do not use LMDB databases on remote filesystems, even between processes on the same
///     host. This breaks flock() on some OSes, possibly memory map sync, and certainly sync between
///     programs on different hosts.
///
/// - Opening a database can fail if another process is opening or closing it at exactly the
///     same time.
/// @copyright Copyright 2011-2021 Howard Chu, Symas Corp. All rights reserved.
///
/// Redistribution and use in source and binary forms, with or without modification, are
///     permitted only as authorized by the OpenLDAP Public License.
///
/// A copy of this license is available in the file LICENSE in the top-level directory of the
///     distribution or, alternatively, at <http://www.OpenLDAP.org/license.html>.
/// @par Derived From: This code is derived from btree.c written by Martin Hedenfalk.
///
/// Copyright (c) 2009, 2010 Martin Hedenfalk <martin@bzero.se>
///
/// Permission to use, copy, modify, and distribute this software for any purpose with or
///     without fee is hereby granted, provided that the above copyright notice and this permission
///     notice appear in all copies.
///
/// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO
///     THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT
///     SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR
// ANY
///     DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF
///     CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE
///     OR PERFORMANCE OF THIS SOFTWARE.
public class MDB {
  static {
    Loader.load();
  }

  static void loadLMDB() throws Throwable {
    try (Arena arena = Arena.ofConfined()) {
      final var pointerRef = arena.allocate(8);
      final var result = (int) Library.MDB_ENV_CREATE.invoke(pointerRef);
      final var env = pointerRef.get(ValueLayout.JAVA_LONG, 0);
      System.out.println("MDB_env* = " + env);
    }
  }

  /////////////////////////////////////////////////////////////////////////////
  // mdb_env	Environment Flags
  /////////////////////////////////////////////////////////////////////////////

  static long invokeStrlen(String s) throws Throwable {
    try (Arena arena = Arena.ofConfined()) {
      // Allocate off-heap memory and
      // copy the argument, a Java string, into off-heap memory
      MemorySegment nativeString = arena.allocateFrom(s);

      // Link and call the C function strlen
      // Obtain an instance of the native linker
      Linker linker = Linker.nativeLinker();

      // Locate the address of the C function signature
      SymbolLookup stdLib = linker.defaultLookup();
      MemorySegment strlen_addr = stdLib.find("strlen").get();

      // Create a description of the C function
      FunctionDescriptor strlen_sig =
          FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS);

      // Create a downcall handle for the C function
      MethodHandle strlen = linker.downcallHandle(strlen_addr, strlen_sig);

      // Call the C function directly from Java
      return (long) strlen.invokeExact(nativeString);
    }
  }

  public static void main(String[] args) throws Throwable {
    System.out.println(invokeStrlen("hello"));
    loadLMDB();
  }

  public static Config config() {
    return new Config();
  }
}
