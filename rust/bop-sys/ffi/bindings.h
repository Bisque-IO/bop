// Minimal umbrella header for bindgen.
// Adjust includes to the exact C-compatible headers you want exposed.
// The build script adds the project include path: ../../lib/src
// Include your public C ABI surface here:
//   e.g. #include "lib.h" (exports BOP_API functions)

#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// Public C API for bop
#include "lib.h"
#include "raft.h"
#include "alloc.h"
#include "hash.h"
#include "mpmc.h"
#include "uws.h"

#ifdef __cplusplus
}
#endif
