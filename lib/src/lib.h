#ifndef BOP_LIB_H
#define BOP_LIB_H

#ifdef _WIN32
#define BOP_API __declspec(dllexport)
#else
#define BOP_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

#include <inttypes.h>
#include <stddef.h>

#ifdef __cplusplus
}
#endif

#endif
