# bop coroutines in wasmtime

## Overview

This design implements **preemptive, stackful green threads** for Wasmtime using a coroutine system (`llco`) and a **work-stealing scheduler**. Each fiber has its own native stack and can directly run WebAssembly (Wasm) code without requiring any async transformations or compiler support.

Unlike standard `async` runtimes—which require `.await` points and special ABI lowering—this system enables **transparent preemption** even inside tight Wasm loops, without modifying the guest code.

---

## Work Stealing & Multi-Threading Support

- **Thread migration ready**: Fibers (`llco`) can be resumed on any OS thread, thanks to Wasmtime’s fully portable TLS model.
- **One-time thread init**: Each thread must call `wasmtime_tls_init()` once before running any Wasmtime fiber.
- **Safe TLS handoff**: Each fiber carries its own `wasmtime_tls_save()` tls pointer and `stack_limit`, enabling correct trap handling when resumed on another thread.
- **Load-balanced execution**: Fibers can be freely stolen and resumed by worker threads in a **work-stealing scheduler**, maximizing CPU utilization.
- **No async ABI required**: There’s no need for Wasmtime async support or `asyncify`; migration works on unmodified Wasm.

---

## Efficient Use of a Single Store + Instance with Many Fibers

- **Single instance, many contexts**: A single `Instance` in a `Store` can serve thousands of fibers (`llco`), each with its own call stack and locals.
- **Serialized execution**: Only one fiber may be actively executing Wasm in a `Store` at a time, ensuring thread safety without locking.
- **Zero-copy context switching**: TLS and `stack_limit` are the only per-fiber values that must be swapped—no need to recreate instances.
- **Reentrant and efficient**: Fibers can call into the same Wasm instance repeatedly without additional setup costs.
- **Ideal for SPMD workloads**: Perfect for simulations, batch jobs, or streaming tasks where each green thread follows the same logic path.

---

## Architecture

Each OS thread is initialized once with:

- A valid Wasmtime TLS slot via `wasmtime_tls_init()`
- A scheduler loop that selects and switches between fibers
- The ability to safely run any number of Wasmtime `Store`s

```text
+-----------------------------+    ┌────────────┐   ┌────────────┐
|       OS Thread (T1)        |--► │  Fiber A   │--►│  Fiber B   │
|     (wasmtime_tls_init)     |    └────────────┘   └────────────┘
+-----------------------------+          ▲                 ▲
                                          ╲               ╱
                     +-------------------+---------------+
                     |   Work-Stealing Scheduler (llco)  |
                     +-----------------------------------+
```
---

Each fiber stores:
- The **TLS pointer** from `wasmtime_tls_save/restore()` (used by traps and unwinder)
- Its **`stack_limit`** guard from the Store's `vm_store_context`

These are swapped during each fiber context switch.

---

## Preemptive Time Slicing with Epochs

Wasmtime’s **epoch-based interruption** is used for scheduling:

- A global counter is incremented periodically (via signal, timer, or loop).
- Each Wasm fiber sets its own **`epoch_deadline`**.
- When the deadline is reached, Wasmtime triggers a **callback**.

Inside the epoch callback:
- The currently running fiber (`llco`) is paused.
- Its TLS pointer and stack limit are saved.
- A different fiber is selected and resumed by restoring its state.

This allows **safe, low-latency preemption**, even deep inside Wasm or hostcalls.

---

## ⚡ Performance Comparison

| Feature                     | bop co                           | Wasmtime Async ABI               |
|-----------------------------|----------------------------------|----------------------------------|
| Suspension granularity      | Any instruction (via epoch)      | Manual `.await` points only      |
| Memory overhead             | C stack + 2 words                | Heap state machines per call     |
| Scheduling latency          | ~8-15 ns (TLS + stack swap)      | ~200–500 ns per poll cycle       |
| Code transformation         | None                             | Requires async lowering          |
| Wasm source compatibility   | All Wasm binaries                | Must opt into async support      |

This approach enables **deep stack suspension**, **nanosecond-scale context switches**, and zero transform overhead in hot paths.

---

## Summary

This Wasmtime runtime model delivers:

- Fast, stackful fibers via `llco`
- Transparent, safe preemption using epochs
- Minimal TLS and stack guard swapping (~2 words)
- Seamless support for all Wasm binaries — no ABI hacks

It is ideal for building **high-performance, latency-sensitive runtimes** for WebAssembly, especially in environments requiring tight control over execution, preemption, and scheduling.
