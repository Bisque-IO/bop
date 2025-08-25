# Build only the static library with your cross tools
make -j$(nproc) \
  libzstd.a \
  CC=aarch64-linux-gnu-gcc \
  AR=aarch64-linux-gnu-ar \
  RANLIB=aarch64-linux-gnu-ranlib \
  CFLAGS="-O3 -fPIC --sysroot=/usr/aarch64-linux-gnu"
