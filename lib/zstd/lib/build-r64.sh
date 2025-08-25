# Build only the static library with your cross tools
make -j$(nproc) \
  libzstd.a \
  CC=riscv64-linux-gnu-gcc \
  AR=riscv64-linux-gnu-ar \
  RANLIB=riscv64-linux-gnu-ranlib \
  CFLAGS="-O3 -fPIC --sysroot=/usr/riscv64-linux-gnu"
