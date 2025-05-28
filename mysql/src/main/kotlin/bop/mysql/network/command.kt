package bop.mysql.network

import bop.mysql.CommandType
import bop.mysql.binlog.GtidSet
import bop.mysql.io.ByteArrayOutputStream
import java.io.IOException
import java.security.DigestException
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.NoSuchAlgorithmException
import java.security.interfaces.RSAPublicKey
import java.security.spec.X509EncodedKeySpec
import java.util.*
import javax.crypto.Cipher

interface Command : Packet {
   @Throws(IOException::class)
   fun toByteArray(): ByteArray

   @Throws(IOException::class)
   fun writeTo(out: ByteArrayOutputStream)
}

class AuthenticateNativePasswordCommand(
   private val scramble: String,
   private val password: String
) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      return AuthenticateSecurityPasswordCommand.Companion.passwordCompatibleWithMySQL411(
         password,
         scramble
      )
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.write(
         AuthenticateSecurityPasswordCommand.Companion.passwordCompatibleWithMySQL411(
            password,
            scramble
         )
      )
   }
}


class AuthenticateSecurityPasswordCommand(
   val schema: String?,
   val username: String?,
   val password: String?,
   val salt: String,
   var collation: Int,
   var clientCapabilities: Int = 0
) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      var clientCapabilities = this.clientCapabilities
      if (clientCapabilities == 0) {
         clientCapabilities =
            ClientCapabilities.LONG_FLAG or ClientCapabilities.PROTOCOL_41 or ClientCapabilities.SECURE_CONNECTION or ClientCapabilities.PLUGIN_AUTH

         if (schema != null) {
            clientCapabilities = clientCapabilities or ClientCapabilities.CONNECT_WITH_DB
         }
      }
      out.writeInteger(clientCapabilities, 4)
      out.writeInteger(0, 4) // maximum packet length
      out.writeInteger(collation, 1)
      for (i in 0..22) {
         out.write(0)
      }
      out.writeZeroTerminatedString(username)
      val passwordSHA1: ByteArray = passwordCompatibleWithMySQL411(password, salt)
      out.writeInteger(passwordSHA1.size, 1)
      out.write(passwordSHA1)
      if (schema != null) {
         out.writeZeroTerminatedString(schema)
      }
      out.writeZeroTerminatedString("mysql_native_password")
   }

   companion object {
      /**
       * see mysql/sql/password.c scramble(...)
       * @param password the password
       * @param salt salt received from the server
       * @return hashed password
       */
      fun passwordCompatibleWithMySQL411(
         password: String?,
         salt: String
      ): ByteArray {
         if ("" == password || password == null) return ByteArray(0)

         val sha: MessageDigest
         try {
            sha = MessageDigest.getInstance("SHA-1")
         } catch (e: NoSuchAlgorithmException) {
            throw RuntimeException(e)
         }
         val passwordHash = sha.digest(password.toByteArray())
         return CommandUtils.xor(
            passwordHash,
            sha.digest(union(salt.toByteArray(), sha.digest(passwordHash)))
         )
      }

      private fun union(
         a: ByteArray,
         b: ByteArray
      ): ByteArray {
         val r = ByteArray(a.size + b.size)
         System.arraycopy(a, 0, r, 0, a.size)
         System.arraycopy(b, 0, r, a.size, b.size)
         return r
      }
   }
}

class AuthenticateSHA2Command : Command {
   private var schema: String? = null
   private var username: String? = null
   private val password: String?
   private val scramble: String
   private var clientCapabilities = 0
   private var collation = 0
   private var rawPassword = false

   constructor(
      schema: String?,
      username: String?,
      password: String?,
      scramble: String,
      collation: Int
   ) {
      this.schema = schema
      this.username = username
      this.password = password
      this.scramble = scramble
      this.collation = collation
   }

   constructor(
      scramble: String,
      password: String?
   ) {
      this.rawPassword = true
      this.password = password
      this.scramble = scramble
   }

   fun setClientCapabilities(clientCapabilities: Int) {
      this.clientCapabilities = clientCapabilities
   }

   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      if (rawPassword) {
         val passwordSHA1 = encodePassword()
         out.write(passwordSHA1)
         return
      }

      var clientCapabilities = this.clientCapabilities
      if (clientCapabilities == 0) {
         clientCapabilities = clientCapabilities or ClientCapabilities.LONG_FLAG
         clientCapabilities = clientCapabilities or ClientCapabilities.PROTOCOL_41
         clientCapabilities = clientCapabilities or ClientCapabilities.SECURE_CONNECTION
         clientCapabilities = clientCapabilities or ClientCapabilities.PLUGIN_AUTH
         clientCapabilities =
            clientCapabilities or ClientCapabilities.PLUGIN_AUTH_LENENC_CLIENT_DATA

         if (schema != null) {
            clientCapabilities = clientCapabilities or ClientCapabilities.CONNECT_WITH_DB
         }
      }
      out.writeInteger(clientCapabilities, 4)
      out.writeInteger(0, 4) // maximum packet length
      out.writeInteger(collation, 1)
      for (i in 0..22) {
         out.write(0)
      }
      out.writeZeroTerminatedString(username)
      val passwordSHA1 = encodePassword()
      out.writeInteger(passwordSHA1.size, 1)
      out.write(passwordSHA1)
      if (schema != null) {
         out.writeZeroTerminatedString(schema)
      }
      out.writeZeroTerminatedString("caching_sha2_password")
   }

   private fun encodePassword(): ByteArray {
      if (password == null || "" == password) {
         return ByteArray(0)
      }
      // caching_sha2_password
      /*
         * Server does it in 4 steps (see sql/auth/sha2_password_common.cc Generate_scramble::scramble method):
         *
         * SHA2(src) => digest_stage1
         * SHA2(digest_stage1) => digest_stage2
         * SHA2(digest_stage2, m_rnd) => scramble_stage1
         * XOR(digest_stage1, scramble_stage1) => scramble
         */
      val md: MessageDigest
      try {
         md = MessageDigest.getInstance("SHA-256")

         val CACHING_SHA2_DIGEST_LENGTH = 32
         val dig1 = ByteArray(CACHING_SHA2_DIGEST_LENGTH)
         val dig2 = ByteArray(CACHING_SHA2_DIGEST_LENGTH)
         val scramble1 = ByteArray(CACHING_SHA2_DIGEST_LENGTH)

         // SHA2(src) => digest_stage1
         md.update(password.toByteArray(), 0, password.toByteArray().size)
         md.digest(dig1, 0, CACHING_SHA2_DIGEST_LENGTH)
         md.reset()

         // SHA2(digest_stage1) => digest_stage2
         md.update(dig1, 0, dig1.size)
         md.digest(dig2, 0, CACHING_SHA2_DIGEST_LENGTH)
         md.reset()

         // SHA2(digest_stage2, m_rnd) => scramble_stage1
         md.update(dig2, 0, dig1.size)
         md.update(scramble.toByteArray(), 0, scramble.toByteArray().size)
         md.digest(scramble1, 0, CACHING_SHA2_DIGEST_LENGTH)

         // XOR(digest_stage1, scramble_stage1) => scramble
         return CommandUtils.xor(dig1, scramble1)
      } catch (ex: NoSuchAlgorithmException) {
         throw RuntimeException(ex)
      } catch (e: DigestException) {
         throw RuntimeException(e)
      }
   }
}

class AuthenticateSHA2RSAPasswordCommand(
   private val rsaKey: String,
   private val password: String?,
   private val scramble: String
) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val key = decodeKey(rsaKey)

      val passBuffer = ByteArrayOutputStream()
      passBuffer.writeZeroTerminatedString(password)

      val xorBuffer = CommandUtils.xor(passBuffer.toByteArray(), scramble.toByteArray())
      return encrypt(xorBuffer, key, RSA_METHOD)
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.write(toByteArray())
   }

   @Throws(AuthenticationException::class)
   private fun decodeKey(key: String): RSAPublicKey? {
      val beginIndex = key.indexOf("\n") + 1
      val endIndex = key.indexOf("-----END PUBLIC KEY-----")
      val innerKey = key.substring(beginIndex, endIndex).replace("\\n".toRegex(), "")

      val decoder = Base64.getDecoder()
      val certificateData = decoder.decode(innerKey.toByteArray())

      val spec = X509EncodedKeySpec(certificateData)
      try {
         val kf = KeyFactory.getInstance("RSA")
         return kf.generatePublic(spec) as RSAPublicKey?
      } catch (e: Exception) {
         throw AuthenticationException("Unable to decode public key: " + key)
      }
   }

   @Throws(AuthenticationException::class)
   private fun encrypt(
      source: ByteArray,
      key: RSAPublicKey?,
      transformation: String
   ): ByteArray {
      try {
         val cipher = Cipher.getInstance(transformation)
         cipher.init(Cipher.ENCRYPT_MODE, key)
         return cipher.doFinal(source)
      } catch (e: Exception) {
         throw AuthenticationException("couldn't encrypt password: " + e.message)
      }
   }

   companion object {
      private const val RSA_METHOD = "RSA/ECB/OAEPWithSHA-1AndMGF1Padding"
   }
}

class ByteArrayCommand(private val command: ByteArray) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      return command
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.write(command)
   }
}

object CommandUtils {
   fun xor(
      input: ByteArray,
      against: ByteArray
   ): ByteArray {
      val to = ByteArray(input.size)

      for (i in input.indices) {
         to[i] = (input[i].toInt() xor against[i % against.size].toInt()).toByte()
      }
      return to
   }
}

class DumpBinaryLogCommand(
   private val serverId: Long,
   private val binlogFilename: String?,
   private val binlogPosition: Long
) : Command {
   private var sendAnnotateRowsEvent = false

   constructor(
      serverId: Long,
      binlogFilename: String?,
      binlogPosition: Long,
      sendAnnotateRowsEvent: Boolean
   ) : this(serverId, binlogFilename, binlogPosition) {
      this.sendAnnotateRowsEvent = sendAnnotateRowsEvent
   }

   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.writeInteger(CommandType.BINLOG_DUMP.ordinal, 1)
      out.writeLong(this.binlogPosition, 4)
      var binlogFlags = 0
      if (sendAnnotateRowsEvent) {
         binlogFlags = binlogFlags or BINLOG_SEND_ANNOTATE_ROWS_EVENT
      }
      out.writeInteger(binlogFlags, 2) // flag
      out.writeLong(this.serverId, 4)
      out.writeString(this.binlogFilename ?: "")
   }

   companion object {
      const val BINLOG_SEND_ANNOTATE_ROWS_EVENT: Int = 2
   }
}

class DumpBinaryLogGtidCommand(
   private val serverId: Long,
   private val binlogFilename: String?,
   private val binlogPosition: Long,
   private val gtidSet: GtidSet
) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.writeInteger(CommandType.BINLOG_DUMP_GTID.ordinal, 1)
      out.writeInteger(0, 2) // flag
      out.writeLong(this.serverId, 4)
      out.writeInteger(this.binlogFilename?.length ?: 0, 4)
      out.writeString(this.binlogFilename ?: "")
      out.writeLong(this.binlogPosition, 8)
      val uuidSets = gtidSet.uuidSets
      var dataSize = 8 /* number of uuidSets */
      for (uuidSet in uuidSets) {
         dataSize += 16 /* uuid */ + 8 /* number of intervals */ + uuidSet.intervals.size /* number of intervals */ * 16 /* start-end */
      }
      out.writeInteger(dataSize, 4)
      out.writeLong(uuidSets.size.toLong(), 8)
      for (uuidSet in uuidSets) {
         out.write(hexToByteArray(uuidSet.uUID!!.replace("-", "")))
         val intervals: MutableCollection<GtidSet.Interval> = uuidSet.intervals
         out.writeLong(intervals.size.toLong(), 8)
         for (interval in intervals) {
            out.writeLong(interval.start, 8)
            out.writeLong(interval.end + 1,  /* right-open */8)
         }
      }
   }

   companion object {
      private fun hexToByteArray(uuid: String): ByteArray {
         val b = ByteArray(uuid.length / 2)
         var i = 0
         var j = 0
         while (j < uuid.length) {
            b[i++] = (uuid.get(j).toString() + "" + uuid.get(j + 1)).toInt(16).toByte()
            j += 2
         }
         return b
      }
   }
}

class PingCommand : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.writeInteger(CommandType.PING.ordinal, 1)
   }
}

class QueryCommand(private val sql: String) : Command {
   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      out.writeInteger(CommandType.QUERY.ordinal, 1)
      out.writeString(sql)
   }
}

class SSLRequestCommand : Command {
   private var clientCapabilities = 0
   private var collation = 0

   fun setClientCapabilities(clientCapabilities: Int) {
      this.clientCapabilities = clientCapabilities
   }

   fun setCollation(collation: Int) {
      this.collation = collation
   }

   @Throws(IOException::class)
   override fun toByteArray(): ByteArray {
      val buffer = ByteArrayOutputStream()
      writeTo(buffer)
      return buffer.toByteArray()
   }

   override fun writeTo(out: ByteArrayOutputStream) {
      var clientCapabilities = this.clientCapabilities
      if (clientCapabilities == 0) {
         clientCapabilities =
            ClientCapabilities.LONG_FLAG or ClientCapabilities.PROTOCOL_41 or ClientCapabilities.SECURE_CONNECTION or ClientCapabilities.PLUGIN_AUTH
      }
      clientCapabilities = clientCapabilities or ClientCapabilities.SSL
      out.writeInteger(clientCapabilities, 4)
      out.writeInteger(0, 4) // maximum packet length
      out.writeInteger(collation, 1)
      for (i in 0..22) {
         out.write(0)
      }
   }
}
