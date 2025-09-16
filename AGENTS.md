# Agent Guidelines for bop Repository

## Build Commands
- Build: `xmake f -m release && xmake`
- Run single test: `xmake run test-uws`
- Alternative build: `cmake -S . -B build && cmake --build build -j`
- Format code: `clang-format -i file.cpp`

## Code Style
- Language: C/C++23, format with `.clang-format`
- Files: `snake_case.cpp/h`, types `PascalCase`, functions/vars `lower_snake_case`, constants `UPPER_SNAKE_CASE`
- Imports: relative paths in `lib/src/**`, minimal includes
- Error handling: check returns, prefer explicit error codes over exceptions

## Testing
- Location: `tests/`, custom runner `TestRunner.cpp`
- Test files: `NameTest.cpp`, mirror existing patterns
- Run: `xmake run test-uws` or CTest in tests/build

## Project Structure
- Core: `lib/src/` (uws, raft, mpmc, spsc, alloc, hashing)
- Vendor: `lib/` (usockets, libuv, wolfssl, snmalloc, mdbx)
- Config: `xmake.lua`, `CMakeLists.txt`
