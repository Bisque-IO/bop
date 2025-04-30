package bop.c.mdbx;

import java.lang.foreign.MemoryLayout;
import java.lang.foreign.ValueLayout;

public interface VersionInfo {

  /// extern LIBMDBX_VERINFO_API const struct MDBX_version_info {
  ///   uint16_t major;                /**< Major version number */
  ///   uint16_t minor;                /**< Minor version number */
  ///   uint16_t patch;                /**< Patch number */
  ///   uint16_t tweak;                /**< Tweak number */
  ///   const char *semver_prerelease; /**< Semantic Versioning `pre-release` */
  ///   struct {
  ///     const char *datetime; /**< committer date, strict ISO-8601 format */
  ///     const char *tree;     /**< commit hash (hexadecimal digits) */
  ///     const char *commit;   /**< tree hash, i.e. digest of the source code */
  ///     const char *describe; /**< git-describe string */
  ///   } git;                  /**< source information from git */
  ///   const char *sourcery;   /**< sourcery anchor for pinning */
  /// } /** \brief libmdbx version information */ mdbx_version;
  MemoryLayout LAYOUT = MemoryLayout.structLayout(
          ValueLayout.JAVA_CHAR.withName("major"),
          ValueLayout.JAVA_CHAR.withName("minor"),
          ValueLayout.JAVA_CHAR.withName("patch"),
          ValueLayout.JAVA_CHAR.withName("tweak"),
          ValueLayout.ADDRESS.withName("semver_prerelease"),
          ValueLayout.ADDRESS.withName("datetime"),
          ValueLayout.ADDRESS.withName("tree"),
          ValueLayout.ADDRESS.withName("commit"),
          ValueLayout.ADDRESS.withName("describe"),
          ValueLayout.ADDRESS.withName("sourcery"))
      .withName("MDBX_version_info");
}
