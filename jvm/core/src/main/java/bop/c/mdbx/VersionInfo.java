package bop.c.mdbx;

import bop.c.Memory;
import bop.unsafe.Danger;
import java.nio.charset.StandardCharsets;

public record VersionInfo(
    int major,
    int minor,
    int patch,
    int tweak,
    String semverPrerelease,
    String datetime,
    String tree,
    String commit,
    String describe,
    String sourcery) {
  public static final VersionInfo INSTANCE = load();

  public static VersionInfo get() {
    return INSTANCE;
  }

  static VersionInfo load() {
    /// extern LIBMDBX_VERINFO_API const struct MDBX_version_info {
    ///   uint16_t major;                /**< Major version number */
    ///   uint16_t minor;                /**< Minor version number */
    ///   uint16_t patch;                /**< Patch number */
    ///   uint16_t tweak;                /**< Tweak number */
    ///   const char *semver_prerelease; /**< Semantic Versioning `prerelease` */
    ///   struct {
    ///     const char *datetime; /**< committer date, strict ISO-8601 format */
    ///     const char *tree;     /**< commit hash (hexadecimal digits) */
    ///     const char *commit;   /**< tree hash, i.e. digest of the source code */
    ///     const char *describe; /**< git-describe string */
    ///   } git;                  /**< source information from git */
    ///   const char *sourcery;   /**< sourcery anchor for pinning */
    /// } /** \brief libmdbx version information */ mdbx_version;
    final var address = Memory.zalloc(64L);
    try {
      try {
        CFunctions.BOP_MDBX_VERSION.invokeExact(address);
      } catch (Throwable e) {
        throw new RuntimeException(e);
      }

      final var LIMIT = 4096;

      final var major = (int) Danger.getChar(address);
      final var minor = (int) Danger.getChar(address + 2L);
      final var patch = (int) Danger.getChar(address + 4L);
      final var tweak = (int) Danger.getChar(address + 6L);
      final var semverPrerelease =
          Memory.fromCString(Danger.getLong(address + 8L), LIMIT, StandardCharsets.UTF_8);
      final var datetime =
          Memory.fromCString(Danger.getLong(address + 16L), LIMIT, StandardCharsets.UTF_8);
      final var tree =
          Memory.fromCString(Danger.getLong(address + 24L), LIMIT, StandardCharsets.UTF_8);
      final var commit =
          Memory.fromCString(Danger.getLong(address + 32L), LIMIT, StandardCharsets.UTF_8);
      final var describe =
          Memory.fromCString(Danger.getLong(address + 40L), LIMIT, StandardCharsets.UTF_8);
      final var sourcery =
          Memory.fromCString(Danger.getLong(address + 48L), LIMIT, StandardCharsets.UTF_8);

      return new VersionInfo(
          major, minor, patch, tweak, semverPrerelease, datetime, tree, commit, describe, sourcery);
    } finally {
      Memory.dealloc(address);
    }
  }
}
