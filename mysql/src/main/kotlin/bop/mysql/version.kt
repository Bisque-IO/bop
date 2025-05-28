package bop.mysql

import java.util.Locale

/**
 *
 */
data class MySQLVersion(
   val major: Int,
   val minor: Int,
   val patch: Int = 0,
   val preRelease: String = "",
   val buildMetadata: String,
   val serverVersion: String = ""
) {
   val isMariaDb: Boolean = serverVersion.lowercase(Locale.getDefault()).contains("mariadb")

   fun isGreaterThan(
      major: Int,
      minor: Int,
      patch: Int = -1
   ): Boolean {
      return if (patch < 0) {
         this.major > major || (this.major == major && this.minor > minor)
      } else {
         this.major > major || (this.major == major && this.minor > minor && this.patch > patch)
      }
   }

   fun isEqualTo(
      major: Int,
      minor: Int,
      patch: Int = -1
   ): Boolean {
      return if (patch < 0) {
         this.major == major && this.minor == minor
      } else {
         this.major == major && this.minor == minor && this.patch == patch
      }
   }

   fun isGreaterThanOrEqualTo(
      major: Int,
      minor: Int,
      patch: Int = -1
   ): Boolean {
      return isGreaterThan(major, minor, patch) || isEqualTo(major, minor, patch)
   }

   companion object {
      val regex =
         Regex("""^(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$""")

      @JvmStatic
      fun parse(serverVersion: String): MySQLVersion {
         val match = regex.matchEntire(serverVersion) ?: return MySQLVersion(
            -1, -1, -1, "", "", serverVersion
         )

         val (major, minor, patch, preRelease, buildMetadata) = match.destructured

         return MySQLVersion(
            major = major.toInt(),
            minor = if (minor.isBlank()) 0 else minor.toInt(),
            patch = if (patch.isBlank()) 0 else patch.toInt(),
            preRelease = preRelease.ifBlank { "" },
            buildMetadata = buildMetadata.ifBlank { "" },
            serverVersion = serverVersion
         )
      }
   }
}