#ifndef BOP_LIB_H
#define BOP_LIB_H

#ifndef BOP_API
#  ifdef __cplusplus
#    define BOP_EXTERN extern "C"
#  else
#    define BOP_EXTERN
#  endif
#  ifdef _WIN32
#    define BOP_API BOP_EXTERN __declspec(dllexport)
#  else
#    define BOP_API BOP_EXTERN
#  endif
#endif

// #ifdef _WIN32
// #define BOP_API __declspec(dllexport)
// #else
// #define BOP_API
// #endif

#ifdef __cplusplus
extern "C" {
#endif

#include <inttypes.h>
#include <stddef.h>

#ifdef __cplusplus
}
#endif

#endif
