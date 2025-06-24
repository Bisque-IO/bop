#include <snmalloc/snmalloc.h>
#include <snmalloc/pal/pal_consts.h>
#include <snmalloc/mem/sizeclasstable.h>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <memory>

#ifdef _WIN32
#define BOP_API __declspec(dllexport)
#else
#define BOP_API
#endif