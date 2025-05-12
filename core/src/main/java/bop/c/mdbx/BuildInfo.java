package bop.c.mdbx;

import bop.c.Memory;
import bop.unsafe.Danger;

import java.lang.foreign.MemoryLayout;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;

public record BuildInfo(
  String datetime, String target, String options, String compiler, String flags, String metadata) {
  public static final BuildInfo INSTANCE = load();

  public static BuildInfo get() {
    return INSTANCE;
  }

  static BuildInfo load() {
    /// /** \brief libmdbx build information
    ///  * \attention Some strings could be NULL in case no corresponding information
    ///  *            was provided at build time (i.e. flags). */
    /// extern LIBMDBX_VERINFO_API const struct MDBX_build_info {
    ///   const char *datetime; /**< build timestamp (ISO-8601 or __DATE__ __TIME__) */
    ///   const char *target;   /**< cpu/arch-system-config triplet */
    ///   const char *options;  /**< mdbx-related options */
    ///   const char *compiler; /**< compiler */
    ///   const char *flags;    /**< CFLAGS and CXXFLAGS */
    ///   const char *metadata; /**< an extra/custom information provided via
    ///                              the MDBX_BUILD_METADATA definition
    ///                              during library build */
    /// } /** \brief libmdbx build information */ mdbx_build;
    final var address = Memory.zalloc(48L);
    try {
      try {
        CFunctions.BOP_MDBX_BUILD_INFO.invokeExact(address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }

      final var LIMIT = 4096;

      final var datetime =
        Memory.fromCString(Danger.getLong(address), LIMIT, StandardCharsets.UTF_8);
      final var target =
        Memory.fromCString(Danger.getLong(address + 8L), LIMIT, StandardCharsets.UTF_8);
      final var options =
        Memory.fromCString(Danger.getLong(address + 16L), LIMIT, StandardCharsets.UTF_8);
      final var compiler =
        Memory.fromCString(Danger.getLong(address + 24L), LIMIT, StandardCharsets.UTF_8);
      final var flags =
        Memory.fromCString(Danger.getLong(address + 32L), LIMIT, StandardCharsets.UTF_8);
      final var metadata =
        Memory.fromCString(Danger.getLong(address + 40L), LIMIT, StandardCharsets.UTF_8);

      return new BuildInfo(datetime, target, options, compiler, flags, metadata);
    } finally {
      Memory.dealloc(address);
    }
  }
}
