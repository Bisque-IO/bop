# Repository Guidelines

## Project Structure & Module Organization
- Source: `lib/src/` (core modules: `uws/`, `raft.*`, `mpmc.*`, `spsc.*`, alloc, hashing)
- Third‑party/vendor: `lib/` (usockets, libuv, wolfssl, snmalloc, mdbx, etc.)
- Tests: `tests/` (e.g., `HttpClientTest.cpp`, `TCPTest.cpp`, `TestRunner.cpp`)
- Build config: `xmake.lua` (root and `lib/`, `tests/`), `CMakeLists.txt`
- Docs & assets: `docs/`, `data/`, `ui/`

## Build, Test, and Development Commands
- Configure + build (xmake): `xmake f -m release` then `xmake`
- Build tests (xmake): `xmake` (includes `tests/` targets)
- Run tests (xmake): `xmake run test-uws`
- CMake (alt): `cmake -S . -B build && cmake --build build -j`
- CTest (alt): `cd tests && cmake -S . -B build && cmake --build build && ctest --test-dir build -V`
- Scripted tests: `bash tests/build_and_run.sh` (on POSIX), `tests/build_and_run.bat` (Windows)

## Coding Style & Naming Conventions
- Language: C/C++ (C++23). Format with `clang-format` (repo `.clang-format`).
- Indentation: spaces, keep existing style in touched files.
- Files: snake_case for sources/headers (e.g., `mpmc.cpp`, `uws.h`); types `PascalCase`; functions/variables `lower_snake_case`; constants/macros `UPPER_SNAKE_CASE`.
- Headers in `lib/src/**` include locally with relative paths; prefer minimal includes.

## Testing Guidelines
- Location: `tests/`. Custom runner: `TestRunner.cpp`; per‑area tests in `*Test.cpp`.
- Frameworks: custom harness; `doctest` is available via xmake but not required.
- Run: `xmake run test-uws` or CTest as above.
- Add tests: mirror existing patterns; name files `NameTest.cpp`; keep fast and deterministic.

## Commit & Pull Request Guidelines
- Commits: imperative mood, concise subject (≤50 chars), optional body explaining rationale. Example: `uws: add TCP client backpressure handling`.
- PRs: clear description, scope, and rationale; link issues; list platforms tested (Windows/Linux/macOS), attach logs for build/test; include screenshots for UI (if any).
- CI expectations: PRs should build (xmake or CMake) and pass `test-uws` locally.

## Security & Configuration Tips
- SSL: tests use WolfSSL; local keys (`cert.pem`, `key.pem`) are for development—do not commit secrets.
- Platform notes: Windows links system libs; Linux may require `-mcx16` and pthread; prefer xmake presets.
