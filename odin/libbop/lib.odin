package libbop

import c "core:c/libc"
import "core:fmt"
import "core:testing"

#assert(size_of(c.int) == size_of(i32))

when ODIN_OS == .Windows && ODIN_ARCH == .amd64 {
	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/windows/x64/release/bop.lib"
	} else {
		@(private)
		LIB_PATH :: "windows/amd64/bop.lib"
	}

	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		MSVCRT_NAME :: "system:msvcrt.lib"
	} else {
		@(private)
		MSVCRT_NAME :: "system:msvcrt.lib"
	}

	foreign import lib {
//	    "windows/amd64/libcrypto_static.lib",
//	    "windows/amd64/libssl_static.lib",
//		"windows/amd64/libcrypto.a",
//		"windows/amd64/libssl.a",
		"windows/amd64/iwasm.lib",
		"windows/amd64/wolfssl.lib",
		"system:Kernel32.lib",
		"system:User32.lib",
		"system:Advapi32.lib",
		"system:ntdll.lib",
		"system:onecore.lib",
		"system:Synchronization.lib",
		"system:Dbghelp.lib",
		"system:ws2_32.lib",
		"system:bcrypt.lib",
//		"system:libcmt.lib",
//		"system:psapi.lib",
//		"system:iphlpapi.lib",
//		"system:ole32.lib",
//		"system:shell32.lib",
//		"system:uuid.lib",
//		"system:ucrt.lib",
		MSVCRT_NAME,
		LIB_PATH,
 	}
} else when ODIN_OS == .Windows && ODIN_ARCH == .arm64 {
	#panic("libbop does not support Windows ARM64 yet")
} else when ODIN_OS == .Linux && ODIN_ARCH == .amd64 {
	// odin build . -o:aggressive -define:BOP_DEBUG=1 -define:BOP_WOLFSSL=1 -extra-linker-flags:"-Wl,-rpath,$ORIGIN/libbop.so" -linker:lld
	// odin build . -o:aggressive -define:BOP_DEBUG=1 -define:BOP_WOLFSSL=1 -linker:lld -extra-linker-flags:"-rdynamic -static"
	when #config(BOP_OPENSSL, 0) == 0 {
		when #config(BOP_DEBUG, 0) == 1 {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "../../build/linux/x86_64/release/libbop.so"
			} else {
				@(private)
				LIB_PATH :: "../../build/linux/x86_64/release/libbop.a"
			}
		} else {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "linux/amd64/libbop.so"
			} else {
				@(private)
				LIB_PATH :: "linux/amd64/libbop.a"
			}
		}
		foreign import lib {
			"linux/amd64/libwolfssl.a",
			"linux/amd64/libiwasm.a",
			"system:stdc++",
			LIB_PATH,
		}
	} else {
		when #config(BOP_DEBUG, 0) == 1 {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "../../build/linux/x86_64/release/libbop-openssl.so"
			} else {
				@(private)
				LIB_PATH :: "../../build/linux/x86_64/release/libbop-openssl.a"
			}
		} else {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "linux/amd64/libbop-openssl.so"
			} else {
				@(private)
				LIB_PATH :: "linux/amd64/libbop-openssl.a"
			}
		}
		foreign import lib {
			"system:crypto",
			"system:ssl",
			"system:stdc++",
			"linux/amd64/libiwasm.a",
			LIB_PATH,
		}
	}
} else when ODIN_OS == .Linux && ODIN_ARCH == .arm64 {
	// odin build . -o:aggressive -define:BOP_DEBUG=1 -define:BOP_WOLFSSL=1 -extra-linker-flags:"-Wl,-rpath,$ORIGIN/libbop.so" -linker:lld
	// odin build . -o:aggressive -define:BOP_DEBUG=1 -define:BOP_WOLFSSL=1 -linker:lld -extra-linker-flags:"-rdynamic -static"
	when #config(BOP_OPENSSL, 0) == 0 {
		when #config(BOP_DEBUG, 1) == 1 {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "../../build/linux/arm64/release/libbop.so"
			} else {
				@(private)
				LIB_PATH :: "../../build/linux/arm64/release/libbop.a"
			}
		} else {
			@(private)
			LIB_PATH :: "linux/arm64/libbop-wolfssl.a"
		}
		foreign import lib {
			"linux/arm64/libwolfssl.a",
			"system:stdc++",
			LIB_PATH,
		}
	} else {
		when #config(BOP_DEBUG, 1) == 1 {
			when #config(BOP_SHARED, 0) == 1 {
				@(private)
				LIB_PATH :: "../../build/linux/arm64/release/libbop.so"
			} else {
				@(private)
				LIB_PATH :: "../../build/linux/arm64/release/libbop.a"
			}
		} else {
			@(private)
			LIB_PATH :: "linux/arm64/libbop.a"
		}
		foreign import lib {
			"system:crypto",
			"system:ssl",
			"system:stdc++",
			LIB_PATH,
		}
	}
} else when ODIN_OS == .Darwin && ODIN_ARCH == .amd64 {
	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/macosx/x86_64/debug/libbop.a"
	} else {
		@(private)
		LIB_PATH :: "macos/amd64/libbop.a"
	}
	//odinfmt:disable
	foreign import lib {
		LIB_PATH,
	}
	//odinfmt:enable
} else when ODIN_OS == .Darwin && ODIN_ARCH == .arm64 {
    when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/macosx/arm64/release/libbop.a"
	} else {
		@(private)
		LIB_PATH :: "macos/arm64/libbop.a"
	}
	foreign import lib {
		"macos/arm64/libwolfssl.a",
		"system:stdc++",
		"system:CoreFoundation.framework",
		"system:Security.framework",
		LIB_PATH,
	}
} else {
	#panic("libbop does not support this platform yet")
}

when !#exists(LIB_PATH) {
	#panic(
		"Could not find the compiled libbop libraries at \"" +
		LIB_PATH +
		"\", they can be compiled by running `xmake b` on the bop repo root",
	)
}

/*
*/

/*
///////////////////////////////////////////////////////////////////////////////////
//
// snmalloc
//
//
///////////////////

https://github.com/microsoft/snmalloc

snmalloc is a high-performance allocator. snmalloc can be used directly in a project as a
header-only C++ library, it can be LD_PRELOADed on Elf platforms (e.g. Linux, BSD), and
there is a crate to use it from Rust.

Its key design features are:

 - Memory that is freed by the same thread that allocated it does not require any synchronising
   operations.
 - Freeing memory in a different thread to initially allocated it, does not take any locks and
   instead uses a novel message passing scheme to return the memory to the original allocator,
   where it is recycled. This enables 1000s of remote deallocations to be performed with only a
   single atomic operation enabling great scaling with core count.
 - The allocator uses large ranges of pages to reduce the amount of meta-data required.
 - The fast paths are highly optimised with just two branches on the fast path for malloc
   (On Linux compiled with Clang).
 - The platform dependencies are abstracted away to enable porting to other platforms. snmalloc's
   design is particular well suited to the following two difficult scenarios that can be problematic
   for other allocators:

 - Allocations on one thread are freed by a different thread
 - Deallocations occur in large batches
 - Both of these can cause massive reductions in performance of other allocators, but do not for snmalloc.

The implementation of snmalloc has evolved significantly since the initial paper. The mechanism
for returning memory to remote threads has remained, but most of the meta-data layout has changed.
We recommend you read docs/security to find out about the current design, and if you want to dive
into the code docs/AddressSpace.md provides a good overview of the allocation and deallocation paths.
*/
@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	alloc :: proc(size: c.size_t) -> rawptr ---
	alloc_aligned :: proc(alignment, size: c.size_t) -> rawptr ---
	zalloc :: proc(size: c.size_t) -> rawptr ---
	zalloc_aligned :: proc(alignment, size: c.size_t) -> rawptr ---
	realloc :: proc(p: rawptr, s: c.size_t) -> rawptr ---
	dealloc :: proc(p: rawptr) ---
	dealloc_sized :: proc(p: rawptr, s: c.size_t) ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// concurrentqueue
//
//
///////////////////

https://github.com/cameron314/concurrentqueue

moodycamel::ConcurrentQueue

An industrial-strength lock-free queue for C++.

Note: If all you need is a single-producer, single-consumer queue, I have one of those too.

Features
	- Knock-your-socks-off blazing fast performance.
	- Single-header implementation. Just drop it in your project.
	- Fully thread-safe lock-free queue. Use concurrently from any number of threads.
	- C++11 implementation -- elements are moved (instead of copied) where possible.
	- Templated, obviating the need to deal exclusively with pointers -- memory is managed for you.
	- No artificial limitations on element types or maximum count.
	- Memory can be allocated once up-front, or dynamically as needed.
	- Fully portable (no assembly; all is done through standard C++11 primitives).
	- Supports super-fast bulk operations.
	- Includes a low-overhead blocking version (BlockingConcurrentQueue).
	- Exception safe.
*/
@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	mpmc_create :: proc() -> ^MPMC ---
	mpmc_destroy :: proc(m: ^MPMC) ---
	mpmc_size_approx :: proc(m: ^MPMC) -> c.size_t ---
	mpmc_enqueue :: proc(m: ^MPMC, item: rawptr) -> bool ---
	mpmc_dequeue :: proc(m: ^MPMC, item: ^rawptr) -> bool ---
	mpmc_dequeue_bulk :: proc(m: ^MPMC, items: rawptr, max_size: c.size_t) -> i64 ---

	mpmc_blocking_create :: proc() -> ^MPMC_Blocking ---
	mpmc_blocking_destroy :: proc(m: ^MPMC_Blocking) ---
	mpmc_blocking_size_approx :: proc(m: ^MPMC_Blocking) -> c.size_t ---
	mpmc_blocking_enqueue :: proc(m: ^MPMC_Blocking, item: rawptr) -> bool ---
	mpmc_blocking_dequeue :: proc(m: ^MPMC_Blocking, item: ^rawptr) -> bool ---
	mpmc_blocking_dequeue_wait :: proc(m: ^MPMC_Blocking, item: ^rawptr, timeout_micros: i64) -> bool ---
	mpmc_blocking_dequeue_bulk :: proc(m: ^MPMC_Blocking, items: rawptr, max_size: c.size_t) -> i64 ---
	mpmc_blocking_dequeue_bulk_wait :: proc(
		m: ^MPMC_Blocking,
		items: [^]rawptr,
		max_items: c.size_t,
		timeout_micros: i64,
	) -> i64 ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// readerwriterqueue (SPSC)
//
//
///////////////////

https://github.com/cameron314/readerwriterqueue

A single-producer, single-consumer lock-free queue for C++

It only supports a two-thread use case (one consuming, and one producing).
The threads can't switch roles, though you could use this queue completely
from a single thread if you wish (but that would sort of defeat the purpose!).


Features
	- Blazing fast
	- Compatible with C++11 (supports moving objects instead of making copies)
	- Fully generic (templated container of any type) -- just like std::queue, you never need
	  to allocate memory for elements yourself (which saves you the hassle of writing a lock-free
	  memory manager to hold the elements you're queueing)
	- Allocates memory up front, in contiguous blocks
	- Provides a try_enqueue method which is guaranteed never to allocate memory
	  (the queue starts with an initial capacity)
	- Also provides an enqueue method which can dynamically grow the size of the queue as needed
	- Also provides try_emplace/emplace convenience methods
	- Has a blocking version with wait_dequeue
	- Completely "wait-free" (no compare-and-swap loop). Enqueue and dequeue are always O(1)
	  (not counting memory allocation)
	- On x86, the memory barriers compile down to no-ops, meaning enqueue and dequeue are just a
	  simple series of loads and stores (and branches)
*/
@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	spsc_create :: proc() -> ^SPSC ---
	spsc_destroy :: proc(m: ^SPSC) ---
	spsc_size_approx :: proc(m: ^SPSC) -> c.size_t ---
	spsc_max_capacity :: proc(m: ^SPSC) -> c.size_t ---
	spsc_enqueue :: proc(m: ^SPSC, item: rawptr) -> bool ---
	spsc_dequeue :: proc(m: ^SPSC, item: ^rawptr) -> bool ---

	spsc_blocking_create :: proc() -> ^SPSC_Blocking ---
	spsc_blocking_destroy :: proc(m: ^SPSC_Blocking) ---
	spsc_blocking_size_approx :: proc(m: ^SPSC_Blocking) -> c.size_t ---
	spsc_blocking_max_capacity :: proc(m: ^SPSC_Blocking) -> c.size_t ---
	spsc_blocking_enqueue :: proc(m: ^SPSC_Blocking, item: rawptr) -> bool ---
	spsc_blocking_dequeue :: proc(m: ^SPSC_Blocking, item: ^rawptr) -> bool ---
	spsc_blocking_dequeue_wait :: proc(m: ^SPSC_Blocking, item: ^rawptr, timeout_micros: i64) -> bool ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// llco
// low-level coroutines
//
///////////////////

https://github.com/tidwall/llco

*/

@(default_calling_convention = "c")
foreign lib {
	@(link_name = "llco_current")
	co_current :: proc() -> ^Co ---
	@(link_name = "llco_start")
	co_start :: proc(desc: ^Co_Descriptor, final: bool) ---
	@(link_name = "llco_switch")
	co_switch :: proc(co: ^Co, final: bool) ---
	@(link_name = "llco_method")
	co_method :: proc(co: ^Co) -> cstring ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// libmdbx
// mmap transactional btree database
//
///////////////////

https://github.com/erthink/libmdbx

*/

@(default_calling_convention = "c")
foreign lib {
	/*
	Set or reset the assert() callback of the environment.

    Does nothing if libmdbx was built with MDBX_DEBUG=0 or with NDEBUG,
    and will return `MDBX_ENOSYS` in such case.

    @param [in] env   An environment handle returned by mdbx_env_create().
    @param [in] func  An MDBX_assert_func function, or 0.
    @returns A non-zero error value on failure and 0 on success. */
	mdbx_env_set_assert :: proc(env: ^MDBX_Env, func: MDBX_Assert_Func) -> i32 ---

	/*
	Return a string describing a given error code.

    This function is a superset of the ANSI C X3.159-1989 (ANSI C) `strerror()`
    function. If the error code is greater than or equal to 0, then the string
    returned by the system function `strerror()` is returned. If the error code
    is less than 0, an error string corresponding to the MDBX library error is
    returned. See errors for a list of MDBX-specific error codes.

    `mdbx_strerror()` is NOT thread-safe because may share common internal buffer
    for system messages. The returned string must NOT be modified by the
    application, but MAY be modified by a subsequent call to
    @ref mdbx_strerror(), `strerror()` and other related functions.
    @see mdbx_strerror_r()

    @param [in] errnum  The error code.

    @returns "error message" The description of the error.
    */
	mdbx_strerror :: proc(err: MDBX_Error) -> cstring ---

	mdbx_liberr2str :: proc(err: MDBX_Error) -> cstring ---

	/*
	Create an MDBX environment instance.

    This function allocates memory for a @ref MDBX_env structure. To release
    the allocated memory and discard the handle, call @ref mdbx_env_close().
    Before the handle may be used, it must be opened using @ref mdbx_env_open().

    Various other options may also need to be set before opening the handle,
    e.g. @ref mdbx_env_set_geometry(), @ref mdbx_env_set_maxreaders(),
    @ref mdbx_env_set_maxdbs(), depending on usage requirements.

    @param [out] penv  The address where the new handle will be stored.
    @returns a non-zero error value on failure and 0 on success.
    */
	mdbx_env_create :: proc(penv: ^^MDBX_Env) -> MDBX_Error ---

	/*
	Sets the value of a extra runtime options for an environment.

    @param [in] env     An environment handle returned by @ref mdbx_env_create().
    @param [in] option  The option from @ref MDBX_option_t to set value of it.
    @param [in] value   The value of option to be set.

    @see MDBX_option_t
    @see mdbx_env_get_option()
    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_set_option :: proc(env: ^MDBX_Env, option: MDBX_Option, value: u64) -> MDBX_Error ---

	/*
	Gets the value of extra runtime options from an environment.

    @param [in] env     An environment handle returned by @ref mdbx_env_create().
    @param [in] option  The option from @ref MDBX_option_t to get value of it.
    @param [out] pvalue The address where the option's value will be stored.

    @see MDBX_option_t
    @see mdbx_env_get_option()
    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_get_option :: proc(env: ^MDBX_Env, option: MDBX_Option, value: ^u64) -> MDBX_Error ---

	/*\brief Open an environment instance.

    Indifferently this function will fails or not, the @ref mdbx_env_close() must
    be called later to discard the @ref MDBX_env handle and release associated
    resources.

    @note On Windows the @ref mdbx_env_openW() is recommended to use.

    @param [in] env       An environment handle returned
                          by @ref mdbx_env_create()

    @param [in] pathname  The pathname for the database or the directory in which
                          the database files reside. In the case of directory it
                          must already exist and be writable.

    @param [in] flags     Specifies options for this environment.
                          This parameter must be bitwise OR'ing together
                          any constants described above in the @ref env_flags
                          and @ref sync_modes sections.

    Flags set by mdbx_env_set_flags() are also used:
     - @ref MDBX_ENV_DEFAULTS, @ref MDBX_NOSUBDIR, @ref MDBX_RDONLY,
       @ref MDBX_EXCLUSIVE, @ref MDBX_WRITEMAP, @ref MDBX_NOSTICKYTHREADS,
       @ref MDBX_NORDAHEAD, @ref MDBX_NOMEMINIT, @ref MDBX_COALESCE,
       @ref MDBX_LIFORECLAIM. See @ref env_flags section.

     - @ref MDBX_SYNC_DURABLE, @ref MDBX_NOMETASYNC, @ref MDBX_SAFE_NOSYNC,
       @ref MDBX_UTTERLY_NOSYNC. See @ref sync_modes section.

    @note `MDB_NOLOCK` flag don't supported by MDBX,
          try use @ref MDBX_EXCLUSIVE as a replacement.

    @note MDBX don't allow to mix processes with different @ref MDBX_SAFE_NOSYNC
          flags on the same environment.
          In such case @ref MDBX_INCOMPATIBLE will be returned.

    If the database is already exist and parameters specified early by
    @ref mdbx_env_set_geometry() are incompatible (i.e. for instance, different
    page size) then @ref mdbx_env_open() will return @ref MDBX_INCOMPATIBLE
    error.

    @param [in] mode   The UNIX permissions to set on created files.
                       Zero value means to open existing, but do not create.

    \return A non-zero error value on failure and 0 on success,
            some possible errors are:
    @retval MDBX_VERSION_MISMATCH The version of the MDBX library doesn't match
                               the version that created the database environment.
    @retval MDBX_INVALID       The environment file headers are corrupted.
    @retval MDBX_ENOENT        The directory specified by the path parameter
                               doesn't exist.
    @retval MDBX_EACCES        The user didn't have permission to access
                               the environment files.
    @retval MDBX_BUSY          The @ref MDBX_EXCLUSIVE flag was specified and the
                               environment is in use by another process,
                               or the current process tries to open environment
                               more than once.
    @retval MDBX_INCOMPATIBLE  Environment is already opened by another process,
                               but with different set of @ref MDBX_SAFE_NOSYNC,
                               @ref MDBX_UTTERLY_NOSYNC flags.
                               Or if the database is already exist and parameters
                               specified early by @ref mdbx_env_set_geometry()
                               are incompatible (i.e. different pagesize, etc).
	 *
    @retval MDBX_WANNA_RECOVERY The @ref MDBX_RDONLY flag was specified but
                                read-write access is required to rollback
                                inconsistent state after a system crash.
	 *
    @retval MDBX_TOO_LARGE      Database is too large for this process,
                                i.e. 32-bit process tries to open >4Gb database.
	*/
	mdbx_env_open :: proc(env: ^MDBX_Env, pathname: cstring, flags: MDBX_Env_Flags, mode: u16) -> MDBX_Error ---

	/*
	Delete the environment's files in a proper and multiprocess-safe way.

    @note On Windows the @ref mdbx_env_deleteW() is recommended to use.

    @param [in] pathname  The pathname for the database or the directory in which
                          the database files reside.

    @param [in] mode      Specifies deletion mode for the environment. This
                          parameter must be set to one of the constants described
                          above in the @ref MDBX_env_delete_mode_t section.

    @note The @ref MDBX_ENV_JUST_DELETE don't supported on Windows since system
    unable to delete a memory-mapped files.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_RESULT_TRUE   No corresponding files or directories were found,
                               so no deletion was performed.
    */
	mdbx_env_delete :: proc(pathname: cstring, mode: MDBX_Env_Delete_Mode) -> MDBX_Error ---

	/*
	Copy an MDBX environment to the specified path, with options.

    This function may be used to make a backup of an existing environment.
    No lockfile is created, since it gets recreated at need.
    @note This call can trigger significant file size growth if run in
    parallel with write transactions, because it employs a read-only
    transaction. See long-lived transactions under @ref restrictions section.

    @note On Windows the @ref mdbx_env_copyW() is recommended to use.
    @see mdbx_env_copy2fd()
    @see mdbx_txn_copy2pathname()

    @param [in] env    An environment handle returned by mdbx_env_create().
                       It must have already been opened successfully.
    @param [in] dest   The pathname of a file in which the copy will reside.
                       This file must not be already exist, but parent directory
                       must be writable.
    @param [in] flags  Specifies options for this operation. This parameter
                       must be bitwise OR'ing together any of the constants
                       described here:

     - @ref MDBX_CP_DEFAULTS
         Perform copy as-is without compaction, etc.

     - @ref MDBX_CP_COMPACT
         Perform compaction while copying: omit free pages and sequentially
         renumber all pages in output. This option consumes little bit more
         CPU for processing, but may running quickly than the default, on
         account skipping free pages.

     - @ref MDBX_CP_FORCE_DYNAMIC_SIZE
         Force to make resizable copy, i.e. dynamic size instead of fixed.

     - @ref MDBX_CP_DONT_FLUSH
         Don't explicitly flush the written data to an output media to reduce
         the time of the operation and the duration of the transaction.

     - @ref MDBX_CP_THROTTLE_MVCC
         Use read transaction parking during copying MVCC-snapshot
         to avoid stopping recycling and overflowing the database.
         This allows the writing transaction to oust the read
         transaction used to copy the database if copying takes so long
         that it will interfere with the recycling old MVCC snapshots
         and may lead to an overflow of the database.
         However, if the reading transaction is ousted the copy will
         be aborted until successful completion. Thus, this option
         allows copy the database without interfering with write
         transactions and a threat of database overflow, but at the cost
         that copying will be aborted to prevent such conditions.
         @see mdbx_txn_park()

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_copy :: proc(env: ^MDBX_Env, dest: cstring, flags: MDBX_Copy_Flags) -> MDBX_Error ---

	/*
	Copy an MDBX environment by given read transaction to the specified
    path, with options.

    This function may be used to make a backup of an existing environment.
    No lockfile is created, since it gets recreated at need.
    @note This call can trigger significant file size growth if run in
    parallel with write transactions, because it employs a read-only
    transaction. See long-lived transactions under @ref restrictions section.

    @note On Windows the @ref mdbx_txn_copy2pathnameW() is recommended to use.
    @see mdbx_txn_copy2fd()
    @see mdbx_env_copy()

    @param [in] txn    A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dest   The pathname of a file in which the copy will reside.
                       This file must not be already exist, but parent directory
                       must be writable.
    @param [in] flags  Specifies options for this operation. This parameter
                       must be bitwise OR'ing together any of the constants
                       described here:

     - @ref MDBX_CP_DEFAULTS
         Perform copy as-is without compaction, etc.

     - @ref MDBX_CP_COMPACT
         Perform compaction while copying: omit free pages and sequentially
         renumber all pages in output. This option consumes little bit more
         CPU for processing, but may running quickly than the default, on
         account skipping free pages.

     - @ref MDBX_CP_FORCE_DYNAMIC_SIZE
         Force to make resizable copy, i.e. dynamic size instead of fixed.

     - @ref MDBX_CP_DONT_FLUSH
         Don't explicitly flush the written data to an output media to reduce
         the time of the operation and the duration of the transaction.

     - @ref MDBX_CP_THROTTLE_MVCC
         Use read transaction parking during copying MVCC-snapshot
         to avoid stopping recycling and overflowing the database.
         This allows the writing transaction to oust the read
         transaction used to copy the database if copying takes so long
         that it will interfere with the recycling old MVCC snapshots
         and may lead to an overflow of the database.
         However, if the reading transaction is ousted the copy will
         be aborted until successful completion. Thus, this option
         allows copy the database without interfering with write
         transactions and a threat of database overflow, but at the cost
         that copying will be aborted to prevent such conditions.
         @see mdbx_txn_park()

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_txn_copy2pathname :: proc(txn: ^MDBX_Txn, dest: cstring, flags: MDBX_Copy_Flags) -> MDBX_Error ---

	/*
	Return statistics about the MDBX environment.

    At least one of `env` or `txn` argument must be non-null. If txn is passed
    non-null then stat will be filled accordingly to the given transaction.
    Otherwise, if txn is null, then stat will be populated by a snapshot from
    the last committed write transaction, and at next time, other information
    can be returned.

    Legacy mdbx_env_stat() correspond to calling @ref mdbx_env_stat_ex() with the
    null `txn` argument.

    @param [in] env     An environment handle returned by @ref mdbx_env_create()
    @param [in] txn     A transaction handle returned by @ref mdbx_txn_begin()
    @param [out] stat   The address of an @ref MDBX_stat structure where
                        the statistics will be copied
    @param [in] bytes   The size of @ref MDBX_stat.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_stat_ex :: proc(env: ^MDBX_Env, txn: ^MDBX_Txn, stat: ^MDBX_Stat, bytes: c.size_t) -> MDBX_Error ---

	/*
	Return information about the MDBX environment.

    At least one of `env` or `txn` argument must be non-null. If txn is passed
    non-null then stat will be filled accordingly to the given transaction.
    Otherwise, if txn is null, then stat will be populated by a snapshot from
    the last committed write transaction, and at next time, other information
    can be returned.

    Legacy @ref mdbx_env_info() correspond to calling @ref mdbx_env_info_ex()
    with the null `txn` argument.

    @param [in] env     An environment handle returned by @ref mdbx_env_create()
    @param [in] txn     A transaction handle returned by @ref mdbx_txn_begin()
    @param [out] info   The address of an @ref MDBX_envinfo structure
                        where the information will be copied
    @param [in] bytes   The actual size of @ref MDBX_envinfo,
                        this value is used to provide ABI compatibility.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_info_ex :: proc(env: ^MDBX_Env, txn: ^MDBX_Txn, info: ^MDBX_Env_Info, bytes: c.size_t) -> MDBX_Error ---

	/*
	Flush the environment data buffers to disk.

    Unless the environment was opened with no-sync flags (@ref MDBX_NOMETASYNC,
    @ref MDBX_SAFE_NOSYNC and @ref MDBX_UTTERLY_NOSYNC), then
    data is always written an flushed to disk when @ref mdbx_txn_commit() is
    called. Otherwise @ref mdbx_env_sync() may be called to manually write and
    flush unsynced data to disk.

    Besides, @ref mdbx_env_sync_ex() with argument `force=false` may be used to
    provide polling mode for lazy/asynchronous sync in conjunction with
    @ref mdbx_env_set_syncbytes() and/or @ref mdbx_env_set_syncperiod().

    @note This call is not valid if the environment was opened with MDBX_RDONLY.

    @param [in] env      An environment handle returned by @ref mdbx_env_create()
    @param [in] force    If non-zero, force a flush. Otherwise, If force is
                         zero, then will run in polling mode,
                         i.e. it will check the thresholds that were
                         set @ref mdbx_env_set_syncbytes()
                         and/or @ref mdbx_env_set_syncperiod() and perform flush
                         if at least one of the thresholds is reached.

    @param [in] nonblock Don't wait if write transaction
                         is running by other thread.

    @returns A non-zero error value on failure and @ref MDBX_RESULT_TRUE or 0 on
        success. The @ref MDBX_RESULT_TRUE means no data pending for flush
        to disk, and 0 otherwise. Some possible errors are:

    @retval MDBX_EACCES   The environment is read-only.
    @retval MDBX_BUSY     The environment is used by other thread
                          and `nonblock=true`.
    @retval MDBX_EINVAL   An invalid parameter was specified.
    @retval MDBX_EIO      An error occurred during the flushing/writing data
                          to a storage medium/disk.
  	*/
	mdbx_env_sync_ex :: proc(env: ^MDBX_Env, force, nonblock: bool) -> MDBX_Error ---


	/*
	Close the environment and release the memory map.

    Only a single thread may call this function. All transactions, tables,
    and cursors must already be closed before calling this function. Attempts
    to use any such handles after calling this function is UB and would cause
    a `SIGSEGV`. The environment handle will be freed and must not be used again
    after this call.

    @param [in] env        An environment handle returned by
                           @ref mdbx_env_create().

    @param [in] dont_sync  A dont'sync flag, if non-zero the last checkpoint
                           will be kept "as is" and may be still "weak" in the
                           @ref MDBX_SAFE_NOSYNC or @ref MDBX_UTTERLY_NOSYNC
                           modes. Such "weak" checkpoint will be ignored on
                           opening next time, and transactions since the last
                           non-weak checkpoint (meta-page update) will rolledback
                           for consistency guarantee.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_BUSY   The write transaction is running by other thread,
                        in such case @ref MDBX_env instance has NOT be destroyed
                        not released!
                        @note If any OTHER error code was returned then
                        given MDBX_env instance has been destroyed and released.

    @retval MDBX_EBADSIGN  Environment handle already closed or not valid,
                           i.e. @ref mdbx_env_close() was already called for the
                           `env` or was not created by @ref mdbx_env_create().

    @retval MDBX_PANIC  If @ref mdbx_env_close_ex() was called in the child
                        process after `fork()`. In this case @ref MDBX_PANIC
                        is expected, i.e. @ref MDBX_env instance was freed in
                        proper manner.

    @retval MDBX_EIO    An error occurred during the flushing/writing data
                        to a storage medium/disk.
	*/
	mdbx_env_close_ex :: proc(env: ^MDBX_Env, dont_sync: bool) -> MDBX_Error ---

	/*
	Warms up the database by loading pages into memory,
    optionally lock ones.

    Depending on the specified flags, notifies OS kernel about following access,
    force loads the database pages, including locks ones in memory or releases
    such a lock. However, the function does not analyze the b-tree nor the GC.
    Therefore an unused pages that are in GC handled (i.e. will be loaded) in
    the same way as those that contain payload.

    At least one of `env` or `txn` argument must be non-null.

    @param [in] env              An environment handle returned
                                 by @ref mdbx_env_create().
    @param [in] txn              A transaction handle returned
                                 by @ref mdbx_txn_begin().
    @param [in] flags            The @ref warmup_flags, bitwise OR'ed together.

    @param [in] timeout_seconds_16dot16  Optional timeout which checking only
                                 during explicitly peeking database pages
                                 for loading ones if the @ref MDBX_warmup_force
                                 option was specified.

    @returns A non-zero error value on failure and 0 on success.
    Some possible errors are:

    @retval MDBX_ENOSYS        The system does not support requested
    operation(s).

    @retval MDBX_RESULT_TRUE   The specified timeout is reached during load
                               data into memory.
    */
	mdbx_env_warmup :: proc(
		env: ^MDBX_Env,
		txn: ^MDBX_Txn,
		flags: MDBX_Warmup_Flags,
		timeout_seconds_16dot16: u32
	) -> MDBX_Error ---

	/*
	Set environment flags.

    This may be used to set some flags in addition to those from
    mdbx_env_open(), or to unset these flags.
    @see mdbx_env_get_flags()

    @note In contrast to LMDB, the MDBX serialize threads via mutex while
    changing the flags. Therefore this function will be blocked while a write
    transaction running by other thread, or @ref MDBX_BUSY will be returned if
    function called within a write transaction.

    @param [in] env      An environment handle returned
                         by @ref mdbx_env_create().
    @param [in] flags    The @ref env_flags to change, bitwise OR'ed together.
    @param [in] onoff    A non-zero value sets the flags, zero clears them.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_EINVAL  An invalid parameter was specified.
    */
	mdbx_env_set_flags :: proc(env: ^MDBX_Env, flags: MDBX_Env_Flags, onoff: bool) -> MDBX_Error ---

	/*
	Get environment flags.
	@see mdbx_env_set_flags()
	@param [in] env     An environment handle returned by @ref mdbx_env_create().
	@param [out] flags  The address of an integer to store the flags.

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_EINVAL An invalid parameter was specified.
	*/
	mdbx_env_get_flags :: proc(env: ^MDBX_Env, flags_out: ^MDBX_Env_Flags) -> MDBX_Error ---

	/*
	Return the path that was used in mdbx_env_open().
	\ingroup c_statinfo

	@note On Windows the @ref mdbx_env_get_pathW() is recommended to use.

	@param [in] env     An environment handle returned by @ref mdbx_env_create()
	@param [out] dest   Address of a string pointer to contain the path.
	                    This is the actual string in the environment, not a
	                    copy. It should not be altered in any way.

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_EINVAL  An invalid parameter was specified.
	*/
	mdbx_env_get_path :: proc(env: ^MDBX_Env, dest: ^cstring) -> MDBX_Error ---

	/*
	Return the file descriptor for the given environment.

	@note All MDBX file descriptors have `FD_CLOEXEC` and
	      couldn't be used after exec() and or `fork()`.

	@param [in] env   An environment handle returned by @ref mdbx_env_create().
	@param [out] fd   Address of a int to contain the descriptor.

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_EINVAL  An invalid parameter was specified.
	*/
	mdbx_env_get_fd :: proc(env: ^MDBX_Env, fd: ^FD) -> MDBX_Error ---

	/*
	Set all size-related parameters of environment, including page size
	and the min/max size of the memory map.

	In contrast to LMDB, the MDBX provide automatic size management of an
	database according the given parameters, including shrinking and resizing
	on the fly. From user point of view all of these just working. Nevertheless,
	it is reasonable to know some details in order to make optimal decisions
	when choosing parameters.

	@see mdbx_env_info_ex()

	Both @ref mdbx_env_set_geometry() and legacy @ref mdbx_env_set_mapsize() are
	inapplicable to read-only opened environment.

	Both @ref mdbx_env_set_geometry() and legacy @ref mdbx_env_set_mapsize()
	could be called either before or after @ref mdbx_env_open(), either within
	the write transaction running by current thread or not:

	 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
	   was called BEFORE @ref mdbx_env_open(), i.e. for closed environment, then
	   the specified parameters will be used for new database creation,
	   or will be applied during opening if database exists and no other process
	   using it.

	   If the database is already exist, opened with @ref MDBX_EXCLUSIVE or not
	   used by any other process, and parameters specified by
	   @ref mdbx_env_set_geometry() are incompatible (i.e. for instance,
	   different page size) then @ref mdbx_env_open() will return
	   @ref MDBX_INCOMPATIBLE error.

	   In another way, if database will opened read-only or will used by other
	   process during calling @ref mdbx_env_open() that specified parameters will
	   silently discarded (open the database with @ref MDBX_EXCLUSIVE flag
	   to avoid this).

	 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
	   was called after @ref mdbx_env_open() WITHIN the write transaction running
	   by current thread, then specified parameters will be applied as a part of
	   write transaction, i.e. will not be completely visible to any others
	   processes until the current write transaction has been committed by the
	   current process. However, if transaction will be aborted, then the
	   database file will be reverted to the previous size not immediately, but
	   when a next transaction will be committed or when the database will be
	   opened next time.

	 - In case @ref mdbx_env_set_geometry() or legacy @ref mdbx_env_set_mapsize()
	   was called after @ref mdbx_env_open() but OUTSIDE a write transaction,
	   then MDBX will execute internal pseudo-transaction to apply new parameters
	   (but only if anything has been changed), and changes be visible to any
	   others processes immediately after successful completion of function.

	Essentially a concept of "automatic size management" is simple and useful:
	 - There are the lower and upper bounds of the database file size;
	 - There is the growth step by which the database file will be increased,
	   in case of lack of space;
	 - There is the threshold for unused space, beyond which the database file
	   will be shrunk;
	 - The size of the memory map is also the maximum size of the database;
	 - MDBX will automatically manage both the size of the database and the size
	   of memory map, according to the given parameters.

	So, there some considerations about choosing these parameters:
	 - The lower bound allows you to prevent database shrinking below certain
	   reasonable size to avoid unnecessary resizing costs.
	 - The upper bound allows you to prevent database growth above certain
	   reasonable size. Besides, the upper bound defines the linear address space
	   reservation in each process that opens the database. Therefore changing
	   the upper bound is costly and may be required reopening environment in
	   case of @ref MDBX_UNABLE_EXTEND_MAPSIZE errors, and so on. Therefore, this
	   value should be chosen reasonable large, to accommodate future growth of
	   the database.
	 - The growth step must be greater than zero to allow the database to grow,
	   but also reasonable not too small, since increasing the size by little
	   steps will result a large overhead.
	 - The shrink threshold must be greater than zero to allow the database
	   to shrink but also reasonable not too small (to avoid extra overhead) and
	   not less than growth step to avoid up-and-down flouncing.
	 - The current size (i.e. `size_now` argument) is an auxiliary parameter for
	   simulation legacy @ref mdbx_env_set_mapsize() and as workaround Windows
	   issues (see below).

	Unfortunately, Windows has is a several issue
	with resizing of memory-mapped file:
	 - Windows unable shrinking a memory-mapped file (i.e memory-mapped section)
	   in any way except unmapping file entirely and then map again. Moreover,
	   it is impossible in any way when a memory-mapped file is used more than
	   one process.
	 - Windows does not provide the usual API to augment a memory-mapped file
	   (i.e. a memory-mapped partition), but only by using "Native API"
	   in an undocumented way.

	MDBX bypasses all Windows issues, but at a cost:
	 - Ability to resize database on the fly requires an additional lock
	   and release `SlimReadWriteLock` during each read-only transaction.
	 - During resize all in-process threads should be paused and then resumed.
	 - Shrinking of database file is performed only when it used by single
	   process, i.e. when a database closes by the last process or opened
	   by the first.
	 = Therefore, the size_now argument may be useful to set database size
	   by the first process which open a database, and thus avoid expensive
	   remapping further.

	For create a new database with particular parameters, including the page
	size, @ref mdbx_env_set_geometry() should be called after
	@ref mdbx_env_create() and before @ref mdbx_env_open(). Once the database is
	created, the page size cannot be changed. If you do not specify all or some
	of the parameters, the corresponding default values will be used. For
	instance, the default for database size is 10485760 bytes.

	If the mapsize is increased by another process, MDBX silently and
	transparently adopt these changes at next transaction start. However,
	@ref mdbx_txn_begin() will return @ref MDBX_UNABLE_EXTEND_MAPSIZE if new
	mapping size could not be applied for current process (for instance if
	address space is busy).  Therefore, in the case of
	@ref MDBX_UNABLE_EXTEND_MAPSIZE error you need close and reopen the
	environment to resolve error.

	@note Actual values may be different than your have specified because of
	rounding to specified database page size, the system page size and/or the
	size of the system virtual memory management unit. You can get actual values
	by @ref mdbx_env_info_ex() or see by using the tool `mdbx_chk` with the `-v`
	option.

	Legacy @ref mdbx_env_set_mapsize() correspond to calling
	@ref mdbx_env_set_geometry() with the arguments `size_lower`, `size_now`,
	`size_upper` equal to the `size` and `-1` (i.e. default) for all other
	parameters.

	@param [in] env         An environment handle returned
	                        by @ref mdbx_env_create()

	@param [in] size_lower  The lower bound of database size in bytes.
	                        Zero value means "minimal acceptable",
	                        and negative means "keep current or use default".

	@param [in] size_now    The size in bytes to setup the database size for
	                        now. Zero value means "minimal acceptable", and
	                        negative means "keep current or use default". So,
	                        it is recommended always pass -1 in this argument
	                        except some special cases.

	@param [in] size_upper The upper bound of database size in bytes.
	                       Zero value means "minimal acceptable",
	                       and negative means "keep current or use default".
	                       It is recommended to avoid change upper bound while
	                       database is used by other processes or threaded
	                       (i.e. just pass -1 in this argument except absolutely
	                       necessary). Otherwise you must be ready for
	                       @ref MDBX_UNABLE_EXTEND_MAPSIZE error(s), unexpected
	                       pauses during remapping and/or system errors like
	                       "address busy", and so on. In other words, there
	                       is no way to handle a growth of the upper bound
	                       robustly because there may be a lack of appropriate
	                       system resources (which are extremely volatile in
	                       a multi-process multi-threaded environment).

	@param [in] growth_step  The growth step in bytes, must be greater than
	                         zero to allow the database to grow. Negative value
	                         means "keep current or use default".

	@param [in] shrink_threshold  The shrink threshold in bytes, must be greater
	                              than zero to allow the database to shrink and
	                              greater than growth_step to avoid shrinking
	                              right after grow.
	                              Negative value means "keep current
	                              or use default". Default is 2*growth_step.

	@param [in] pagesize          The database page size for new database
	                              creation or -1 otherwise. Once the database
	                              is created, the page size cannot be changed.
	                              Must be power of 2 in the range between
	                              @ref MDBX_MIN_PAGESIZE and
	                              @ref MDBX_MAX_PAGESIZE. Zero value means
	                              "minimal acceptable", and negative means
	                              "keep current or use default".

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_EINVAL    An invalid parameter was specified,
	                       or the environment has an active write transaction.
	@retval MDBX_EPERM     Two specific cases for Windows:
	                       1) Shrinking was disabled before via geometry settings
	                       and now it enabled, but there are reading threads that
	                       don't use the additional `SRWL` (which is required to
	                       avoid Windows issues).
	                       2) Temporary close memory mapped is required to change
	                       geometry, but there read transaction(s) is running
	                       and no corresponding thread(s) could be suspended
	                       since the @ref MDBX_NOSTICKYTHREADS mode is used.
	@retval MDBX_EACCESS   The environment opened in read-only.
	@retval MDBX_MAP_FULL  Specified size smaller than the space already
	                       consumed by the environment.
	@retval MDBX_TOO_LARGE Specified size is too large, i.e. too many pages for
	                       given size, or a 32-bit process requests too much
	                       bytes for the 32-bit address space.
   	*/
	mdbx_env_set_geometry :: proc(
		env: ^MDBX_Env,
		size_lower: c.ssize_t,
		size_now: c.ssize_t,
		size_upper: c.ssize_t,
		growth_step: c.ssize_t,
		shrink_threshold: c.ssize_t,
		page_size: c.ssize_t,
	) -> MDBX_Error ---

	/*
	Find out whether to use readahead or not, based on the given database
	size and the amount of available memory.

	@param [in] volume      The expected database size in bytes.
	@param [in] redundancy  Additional reserve or overload in case of negative
	                        value.

	@returns A @ref MDBX_RESULT_TRUE or @ref MDBX_RESULT_FALSE value,
	         otherwise the error code.
	@retval MDBX_RESULT_TRUE   Readahead is reasonable.
	@retval MDBX_RESULT_FALSE  Readahead is NOT reasonable,
	                           i.e. @ref MDBX_NORDAHEAD is useful to
	                           open environment by @ref mdbx_env_open().
	@retval Otherwise the error code.
	*/
	mdbx_is_readahead_reasonable :: proc(volume: c.size_t, redundancy: i64) -> MDBX_Error ---

	/*
	Returns the minimal database page size in bytes for a given page size.
	*/
	mdbx_limits_dbsize_min :: proc(page_size: c.size_t) -> i64 ---

	/*
	Returns the maximal database page size in bytes for a given page size.
	*/
	mdbx_limits_dbsize_max :: proc(page_size: c.size_t) -> i64 ---

	/*
	Returns basic information about system RAM.
	This function provides a portable way to get information about available RAM
	and can be useful in that it returns the same information that libmdbx uses
	internally to adjust various options and control readahead.

	@param [out] page_size     Optional address where the system page size
	                           will be stored.
	@param [out] total_pages   Optional address where the number of total RAM
	                           pages will be stored.
	@param [out] avail_pages   Optional address where the number of
	                           available/free RAM pages will be stored.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_get_sysraminfo :: proc(page_size, total_pages, avail_pages: ^i64) -> MDBX_Error ---

	/*
	Sets application information (a context pointer) associated with
	the environment.
	@see mdbx_env_get_userctx()

	@param [in] env  An environment handle returned by @ref mdbx_env_create().
	@param [in] ctx  An arbitrary pointer for whatever the application needs.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_env_set_userctx :: proc(env: ^MDBX_Env, ctx: rawptr) -> MDBX_Error ---

	/*
	Returns an application information (a context pointer) associated
	with the environment.
	@see mdbx_env_set_userctx()

	@param [in] env An environment handle returned by @ref mdbx_env_create()
	@returns The pointer set by @ref mdbx_env_set_userctx()
	         or `NULL` if something wrong.
	*/
	mdbx_env_get_userctx :: proc(env: ^MDBX_Env) -> rawptr ---

	/*
	Create a transaction with a user provided context pointer
	for use with the environment.

	The transaction handle may be discarded using @ref mdbx_txn_abort()
	or @ref mdbx_txn_commit().
	@see mdbx_txn_begin()

	@note A transaction and its cursors must only be used by a single thread,
	and a thread may only have a single transaction at a time unless
	the @ref MDBX_NOSTICKYTHREADS is used.

	@note Cursors may not span transactions.

	@param [in] env     An environment handle returned by @ref mdbx_env_create().

	@param [in] parent  If this parameter is non-NULL, the new transaction will
	                    be a nested transaction, with the transaction indicated
	                    by parent as its parent. Transactions may be nested
	                    to any level. A parent transaction and its cursors may
	                    not issue any other operations than mdbx_txn_commit and
	                    @ref mdbx_txn_abort() while it has active child
	                    transactions.

	@param [in] flags   Special options for this transaction. This parameter
	                    must be set to 0 or by bitwise OR'ing together one
	                    or more of the values described here:
	                     - @ref MDBX_RDONLY   This transaction will not perform
	                                          any write operations.

	                     - @ref MDBX_TXN_TRY  Do not block when starting
	                                          a write transaction.

	                     - @ref MDBX_SAFE_NOSYNC, @ref MDBX_NOMETASYNC.
	                       Do not sync data to disk corresponding
	                       to @ref MDBX_NOMETASYNC or @ref MDBX_SAFE_NOSYNC
	                       description. @see sync_modes

	@param [out] txn    Address where the new @ref MDBX_txn handle
	                    will be stored.

	@param [in] context A pointer to application context to be associated with
	                    created transaction and could be retrieved by
	                    @ref mdbx_txn_get_userctx() until transaction finished.

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_PANIC         A fatal error occurred earlier and the
	                           environment must be shut down.
	@retval MDBX_UNABLE_EXTEND_MAPSIZE  Another process wrote data beyond
	                                    this MDBX_env's mapsize and this
	                                    environment map must be resized as well.
	                                    See @ref mdbx_env_set_mapsize().
	@retval MDBX_READERS_FULL  A read-only transaction was requested and
	                           the reader lock table is full.
	                           See @ref mdbx_env_set_maxreaders().
	@retval MDBX_ENOMEM        Out of memory.
	@retval MDBX_BUSY          The write transaction is already started by the
	                           current thread.
    */
	mdbx_txn_begin_ex :: proc(
		env: ^MDBX_Env,
		parent: ^MDBX_Txn,
		flags: MDBX_Txn_Flags,
		txn: ^^MDBX_Txn,
		ctx: rawptr
	) -> MDBX_Error ---

	/*\brief Sets application information associated (a context pointer) with the
	transaction.
	@see mdbx_txn_get_userctx()

	@param [in] txn  An transaction handle returned by @ref mdbx_txn_begin_ex()
	                 or @ref mdbx_txn_begin().
	@param [in] ctx  An arbitrary pointer for whatever the application needs.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_txn_set_userctx :: proc(txn: ^MDBX_Txn, ctx: rawptr) -> MDBX_Error ---

	/*
	Returns an application information (a context pointer) associated
	with the transaction.
	@see mdbx_txn_set_userctx()

	@param [in] txn  An transaction handle returned by @ref mdbx_txn_begin_ex()
	                 or @ref mdbx_txn_begin().
	@returns The pointer which was passed via the `context` parameter
	         of `mdbx_txn_begin_ex()` or set by @ref mdbx_txn_set_userctx(),
	         or `NULL` if something wrong.
	*/
	mdbx_txn_get_userctx :: proc(txn: ^MDBX_Txn) -> rawptr ---

	/*
	Return information about the MDBX transaction.

	@param [in] txn        A transaction handle returned by @ref mdbx_txn_begin()
	@param [out] info      The address of an @ref MDBX_txn_info structure
	                       where the information will be copied.
	@param [in] scan_rlt   The boolean flag controls the scan of the read lock
	                       table to provide complete information. Such scan
	                       is relatively expensive and you can avoid it
	                       if corresponding fields are not needed.
	                       See description of @ref MDBX_txn_info.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_txn_info :: proc(txn: ^MDBX_Txn, info: ^MDBX_Txn_Info, scan_rlt: bool) -> MDBX_Error ---

	/*
	Returns the transaction's MDBX_env.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin()
	*/
	mdbx_txn_env :: proc(txn: ^MDBX_Txn) -> ^MDBX_Env ---

	/*
	Return the transaction's flags.

	This returns the flags, including internal, associated with this transaction.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A transaction flags, valid if input is an valid transaction,
	         otherwise @ref MDBX_TXN_INVALID.
	*/
	mdbx_txn_flags :: proc(txn: ^MDBX_Txn) -> MDBX_Txn_Flags ---

	/*
	Return the transaction's ID.

	This returns the identifier associated with this transaction. For a
	read-only transaction, this corresponds to the snapshot being read;
	concurrent readers will frequently have the same transaction ID.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A transaction ID, valid if input is an active transaction,
	         otherwise 0.
	*/
	mdbx_txn_id :: proc(txn: ^MDBX_Txn) -> u64 ---

	/*
	Commit all the operations of a transaction into the database and
	collect latency information.
	@see mdbx_txn_commit()
	@warning This function may be changed in future releases.
	*/
	mdbx_txn_commit_ex :: proc(txn: ^MDBX_Txn, latency: ^MDBX_Commit_Latency) -> MDBX_Error ---

	/*
	Commit all the operations of a transaction into the database.

	If the current thread is not eligible to manage the transaction then
	the @ref MDBX_THREAD_MISMATCH error will returned. Otherwise the transaction
	will be committed and its handle is freed. If the transaction cannot
	be committed, it will be aborted with the corresponding error returned.

	Thus, a result other than @ref MDBX_THREAD_MISMATCH means that the
	transaction is terminated:
	 - Resources are released;
	 - Transaction handle is invalid;
	 - Cursor(s) associated with transaction must not be used, except with
	   mdbx_cursor_renew() and @ref mdbx_cursor_close().
	   Such cursor(s) must be closed explicitly by @ref mdbx_cursor_close()
	   before or after transaction commit, either can be reused with
	   @ref mdbx_cursor_renew() until it will be explicitly closed by
	   @ref mdbx_cursor_close().

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_RESULT_TRUE      Transaction was aborted since it should
	                              be aborted due to previous errors.
	@retval MDBX_PANIC            A fatal error occurred earlier
	                              and the environment must be shut down.
	@retval MDBX_BAD_TXN          Transaction is already finished or never began.
	@retval MDBX_EBADSIGN         Transaction object has invalid signature,
	                              e.g. transaction was already terminated
	                              or memory was corrupted.
	@retval MDBX_THREAD_MISMATCH  Given transaction is not owned
	                              by current thread.
	@retval MDBX_EINVAL           Transaction handle is NULL.
	@retval MDBX_ENOSPC           No more disk space.
	@retval MDBX_EIO              An error occurred during the flushing/writing
	                              data to a storage medium/disk.
	@retval MDBX_ENOMEM           Out of memory.
	*/
	mdbx_txn_commit :: proc(txn: ^MDBX_Txn) -> MDBX_Error ---

	/*
	Abandon all the operations of the transaction instead of saving them.

	The transaction handle is freed. It and its cursors must not be used again
	after this call, except with @ref mdbx_cursor_renew() and
	@ref mdbx_cursor_close().

	If the current thread is not eligible to manage the transaction then
	the @ref MDBX_THREAD_MISMATCH error will returned. Otherwise the transaction
	will be aborted and its handle is freed. Thus, a result other than
	@ref MDBX_THREAD_MISMATCH means that the transaction is terminated:
	 - Resources are released;
	 - Transaction handle is invalid;
	 - Cursor(s) associated with transaction must not be used, except with
	   @ref mdbx_cursor_renew() and @ref mdbx_cursor_close().
	   Such cursor(s) must be closed explicitly by @ref mdbx_cursor_close()
	   before or after transaction abort, either can be reused with
	   @ref mdbx_cursor_renew() until it will be explicitly closed by
	   @ref mdbx_cursor_close().

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_PANIC            A fatal error occurred earlier and
	                              the environment must be shut down.
	@retval MDBX_BAD_TXN          Transaction is already finished or never began.
	@retval MDBX_EBADSIGN         Transaction object has invalid signature,
	                              e.g. transaction was already terminated
	                              or memory was corrupted.
	@retval MDBX_THREAD_MISMATCH  Given transaction is not owned
	                              by current thread.
	@retval MDBX_EINVAL           Transaction handle is NULL.
	*/
	mdbx_txn_abort :: proc(txn: ^MDBX_Txn) -> MDBX_Error ---

	/*
	Marks transaction as broken.

	Function keeps the transaction handle and corresponding locks, but makes
	impossible to perform any operations within a broken transaction.
	Broken transaction must then be aborted explicitly later.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@see mdbx_txn_abort() @see mdbx_txn_reset() @see mdbx_txn_commit()
	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_txn_break :: proc(txn: ^MDBX_Txn) -> MDBX_Error ---

	/*
	Reset a read-only transaction.

	Abort the read-only transaction like @ref mdbx_txn_abort(), but keep the
	transaction handle. Therefore @ref mdbx_txn_renew() may reuse the handle.
	This saves allocation overhead if the process will start a new read-only
	transaction soon, and also locking overhead if @ref MDBX_NOSTICKYTHREADS is
	in use. The reader table lock is released, but the table slot stays tied to
	its thread or @ref MDBX_txn. Use @ref mdbx_txn_abort() to discard a reset
	handle, and to free its lock table slot if @ref MDBX_NOSTICKYTHREADS
	is in use.

	Cursors opened within the transaction must not be used again after this
	call, except with @ref mdbx_cursor_renew() and @ref mdbx_cursor_close().

	Reader locks generally don't interfere with writers, but they keep old
	versions of database pages allocated. Thus they prevent the old pages from
	being reused when writers commit new data, and so under heavy load the
	database size may grow much more rapidly than otherwise.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_PANIC            A fatal error occurred earlier and
	                              the environment must be shut down.
	@retval MDBX_BAD_TXN          Transaction is already finished or never began.
	@retval MDBX_EBADSIGN         Transaction object has invalid signature,
	                              e.g. transaction was already terminated
	                              or memory was corrupted.
	@retval MDBX_THREAD_MISMATCH  Given transaction is not owned
	                              by current thread.
	@retval MDBX_EINVAL           Transaction handle is NULL.
	*/
	mdbx_txn_reset :: proc(txn: ^MDBX_Txn) -> MDBX_Error ---

	/*
	Puts the reading transaction into a "parked" state.

	Running read transactions do not allow recycling of old MVCC data snapshots,
	starting with the oldest used/read version and all subsequent ones. A parked
	transaction can be evicted by a write transaction if it interferes with garbage
	recycling (old MVCC data snapshots). And if evicting does not occur, then
	recovery (transition to a working state and continued execution) of the reading
	transaction will be significantly cheaper. Thus, parking transactions allows you
	to prevent negative consequences associated with stopping garbage recycling,
	while keeping overhead costs at a minimum.

	To continue execution (reading and/or using data), the parked transaction must
	be restored using @ref mdbx_txn_unpark(). For ease of use and to prevent
	unnecessary API calls, the `autounpark` parameter provides the ability to
	automatically "unpark" when using a parked transaction in API functions that
	involve reading data.

	@warning Until the transaction is restored/unparked, regardless of the `autounpark`
	argument, pointers previously obtained when reading data within the parked
	transaction must not be allowed to be dereferenced, since the MVCC snapshot in
	which this data is located is not held and can be recycled at any time.

	A parked transaction without "unparking" can be aborted, reset, or restarted at
	any time using @ref mdbx_txn_abort(), @ref mdbx_txn_reset(), and
	@ref mdbx_txn_renew(), respectively.

	@see mdbx_txn_unpark()
	@see mdbx_txn_flags()
	@see mdbx_env_set_hsr()

	@param [in] txn          Read transaction started by
	                         @ref mdbx_txn_begin().

	@param [in] autounpark   Allows you to enable automatic
	                         unpark/restore transaction when called
	                         API functions that involve reading data.

	@returns A non-zero error code value, or 0 on success.
	*/
	mdbx_txn_park :: proc(txn: ^MDBX_Txn, autounpark: bool) -> MDBX_Error ---

	/*
	Unparks a previously parked read transaction.

	The function attempts to restore a previously parked transaction. If the
	parked transaction was ousted to recycle old MVCC snapshots, then depending
	on the `restart_if_ousted` argument, it is restarted similarly to
	@ref mdbx_txn_renew(), or the transaction is reset and the error code
	@ref MDBX_OUSTED is returned.

	@see mdbx_txn_park()
	@see mdbx_txn_flags()
	@see <a href="intro.html#long-lived-read">Long-lived read transactions</a>

	@param [in] txn     A read transaction started via
						@ref mdbx_txn_begin() and then parked
						via @ref mdbx_txn_park.

	@param [in] restart_if_ousted   Allows you to immediately restart a transaction
									if it has been taken out.

	@returns A non-zero error code value, or 0 on success. Some specific result codes:

	@retval MDBX_SUCCESS      The parked transaction was successfully restored, or it
							  was not parked.

	@retval MDBX_OUSTED       The reader transaction was preempted by the writer
							  transaction to recycle old MVCC snapshots, and the
							  `restart_if_ousted` argument was set to `false`.
							  The transaction is reset to a state similar to that
							  after calling @ref mdbx_txn_reset(), but the instance
							  (handle) is not deallocated and can be reused
							  via @ref mdbx_txn_renew(), or deallocated via
							  @ref mdbx_txn_abort().

	@retval MDBX_RESULT_TRUE  The reading transaction was ousted, but is now
							  restarted to read another (latest) MVCC snapshot
							  because restart_if_ousted` was set to `true`.

	@retval MDBX_BAD_TXN      The transaction has already been completed or was not started.
	*/
	mdbx_txn_unpark :: proc(txn: ^MDBX_Txn, restart_if_ousted: bool) -> MDBX_Error ---

	/*
	Renew a read-only transaction.

	This acquires a new reader lock for a transaction handle that had been
	released by @ref mdbx_txn_reset(). It must be called before a reset
	transaction may be used again.

	@param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().

	@returns A non-zero error value on failure and 0 on success,
	         some possible errors are:
	@retval MDBX_PANIC            A fatal error occurred earlier and
	                              the environment must be shut down.
	@retval MDBX_BAD_TXN          Transaction is already finished or never began.
	@retval MDBX_EBADSIGN         Transaction object has invalid signature,
	                              e.g. transaction was already terminated
	                              or memory was corrupted.
	@retval MDBX_THREAD_MISMATCH  Given transaction is not owned
	                              by current thread.
	@retval MDBX_EINVAL           Transaction handle is NULL.
	*/
	mdbx_txn_renew :: proc(txn: ^MDBX_Txn) -> MDBX_Error ---

	/*
	Set integers markers (aka "canary") associated with the environment.
	@see mdbx_canary_get()

	@param [in] txn     A transaction handle returned by @ref mdbx_txn_begin()
	@param [in] canary  A optional pointer to @ref MDBX_canary structure for `x`,
	             `y` and `z` values from.
	           - If canary is NOT NULL then the `x`, `y` and `z` values will be
	             updated from given canary argument, but the 'v' be always set
	             to the current transaction number if at least one `x`, `y` or
	             `z` values have changed (i.e. if `x`, `y` and `z` have the same
	             values as currently present then nothing will be changes or
	             updated).
	           - if canary is NULL then the `v` value will be explicitly update
	             to the current transaction number without changes `x`, `y` nor
	             `z`.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_canary_put :: proc(txn: ^MDBX_Txn, canary: ^MDBX_Canary) -> MDBX_Error ---

	/*
	Returns fours integers markers (aka "canary") associated with the
	environment.
	@see mdbx_canary_put()

	@param [in] txn     A transaction handle returned by @ref mdbx_txn_begin().
	@param [in] canary  The address of an @ref MDBX_canary structure where the
	                    information will be copied.

	@returns A non-zero error value on failure and 0 on success.
	*/
	mdbx_canary_get :: proc(txn: ^MDBX_Txn, canary: ^MDBX_Canary) -> MDBX_Error ---

	/*
	Open or Create a named table in the environment.

    A table handle denotes the name and parameters of a table,
    independently of whether such a table exists. The table handle may be
    discarded by calling @ref mdbx_dbi_close(). The old table handle is
    returned if the table was already open. The handle may only be closed
    once.

    @note A notable difference between MDBX and LMDB is that MDBX make handles
    opened for existing tables immediately available for other transactions,
    regardless this transaction will be aborted or reset. The REASON for this is
    to avoiding the requirement for multiple opening a same handles in
    concurrent read transactions, and tracking of such open but hidden handles
    until the completion of read transactions which opened them.

    Nevertheless, the handle for the NEWLY CREATED table will be invisible
    for other transactions until the this write transaction is successfully
    committed. If the write transaction is aborted the handle will be closed
    automatically. After a successful commit the such handle will reside in the
    shared environment, and may be used by other transactions.

    In contrast to LMDB, the MDBX allow this function to be called from multiple
    concurrent transactions or threads in the same process.

    To use named table (with name != NULL), @ref mdbx_env_set_maxdbs()
    must be called before opening the environment. Table names are
    keys in the internal unnamed table, and may be read but not written.

    @param [in] txn    transaction handle returned by @ref mdbx_txn_begin().
    @param [in] name   The name of the table to open. If only a single
                       table is needed in the environment,
                       this value may be NULL.
    @param [in] flags  Special options for this table. This parameter must
                       be bitwise OR'ing together any of the constants
                       described here:

     - @ref MDBX_DB_DEFAULTS
         Keys are arbitrary byte strings and compared from beginning to end.
     - @ref MDBX_REVERSEKEY
         Keys are arbitrary byte strings to be compared in reverse order,
         from the end of the strings to the beginning.
     - @ref MDBX_INTEGERKEY
         Keys are binary integers in native byte order, either uint32_t or
         uint64_t, and will be sorted as such. The keys must all be of the
         same size and must be aligned while passing as arguments.
     - @ref MDBX_DUPSORT
         Duplicate keys may be used in the table. Or, from another point of
         view, keys may have multiple data items, stored in sorted order. By
         default keys must be unique and may have only a single data item.
     - @ref MDBX_DUPFIXED
         This flag may only be used in combination with @ref MDBX_DUPSORT. This
         option tells the library that the data items for this table are
         all the same size, which allows further optimizations in storage and
         retrieval. When all data items are the same size, the
         @ref MDBX_GET_MULTIPLE, @ref MDBX_NEXT_MULTIPLE and
         @ref MDBX_PREV_MULTIPLE cursor operations may be used to retrieve
         multiple items at once.
     - @ref MDBX_INTEGERDUP
         This option specifies that duplicate data items are binary integers,
         similar to @ref MDBX_INTEGERKEY keys. The data values must all be of the
         same size and must be aligned while passing as arguments.
     - @ref MDBX_REVERSEDUP
         This option specifies that duplicate data items should be compared as
         strings in reverse order (the comparison is performed in the direction
         from the last byte to the first).
     - @ref MDBX_CREATE
         Create the named table if it doesn't exist. This option is not
         allowed in a read-only transaction or a read-only environment.

    @param [out] dbi     Address where the new @ref MDBX_dbi handle
                         will be stored.

    For @ref mdbx_dbi_open_ex() additional arguments allow you to set custom
    comparison functions for keys and values (for multimaps).
    @see avoid_custom_comparators

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_NOTFOUND   The specified table doesn't exist in the
                            environment and @ref MDBX_CREATE was not specified.
    @retval MDBX_DBS_FULL   Too many tables have been opened.
                            @see mdbx_env_set_maxdbs()
    @retval MDBX_INCOMPATIBLE  Table is incompatible with given flags,
                            i.e. the passed flags is different with which the
                            table was created, or the table was already
                            opened with a different comparison function(s).
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    */
	mdbx_dbi_open :: proc(txn: ^MDBX_Txn, name: cstring, flags: MDBX_DB_Flags, dbi: ^MDBX_DBI) -> MDBX_Error ---

	mdbx_dbi_open2 :: proc(txn: ^MDBX_Txn, name: ^MDBX_Val, flags: MDBX_DB_Flags, dbi: ^MDBX_DBI) -> MDBX_Error ---

	/*
	Renames a table by DBI handle

    Renames the user-named table associated with the passed DBI handle.

    @param [in,out] txn   Write transaction started by
                          @ref mdbx_txn_begin().
    @param [in]     dbi   Table descriptor
                          открытый посредством @ref mdbx_dbi_open().

    @param [in]     name  New name to rename.

    @returns A non-zero error code value, or 0 on success.
    */
	mdbx_dbi_rename :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, name: cstring) -> MDBX_Error ---
	mdbx_dbi_rename2 :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, name: ^MDBX_Val) -> MDBX_Error ---

	/*
	Lists user-defined named tables.

    Enumerates user-defined named tables, calling a user-specified visitor
    function for each named table. Enumeration continues until there are no
    more named tables, or until the user-defined function returns a nonzero
    result, which will be immediately returned as the result.

    @see MDBX_table_enum_func

    @param [in] txn     Transaction started via
                        @ref mdbx_txn_begin().
    @param [in] func    Pointer to a user-defined function with signature
    					@ref MDBX_table_enum_func,
    					which will be called for each table.
    @param [in] ctx     Pointer to some content that will be passed to the
    					`func()` function as is.

    @returns A non-zero error code value, or 0 on success.
    */
	mdbx_enumerate_tables :: proc(txn: ^MDBX_Txn, handler: MDBX_Table_Enum_Func, ctx: rawptr) -> MDBX_Error ---

	/*
	Retrieve statistics for a table.

    @param [in] txn     A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi     A table handle returned by @ref mdbx_dbi_open().
    @param [out] stat   The address of an @ref MDBX_stat structure where
                        the statistics will be copied.
    @param [in] bytes   The size of @ref MDBX_stat.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL   An invalid parameter was specified.
    */
	mdbx_dbi_stat :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, stat: ^MDBX_Stat, bytes: c.size_t) -> MDBX_Error ---

	/*
	Retrieve depth (bitmask) information of nested dupsort (multi-value)
    B+trees for given table.

    @param [in] txn     A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi     A table handle returned by @ref mdbx_dbi_open().
    @param [out] mask   The address of an uint32_t value where the bitmask
                        will be stored.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL       An invalid parameter was specified.
    @retval MDBX_RESULT_TRUE  The dbi isn't a dupsort (multi-value) table.
    */
	mdbx_dbi_dupsort_depthmask :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, mask: ^u32) -> MDBX_Error ---

	/*
	Retrieve the DB flags and status for a table handle.

    @see MDBX_db_flags_t
    @see MDBX_dbi_state_t

    @param [in] txn     A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi     A table handle returned by @ref mdbx_dbi_open().
    @param [out] flags  Address where the flags will be returned.
    @param [out] state  Address where the state will be returned.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_dbi_flags_ex :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, flags: ^u32, state: ^u32) -> MDBX_Error ---

	/*
	Close a table handle. Normally unnecessary.

    Closing a table handle is not necessary, but lets @ref mdbx_dbi_open()
    reuse the handle value. Usually it's better to set a bigger
    @ref mdbx_env_set_maxdbs(), unless that value would be large.

    @note Use with care.
    This call is synchronized via mutex with @ref mdbx_dbi_open(), but NOT with
    any transaction(s) running by other thread(s).
    So the `mdbx_dbi_close()` MUST NOT be called in-parallel/concurrently
    with any transactions using the closing dbi-handle, nor during other thread
    commit/abort a write transacton(s). The "next" version of libmdbx (\ref
    MithrilDB) will solve this issue.

    Handles should only be closed if no other threads are going to reference
    the table handle or one of its cursors any further. Do not close a handle
    if an existing transaction has modified its table. Doing so can cause
    misbehavior from table corruption to errors like @ref MDBX_BAD_DBI
    (since the DB name is gone).

    @param [in] env  An environment handle returned by @ref mdbx_env_create().
    @param [in] dbi  A table handle returned by @ref mdbx_dbi_open().

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_dbi_close :: proc(env: ^MDBX_Env, dbi: MDBX_DBI) -> MDBX_Error ---

	/*
	Empty or delete and close a table.
    \ingroup c_crud

    @see mdbx_dbi_close() @see mdbx_dbi_open()

    @param [in] txn  A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi  A table handle returned by @ref mdbx_dbi_open().
    @param [in] del  `false` to empty the DB, `true` to delete it
                     from the environment and close the DB handle.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_drop :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, del: bool) -> MDBX_Error ---

	/*
	Get items from a table.

    This function retrieves key/data pairs from the table. The address
    and length of the data associated with the specified key are returned
    in the structure to which data refers.
    If the table supports duplicate keys (@ref MDBX_DUPSORT) then the
    first data item for the key will be returned. Retrieval of other
    items requires the use of @ref mdbx_cursor_get().

    @note The memory pointed to by the returned values is owned by the
    table. The caller MUST not dispose of the memory, and MUST not modify it
    in any way regardless in a read-only nor read-write transactions!
    For case a table opened without the @ref MDBX_WRITEMAP modification
    attempts likely will cause a `SIGSEGV`. However, when a table opened with
    the @ref MDBX_WRITEMAP or in case values returned inside read-write
    transaction are located on a "dirty" (modified and pending to commit) pages,
    such modification will silently accepted and likely will lead to DB and/or
    data corruption.

    @note Values returned from the table are valid only until a
    subsequent update operation, or the end of the transaction.

    @param [in] txn       A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi       A table handle returned by @ref mdbx_dbi_open().
    @param [in] key       The key to search for in the table.
    @param [in,out] data  The data corresponding to the key.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_NOTFOUND  The key was not in the table.
    @retval MDBX_EINVAL    An invalid parameter was specified.
    */
	mdbx_get :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, key, value: ^MDBX_Val) -> MDBX_Error ---

	/*
	Get items from a table
    and optionally number of data items for a given key.

    Briefly this function does the same as @ref mdbx_get() with a few
    differences:
     1. If values_count is NOT NULL, then returns the count
        of multi-values/duplicates for a given key.
     2. Updates BOTH the key and the data for pointing to the actual key-value
        pair inside the table.

    @param [in] txn           A transaction handle returned
                              by @ref mdbx_txn_begin().
    @param [in] dbi           A table handle returned by @ref mdbx_dbi_open().
    @param [in,out] key       The key to search for in the table.
    @param [in,out] data      The data corresponding to the key.
    @param [out] values_count The optional address to return number of values
                              associated with given key:
                               = 0 - in case @ref MDBX_NOTFOUND error;
                               = 1 - exactly for tables
                                     WITHOUT @ref MDBX_DUPSORT;
                               >= 1 for tables WITH @ref MDBX_DUPSORT.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_NOTFOUND  The key was not in the table.
    @retval MDBX_EINVAL    An invalid parameter was specified.
    */
	mdbx_get_ex :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, value: ^MDBX_Val,
		values_count: ^c.size_t,
	) -> MDBX_Error ---

	/*
	Get equal or great item from a table.

    Briefly this function does the same as @ref mdbx_get() with a few
    differences:
    1. Return equal or great (due comparison function) key-value
       pair, but not only exactly matching with the key.
    2. On success return @ref MDBX_SUCCESS if key found exactly,
       and @ref MDBX_RESULT_TRUE otherwise. Moreover, for tables with
       @ref MDBX_DUPSORT flag the data argument also will be used to match over
       multi-value/duplicates, and @ref MDBX_SUCCESS will be returned only when
       BOTH the key and the data match exactly.
    3. Updates BOTH the key and the data for pointing to the actual key-value
       pair inside the table.

    @param [in] txn           A transaction handle returned
                              by @ref mdbx_txn_begin().
    @param [in] dbi           A table handle returned by @ref mdbx_dbi_open().
    @param [in,out] key       The key to search for in the table.
    @param [in,out] data      The data corresponding to the key.

    @returns A non-zero error value on failure and @ref MDBX_RESULT_FALSE
             or @ref MDBX_RESULT_TRUE on success (as described above).
             Some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_NOTFOUND      The key was not in the table.
    @retval MDBX_EINVAL        An invalid parameter was specified.
    */
	mdbx_get_equal_or_great :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, value: ^MDBX_Val,
	) -> MDBX_Error ---

	/*
	Store items into a table.

    This function stores key/data pairs in the table. The default behavior
    is to enter the new key/data pair, replacing any previously existing key
    if duplicates are disallowed, or adding a duplicate data item if
    duplicates are allowed (see @ref MDBX_DUPSORT).

    @param [in] txn        A transaction handle returned
                           by @ref mdbx_txn_begin().
    @param [in] dbi        A table handle returned by @ref mdbx_dbi_open().
    @param [in] key        The key to store in the table.
    @param [in,out] data   The data to store.
    @param [in] flags      Special options for this operation.
                           This parameter must be set to 0 or by bitwise OR'ing
                           together one or more of the values described here:
      - @ref MDBX_NODUPDATA
         Enter the new key-value pair only if it does not already appear
         in the table. This flag may only be specified if the table
         was opened with @ref MDBX_DUPSORT. The function will return
         @ref MDBX_KEYEXIST if the key/data pair already appears in the table.

     - @ref MDBX_NOOVERWRITE
         Enter the new key/data pair only if the key does not already appear
         in the table. The function will return @ref MDBX_KEYEXIST if the key
         already appears in the table, even if the table supports
         duplicates (see @ref  MDBX_DUPSORT). The data parameter will be set
         to point to the existing item.

     - @ref MDBX_CURRENT
         Update an single existing entry, but not add new ones. The function will
         return @ref MDBX_NOTFOUND if the given key not exist in the table.
         In case multi-values for the given key, with combination of
         the @ref MDBX_ALLDUPS will replace all multi-values,
         otherwise return the @ref MDBX_EMULTIVAL.

     - @ref MDBX_RESERVE
         Reserve space for data of the given size, but don't copy the given
         data. Instead, return a pointer to the reserved space, which the
         caller can fill in later - before the next update operation or the
         transaction ends. This saves an extra memcpy if the data is being
         generated later. MDBX does nothing else with this memory, the caller
         is expected to modify all of the space requested. This flag must not
         be specified if the table was opened with @ref MDBX_DUPSORT.

     - @ref MDBX_APPEND
         Append the given key/data pair to the end of the table. This option
         allows fast bulk loading when keys are already known to be in the
         correct order. Loading unsorted keys with this flag will cause
         a @ref MDBX_EKEYMISMATCH error.

     - @ref MDBX_APPENDDUP
         As above, but for sorted dup data.

     - @ref MDBX_MULTIPLE
         Store multiple contiguous data elements in a single request. This flag
         may only be specified if the table was opened with
         @ref MDBX_DUPFIXED. With combination the @ref MDBX_ALLDUPS
         will replace all multi-values.
         The data argument must be an array of two @ref MDBX_val. The `iov_len`
         of the first @ref MDBX_val must be the size of a single data element.
         The `iov_base` of the first @ref MDBX_val must point to the beginning
         of the array of contiguous data elements which must be properly aligned
         in case of table with @ref MDBX_INTEGERDUP flag.
         The `iov_len` of the second @ref MDBX_val must be the count of the
         number of data elements to store. On return this field will be set to
         the count of the number of elements actually written. The `iov_base` of
         the second @ref MDBX_val is unused.

    @see @ref c_crud_hints "Quick reference for Insert/Update/Delete operations"

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_KEYEXIST  The key/value pair already exists in the table.
    @retval MDBX_MAP_FULL  The database is full, see @ref mdbx_env_set_mapsize().
    @retval MDBX_TXN_FULL  The transaction has too many dirty pages.
    @retval MDBX_EACCES    An attempt was made to write
                           in a read-only transaction.
    @retval MDBX_EINVAL    An invalid parameter was specified.
    */
	mdbx_put :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, value: ^MDBX_Val,
		flags: MDBX_Put_Flags,
	) -> MDBX_Error ---

	/*
	Replace items in a table.

    This function allows to update or delete an existing value at the same time
    as the previous value is retrieved. If the argument new_data equal is NULL
    zero, the removal is performed, otherwise the update/insert.

    The current value may be in an already changed (aka dirty) page. In this
    case, the page will be overwritten during the update, and the old value will
    be lost. Therefore, an additional buffer must be passed via old_data
    argument initially to copy the old value. If the buffer passed in is too
    small, the function will return @ref MDBX_RESULT_TRUE by setting iov_len
    field pointed by old_data argument to the appropriate value, without
    performing any changes.

    For tables with non-unique keys (i.e. with @ref MDBX_DUPSORT flag),
    another use case is also possible, when by old_data argument selects a
    specific item from multi-value/duplicates with the same key for deletion or
    update. To select this scenario in flags should simultaneously specify
    @ref MDBX_CURRENT and @ref MDBX_NOOVERWRITE. This combination is chosen
    because it makes no sense, and thus allows you to identify the request of
    such a scenario.

    @param [in] txn           A transaction handle returned
                              by @ref mdbx_txn_begin().
    @param [in] dbi           A table handle returned by @ref mdbx_dbi_open().
    @param [in] key           The key to store in the table.
    @param [in] new_data      The data to store, if NULL then deletion will
                              be performed.
    @param [in,out] old_data  The buffer for retrieve previous value as describe
                              above.
    @param [in] flags         Special options for this operation.
                              This parameter must be set to 0 or by bitwise
                              OR'ing together one or more of the values
                              described in @ref mdbx_put() description above,
                              and additionally
                              (@ref MDBX_CURRENT | @ref MDBX_NOOVERWRITE)
                              combination for selection particular item from
                              multi-value/duplicates.

    @see @ref c_crud_hints "Quick reference for Insert/Update/Delete operations"

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_replace :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, new_value, old_value: ^MDBX_Val,
		flags: MDBX_Put_Flags,
	) -> MDBX_Error ---

	mdbx_replace_ex :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, new_value, old_value: ^MDBX_Val,
		flags: MDBX_Put_Flags,
		preserver: MDBX_Preserve_Func,
	) -> MDBX_Error ---

	/*
	Delete items from a table.

    This function removes key/data pairs from the table.

    @note The data parameter is NOT ignored regardless the table does
    support sorted duplicate data items or not. If the data parameter
    is non-NULL only the matching data item will be deleted. Otherwise, if data
    parameter is NULL, any/all value(s) for specified key will be deleted.

    This function will return @ref MDBX_NOTFOUND if the specified key/data
    pair is not in the table.

    @see @ref c_crud_hints "Quick reference for Insert/Update/Delete operations"

    @param [in] txn   A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi   A table handle returned by @ref mdbx_dbi_open().
    @param [in] key   The key to delete from the table.
    @param [in] data  The data to delete.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_EACCES   An attempt was made to write
                          in a read-only transaction.
    @retval MDBX_EINVAL   An invalid parameter was specified.
    */
	mdbx_del :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		key, value: ^MDBX_Val,
	) -> MDBX_Error ---

	/*
	Create a cursor handle but not bind it to transaction nor DBI-handle.

    A cursor cannot be used when its table handle is closed. Nor when its
    transaction has ended, except with @ref mdbx_cursor_bind() and \ref
    mdbx_cursor_renew(). Also it can be discarded with @ref mdbx_cursor_close().

    A cursor must be closed explicitly always, before or after its transaction
    ends. It can be reused with @ref mdbx_cursor_bind()
    or @ref mdbx_cursor_renew() before finally closing it.

    @note In contrast to LMDB, the MDBX required that any opened cursors can be
    reused and must be freed explicitly, regardless ones was opened in a
    read-only or write transaction. The REASON for this is eliminates ambiguity
    which helps to avoid errors such as: use-after-free, double-free, i.e.
    memory corruption and segfaults.

    @param [in] context A pointer to application context to be associated with
                        created cursor and could be retrieved by
                        @ref mdbx_cursor_get_userctx() until cursor closed.

    @returns Created cursor handle or NULL in case out of memory.
    */
	mdbx_cursor_create :: proc(ctx: rawptr) -> ^MDBX_Cursor ---

	/*
	Set application information associated with the cursor.
    \ingroup c_cursors
    @see mdbx_cursor_get_userctx()

    @param [in] cursor  An cursor handle returned by @ref mdbx_cursor_create()
                        or @ref mdbx_cursor_open().
    @param [in] ctx     An arbitrary pointer for whatever the application needs.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_cursor_set_userctx :: proc(cursor: ^MDBX_Cursor, ctx: rawptr) -> MDBX_Error ---

	/*
	Get the application information associated with the MDBX_cursor.
    @see mdbx_cursor_set_userctx()

    @param [in] cursor  An cursor handle returned by @ref mdbx_cursor_create()
                        or @ref mdbx_cursor_open().
    @returns The pointer which was passed via the `context` parameter
             of `mdbx_cursor_create()` or set by @ref mdbx_cursor_set_userctx(),
             or `NULL` if something wrong.
	*/
	mdbx_cursor_get_userctx :: proc(cursor: ^MDBX_Cursor) -> rawptr ---

	/*
	Bind cursor to specified transaction and DBI-handle.

    Using of the `mdbx_cursor_bind()` is equivalent to calling
    @ref mdbx_cursor_renew() but with specifying an arbitrary DBI-handle.

    A cursor may be associated with a new transaction, and referencing a new or
    the same table handle as it was created with. This may be done whether the
    previous transaction is live or dead.

    @note In contrast to LMDB, the MDBX required that any opened cursors can be
    reused and must be freed explicitly, regardless ones was opened in a
    read-only or write transaction. The REASON for this is eliminates ambiguity
    which helps to avoid errors such as: use-after-free, double-free, i.e.
    memory corruption and segfaults.

    @param [in] txn      A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi      A table handle returned by @ref mdbx_dbi_open().
    @param [in] cursor   A cursor handle returned by @ref mdbx_cursor_create().

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL  An invalid parameter was specified.
    */
	mdbx_cursor_bind :: proc(txn: ^MDBX_Txn, cursor: ^MDBX_Cursor, dbi: MDBX_DBI) -> MDBX_Error ---

	/*
	Unbind cursor from a transaction.

    Unbinded cursor is disassociated with any transactions but still holds
    the original DBI-handle internally. Thus it could be renewed with any running
    transaction or closed.

    @see mdbx_cursor_renew()
    @see mdbx_cursor_bind()
    @see mdbx_cursor_close()
    @see mdbx_cursor_reset()

    @note In contrast to LMDB, the MDBX required that any opened cursors can be
    reused and must be freed explicitly, regardless ones was opened in a
    read-only or write transaction. The REASON for this is eliminates ambiguity
    which helps to avoid errors such as: use-after-free, double-free, i.e.
    memory corruption and segfaults.

    @param [in] cursor   A cursor handle returned by @ref mdbx_cursor_open().

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_cursor_unbind :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Resets the cursor state.

    As a result of the reset, the cursor becomes unpositioned and does not allow
    relative positioning operations, retrieval or modification of data, until it
    is set to a position independent of the current one. This allows the application
    to prevent further operations without first positioning the cursor.

    @param [in] cursor   Pointer to cursor.

    @returns The result of the scanning operation, or an error code.
    */
	mdbx_cursor_reset :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Create a cursor handle for the specified transaction and DBI handle.

    Using of the `mdbx_cursor_open()` is equivalent to calling
    @ref mdbx_cursor_create() and then @ref mdbx_cursor_bind() functions.

    A cursor cannot be used when its table handle is closed. Nor when its
    transaction has ended, except with @ref mdbx_cursor_bind() and \ref
    mdbx_cursor_renew(). Also it can be discarded with @ref mdbx_cursor_close().

    A cursor must be closed explicitly always, before or after its transaction
    ends. It can be reused with @ref mdbx_cursor_bind()
    or @ref mdbx_cursor_renew() before finally closing it.

    @note In contrast to LMDB, the MDBX required that any opened cursors can be
    reused and must be freed explicitly, regardless ones was opened in a
    read-only or write transaction. The REASON for this is eliminates ambiguity
    which helps to avoid errors such as: use-after-free, double-free, i.e.
    memory corruption and segfaults.

    @param [in] txn      A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] dbi      A table handle returned by @ref mdbx_dbi_open().
    @param [out] cursor  Address where the new @ref MDBX_cursor handle will be
                         stored.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL  An invalid parameter was specified.
    */
	mdbx_cursor_open :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, cursor: ^^MDBX_Cursor) -> MDBX_Error ---

	/*
	Close a cursor handle.

    The cursor handle will be freed and must not be used again after this call,
    but its transaction may still be live.

    @note In contrast to LMDB, the MDBX required that any opened cursors can be
    reused and must be freed explicitly, regardless ones was opened in a
    read-only or write transaction. The REASON for this is eliminates ambiguity
    which helps to avoid errors such as: use-after-free, double-free, i.e.
    memory corruption and segfaults.

    @param [in] cursor  A cursor handle returned by @ref mdbx_cursor_open()
                        or @ref mdbx_cursor_create().
    */
	mdbx_cursor_close :: proc(cursor: ^MDBX_Cursor) ---

	/*
	Unbind or closes all cursors of a given transaction.

    Unbinds either closes all cursors associated (opened or renewed) with
    a given transaction in a bulk with minimal overhead.

    @see mdbx_cursor_unbind()
    @see mdbx_cursor_close()

    @param [in] txn      A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] unbind   If non-zero, unbinds cursors and leaves ones reusable.
                         Otherwise close and dispose cursors.

    @returns A negative error value on failure or the number of closed cursors
             on success, some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_BAD_TXN          Given transaction is invalid or has
                                  a child/nested transaction transaction.
  	*/
	mdbx_txn_release_all_cursors :: proc(txn: ^MDBX_Txn, unbind: bool) -> MDBX_Error ---

	/*
	Renew a cursor handle for use within the given transaction.

    A cursor may be associated with a new transaction whether the previous
    transaction is running or finished.

    Using of the `mdbx_cursor_renew()` is equivalent to calling
    @ref mdbx_cursor_bind() with the DBI-handle that previously
    the cursor was used with.

    @note In contrast to LMDB, the MDBX allow any cursor to be re-used by using
    @ref mdbx_cursor_renew(), to avoid unnecessary malloc/free overhead until it
    freed by @ref mdbx_cursor_close().

    @param [in] txn      A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] cursor   A cursor handle returned by @ref mdbx_cursor_open().

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL  An invalid parameter was specified.
    @retval MDBX_BAD_DBI The cursor was not bound to a DBI-handle
                         or such a handle became invalid.
	*/
	mdbx_cursor_renew :: proc(txn: ^MDBX_Txn, cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Return the cursor's transaction handle.

    @param [in] cursor A cursor handle returned by @ref mdbx_cursor_open().
    */
	mdbx_cursor_txn :: proc(cursor: ^MDBX_Cursor) -> ^MDBX_Txn ---

	/*
	Return the cursor's table handle.

    @param [in] cursor  A cursor handle returned by @ref mdbx_cursor_open().
    */
	mdbx_cursor_dbi :: proc(cursor: ^MDBX_Cursor) -> MDBX_DBI ---

	/*
	Copy cursor position and state.

    @param [in] src       A source cursor handle returned
    by @ref mdbx_cursor_create() or @ref mdbx_cursor_open().

    @param [in,out] dest  A destination cursor handle returned
    by @ref mdbx_cursor_create() or @ref mdbx_cursor_open().

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_cursor_copy :: proc(src, dest: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Compares the position of cursors.

    The function is intended to compare the positions of two initialized/installed
    cursors associated with one transaction and one table (DBI descriptor).

    If the cursors are associated with different transactions, or with different tables,
    or one of them is not initialized, then the result of the comparison is undefined
    (the behavior may change in future versions).

    @param [in] left             Left cursor to compare positions.
    @param [in] right            Right cursor to compare positions.
    @param [in] ignore_multival  A Boolean flag that affects the result only when comparing
    							 cursors for multi-value tables, i.e. with the
								 @ref MDBX_DUPSORT flag. If `true`, cursor positions are
								 compared only by keys, without taking into account
								 positioning among multi-values.
								 Otherwise, if `false`, if the positions by keys match, the
								 positions by multi-values are also compared.

    @retval A signed value in the semantics of the `<=>` operator (less than zero, zero,
    	    or greater than zero) as a result of comparing cursor positions.
    */
	mdbx_cursor_compare :: proc(left, right: ^MDBX_Cursor, ignore_multival: bool) -> i32 ---

	/*
	Retrieve by cursor.

    This function retrieves key/data pairs from the table. The address and
    length of the key are returned in the object to which key refers (except
    for the case of the @ref MDBX_SET option, in which the key object is
    unchanged), and the address and length of the data are returned in the object
    to which data refers.
    @see mdbx_get()

    @note The memory pointed to by the returned values is owned by the
    database. The caller MUST not dispose of the memory, and MUST not modify it
    in any way regardless in a read-only nor read-write transactions!
    For case a database opened without the @ref MDBX_WRITEMAP modification
    attempts likely will cause a `SIGSEGV`. However, when a database opened with
    the @ref MDBX_WRITEMAP or in case values returned inside read-write
    transaction are located on a "dirty" (modified and pending to commit) pages,
    such modification will silently accepted and likely will lead to DB and/or
    data corruption.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().
    @param [in,out] key   The key for a retrieved item.
    @param [in,out] data  The data of a retrieved item.
    @param [in] op        A cursor operation @ref MDBX_cursor_op.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_NOTFOUND  No matching key found.
    @retval MDBX_EINVAL    An invalid parameter was specified.
    */
	mdbx_cursor_get :: proc(cursor: ^MDBX_Cursor, key, value: ^MDBX_Val, op: MDBX_Cursor_Op) -> MDBX_Error ---

	/*
	A utility function for use in utilities.

    When using user-defined comparison functions (aka custom comparison functions),
    checking the order of keys may lead to incorrect results and return the error
    @ref MDBX_CORRUPTED.

    This function disables the control of the order of keys on pages when reading
    database pages for this cursor, and thus allows reading data in the absence/unavailability
    of the used comparison functions.
    @see avoid_custom_comparators

    @returns The result of the scanning operation, or an error code.
    */
	mdbx_cursor_ignord :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Scans a table using the passed predicate, reducing the associated overhead.

    Implements functionality similar to the `std::find_if<>()` template using a
    cursor and a user-defined predicate function, while saving on the associated
    overhead, including not performing some of the checks inside the record
    iteration loop and potentially reducing the number of DSO-cross-boundary calls.

    The function takes a cursor, which must be bound to some transaction and a DBI
    table handle, performs initial cursor positioning specified by the `start_op`
    argument. It then evaluates each key-value pair using the `predicate` function
    you provide, and then moves on to the next element if necessary using the
    `turn_op` operation, until one of four events occurs:
     - end of data reached;
     - an error will occur when positioning the cursor;
     - the evaluation function will return @ref MDBX_RESULT_TRUE, signaling the
       need to stop further scanning;
     - the evaluation function will return a value different from
       @ref MDBX_RESULT_FALSE and @ref MDBX_RESULT_TRUE signaling an error.

    @param [in,out] cursor   A cursor for performing a scan operation,
							 associated with an active transaction and a DBI
							 handle to the table. For example, a cursor created
							 via @ref mdbx_cursor_open().
    @param [in] predicate    Predictive function for evaluating iterable key-value
    						 pairs, see @ref MDBX_predicate_func for more details.
    @param [in,out] context  A pointer to a context with the information needed for
    						 the assessment, which is entirely prepared and controlled
    						 by you.
    @param [in] start_op     Start cursor positioning operation, for more details see
    						 @ref MDBX_cursor_op. To scan without changing the initial
    						 cursor position, use @ref MDBX_GET_CURRENT. Valid values are
    						 @ref MDBX_FIRST, @ref MDBX_FIRST_DUP, @ref MDBX_LAST,
    						 @ref MDBX_LAST_DUP, @ref MDBX_GET_CURRENT,
    						 and @ref MDBX_GET_MULTIPLE.
    @param [in] turn_op      The operation of positioning the cursor to move to the next
    						 element. Valid values are @ref MDBX_NEXT, @ref MDBX_NEXT_DUP,
    						 @ref MDBX_NEXT_NODUP, @ref MDBX_PREV, @ref MDBX_PREV_DUP,
    						 @ref MDBX_PREV_NODUP, and @ref MDBX_NEXT_MULTIPLE and
    						 @ref MDBX_PREV_MULTIPLE.
    @param [in,out] arg      An additional argument to the predicate function, which is
    						 entirely prepared and controlled by you.

    @note When using @ref MDBX_GET_MULTIPLE, @ref MDBX_NEXT_MULTIPLE or
    @ref MDBX_PREV_MULTIPLE, carefully consider the batch specifics of passing values
    through the parameters of the predictive function.

    @see MDBX_predicate_func
    @see mdbx_cursor_scan_from

    @returns The result of the scanning operation, or an error code.

    @retval MDBX_RESULT_TRUE if a key-value pair is found for which the predictive
    		function returned @ref MDBX_RESULT_TRUE.
    @retval MDBX_RESULT_FALSE if a matching key-value pair is NOT found, the search
    		has reached the end of the data, or there is no data to search.
    @retval ELSE any value other than @ref MDBX_RESULT_TRUE and @ref MDBX_RESULT_FALSE
    		is a course positioning error code, or a user-defined search stop code
			or error condition.
    */
	mdbx_cursor_scan :: proc(
		cursor: ^MDBX_Cursor,
		predicate: MDBX_Predicate_Func,
		ctx: rawptr,
		start_op, turn_op: MDBX_Cursor_Op,
		arg: rawptr,
	) -> MDBX_Error ---

	/*
	Scans a table using the passed predicate, starting with the passed key-value pair,
	reducing the associated overhead.

    The function takes a cursor, which must be bound to some transaction
	and a DBI table handle, performs initial cursor positioning
	specified by the `from_op` argument, as well as the `from_key` and
	`from_value` arguments. It then evaluates each key-value pair using the
	`predicate` predicate function you provide, and then, if necessary,
	moves on to the next element using the `turn_op` operation, until one
	of four events occurs:
     - end of data reached;
     - an error will occur when positioning the cursor;
     - the evaluation function will return @ref MDBX_RESULT_TRUE, signaling
       the need to stop further scanning;
     - the evaluation function will return a value different from
       @ref MDBX_RESULT_FALSE and @ref MDBX_RESULT_TRUE signaling an error.

    @param [in,out] cursor    A cursor for performing a scan operation,
    						  associated with an active transaction and a
    						  DBI handle to the table. For example, a cursor
    						  created via @ref mdbx_cursor_open().
    @param [in] predicate     Predictive function for evaluating iterable
    						  key-value pairs, For more details see
    						  @ref MDBX_predicate_func.
    @param [in,out] context   A pointer to a context with the information needed
    						  for the assessment, which is entirely prepared and
    						  controlled by you.
    @param [in] from_op       Operation of positioning the cursor to the initial
							  position, for more details see
                              @ref MDBX_cursor_op.
                              Acceptable values @ref MDBX_GET_BOTH,
                              @ref MDBX_GET_BOTH_RANGE, @ref MDBX_SET_KEY,
                              @ref MDBX_SET_LOWERBOUND, @ref MDBX_SET_UPPERBOUND,
                              @ref MDBX_TO_KEY_LESSER_THAN,
                              @ref MDBX_TO_KEY_LESSER_OR_EQUAL,
                              @ref MDBX_TO_KEY_EQUAL,
                              @ref MDBX_TO_KEY_GREATER_OR_EQUAL,
                              @ref MDBX_TO_KEY_GREATER_THAN,
                              @ref MDBX_TO_EXACT_KEY_VALUE_LESSER_THAN,
                              @ref MDBX_TO_EXACT_KEY_VALUE_LESSER_OR_EQUAL,
                              @ref MDBX_TO_EXACT_KEY_VALUE_EQUAL,
                              @ref MDBX_TO_EXACT_KEY_VALUE_GREATER_OR_EQUAL,
                              @ref MDBX_TO_EXACT_KEY_VALUE_GREATER_THAN,
                              @ref MDBX_TO_PAIR_LESSER_THAN,
                              @ref MDBX_TO_PAIR_LESSER_OR_EQUAL,
                              @ref MDBX_TO_PAIR_EQUAL,
                              @ref MDBX_TO_PAIR_GREATER_OR_EQUAL,
                              @ref MDBX_TO_PAIR_GREATER_THAN,
                              and also @ref MDBX_GET_MULTIPLE.
    @param [in,out] from_key  A pointer to a key used for both initial positioning
    						  and subsequent iterations of the transition.
    @param [in,out] from_value Pointer to a value used for both the initial positioning
    						   and subsequent iterations of the transition.
    @param [in] turn_op       The operation of positioning the cursor to move to the next
    						  element. Valid values
                              @ref MDBX_NEXT, @ref MDBX_NEXT_DUP,
                              @ref MDBX_NEXT_NODUP, @ref MDBX_PREV,
                              @ref MDBX_PREV_DUP, @ref MDBX_PREV_NODUP, and also
                              @ref MDBX_NEXT_MULTIPLE и @ref MDBX_PREV_MULTIPLE.
    @param [in,out] arg       An additional argument to the predicate function,
							  which is entirely prepared and controlled by you.

    @note When using @ref MDBX_GET_MULTIPLE, @ref MDBX_NEXT_MULTIPLE
		  or @ref MDBX_PREV_MULTIPLE, carefully consider the batch specifics
		  of passing values ​​through the parameters of the predictive function.

    @see MDBX_predicate_func
    @see mdbx_cursor_scan

    @returns The result of the scanning operation, or an error code.

    @retval MDBX_RESULT_TRUE if a key-value pair is found for which the predictive
    		function returned @ref MDBX_RESULT_TRUE.
    @retval MDBX_RESULT_FALSE if a matching key-value pair is NOT found, the search
    		has reached the end of the data, or there is no data to search.
    @retval ELSE any value other than @ref MDBX_RESULT_TRUE and @ref MDBX_RESULT_FALSE
    		is a course positioning error code, or a user-defined search stop code
			or error condition.
    */
	mdbx_cursor_scan_from :: proc(
		cursor: ^MDBX_Cursor,
		predicate: MDBX_Predicate_Func,
		ctx: rawptr,
		from_op: MDBX_Cursor_Op,
		from_key, from_value: ^MDBX_Val,
		turn_op: MDBX_Cursor_Op,
		arg: rawptr,
	) -> MDBX_Error ---

	/*
	Retrieve multiple non-dupsort key/value pairs by cursor.

    This function retrieves multiple key/data pairs from the table without
    @ref MDBX_DUPSORT option. For `MDBX_DUPSORT` tables please
    use @ref MDBX_GET_MULTIPLE and @ref MDBX_NEXT_MULTIPLE.

    The number of key and value items is returned in the `size_t count`
    refers. The addresses and lengths of the keys and values are returned in the
    array to which `pairs` refers.
    @see mdbx_cursor_get()

    @note The memory pointed to by the returned values is owned by the
    database. The caller MUST not dispose of the memory, and MUST not modify it
    in any way regardless in a read-only nor read-write transactions!
    For case a database opened without the @ref MDBX_WRITEMAP modification
    attempts likely will cause a `SIGSEGV`. However, when a database opened with
    the @ref MDBX_WRITEMAP or in case values returned inside read-write
    transaction are located on a "dirty" (modified and pending to commit) pages,
    such modification will silently accepted and likely will lead to DB and/or
    data corruption.

    @param [in] cursor     A cursor handle returned by @ref mdbx_cursor_open().
    @param [out] count     The number of key and value item returned, on success
                           it always be the even because the key-value
                           pairs are returned.
    @param [in,out] pairs  A pointer to the array of key value pairs.
    @param [in] limit      The size of pairs buffer as the number of items,
                           but not a pairs.
    @param [in] op         A cursor operation @ref MDBX_cursor_op (only
                           @ref MDBX_FIRST and @ref MDBX_NEXT are supported).

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_NOTFOUND         No any key-value pairs are available.
    @retval MDBX_ENODATA          The cursor is already at the end of data.
    @retval MDBX_RESULT_TRUE      The returned chunk is the last one,
                                  and there are no pairs left.
    @retval MDBX_EINVAL           An invalid parameter was specified.
    */
	mdbx_cursor_get_batch :: proc(
		cursor: ^MDBX_Cursor,
		count: ^c.size_t,
		pairs: ^MDBX_Val,
		limit: c.size_t,
		op: MDBX_Cursor_Op,
	) -> MDBX_Error ---

	/*
	Store by cursor.

    This function stores key/data pairs into the table. The cursor is
    positioned at the new item, or on failure usually near it.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().
    @param [in] key       The key operated on.
    @param [in,out] data  The data operated on.
    @param [in] flags     Options for this operation. This parameter
                          must be set to 0 or by bitwise OR'ing together
                          one or more of the values described here:
     - @ref MDBX_CURRENT
         Replace the item at the current cursor position. The key parameter
         must still be provided, and must match it, otherwise the function
         return @ref MDBX_EKEYMISMATCH. With combination the
         @ref MDBX_ALLDUPS will replace all multi-values.

         @note MDBX allows (unlike LMDB) you to change the size of the data and
         automatically handles reordering for sorted duplicates
         (see @ref MDBX_DUPSORT).

     - @ref MDBX_NODUPDATA
         Enter the new key-value pair only if it does not already appear in the
         table. This flag may only be specified if the table was opened
         with @ref MDBX_DUPSORT. The function will return @ref MDBX_KEYEXIST
         if the key/data pair already appears in the table.

     - @ref MDBX_NOOVERWRITE
         Enter the new key/data pair only if the key does not already appear
         in the table. The function will return @ref MDBX_KEYEXIST if the key
         already appears in the table, even if the table supports
         duplicates (@ref MDBX_DUPSORT).

     - @ref MDBX_RESERVE
         Reserve space for data of the given size, but don't copy the given
         data. Instead, return a pointer to the reserved space, which the
         caller can fill in later - before the next update operation or the
         transaction ends. This saves an extra memcpy if the data is being
         generated later. This flag must not be specified if the table
         was opened with @ref MDBX_DUPSORT.

     - @ref MDBX_APPEND
         Append the given key/data pair to the end of the table. No key
         comparisons are performed. This option allows fast bulk loading when
         keys are already known to be in the correct order. Loading unsorted
         keys with this flag will cause a @ref MDBX_KEYEXIST error.

     - @ref MDBX_APPENDDUP
         As above, but for sorted dup data.

     - @ref MDBX_MULTIPLE
         Store multiple contiguous data elements in a single request. This flag
         may only be specified if the table was opened with
         @ref MDBX_DUPFIXED. With combination the @ref MDBX_ALLDUPS
         will replace all multi-values.
         The data argument must be an array of two @ref MDBX_val. The `iov_len`
         of the first @ref MDBX_val must be the size of a single data element.
         The `iov_base` of the first @ref MDBX_val must point to the beginning
         of the array of contiguous data elements which must be properly aligned
         in case of table with @ref MDBX_INTEGERDUP flag.
         The `iov_len` of the second @ref MDBX_val must be the count of the
         number of data elements to store. On return this field will be set to
         the count of the number of elements actually written. The `iov_base` of
         the second @ref MDBX_val is unused.

    @see @ref c_crud_hints "Quick reference for Insert/Update/Delete operations"

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EKEYMISMATCH  The given key value is mismatched to the current
                               cursor position
    @retval MDBX_MAP_FULL      The database is full,
                                see @ref mdbx_env_set_mapsize().
    @retval MDBX_TXN_FULL      The transaction has too many dirty pages.
    @retval MDBX_EACCES        An attempt was made to write in a read-only
                               transaction.
    @retval MDBX_EINVAL        An invalid parameter was specified.
    */
	mdbx_cursor_put :: proc(
		cursor: ^MDBX_Cursor,
		key, value: ^MDBX_Val,
		flags: MDBX_Put_Flags,
	) -> MDBX_Error ---

	/*
	Delete current key/data pair.

    This function deletes the key/data pair to which the cursor refers. This
    does not invalidate the cursor, so operations such as @ref MDBX_NEXT can
    still be used on it. Both @ref MDBX_NEXT and @ref MDBX_GET_CURRENT will
    return the same record after this operation.

    @param [in] cursor  A cursor handle returned by mdbx_cursor_open().
    @param [in] flags   Options for this operation. This parameter must be set
    to one of the values described here.

     - @ref MDBX_CURRENT Delete only single entry at current cursor position.
     - @ref MDBX_ALLDUPS
       or @ref MDBX_NODUPDATA (supported for compatibility)
         Delete all of the data items for the current key. This flag has effect
         only for table(s) was created with @ref MDBX_DUPSORT.

    @see @ref c_crud_hints "Quick reference for Insert/Update/Delete operations"

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_MAP_FULL      The database is full,
                               see @ref mdbx_env_set_mapsize().
    @retval MDBX_TXN_FULL      The transaction has too many dirty pages.
    @retval MDBX_EACCES        An attempt was made to write in a read-only
                               transaction.
    @retval MDBX_EINVAL        An invalid parameter was specified.
    */
	mdbx_cursor_del :: proc(
		cursor: ^MDBX_Cursor,
		flags: MDBX_Put_Flags,
	) -> MDBX_Error ---

	/*
	Return count of duplicates for current key.

    This call is valid for all tables, but reasonable only for that support
    sorted duplicate data items @ref MDBX_DUPSORT.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().
    @param [out] pcount   Address where the count will be stored.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_THREAD_MISMATCH  Given transaction is not owned
                                  by current thread.
    @retval MDBX_EINVAL   Cursor is not initialized, or an invalid parameter
                          was specified.
	*/
	mdbx_cursor_count :: proc(cursor: ^MDBX_Cursor, pcount: ^c.size_t) -> MDBX_Error ---

	/*
	Determines whether the cursor is pointed to a key-value pair or not,
    i.e. was not positioned or points to the end of data.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().

    @returns A @ref MDBX_RESULT_TRUE or @ref MDBX_RESULT_FALSE value,
             otherwise the error code.
    @retval MDBX_RESULT_TRUE    No more data available or cursor not
                                positioned
    @retval MDBX_RESULT_FALSE   A data is available
    @retval Otherwise the error code
    */
	mdbx_cursor_eof :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Determines whether the cursor is pointed to the first key-value pair or not.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().

    @returns A MDBX_RESULT_TRUE or MDBX_RESULT_FALSE value,
             otherwise the error code.
    @retval MDBX_RESULT_TRUE   Cursor positioned to the first key-value pair
    @retval MDBX_RESULT_FALSE  Cursor NOT positioned to the first key-value
    pair @retval Otherwise the error code
    */
	mdbx_cursor_on_first :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Determines whether the cursor is on the first or only multi-value matching the key.

    @param [in] cursor    Cursor created by @ref mdbx_cursor_open().
    @returns Meaning @ref MDBX_RESULT_TRUE, or @ref MDBX_RESULT_FALSE,
             otherwise error code.
    @retval MDBX_RESULT_TRUE   the cursor is positioned on the first or only multi-value
    						   corresponding to the key.
    @retval MDBX_RESULT_FALSE  the cursor is NOT positioned on the first or only
    						   multi-value matching the key.
    @retval NACHE error code.
    */
	mdbx_cursor_on_first_dup :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Determines whether the cursor is pointed to the last key-value pair or not.

    @param [in] cursor    A cursor handle returned by @ref mdbx_cursor_open().

    @returns A @ref MDBX_RESULT_TRUE or @ref MDBX_RESULT_FALSE value,
             otherwise the error code.
    @retval MDBX_RESULT_TRUE   Cursor positioned to the last key-value pair
    @retval MDBX_RESULT_FALSE  Cursor NOT positioned to the last key-value pair
    @retval Otherwise the error code
    */
	mdbx_cursor_on_last :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	Determines whether the cursor is on the last or only multi-value matching the key.

    @param [in] cursor    Cursor created by @ref mdbx_cursor_open().
    @returns Meaning @ref MDBX_RESULT_TRUE, or @ref MDBX_RESULT_FALSE,
             otherwise error code.
    @retval MDBX_RESULT_TRUE   the cursor is positioned on the last or only multi-value
    						   corresponding to the key.
    @retval MDBX_RESULT_FALSE  the cursor is NOT positioned on the last or only
    						   multi-value matching the key.
    @retval OTHERWISE error code.
    */
	mdbx_cursor_on_last_dup :: proc(cursor: ^MDBX_Cursor) -> MDBX_Error ---

	/*
	The estimation result varies greatly depending on the filling
	of specific pages and the overall balance of the b-tree:

	1. The number of items is estimated by analyzing the height and fullness of
	the b-tree. The accuracy of the result directly depends on the balance of
	the b-tree, which in turn is determined by the history of previous
	insert/delete operations and the nature of the data (i.e. variability of
	keys length and so on). Therefore, the accuracy of the estimation can vary
	greatly in a particular situation.

	2. To understand the potential spread of results, you should consider a
	possible situations basing on the general criteria for splitting and merging
	b-tree pages:
	 - the page is split into two when there is no space for added data;
	 - two pages merge if the result fits in half a page;
	 - thus, the b-tree can consist of an arbitrary combination of pages filled
	   both completely and only 1/4. Therefore, in the worst case, the result
	   can diverge 4 times for each level of the b-tree excepting the first and
	   the last.

	3. In practice, the probability of extreme cases of the above situation is
	close to zero and in most cases the error does not exceed a few percent. On
	the other hand, it's just a chance you shouldn't overestimate.
	*/

	/*
	Estimates the distance between cursors as a number of elements.

    This function performs a rough estimate based only on b-tree pages that are
    common for the both cursor's stacks. The results of such estimation can be
    used to build and/or optimize query execution plans.

    Please see notes on accuracy of the result in the details
    of @ref c_rqest section.

    Both cursors must be initialized for the same table and the same
    transaction.

    @param [in] first            The first cursor for estimation.
    @param [in] last             The second cursor for estimation.
    @param [out] distance_items  The pointer to store estimated distance value,
                                 i.e. `*distance_items = distance(first, last)`.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_estimate_distance :: proc(first, last: ^MDBX_Cursor, distance_items: ^c.ptrdiff_t) -> MDBX_Error ---

	/*
	Estimates the move distance.

    This function performs a rough estimate distance between the current
    cursor position and next position after the specified move-operation with
    given key and data. The results of such estimation can be used to build
    and/or optimize query execution plans. Current cursor position and state are
    preserved.

    Please see notes on accuracy of the result in the details
    of @ref c_rqest section.

    @param [in] cursor            Cursor for estimation.
    @param [in,out] key           The key for a retrieved item.
    @param [in,out] data          The data of a retrieved item.
    @param [in] move_op           A cursor operation @ref MDBX_cursor_op.
    @param [out] distance_items   A pointer to store estimated move distance
                                  as the number of elements.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_estimate_move :: proc(
		cursor: ^MDBX_Cursor,
		key, value: ^MDBX_Val,
		move_op: MDBX_Cursor_Op,
		distance_items: ^c.ptrdiff_t,
	) -> MDBX_Error ---

	/*
	Estimates the size of a range as a number of elements.

    The results of such estimation can be used to build and/or optimize query
    execution plans.

    Please see notes on accuracy of the result in the details
    of @ref c_rqest section.

    @param [in] txn        A transaction handle returned
                           by @ref mdbx_txn_begin().
    @param [in] dbi        A table handle returned by  @ref mdbx_dbi_open().
    @param [in] begin_key  The key of range beginning or NULL for explicit FIRST.
    @param [in] begin_data Optional additional data to seeking among sorted
                           duplicates.
                           Only for @ref MDBX_DUPSORT, NULL otherwise.
    @param [in] end_key    The key of range ending or NULL for explicit LAST.
    @param [in] end_data   Optional additional data to seeking among sorted
                           duplicates.
                           Only for @ref MDBX_DUPSORT, NULL otherwise.
    @param [out] distance_items  A pointer to store range estimation result.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_estimate_range :: proc(
		txn: ^MDBX_Txn,
		dbi: MDBX_DBI,
		begin_key, begin_value: ^MDBX_Val,
		end_key, end_value: ^MDBX_Val,
		distance_items: ^c.ptrdiff_t,
	) -> MDBX_Error ---

	/*
	Determines whether the given address is on a dirty database page of
    the transaction or not.

    Ultimately, this allows to avoid copy data from non-dirty pages.

    "Dirty" pages are those that have already been changed during a write
    transaction. Accordingly, any further changes may result in such pages being
    overwritten. Therefore, all functions libmdbx performing changes inside the
    database as arguments should NOT get pointers to data in those pages. In
    turn, "not dirty" pages before modification will be copied.

    In other words, data from dirty pages must either be copied before being
    passed as arguments for further processing or rejected at the argument
    validation stage. Thus, `mdbx_is_dirty()` allows you to get rid of
    unnecessary copying, and perform a more complete check of the arguments.

    @note The address passed must point to the beginning of the data. This is
    the only way to ensure that the actual page header is physically located in
    the same memory page, including for multi-pages with long data.

    @note In rare cases the function may return a false positive answer
    (@ref MDBX_RESULT_TRUE when data is NOT on a dirty page), but never a false
    negative if the arguments are correct.

    @param [in] txn      A transaction handle returned by @ref mdbx_txn_begin().
    @param [in] ptr      The address of data to check.

    @returns A MDBX_RESULT_TRUE or MDBX_RESULT_FALSE value,
             otherwise the error code.
    @retval MDBX_RESULT_TRUE    Given address is on the dirty page.
    @retval MDBX_RESULT_FALSE   Given address is NOT on the dirty page.
    @retval Otherwise the error code.
    */
	mdbx_is_dirty :: proc(txn: ^MDBX_Txn, ptr: rawptr) -> MDBX_Error ---

	/*
	Sequence generation for a table.

    The function allows to create a linear sequence of unique positive integers
    for each table. The function can be called for a read transaction to
    retrieve the current sequence value, and the increment must be zero.
    Sequence changes become visible outside the current write transaction after
    it is committed, and discarded on abort.

    @param [in] txn        A transaction handle returned
                           by @ref mdbx_txn_begin().
    @param [in] dbi        A table handle returned by @ref mdbx_dbi_open().
    @param [out] result    The optional address where the value of sequence
                           before the change will be stored.
    @param [in] increment  Value to increase the sequence,
                           must be 0 for read-only transactions.

    @returns A non-zero error value on failure and 0 on success,
             some possible errors are:
    @retval MDBX_RESULT_TRUE   Increasing the sequence has resulted in an
                               overflow and therefore cannot be executed.
    */
	mdbx_dbi_sequence :: proc(txn: ^MDBX_Txn, dbi: MDBX_DBI, result: ^u64, increment: u64) -> MDBX_Error ---

	/*
	Enumerate the entries in the reader lock table.

    \ingroup c_statinfo

    @param [in] env     An environment handle returned by @ref mdbx_env_create().
    @param [in] func    A @ref MDBX_reader_list_func function.
    @param [in] ctx     An arbitrary context pointer for the enumeration
                        function.

    @returns A non-zero error value on failure and 0 on success,
    or @ref MDBX_RESULT_TRUE if the reader lock table is empty.
    */
	mdbx_reader_list :: proc(env: ^MDBX_Env, handler: MDBX_Reader_List_Func, ctx: rawptr) -> MDBX_Error ---

	/*
	Check for stale entries in the reader lock table.

    @param [in] env     An environment handle returned by @ref mdbx_env_create().
    @param [out] dead   Number of stale slots that were cleared.

    @returns A non-zero error value on failure and 0 on success,
    or @ref MDBX_RESULT_TRUE if a dead reader(s) found or mutex was recovered.
    */
	mdbx_reader_check :: proc(env: ^MDBX_Env, dead: ^c.int) -> MDBX_Error ---

	/*
	Returns a lag of the reading for the given transaction.

    Returns an information for estimate how much given read-only
    transaction is lagging relative the to actual head.
    \deprecated Please use @ref mdbx_txn_info() instead.

    @param [in] txn       A transaction handle returned by @ref mdbx_txn_begin().
    @param [out] percent  Percentage of page allocation in the database.

    @returns Number of transactions committed after the given was started for
             read, or negative value on failure.
    */
	mdbx_txn_straggler :: proc(txn: ^MDBX_Txn, percent: ^c.int) -> c.int ---

	/*
	Registers the current thread as a reader for the environment.

    To perform read operations without blocking, a reader slot must be assigned
    for each thread. However, this assignment requires a short-term lock
    acquisition which is performed automatically. This function allows you to
    assign the reader slot in advance and thus avoid capturing the blocker when
    the read transaction starts firstly from current thread.
    @see mdbx_thread_unregister()

    @note Threads are registered automatically the first time a read transaction
          starts. Therefore, there is no need to use this function, except in
          special cases.

    @param [in] env   An environment handle returned by @ref mdbx_env_create().

    @returns A non-zero error value on failure and 0 on success,
    or @ref MDBX_RESULT_TRUE if thread is already registered.
    */
	mdbx_thread_register :: proc(env: ^MDBX_Env) -> MDBX_Error ---

	/*
	Unregisters the current thread as a reader for the environment.

    To perform read operations without blocking, a reader slot must be assigned
    for each thread. However, the assigned reader slot will remain occupied until
    the thread ends or the environment closes. This function allows you to
    explicitly release the assigned reader slot.
    @see mdbx_thread_register()

    @param [in] env   An environment handle returned by @ref mdbx_env_create().

    @returns A non-zero error value on failure and 0 on success, or
    @ref MDBX_RESULT_TRUE if thread is not registered or already unregistered.
    */
	mdbx_thread_unregister :: proc(env: ^MDBX_Env) -> MDBX_Error ---

	/*
	Sets a Handle-Slow-Readers callback to resolve database full/overflow
    issue due to a reader(s) which prevents the old data from being recycled.

    The callback will only be triggered when the database is full due to a
    reader(s) prevents the old data from being recycled.

    @see MDBX_hsr_func
    @see mdbx_env_get_hsr()
    @see mdbx_txn_park()
    @see <a href="intro.html#long-lived-read">Long-lived read transactions</a>

    @param [in] env             An environment handle returned
                                by @ref mdbx_env_create().
    @param [in] hsr_callback    A @ref MDBX_hsr_func function
                                or NULL to disable.

    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_env_set_hsr :: proc(env: ^MDBX_Env, hsr_callback: MDBX_Hsr_Func) -> MDBX_Error ---

	/*
	Gets current Handle-Slow-Readers callback used to resolve database
    full/overflow issue due to a reader(s) which prevents the old data from being
    recycled.

    @see MDBX_hsr_func
    @see mdbx_env_set_hsr()
    @see mdbx_txn_park()
    @see <a href="intro.html#long-lived-read">Long-lived read transactions</a>

    @param [in] env   An environment handle returned by @ref mdbx_env_create().

    @returns A MDBX_hsr_func function or NULL if disabled
             or something wrong.
    */
	mdbx_env_get_hsr :: proc(env: ^MDBX_Env) -> MDBX_Hsr_Func ---

	/*
	Acquires write-transaction lock.
    Provided for custom and/or complex locking scenarios.
    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_txn_lock :: proc(env: ^MDBX_Env, dont_wait: bool) -> MDBX_Error ---

	/*
	Releases write-transaction lock.
    Provided for custom and/or complex locking scenarios.
    @returns A non-zero error value on failure and 0 on success.
    */
	mdbx_txn_unlock :: proc(env: ^MDBX_Env) -> MDBX_Error ---

	/*
	Open an environment instance using specific meta-page
	for checking and recovery.

	This function mostly of internal API for `mdbx_chk` utility and subject to
	change at any time. Do not use this function to avoid shooting your own
	leg(s).

	@note On Windows the @ref mdbx_env_open_for_recoveryW() is recommended
	to use.
	*/
	mdbx_env_open_for_recovery :: proc(
		env: ^MDBX_Env,
		pathname: cstring,
		target_meta: c.uint,
		writeable: bool,
	) -> MDBX_Error ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// raft (wraps C++ NuRaft by eBay)
//
///////////////////

https://github.com/eBay/NuRaft

Features

 - Core Raft algorithm
 - Log replication & compaction
 - Leader election
 - Snapshot
 - Dynamic membership & configuration change
 - Group commit & pipelined write
 - User-defined log store & state machine support
 - New features added in this project
 - Pre-vote protocol
 - Leadership expiration
 - Priority-based semi-deterministic leader election
 - Read-only member (learner)
 - Object-based logical snapshot
 - Custom/separate quorum size for commit & leader election
 - Asynchronous replication
 - SSL/TLS support
 - Parallel Log Appending
 - Custom Commit Policy
 - Streaming Mode

NuRaft makes extensive use of C++ paradigms and APIs including std::shared_ptr

Hot paths avoid
*/


@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	raft_buffer_new :: proc(size: c.size_t) -> ^Raft_Buffer ---
	raft_buffer_free :: proc(buf: ^Raft_Buffer) ---
	raft_buffer_container_size :: proc(buf: ^Raft_Buffer) -> c.size_t ---
	raft_buffer_data :: proc(buf: ^Raft_Buffer) -> [^]byte ---
	raft_buffer_size :: proc(buf: ^Raft_Buffer) -> c.size_t ---
	raft_buffer_pos :: proc(buf: ^Raft_Buffer) -> c.size_t ---
	raft_buffer_set_pos :: proc(buf: ^Raft_Buffer, pos: c.size_t) -> c.size_t ---

	raft_async_u64_make :: proc(user_data: rawptr, when_ready: Raft_Async_U64_Done) -> ^Raft_Async_U64_Ptr ---
	raft_async_u64_delete :: proc(ptr: ^Raft_Async_U64_Ptr) ---
	raft_async_u64_get_user_data :: proc(ptr: ^Raft_Async_U64_Ptr) -> rawptr ---
	raft_async_u64_set_user_data :: proc(ptr: ^Raft_Async_U64_Ptr, user_data: rawptr) ---
	raft_async_u64_get_when_ready :: proc(ptr: ^Raft_Async_U64_Ptr) -> Raft_Async_U64_Done ---
	raft_async_u64_set_when_ready :: proc(ptr: ^Raft_Async_U64_Ptr, when_ready: Raft_Async_U64_Done) ---

	raft_async_buffer_make :: proc(user_data: rawptr, when_ready: Raft_Async_Buffer_Done) -> ^Raft_Async_Buffer_Ptr ---
	raft_async_buffer_delete :: proc(ptr: ^Raft_Async_Buffer_Ptr) ---
	raft_async_buffer_get_user_data :: proc(ptr: ^Raft_Async_Buffer_Ptr) -> rawptr ---
	raft_async_buffer_set_user_data :: proc(ptr: ^Raft_Async_Buffer_Ptr, user_data: rawptr) ---
	raft_async_buffer_get_when_ready :: proc(ptr: ^Raft_Async_Buffer_Ptr) -> Raft_Async_Buffer_Done ---
	raft_async_buffer_set_when_ready :: proc(ptr: ^Raft_Async_Buffer_Ptr, when_ready: Raft_Async_Buffer_Done) ---

	raft_snapshot_serialize :: proc(ptr: ^Raft_Snapshot) -> ^Raft_Buffer ---
	raft_snapshot_deserialize :: proc(ptr: ^Raft_Buffer) -> ^Raft_Snapshot ---

	raft_cluster_config_new :: proc() -> ^Raft_Cluster_Config ---
	raft_cluster_config_free :: proc(config: ^Raft_Cluster_Config) ---
	raft_cluster_config_ptr_create :: proc(config: ^Raft_Cluster_Config) -> ^Raft_Cluster_Config_Ptr ---
	raft_cluster_config_ptr_delete :: proc(cfg_ptr: ^Raft_Cluster_Config_Ptr) ---
	raft_cluster_config_serialize :: proc(cfg: ^Raft_Cluster_Config) -> ^Raft_Buffer ---
	raft_cluster_config_deserialize :: proc(cfg: ^Raft_Buffer) -> ^Raft_Cluster_Config ---
	raft_cluster_config_log_idx :: proc(cfg: ^Raft_Cluster_Config) -> u64 ---
	raft_cluster_config_prev_log_idx :: proc(cfg: ^Raft_Cluster_Config) -> u64 ---
	raft_cluster_config_is_async_replication :: proc(cfg: ^Raft_Cluster_Config) -> bool ---
	raft_cluster_config_user_ctx :: proc(cfg: ^Raft_Cluster_Config, out_data: ^byte, out_data_size: c.size_t) -> c.size_t ---
	raft_cluster_config_user_ctx_size :: proc(cfg: ^Raft_Cluster_Config) -> c.size_t ---
	raft_cluster_config_servers_size :: proc(cfg: ^Raft_Cluster_Config) -> c.size_t ---
	raft_cluster_config_server :: proc(cfg: ^Raft_Cluster_Config, idx: i32) -> ^Raft_Srv_Config ---

	raft_srv_config_vec_create :: proc() -> ^Raft_Srv_Config_Vec ---
	raft_srv_config_vec_delete :: proc(vec: ^Raft_Srv_Config_Vec) ---
	raft_srv_config_vec_size :: proc(vec: ^Raft_Srv_Config_Vec) -> c.size_t ---
	raft_srv_config_vec_get :: proc(vec: ^Raft_Srv_Config_Vec, idx: c.size_t) -> ^Raft_Srv_Config ---

	raft_srv_config_ptr_make :: proc(config: ^Raft_Srv_Config) -> ^Raft_Srv_Config_Ptr ---
	raft_srv_config_ptr_delete :: proc(config_ptr: ^Raft_Srv_Config_Ptr) ---
	raft_srv_config_make :: proc(
		id: i32,
		dc_id: i32,
		endpoint: [^]byte,
		endpoint_size: c.size_t,
		aux: [^]byte,
		aux_size: c.size_t,
		learner: bool,
		priority: i32,
	) -> ^Raft_Srv_Config ---
	raft_srv_config_delete :: proc(config: ^Raft_Srv_Config) ---
	raft_srv_config_id :: proc(config: ^Raft_Srv_Config) -> i32 ---
	raft_srv_config_dc_id :: proc(config: ^Raft_Srv_Config) -> i32 ---
	raft_srv_config_endpoint :: proc(config: ^Raft_Srv_Config) -> cstring ---
	raft_srv_config_endpoint_size :: proc(config: ^Raft_Srv_Config) -> c.size_t ---
	raft_srv_config_aux :: proc(config: ^Raft_Srv_Config) -> cstring ---
	raft_srv_config_aux_size :: proc(config: ^Raft_Srv_Config) -> c.size_t ---

	// `true` if this node is learner.
	// Learner will not initiate or participate in leader election.
	raft_srv_config_is_learner :: proc(config: ^Raft_Srv_Config) -> bool ---

	// `true` if this node is a new joiner, but not yet fully synced.
	// New joiner will not initiate or participate in leader election.
	raft_srv_config_is_new_joiner :: proc(config: ^Raft_Srv_Config) -> bool ---

	// Priority of this node.
	// 0 will never be a leader.
	raft_srv_config_priority :: proc(config: ^Raft_Srv_Config) -> i32 ---

	raft_srv_state_serialize :: proc(buf: ^Raft_Srv_State) -> ^Raft_Buffer ---
	raft_srv_state_deserialize :: proc(buf: ^Raft_Buffer) -> ^Raft_Srv_State ---
	raft_srv_state_delete :: proc(buf: ^Raft_Srv_State) ---

	// Term
	raft_srv_state_term :: proc(buf: ^Raft_Srv_State) -> u64 ---

	// Server ID that this server voted for.
	// `-1` if not voted.
	raft_srv_state_voted_for :: proc(buf: ^Raft_Srv_State) -> i32 ---

	// `true` if election timer is allowed.
	raft_srv_state_is_election_timer_allowed :: proc(buf: ^Raft_Srv_State) -> bool ---

	// true if this server has joined the cluster but has not yet
	// fully caught up with the latest log. While in the catch-up status,
	// this server will not receive normal append_entries requests.
	raft_srv_state_is_catching_up :: proc(buf: ^Raft_Srv_State) -> bool ---

	// `true` if this server is receiving a snapshot.
	// Same as `catching_up_`, it must be a durable flag so as not to be
	// reset after restart. While this flag is set, this server will neither
	// receive normal append_entries requests nor initiate election.
	raft_srv_state_is_receiving_snapshot :: proc(buf: ^Raft_Srv_State) -> bool ---

	raft_logger_make :: proc(user_data: rawptr, write_func: Raft_Logger_Write_Func) -> ^Raft_Logger_Ptr ---
	raft_logger_delete :: proc(logger: ^Raft_Logger_Ptr) ---

	raft_delayed_task_make :: proc(user_data: rawptr, type: i32, callback: proc "c" (user_data: rawptr)) -> ^Raft_Delayed_Task ---
	raft_delayed_task_delete :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_cancel :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_reset :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_type :: proc(task: ^Raft_Delayed_Task) -> i32 ---
	raft_delayed_task_user_data :: proc(task: ^Raft_Delayed_Task) -> rawptr ---
	raft_delayed_task_set :: proc(task: ^Raft_Delayed_Task, user_data: rawptr, callback: proc "c" (user_data: rawptr)) -> rawptr ---

	raft_asio_service_make :: proc(options: ^Raft_Asio_Options, logger: ^Raft_Logger_Ptr) -> ^Raft_Asio_Service_Ptr ---
	raft_asio_service_delete :: proc(asio_service: ^Raft_Asio_Service_Ptr) ---

	raft_asio_service_stop :: proc(asio_service: ^Raft_Asio_Service_Ptr) ---
	raft_asio_service_get_active_workers :: proc(asio_service: ^Raft_Asio_Service_Ptr) -> u32 ---

	raft_asio_service_schedule :: proc(asio_service: ^Raft_Asio_Service_Ptr, task: ^Raft_Delayed_Task, millis: i32) ---

	raft_asio_rpc_listener_make :: proc(asio_service: ^Raft_Asio_Service_Ptr, listening_port: u16, logger: ^Raft_Logger_Ptr) -> ^Raft_Asio_RPC_Listener_Ptr ---
	raft_asio_rpc_listener_delete :: proc(rpc_listener: ^Raft_Asio_RPC_Listener_Ptr) ---

	raft_asio_rpc_client_make :: proc(
		asio_service: ^Raft_Asio_Service_Ptr,
		endpoint: [^]byte,
		endpoint_size: c.size_t
	) -> ^Raft_Asio_RPC_Client_Ptr ---

	raft_asio_rpc_client_delete :: proc(rpc_listener: ^Raft_Asio_RPC_Client_Ptr) ---

	raft_fsm_make :: proc(
		user_data: rawptr,
		current_conf: ^Raft_Cluster_Config,
		rollback_conf: ^Raft_Cluster_Config,
		commit: Raft_FSM_Commit_Func,
		commit_config: Raft_FSM_Cluster_Config_Func,
		pre_commit: Raft_FSM_Commit_Func,
		rollback: Raft_FSM_Rollback_Func,
		rollback_config: Raft_FSM_Cluster_Config_Func,
		get_next_batch_size_hint_in_bytes: Raft_FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func,
		save_snapshot: Raft_FSM_Save_Snapshot_Func,
		apply_snapshot: Raft_FSM_Apply_Snapshot_Func,
		read_snapshot: Raft_FSM_Read_Snapshot_Func,
		free_snapshot_user_ctx: Raft_FSM_Free_User_Snapshot_Ctx_Func,
		last_snapshot: Raft_FSM_Last_Snapshot_Func,
		last_commit_index: Raft_FSM_Last_Commit_Index_Func,
		create_snapshot: Raft_FSM_Create_Snapshot_Func,
		chk_create_snapshot: Raft_FSM_Chk_Create_Snapshot_Func,
		allow_leadership_transfer: Raft_FSM_Allow_Leadership_Transfer_Func,
		adjust_commit_index: Raft_FSM_Adjust_Commit_Index_Func,
	) -> ^Raft_FSM_Ptr ---

	raft_fsm_delete :: proc(fsm: ^Raft_FSM_Ptr) ---

	raft_log_entry_make :: proc(
		term: u64,
		data: ^Raft_Buffer,
		timestamp: u64,
		has_crc32: bool,
		crc32: u32
	) -> ^Raft_Log_Entry ---

	raft_log_entry_delete :: proc(entry: ^Raft_Log_Entry) ---

	raft_log_entry_vec_push :: proc(
		vec: ^Raft_Log_Entry_Vec,
		term: u64,
		data: ^Raft_Buffer,
		timestamp: u64,
		has_crc32: bool,
		crc32: u32
	) ---

	raft_log_store_make :: proc(
		user_data: rawptr,
		next_slot: Raft_Log_Store_Next_Slot_Func,
		start_index: Raft_Log_Store_Start_Index_Func,
		last_entry: Raft_Log_Store_Last_Entry_Func,
		append: Raft_Log_Store_Append_Func,
		write_at: Raft_Log_Store_Write_At_Func,
		end_of_append_batch: Raft_Log_Store_End_Of_Append_Batch_Func,
		log_entries: Raft_Log_Store_Log_Entries_Func,
		entry_at: Raft_Log_Store_Entry_At_Func,
		term_at: Raft_Log_Store_Term_At_Func,
		pack: Raft_Log_Store_Pack_Func,
		apply_pack: Raft_Log_Store_Apply_Pack_Func,
		compact: Raft_Log_Store_Compact_Func,
		compact_async: Raft_Log_Store_Compact_Async_Func,
		flush: Raft_Log_Store_Flush_Func,
		last_durable_index: Raft_Log_Store_Last_Durable_Index_Func,
	) -> ^Raft_Log_Store_Ptr ---

	raft_log_store_delete :: proc(log_store: ^Raft_Log_Store_Ptr) ---

	raft_state_mgr_make :: proc(
		user_data: rawptr,
		load_config: Raft_State_Mgr_Load_Config_Func,
		save_config: Raft_State_Mgr_Save_Config_Func,
		read_state: Raft_State_Mgr_Read_State_Func,
		save_state: Raft_State_Mgr_Save_State_Func,
		load_log_store: Raft_State_Mgr_Load_Log_Store_Func,
		server_id: Raft_State_Mgr_Server_ID_Func,
		system_exit: Raft_State_Mgr_System_Exit_Func,
	) -> ^Raft_State_Mgr_Ptr ---

	raft_state_mgr_delete :: proc(state_mgr: ^Raft_State_Mgr_Ptr) ---

	raft_server_peer_info_vec_make :: proc() -> ^Raft_Server_Peer_Info_Vec ---

	raft_server_peer_info_vec_delete :: proc(vec: ^Raft_Server_Peer_Info_Vec) ---

	raft_server_peer_info_vec_size :: proc(vec: ^Raft_Server_Peer_Info_Vec) -> c.size_t ---
	raft_server_peer_info_vec_get :: proc(vec: ^Raft_Server_Peer_Info_Vec, index: c.size_t) -> ^Raft_Server_Peer_Info ---

	raft_params_make :: proc() -> ^Raft_Params ---
	raft_params_delete :: proc(params: ^Raft_Params) ---

	raft_server_launch :: proc(
		user_data: rawptr,
		fsm: ^Raft_FSM_Ptr,
		state_mgr: ^Raft_State_Mgr_Ptr,
		logger: ^Raft_Logger_Ptr,
		port_number: i32,
		asio_service: ^Raft_Asio_Service_Ptr,
		params_given: ^Raft_Params,
		skip_initial_election_timeout: bool,
		start_server_in_constructor: bool,
		test_mode_flag: bool,
		cb_func: rawptr
	) -> ^Raft_Server_Ptr ---

	raft_server_stop :: proc(
		rs_ptr: ^Raft_Server_Ptr,
		time_limit_sec: c.size_t
	) -> bool ---

	raft_server_get :: proc(
		rs_ptr: ^Raft_Server_Ptr
	) -> ^Raft_Server ---

	/*
	Check if this server is ready to serve operation.

	@return `true` if it is ready.
	*/
	raft_server_is_initialized :: proc(
		rs: ^Raft_Server,
	) -> bool ---


	/*
	*/
	raft_server_is_catching_up :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	*/
	raft_server_is_receiving_snapshot :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Add a new server to the current cluster. Only leader will accept this operation.
	Note that this is an asynchronous task so that needs more network communications.
	Returning this function does not guarantee adding the server.

	@param srv Configuration of server to add.
	@return `get_accepted()` will be true on success.
	*/
	raft_server_add_srv :: proc(
		rs: ^Raft_Server,
		srv: ^Raft_Srv_Config_Ptr,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Remove a server from the current cluster. Only leader will accept this operation.
	The same as `add_srv`, this is also an asynchronous task.

	@param srv_id ID of server to remove.
	@return `get_accepted()` will be true on success.
	*/
	raft_server_remove_srv :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Flip learner flag of given server. Learner will be excluded from the quorum. Only
	leader will accept this operation. This is also an asynchronous task.

	@param srv_id ID of the server to set as a learner.
	@param to If `true`, set the server as a learner, otherwise, clear learner flag.
	@return `ret->get_result_code()` will be OK on success.
	*/
	raft_server_flip_learner_flag :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		to: bool,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Append and replicate the given logs. Only leader will accept this operation.

	@param entries Set of logs to replicate.
	@return
	    In blocking mode, it will be blocked during replication, and
		return `cmd_result` instance which contains the commit results from
		the state machine.

		In async mode, this function will return immediately, and the commit
		results will be set to returned `cmd_result` instance later.
	*/
	raft_server_append_entries :: proc(
		rs: ^Raft_Server,
		entries: ^Raft_Append_Entries_Ptr,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Update the priority of given server.

	@param rs local bop_raft_server instance
	@param srv_id ID of server to update priority.
	@param new_priority Priority value, greater than or equal to 0.
			If priority is set to 0, this server will never be a leader.
	@param broadcast_when_leader_exists If we're not a leader and a
			leader exists, broadcast priority change to other peers.
			If false, set_priority does nothing. Please note that
			setting this option to true may possibly cause cluster config
			to diverge.

	@return SET If we're a leader and we have committed priority change.

	@return BROADCAST If either there's no:
			- live leader now, or we're a leader, and we want to set our priority to 0
			- we're not a leader and broadcast_when_leader_exists = true.
			  We have sent messages to other peers about priority change but haven't
			  committed this change.
	@return IGNORED If we're not a leader and broadcast_when_leader_exists = false.
			We ignored the request.
	*/
	raft_server_set_priority :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		new_priority: i32,
		broadcast_when_leader_exists: bool,
	) -> Raft_Server_Priority_Set_Result ---

	/*
	Broadcast the priority change of given server to all peers. This function should be used
	only when there is no live leader and leader election is blocked by priorities of live
	followers. In that case, we are not able to change priority by using normal `set_priority`
	operation.

	@param rs local bop_raft_server instance
	@param srv_id ID of server to update priority
	@param new_priority New priority.
	*/
	raft_server_broadcast_priority_change :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		new_priority: i32,
	) ---

	/*
	Yield current leadership and becomes a follower. Only a leader will accept this operation.

	If given `immediate_yield` flag is `true`, it will become a follower immediately.
	The subsequent leader election will be totally random so that there is always a
	chance that this server becomes the next leader again.

	Otherwise, this server will pause write operations first, wait until the successor
	(except for this server) finishes the catch-up of the latest log, and then resign.
	In such a case, the next leader will be much more predictable.

	Users can designate the successor. If not given, this API will automatically choose
	the highest priority server as a successor.

	@param rs local bop_raft_server instance
	@param immediate_yield If `true`, yield immediately.
	@param successor_id The server ID of the successor.
						If `-1`, the successor will be chosen automatically.
	*/
	raft_server_yield_leadership :: proc(
		rs: ^Raft_Server,
		immediate_yield: bool,
		successor_id: i32,
	) ---

	/*
	Send a request to the current leader to yield its leadership, and become the next leader.

	@return `true` on success. But it does not guarantee to become
	        the next leader due to various failures.
	*/
	raft_server_request_leadership :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Start the election timer on this server, if this server is a follower. It will
	allow the election timer permanently, if it was disabled by state manager.
	*/
	raft_server_restart_election_timer :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Set custom context to Raft cluster config. It will create a new configuration log and
	replicate it.

	@param data custom context data
	@param size number of bytes in data
	*/
	raft_server_set_user_ctx :: proc(
		rs: ^Raft_Server,
		data: [^]byte,
		size: c.size_t,
	) ---

	/*
	Get custom context from the current cluster config.

	@return custom context. The caller is transferred ownership of the Raft_Buffer.
			Any non-null value must be freed by calling `raft_buffer_delete` or
			transfer ownership.
	*/
	raft_server_get_user_ctx :: proc(
		rs: ^Raft_Server,
	) -> ^Raft_Buffer ---

	/*
	Get timeout for snapshot_sync_ctx

	@return snapshot_sync_ctx_timeout
	*/
	raft_server_get_snapshot_sync_ctx_timeout :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get ID of this server.

	@return Server ID
	*/
	raft_server_get_id :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get the current term of this server.

	@return Term
	*/
	raft_server_get_term :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the term of given log index number.

	@param log_idx Log index number
	@return Term of given log
	*/
	raft_server_get_log_term :: proc(
		rs: ^Raft_Server,
		log_idx: u64,
	) -> u64 ---

	/*
	Get the term of the last log.

	@return Term of the last log
	*/
	raft_server_get_last_log_term :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the last log index number.

	@return Last log index number.
	*/
	raft_server_get_last_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the last committed log index number of state machine.

	@return Last committed log index number of state machine.
	*/
	raft_server_get_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the target log index number we are required to commit.

	@return Target committed log index number.
	*/
	raft_server_get_target_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the leader's last committed log index number.

	@return The leader's last committed log index number.
	*/
	raft_server_get_leader_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the log index of the first config when this server became a leader.
	This API can be used for checking if the state machine is fully caught up
	with the latest log after a leader election, so that the new leader can
	guarantee strong consistency.

	It will return 0 if this server is not a leader.

	@return The log index of the first config when this server became a leader.
	*/
	raft_server_get_log_idx_at_becoming_leader :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Calculate the log index to be committed from current peers' matched indexes.

	@return Expected committed log index.
	*/
	raft_server_get_expected_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the current Raft cluster config.

	@param rs raft server instance
	@param cluster_config Wrapper for holding ^Raft_Cluster_Config_Ptr
	*/
	raft_server_get_config :: proc(
		rs: ^Raft_Server,
		cluster_config: ^Raft_Cluster_Config_Ptr,
	) ---

	/*
	Get data center ID of the given server.

	@param srv_id Server ID.
	@return -1 if given server ID does not exist.
	         0 if data center ID was not assigned.
	*/
	raft_server_get_dc_id :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get auxiliary context stored in the server config.

	@param srv_id Server ID.
	@return auxiliary context. The caller is transferred ownership of the Raft_Buffer.
			Any non-null value must be freed by calling `raft_buffer_delete` or
			transfer ownership.
	*/
	raft_server_get_aux :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
	) -> ^Raft_Buffer ---

	/*
	Get the ID of current leader.

	@return Leader ID
	        -1 if there is no live leader.
	*/
	raft_server_get_leader :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Check if this server is leader.

	@return `true` if it is leader.
	*/
	raft_server_is_leader :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Check if there is live leader in the current cluster.

	@return `true` if live leader exists.
	*/
	raft_server_is_leader_alive :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Get the configuration of given server.

	@param srv_id Server ID
	@return Server configuration
	*/
	raft_server_get_srv_config :: proc(
		rs: ^Raft_Server,
		svr_config: ^Raft_Srv_Config_Ptr,
		srv_id: i32,
	) ---

	/*
	Get the configuration of all servers.

	@param[out] configs_out Set of server configurations.
	*/
	raft_server_get_srv_config_all :: proc(
		rs: ^Raft_Server,
		configs_out: ^Raft_Srv_Config_Vec,
	) ---

	/*
	Update the server configuration, only leader will accept this operation.
	This function will update the current cluster config and replicate it to all peers.

	We don't allow changing multiple server configurations at once, due to safety reason.

	Change on endpoint will not be accepted (should be removed and then re-added).
	If the server is in new joiner state, it will be rejected.
	If the server ID does not exist, it will also be rejected.

	@param new_config Server configuration to update.
	@return `true` on success, `false` if rejected.
	*/
	raft_server_update_srv_config :: proc(
		rs: ^Raft_Server,
		new_config: ^Raft_Srv_Config_Ptr,
	) ---

	/*
	Get the peer info of the given ID. Only leader will return peer info.

	@param srv_id Server ID
	@return Peer info
	*/
	raft_server_get_peer_info :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		peer: ^Raft_Server_Peer_Info,
	) -> bool ---

	/*
	Get the info of all peers. Only leader will return peer info.

	@param[out] peers_out vector of peers
	*/
	raft_server_get_peer_info_all :: proc(
		rs: ^Raft_Server,
		peers_out: ^Raft_Server_Peer_Info_Vec,
	) ---

	/*
	Shut down server instance.
	*/
	raft_server_shutdown :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Start internal background threads, initialize election
	*/
	raft_server_start_server :: proc(
		rs: ^Raft_Server,
		skip_initial_election_timeout: bool,
	) ---

	/*
	Stop background commit thread.
	*/
	raft_server_stop_server :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Send reconnect request to leader. Leader will re-establish the connection to this server
	in a few seconds. Only follower will accept this operation.
	*/
	raft_server_send_reconnect_request :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Update Raft parameters.

	@param new_params Parameters to set
	*/
	raft_server_update_params :: proc(
		rs: ^Raft_Server,
		new_params: ^Raft_Params,
	) ---

	/*
	Get the current Raft parameters. Returned instance is the clone of the original one,
	so that user can modify its contents.

	@return Clone of Raft parameters.
	*/
	raft_server_get_current_params :: proc(
		rs: ^Raft_Server,
		params: ^Raft_Params,
	) ---

	/*
	Get the counter number of given stat name.

	@param name Stat name to retrieve.
	@return Counter value.
	*/
	raft_server_get_stat_counter :: proc(
		rs: ^Raft_Server,
		counter: ^Raft_Counter,
	) -> u64 ---

	/*
	Get the gauge number of given stat name.

	@param name Stat name to retrieve.
	@return Gauge value.
	*/
	raft_server_get_stat_gauge :: proc(
		rs: ^Raft_Server,
		gauge: ^Raft_Gauge,
	) -> i64 ---

	/*
	Get the histogram of given stat name.

	@param name Stat name to retrieve.
	@param[out] histogram_out Histogram as a map. Key is the upper bound of a bucket, and
	            value is the counter of that bucket.
	@return `true` on success.
	        `false` if stat does not exist, or is not histogram type.
	*/
	raft_server_get_stat_histogram :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) -> bool ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_counter :: proc(
		rs: ^Raft_Server,
		counter: ^Raft_Counter,
	) ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_gauge :: proc(
		rs: ^Raft_Server,
		gauge: ^Raft_Gauge,
	) ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_histogram :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) ---

	/*
	Reset all existing stats to zero.
	*/
	raft_server_reset_all_stats :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) ---

	/*
	Set a custom callback function for increasing term.
	*/
	raft_server_set_inc_term_func :: proc(
		rs: ^Raft_Server,
		user_data: rawptr,
		handler: Raft_Inc_Term_Handler,
	) ---

	/*
	Pause the background execution of the state machine. If an operation execution is
	currently happening, the state machine may not be paused immediately.

	@param timeout_ms If non-zero, this function will be blocked until
	                  either it completely pauses the state machine execution
	                  or reaches the given time limit in milliseconds.
	                  Otherwise, this function will return immediately, and there
	                  is a possibility that the state machine execution
	                  is still happening.
	*/
	raft_server_pause_state_machine_execution :: proc(
		rs: ^Raft_Server,
		timeout_ms: c.size_t,
	) ---

	/*
	Resume the background execution of state machine.
	*/
	raft_server_resume_state_machine_execution :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Check if the state machine execution is paused.

	@return `true` if paused.
	*/
	raft_server_is_state_machine_execution_paused :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Block the current thread and wake it up when the state machine execution is paused.

	@param timeout_ms If non-zero, wake up after the given amount of time
	                  even though the state machine is not paused yet.
  	@return `true` if the state machine is paused.
	*/
	raft_server_wait_for_state_machine_pause :: proc(
		rs: ^Raft_Server,
		timeout_ms: c.size_t,
	) -> bool ---

	/*
	(Experimental)
	This API is used when `raft_params::parallel_log_appending_` is set.

	Everytime an asynchronous log appending job is done, users should call this API to notify.
	Raft server to handle the log. Note that calling this API once for multiple logs is acceptable
	and recommended.

	@param ok `true` if appending succeeded.
	*/
	raft_server_notify_log_append_completion :: proc(
		rs: ^Raft_Server,
		ok: bool,
	) ---

	/*
	Manually create a snapshot based on the latest committed log index of the state machine.

	Note that snapshot creation will fail immediately if the previous snapshot task is still running.

	@param serialize_commit
	       If `true`, the background commit will be blocked until `create_snapshot`
	       returns. However, it will not block the commit for the entire duration
	       of the snapshot creation process, as long as your state machine creates
	       the snapshot asynchronously. The purpose of this flag is to ensure that
	       the log index used for the snapshot creation is the most recent one.

    @return Log index number of the created snapshot or`0` if failed.
	*/
	raft_server_create_snapshot :: proc(
		rs: ^Raft_Server,
		serialize_commit: bool,
	) -> u64 ---

	/*
	Manually and asynchronously create a snapshot on the next earliest available commited
	log index.

	Unlike `create_snapshot`, if the previous snapshot task is running, it will wait
	until the previous task is done. Once the snapshot creation is finished, it will be
	notified via the returned `cmd_result` with the log index number of the snapshot.

	@param `handler` instance.
		   `nullptr` if there is already a scheduled snapshot creation.
	*/
	raft_server_schedule_snapshot_creation :: proc(
		rs: ^Raft_Server,
		handler: ^Raft_Async_U64_Ptr,
	) ---

	/*
	Get the log index number of the last snapshot.

	@return Log index number of the last snapshot. `0` if snapshot does not exist.
	*/
	raft_server_get_last_snapshot_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	raft_mdbx_state_mgr_open :: proc(
		my_srv_config: ^Raft_Srv_Config_Ptr,
		dir: [^]byte,
		dir_size: c.size_t,
		logger: ^Raft_Logger_Ptr,
		size_lower: c.size_t,
		size_now: c.size_t,
		size_upper: c.size_t,
		growth_step: c.size_t,
		shirnk_threshold: c.size_t,
		page_size: c.size_t,
		flags: u32,
		mode: u16,
		log_store: ^Raft_Log_Store_Ptr,
	) -> ^Raft_State_Mgr_Ptr ---

	raft_mdbx_log_store_open :: proc(
		dir: [^]byte,
		dir_size: c.size_t,
		logger: ^Raft_Logger_Ptr,
		size_lower: c.size_t,
		size_now: c.size_t,
		size_upper: c.size_t,
		growth_step: c.size_t,
		shirnk_threshold: c.size_t,
		page_size: c.size_t,
		flags: u32,
		mode: u16,
		compact_batch_size: c.size_t,
	) -> ^Raft_Log_Store_Ptr ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// uSockets
// Miniscule cross-platform eventing, networking & crypto for async applications
//
///////////////////

https://github.com/uNetworking/uSockets
*/

@(default_calling_convention = "c")
foreign lib {
	us_socket_send_buffer :: proc(ssl: c.int, s: ^US_Socket) -> rawptr ---

	/* Public interface for UDP sockets */

	/*
	Peeks data and length of UDP payload
	*/
	us_udp_packet_buffer_payload :: proc(buf: ^US_UDP_Packet_Buffer, index: c.int) -> [^]byte ---
	us_udp_packet_buffer_payload_length :: proc(buf: ^US_UDP_Packet_Buffer, index: c.int) -> c.int ---

	/*
	Copies out local (received destination) ip (4 or 16 bytes) of received packet
	*/
	us_udp_packet_buffer_local_ip :: proc(buf: ^US_UDP_Packet_Buffer, index: c.int, ip: cstring) -> c.int ---

	/*
	Get the bound port in host byte order
	*/
	us_udp_socket_bound_port :: proc(s: ^US_UDP_Socket) -> c.int ---

	/*
	Peeks peer addr (sockaddr) of received packet
	*/
	us_udp_packet_buffer_peer :: proc(buf: ^US_UDP_Packet_Buffer, index: c.int) -> cstring ---

	/*
	Peeks ECN of received packet
	*/
	us_udp_packet_buffer_ecn :: proc(buf: ^US_UDP_Packet_Buffer, index: c.int) -> c.int ---

	/*
	Receives a set of packets into specified packet buffer
	*/
	us_udp_socket_receive :: proc(s: ^US_UDP_Socket, buf: ^US_UDP_Packet_Buffer) -> c.int ---

	us_udp_buffer_set_packet_payload :: proc(
		send_buf: ^US_UDP_Packet_Buffer,
		index: c.int,
		offset: c.int,
		payload: [^]byte,
		length: c.int,
		peer_addr: cstring,
	) ---

	us_udp_socket_send :: proc(s: ^US_UDP_Socket, buf: ^US_UDP_Packet_Buffer, num: c.int) -> c.int ---

	/*
	Allocates a packet buffer that is reuable per thread. Mutated by us_udp_socket_receive.
	*/
	us_create_udp_packet_buffer :: proc() -> ^US_UDP_Packet_Buffer ---

	us_create_udp_socket :: proc(
		loop: ^US_Loop,
		buf: ^US_UDP_Packet_Buffer,
		data_cb: proc "c" (s: ^US_UDP_Socket, buf: ^US_UDP_Packet_Buffer, index: c.int),
		drain_cb: proc "c" (s: ^US_UDP_Socket),
		host: cstring,
		port: c.ushort,
		user: rawptr,
	) -> ^US_UDP_Socket ---

	/*
	This one is ugly, should be ext! not user
	*/
	us_udp_socket_user :: proc(s: ^US_UDP_Socket) -> rawptr ---

	/*
	Binds the UDP socket to an interface and port
	*/
	us_udp_socket_bind :: proc(s: ^US_UDP_Socket, hostname: cstring, port: c.ushort) -> c.int ---

	/*
	Create a new high precision, low performance timer. May fail and return null
	*/
	us_create_timer :: proc(loop: ^US_Loop, _fallthrough: c.int, ext_size: c.uint) ->  ^US_Timer ---

	/*
	Returns user data extension for this timer
	*/
	us_timer_ext :: proc(timer: ^US_Timer) -> rawptr ---

	us_timer_close :: proc(timer: ^US_Timer) ---

	/*
	Arm a timer with a delay from now and eventually a repeat delay.
 	Specify 0 as repeat delay to disable repeating. Specify both 0 to disarm.
 	*/
	us_timer_set :: proc(timer: ^US_Timer, cb: proc "c" (t: ^US_Timer), ms: c.int, repeat_ms: c.int) ---

	/*
	Returns the loop for this timer
	*/
	us_timer_loop :: proc(timer: ^US_Timer) -> ^US_Loop ---

	us_socket_context_timestamp :: proc(ssl: c.int, ctx: ^US_Socket_Context) -> c.ushort ---

	/* Adds SNI domain and cert in asn1 format */

	us_socket_context_add_server_name :: proc(
		ssl: c.int,
		ctx: ^US_Socket_Context,
		hostname_pattern: cstring,
		options: US_Socket_Context_Options,
		user: rawptr,
	) ---

	us_socket_context_remove_server_name :: proc(ssl: c.int, ctx: ^US_Socket_Context, hostname_pattern: cstring) ---

	us_socket_context_on_server_name :: proc(
		ssl: c.int,
		ctx: ^US_Socket_Context,
		cb: proc "c" (c: ^US_Socket_Context),
		hostname: cstring,
	) ---

	us_socket_server_name_userdata :: proc(ssl: c.int, s: ^US_Socket) -> rawptr ---
	us_socket_context_find_server_name_userdata :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
	    hostname_pattern: cstring,
	) -> rawptr ---

	/*
	Returns the underlying SSL native handle, such as SSL_CTX or nullptr
	*/
	us_socket_context_get_native_handle :: proc(ssl: c.int, ctx: ^US_Socket_Context) -> rawptr ---

	/*
	A socket context holds shared callbacks and user data extension for associated sockets
	*/
	us_create_socket_context :: proc(
	    ssl: c.int,
		loop: ^US_Loop,
		ext_size: c.int,
		options: US_Socket_Context_Options,
	) -> ^US_Socket_Context ---

	/*
	Delete resources allocated at creation time.
	*/
	us_socket_context_free :: proc(ssl: c.int, ctx: ^US_Socket_Context) ---

	/* Setters of various async callbacks */

	us_socket_context_on_pre_open :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_pre_open: proc "c" (ctx: ^US_Socket_Context, fd: SOCKET) -> SOCKET,
	) ---

	us_socket_context_on_open :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_open: proc "c" (s: ^US_Socket, is_client: c.int, ip: [^]byte, ip_length: c.int) -> ^US_Socket,
	) ---

	us_socket_context_on_close :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_close: proc "c" (s: ^US_Socket, code: c.int, reason: rawptr) -> ^US_Socket,
	) ---

	us_socket_context_on_data :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_data: proc "c" (s: ^US_Socket, data: [^]byte, length: c.int) -> ^US_Socket,
	) ---

	us_socket_context_on_writable :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_writable: proc "c" (s: ^US_Socket) -> ^US_Socket,
	) ---

	us_socket_context_on_timeout :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_timeout: proc "c" (s: ^US_Socket) -> ^US_Socket,
	) ---

	us_socket_context_on_long_timeout :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_timeout: proc "c" (s: ^US_Socket, ctx: ^US_Socket_Context) -> ^US_Socket,
	) ---

	/*
	This one is only used for when a connecting socket fails in a late stage.
	*/
	us_socket_context_on_connect_error :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_connect_error: proc "c" (s: ^US_Socket, code: c.int) -> ^US_Socket,
	) ---

	/*
	Emitted when a socket has been half-closed
	*/
	us_socket_context_on_end :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		on_end: proc "c" (s: ^US_Socket) -> ^US_Socket,
	) ---

	/*
	Returns user data extension for this socket context
	*/
	us_socket_context_ext :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
	) -> rawptr ---

	/*
	Closes all open sockets, including listen sockets. Does not invalidate the socket context.
	*/
	us_socket_context_close :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
	) ---

	/*
	Listen for connections. Acts as the main driving cog in a server. Will call set async callbacks.
	*/
	us_socket_context_listen :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		host: cstring,
		port: c.int,
		options: c.int,
		socket_ext_size: c.int,
	) -> ^US_Listen_Socket ---

	us_socket_context_listen_unix :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		path: cstring,
		options: c.int,
		socket_ext_size: c.int,
	) -> ^US_Listen_Socket ---

	us_listen_socket_close :: proc(ssl: c.int, ls: ^US_Listen_Socket) ---

	/*
	Adopt a socket which was accepted either internally, or from another accept() outside libusockets
	*/
	us_adopt_accepted_socket :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		client_fd: SOCKET,
		socket_ext_size: c.uint,
		addr_ip: [^]byte,
		addr_ip_length: c.int,
	) -> ^US_Socket ---

	/*
	Land in on_open or on_connection_error or return null or return socket
	*/
	us_socket_context_connect :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		host: cstring,
		port: c.int,
		source_host: cstring,
		options: c.int,
		socket_ext_size: c.int,
	) -> ^US_Socket ---

	us_socket_context_connect_unix :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		server_path: cstring,
		options: c.int,
		socket_ext_size: c.int,
	) -> ^US_Socket ---

	/*
	Is this socket established? Can be used to check if a connecting socket has
	fired the on_open event yet. Can also be used to determine if a socket is a
	listen_socket or not, but you probably know that already.
	*/
	us_socket_is_established :: proc(ssl: c.int, s: ^US_Socket) -> c.int ---

	/*
	Cancel a connecting socket. Can be used together with us_socket_timeout to
	limit connection times. Entirely destroys the socket - this function works
	like us_socket_close but does not trigger on_close event since you never
	got the on_open event first.
	*/
	us_socket_close_connecting :: proc(ssl: c.int, s: ^US_Socket) -> ^US_Socket ---

	/*
	Returns the loop for this socket context.
	*/
	us_socket_context_loop :: proc(ssl: c.int, ctx: ^US_Socket_Context) -> ^US_Loop ---

	/*
	Invalidates passed socket, returning a new resized socket which belongs to a
	different socket context. Used mainly for "socket upgrades" such as when
	transitioning from HTTP to WebSocket.
	*/
	us_socket_context_adopt_socket :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		s: ^US_Socket,
		ext_size: c.int,
	) -> ^US_Socket ---

	/*
	Create a child socket context which acts much like its own socket context with
	its own callbacks yet still relies on the parent socket context for some shared
	resources. Child socket contexts should be used together with socket adoptions
	and nothing else.
	*/
	us_create_child_socket_context :: proc(
	    ssl: c.int,
		ctx: ^US_Socket_Context,
		ctx_ext_size: c.int,
	) -> ^US_Socket ---

	/* Public interfaces for loops */

	/* Returns a new event loop with user data extension */
	us_create_loop :: proc(
	    hint: rawptr,
		wakeup_cb: proc "c" (loop: ^US_Loop),
		pre_cb: proc "c" (loop: ^US_Loop),
		post_cb: proc "c" (loop: ^US_Loop),
		ext_size: c.uint,
	) -> ^US_Loop ---

	/*
	Frees the loop immediately
	*/
	us_loop_free :: proc(loop: ^US_Loop) ---

	/*
	Returns the loop user data extension
	*/
	us_loop_ext :: proc(loop: ^US_Loop) -> rawptr ---

	/*
	Blocks the calling thread and drives the event loop until no more non-fallthrough
	polls are scheduled
	*/
	us_loop_run :: proc(loop: ^US_Loop) ---

	/*
	Signals the loop from any thread to wake up and execute its wakeup handler from
	the loop's own running thread. This is the only fully thread-safe function and
	serves as the basis for thread safety
	*/
	us_wakeup_loop :: proc(loop: ^US_Loop) ---

	/*
	Hook up timers in existing loop
	*/
	us_loop_integrate :: proc(loop: ^US_Loop) ---

	/*
	Returns the loop iteration number
	*/
	us_loop_iteration_number :: proc(loop: ^US_Loop) -> c.longlong ---

	/* Public interfaces for polls */

	/*
	A fallthrough poll does not keep the loop running, it falls through
	*/
	us_create_poll :: proc(loop: ^US_Loop, _fallthrough: c.int, ext_size: c.uint) -> ^US_Poll ---

	/*
	After stopping a poll you must manually free the memory
	*/
	us_poll_free :: proc(p: ^US_Poll, loop: ^US_Loop) ---

	/*
	Associate this poll with a socket descriptor and poll type
	*/
	us_poll_init :: proc(p: ^US_Poll, fd: SOCKET, poll_type: c.int) ---

	/*
	Start, change and stop polling for events
	*/
	us_poll_start :: proc(p: ^US_Poll, loop: ^US_Loop, events: c.int) ---
	us_poll_change :: proc(p: ^US_Poll, loop: ^US_Loop, events: c.int) ---
	us_poll_stop :: proc(p: ^US_Poll, loop: ^US_Loop) ---

	/*
	Return what events we are polling for
	*/
	us_poll_events :: proc (p: ^US_Poll) -> c.int ---

	/*
	Returns the user data extension of this poll
	*/
	us_poll_ext :: proc(p: ^US_Poll) -> rawptr ---

	/*
	Get associated socket descriptor from a poll
	*/
	us_poll_fd :: proc(p: ^US_Poll) -> SOCKET ---

	/*
	Resize an active poll
	*/
	us_poll_resize :: proc(p: ^US_Poll, loop: ^US_Loop, ext_size: c.uint) -> ^US_Poll ---

	/* Public interfaces for sockets */

	/*
	Returns the underlying native handle for a socket, such as SSL or file
	descriptor. In the case of file descriptor, the value of pointer is fd.
	*/
	us_socket_get_native_handle :: proc(ssl: c.int, s: ^US_Socket) -> rawptr ---

	/*
	Write up to length bytes of data. Returns actual bytes written. Will call
	the on_writable callback of active socket context on failure to write
	everything off in one go. Set hint msg_more if you have more immediate data
	to write.
	*/
	us_socket_write :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		data: [^]byte,
		length: c.int,
		msg_more: c.int,
	) -> c.int ---

	/*
	Special path for non-SSL sockets. Used to send header and payload in
	one go. Works like us_socket_write.
	*/
	us_socket_write2 :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		header: [^]byte,
		header_length: c.int,
		payload: [^]byte,
		payload_length: c.int,
	) -> c.int ---

	/*
	Set a low precision, high performance timer on a socket. A socket can
	only have one single active timer at any given point in time. Will remove
	any such pre set timer
	*/
	us_socket_timeout :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		seconds: c.uint,
	) ---

	/*
	Set a low precision, high performance timer on a socket. Suitable for
	per-minute precision.
	*/
	us_socket_long_timeout :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		minutes: c.uint,
	) ---

	/*
	Return the user data extension of this socket
	*/
	us_socket_ext :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> rawptr ---

	/*
	Return the socket context of this socket
	*/
	us_socket_context :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> ^US_Socket_Context ---

	/*
	Withdraw any msg_more status and flush any pending data
	*/
	us_socket_flush :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) ---

	/*
	Shuts down the connection by sending FIN and/or close_notify
	*/
	us_socket_shutdown :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) ---

	/*
	Shuts down the connection in terms of read, meaning next event loop
	iteration will catch the socket being closed. Can be used to defer
	closing to next event loop iteration.
	*/
	us_socket_shutdown_read :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) ---

	/*
	Returns whether the socket has been shut down or not
	*/
	us_socket_is_shut_down :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> c.int ---

	/*
	Returns whether this socket has been closed. Only valid if memory
	has not yet been released.
	*/
	us_socket_is_closed :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> c.int ---

	/*
	Immediately closes the socket
	*/
	us_socket_close :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		code: c.int,
		reason: rawptr,
	) -> ^US_Socket ---

	/*
	Returns local port or -1 on failure.
	*/
	us_socket_local_port :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> c.int ---

	/*
	Returns remote ephemeral port or -1 on failure.
	*/
	us_socket_remote_port :: proc(
	    ssl: c.int,
		s: ^US_Socket,
	) -> c.int ---

	/*
	Copy remote (IP) address of socket, or fail with zero length.
	*/
	us_socket_remote_address :: proc(
	    ssl: c.int,
		s: ^US_Socket,
		buf: [^]byte,
		length: ^c.int,
	) -> c.int ---
}

main :: proc() {
	run_alloc()

	bench_co()
}

run_alloc :: proc() {
	fmt.println("libbop")
	p := zalloc(16)
	fmt.println("pointer", cast(uint)uintptr(p))
	dealloc(p)
}


@(test)
test_alloc :: proc(t: ^testing.T) {
	p := alloc(16)
	dealloc(p)
}
