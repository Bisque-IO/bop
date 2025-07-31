/**
 * \file wasmtime/async.h
 *
 * \brief Wasmtime async functionality
 *
 * Async functionality in Wasmtime is well documented here:
 * https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.async_support
 *
 * All WebAssembly executes synchronously, but an async support enables the Wasm
 * code be executed on a separate stack, so it can be paused and resumed. There
 * are three mechanisms for yielding control from wasm to the caller: fuel,
 * epochs, and async host functions.
 *
 * When WebAssembly is executed, a `wasmtime_call_future_t` is returned. This
 * struct represents the state of the execution and each call to
 * `wasmtime_call_future_poll` will execute the WebAssembly code on a separate
 * stack until the function returns or yields control back to the caller.
 *
 * It's expected these futures are pulled in a loop until completed, at which
 * point the future should be deleted. Functions that return a
 * `wasmtime_call_future_t` are special in that all parameters to that function
 * should not be modified in any way and must be kept alive until the future is
 * deleted. This includes concurrent calls for a single store - another function
 * on a store should not be called while there is a `wasmtime_call_future_t`
 * alive.
 *
 * As for asynchronous host calls - the reverse contract is upheld. Wasmtime
 * will keep all parameters to the function alive and unmodified until the
 * `wasmtime_func_async_continuation_callback_t` returns true.
 *
 */

#ifndef WASMTIME_TLS_H
#define WASMTIME_TLS_H

#include <wasm.h>
#include <wasmtime/conf.h>
#include <wasmtime/config.h>
#include <wasmtime/error.h>
#include <wasmtime/store.h>

#ifdef WASMTIME_FEATURE_TLS

#ifdef __cplusplus
extern "C" {
#endif

/**
 * thread local state
 */
typedef struct {
  /// wasmtime managed pointer saved in internal thread local
  /// for current store and execution stack.
  void *tls_ptr;
  /// stack limit of current executing store and stack.
  size_t stack_limit;
} wasmtime_tls_state_t;

/**
 * saves current thread local state to supplied wasmtime_tls_state_t.
 */
WASM_API_EXTERN void
wasmtime_tls_save(wasmtime_store_t *, wasmtime_tls_state_t *);

/**
 * restores current thread local state to supplied wasmtime_tls_state_t.
 */
WASM_API_EXTERN void
wasmtime_tls_restore(wasmtime_store_t *, wasmtime_tls_state_t *);

/**
 * One time initialization of thread local.
 * Call once per thread.
 */
WASM_API_EXTERN void wasmtime_tls_init();

#ifdef __cplusplus
} // extern "C"
#endif

#endif // WASMTIME_FEATURE_TLS

#endif // WASMTIME_TLS_H
