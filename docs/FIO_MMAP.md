
# Virtual Memory Window Design for Wasmtime + mmap

This document describes a high-performance technique for exposing dynamically-created mmap files to a WebAssembly module running under Wasmtime, using a single imported memory region as a virtual memory "window".

---

## 🧠 Overview

The host manages a dynamic number of file-backed memory regions (via `mmap`) and exposes them to the WebAssembly instance via a **pre-reserved linear memory**.
The WebAssembly code directly accesses these files using zero-copy access via offsets within a shared imported memory.

---

## ✅ Key Concepts

- **Single large imported memory**: Allocated at startup and shared with the Wasm instance.
- **Dynamic mmap files**: Files are created, mapped, and unmapped by the host at runtime.
- **MAP_FIXED mappings**: Used to map files into fixed offsets inside the shared memory window.
- **Zero-copy**: The Wasm code accesses file contents directly, avoiding any memory copying.

---

## 🛠️ Memory Reservation Strategy

### Step 1: Reserve a Large Virtual Memory Region

On **Linux/macOS**:
```c
void* window = mmap(NULL, large_size, PROT_NONE, MAP_ANON | MAP_PRIVATE, -1, 0);
```

On **Windows**:
```c
void* window = VirtualAlloc(NULL, large_size, MEM_RESERVE, PAGE_NOACCESS);
```

---

### Step 2: Import this Memory into Wasmtime

Create an imported memory in Wasmtime and point it to the reserved address range.

- Must match Wasm page size alignment (64 KiB).
- Growth should be controlled or disabled.

---

## 🔄 Dynamic Mapping of Files

### Mapping

On **Linux/macOS**:
```c
void* mapped = mmap(window + offset, size, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_FIXED, fd, 0);
```

On **Windows**:
```c
void* mapped = MapViewOfFileEx(hMap, FILE_MAP_ALL_ACCESS, 0, 0, size, window + offset);
```

### Unmapping

```c
munmap(window + offset, size); // Linux/macOS
UnmapViewOfFile(window + offset); // Windows
```

---

## 📦 File Management Protocol

- Host maintains a metadata table with `offset`, `length`, and `file_id`.
- Wasm accesses file regions directly via known offsets.
- Memory layout and mappings are coordinated entirely by the host.

---

## ⚠️ Considerations

| Concern          | Notes                                                           |
|------------------|-----------------------------------------------------------------|
| Alignment        | Use 64 KiB page alignment for Wasm compatibility.               |
| Bounds Checking  | Wasm must not access unmapped memory.                           |
| Lifetime         | Ensure regions are unmapped after Wasm stops using them.        |
| Performance      | Lazy page loading (on first access) makes memory use efficient. |

---

## ✅ Advantages

- True zero-copy file access.
- High-performance dynamic file integration.
- Cross-platform with careful use of platform-specific APIs.
- Avoids Wasmtime memory growth and duplication overhead.

---

## 🧪 Supported Platforms

| Platform | Supported | Notes                                     |
|----------|-----------|-------------------------------------------|
| Linux    | ✅ Yes    | Fully supported, ideal platform.          |
| macOS    | ✅ Yes    | Works with reserved + MAP_FIXED strategy. |
| Windows  | ✅ Yes    | Requires VirtualAlloc + MapViewOfFileEx.  |

---

## 🧩 Summary

This design enables Wasmtime instances to access a dynamically changing set of host-managed file-backed memory regions efficiently and safely through a fixed virtual memory window. This strategy is ideal for high-performance applications such as fiber schedulers, simulation engines, or embedded Wasm environments.
