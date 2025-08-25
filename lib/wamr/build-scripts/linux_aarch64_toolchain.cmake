set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR aarch64)

# Specify the cross-compiler
set(CMAKE_C_COMPILER /usr/bin/aarch64-linux-gnu-gcc)
set(CMAKE_CXX_COMPILER /usr/bin/aarch64-linux-gnu-g++)
set(CMAKE_AR /usr/bin/aarch64-linux-gnu-ar CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB /usr/bin/aarch64-linux-gnu-ranlib CACHE FILEPATH "Ranlib")

# Specify the sysroot
#set(CMAKE_SYSROOT /usr/aarch64-linux-gnu)

# Search for libraries and headers in the sysroot
set(CMAKE_FIND_ROOT_PATH /usr/aarch64-linux-gnu)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)

set(CMAKE_THREAD_LIBS_INIT "-lpthread")
set(CMAKE_HAVE_THREADS_LIBRARY 1)
set(Threads_FOUND TRUE)

# Optional: Compiler flags
#set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -march=armv8-a")
#set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -march=armv8-a")