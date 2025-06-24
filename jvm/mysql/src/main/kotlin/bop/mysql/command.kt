package bop.mysql

import bop.mysql.binlog.GtidSet
import bop.mysql.network.AuthenticationException
import bop.unsafe.Danger
import java.security.DigestException
import java.security.KeyFactory
import java.security.MessageDigest
import java.security.NoSuchAlgorithmException
import java.security.interfaces.RSAPublicKey
import java.security.spec.X509EncodedKeySpec
import java.util.*
import javax.crypto.Cipher

/**
 *
 */
enum class CommandType {
   /**
    * Internal server command.
    */
   SLEEP,

   /**
    * Used to inform the server that the client wants to close the connection.
    */
   QUIT,

   /**
    * Used to change the default schema of the connection.
    */
   INIT_DB,

   /**
    * Used to send the server a text-based query that is executed immediately.
    */
   QUERY,

   /**
    * Used to get column definitions of the specific table.
    */
   FIELD_LIST,

   /**
    * Used to create a new schema.
    */
   CREATE_DB,

   /**
    * Used to drop existing schema.
    */
   DROP_DB,

   /**
    * A low-level version of several FLUSH ... and RESET ... commands.
    */
   REFRESH,

   /**
    * Used to shut down the mysql-server.
    */
   SHUTDOWN,

   /**
    * Used to get a human-readable string of internal statistics.
    */
   STATISTICS,

   /**
    * Used to get a list of active threads.
    */
   PROCESS_INFO,

   /**
    * Internal server command.
    */
   CONNECT,

   /**
    * Used to ask the server to terminate the connection.
    */
   PROCESS_KILL,

   /**
    * Triggers a dump on internal debug info to stdout of the mysql-server.
    */
   DEBUG,

   /**
    * Used to check if the server is alive.
    */
   PING,

   /**
    * Internal server command.
    */
   TIME,

   /**
    * Internal server command.
    */
   DELAYED_INSERT,

   /**
    * Used to change the user of the current connection and reset the connection state.
    */
   CHANGE_USER,

   /**
    * Requests a binary log network stream from the master starting a given position.
    */
   BINLOG_DUMP,

   /**
    * Used to dump a specific table.
    */
   TABLE_DUMP,

   /**
    * Internal server command.
    */
   CONNECT_OUT,

   /**
    * Registers a slave at the master. Should be sent before requesting a binary log event with [.BINLOG_DUMP].
    */
   REGISTER_SLAVE,

   /**
    * Creates a prepared statement from the passed query string.
    */
   STMT_PREPARE,

   /**
    * Used to execute a prepared statement as identified by statement id.
    */
   STMT_EXECUTE,

   /**
    * Used to send some data for a column.
    */
   STMT_SEND_LONG_DATA,

   /**
    * Deallocates a prepared statement.
    */
   STMT_CLOSE,

   /**
    * Resets the data of a prepared statement which was accumulated with [.STMT_SEND_LONG_DATA] commands.
    */
   STMT_RESET,

   /**
    * Allows enabling and disable [ClientCapabilities.MULTI_STATEMENTS]
    * for the current connection.
    */
   SET_OPTION,

   /**
    * Fetch a row from an existing resultset after a [.STMT_EXECUTE].
    */
   STMT_FETCH,

   /**
    * Internal server command.
    */
   DAEMON,

   /**
    * Used to request the binary log network stream based on a GTID.
    */
   BINLOG_DUMP_GTID
}

fun MySQLConnection.send(block: (out: MySQLBuffer) -> Unit) {
   // save the first 4 bytes for the packet header
   sendPacket.reset().writeInt(0)

   // build command
   block.invoke(sendPacket)

   // set length
   sendPacket.putInt24LE(0, sendPacket.size - 4)

   // see https://dev.mysql.com/doc/dev/mysql-server/8.0.11/page_protocol_basic_packets.html#sect_protocol_basic_packets_sequence_id
   // we only have to maintain a sequence number in the authentication phase.
   // what the point is, I do not know
   if (authenticationComplete) {
      packetSequence = 0.toUByte()
   }

   // set sequence
   sendPacket.putByte(3, packetSequence++.toInt())

   command = sendPacket.getUByte(4)

   // flush to socket
   outputStream.write(sendPacket.base as ByteArray, 0, sendPacket.size)
   outputStream.flush()
}

fun MySQLConnection.send(data: ByteArray) {
   send { it.writeBytes(data) }
}

fun MySQLConnection.send(data: Int) {
   send { it.writeByte(data) }
}

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
   if (password.isNullOrEmpty()) return ByteArray(0)

   val sha: MessageDigest
   try {
      sha = MessageDigest.getInstance("SHA-1")
   } catch (e: NoSuchAlgorithmException) {
      throw RuntimeException(e)
   }
   val passwordHash = sha.digest(password.toByteArray())
   return CommandUtils.xor(
      passwordHash, sha.digest(union(salt.toByteArray(), sha.digest(passwordHash)))
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

internal fun MySQLConnection.sendAuthenticateSecurityPassword(
   schema: String? = null,
   username: String,
   password: String,
   salt: String,
   collation: Int
) {
   send { out ->
      if (clientCapabilities == 0) {
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_LONG_FLAG
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PROTOCOL_41
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_SECURE_CONNECTION
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PLUGIN_AUTH

         if (schema != null) {
            clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_CONNECT_WITH_DB
         }
      }

      out.writeIntLE(clientCapabilities, 4)
      out.writeIntLE(0, 4)
      out.writeIntLE(collation, 1)
      for (i in 0..22) {
         out.writeByte(0)
      }
      out.writeCString(username)
      val passwordSHA1: ByteArray = passwordCompatibleWithMySQL411(
         password, salt
      )
      out.writeIntLE(passwordSHA1.size, 1)
      out.writeBytes(passwordSHA1)
      if (!schema.isNullOrEmpty()) {
         out.writeCString(schema)
      }
      out.writeCString("mysql_native_password")
   }
}


private fun encodePassword(
   password: String?,
   scramble: String
): ByteArray {
   if (password.isNullOrBlank()) {
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

      val passwordBytes = Danger.getBytes(password)
      val scrambleBytes = Danger.getBytes(scramble)

      // SHA2(src) => digest_stage1
      md.update(passwordBytes, 0, passwordBytes.size)
      md.digest(dig1, 0, CACHING_SHA2_DIGEST_LENGTH)
      md.reset()

      // SHA2(digest_stage1) => digest_stage2
      md.update(dig1, 0, dig1.size)
      md.digest(dig2, 0, CACHING_SHA2_DIGEST_LENGTH)
      md.reset()

      // SHA2(digest_stage2, m_rnd) => scramble_stage1
      md.update(dig2, 0, dig1.size)
      md.update(scrambleBytes, 0, scrambleBytes.size)
      md.digest(scramble1, 0, CACHING_SHA2_DIGEST_LENGTH)

      // XOR(digest_stage1, scramble_stage1) => scramble
      return CommandUtils.xor(dig1, scramble1)
   } catch (ex: NoSuchAlgorithmException) {
      throw RuntimeException(ex)
   } catch (e: DigestException) {
      throw RuntimeException(e)
   }
}

internal fun MySQLConnection.sendAuthenticateSHA2(
   schema: String? = null,
   username: String? = null,
   password: String? = null,
   scramble: String,
   collation: Int = 0,
   rawPassword: Boolean = false
) {
   send { out ->
      if (rawPassword) {
         val passwordSHA1 = encodePassword(password, scramble)
         out.writeBytes(passwordSHA1)
         return@send
      }

      if (clientCapabilities == 0) {
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_LONG_FLAG
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PROTOCOL_41
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_SECURE_CONNECTION
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PLUGIN_AUTH
//         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_OPTIONAL_RESULTSET_METADATA
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_DEPRECATE_EOF
//         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_QUERY_ATTRIBUTES
         clientCapabilities =
            clientCapabilities or ClientCapabilities.CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA

         if (schema != null) {
            clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_CONNECT_WITH_DB
         }
      }

      out.writeIntLE(clientCapabilities)
      out.writeIntLE(0) // maximum packet length
      out.writeByte(collation)
      for (i in 0..22) {
         out.writeByte(0)
      }
      out.writeCString(username)
      val passwordSHA1 = encodePassword(password = password, scramble = scramble)
      out.writeByte(passwordSHA1.size)
      out.writeBytes(passwordSHA1)
      if (schema != null) {
         out.writeCString(schema)
      }
      out.writeCString("caching_sha2_password")
   }
}

internal fun MySQLConnection.sendAuthenticateNativePassword(
   password: String,
   scramble: String
) {
   send { out ->
      out.writeBytes(passwordCompatibleWithMySQL411(password, scramble))
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

private const val RSA_METHOD = "RSA/ECB/OAEPWithSHA-1AndMGF1Padding"

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

internal fun MySQLConnection.sendAuthenticateSHA2RSAPassword(
   rsaKey: String,
   password: String,
   scramble: String
) {
   send { out ->
      val key = decodeKey(rsaKey)
      val passwordBytes = toCStringBytes(password)
      val xorBytes = CommandUtils.xor(passwordBytes, scramble.toByteArray())
      out.writeBytes(encrypt(xorBytes, key, RSA_METHOD))
   }
}

const val BINLOG_SEND_ANNOTATE_ROWS_EVENT: Int = 2

internal fun MySQLConnection.sendDumpBinaryLog(
   serverId: Long,
   binlogFilename: String?,
   binlogPosition: Long,
   sendAnnotateRowsEvent: Boolean = false
) {
   send { out ->
      out.writeIntLE(CommandType.BINLOG_DUMP.ordinal, 1)
      out.writeLongLE(this.binlogPosition, 4)
      var binlogFlags = 0
      if (sendAnnotateRowsEvent) {
         binlogFlags = binlogFlags or BINLOG_SEND_ANNOTATE_ROWS_EVENT
      }
      out.writeIntLE(binlogFlags, 2)
      out.writeLongLE(this.serverId, 4)
      out.writeString(this.binlogFilename ?: "")
   }
}

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

internal fun MySQLConnection.sendDumpBinaryLogGtid(
   serverId: Long,
   binlogFilename: String?,
   binlogPosition: Long,
   gtidSet: GtidSet
) {
   send { out ->
      out.writeUByte(CommandType.BINLOG_DUMP_GTID.ordinal)
      out.writeCharLE(0) // flag
      out.writeLongLE(this.serverId, 4)
      out.writeIntLE(this.binlogFilename?.length ?: 0)
      out.writeString(this.binlogFilename ?: "")
      out.writeLongLE(this.binlogPosition)
      val uuidSets = gtidSet.uuidSets
      var dataSize = 8 /* number of uuidSets */
      for (uuidSet in uuidSets) {
         dataSize += 16 /* uuid */ + 8 /* number of intervals */ + uuidSet.intervals.size /* number of intervals */ * 16 /* start-end */
      }
      out.writeIntLE(dataSize)
      out.writeLongLE(uuidSets.size.toLong())
      for (uuidSet in uuidSets) {
         out.writeBytes(hexToByteArray(uuidSet.uUID!!.replace("-", "")))
         val intervals: MutableCollection<GtidSet.Interval> = uuidSet.intervals
         out.writeLongLE(intervals.size.toLong())
         for (interval in intervals) {
            out.writeLongLE(interval.start)
            out.writeLongLE(interval.end + 1)
         }
      }
   }
}

internal fun MySQLConnection.sendSSLRequest(
   collation: Int = 0
) {
   send { out ->
      if (clientCapabilities == 0) {
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_LONG_FLAG
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PROTOCOL_41
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_SECURE_CONNECTION
         clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_PLUGIN_AUTH
      }
      clientCapabilities = clientCapabilities or ClientCapabilities.CLIENT_SSL
      out.writeIntLE(clientCapabilities)
      out.writeIntLE(0) // maximum packet length
      out.writeByte(collation)
      for (i in 0..22) {
         out.writeByte(0)
      }
   }
}

internal fun MySQLConnection.sendPing() {
   send { out ->
      out.writeIntLE(CommandType.PING.ordinal, 1)
   }
}

internal fun MySQLConnection.sendQuery(sql: String) {
   send { out ->
      out.writeUByte(CommandType.QUERY.ordinal)
      out.writeString(sql)
   }
}

internal fun MySQLConnection.sendPrepare(sql: String) {
   send { out ->
      out.writeUByte(CommandType.STMT_PREPARE.ordinal)
      out.writeString(sql)
   }
}

/**
 * COM_STMT_EXECUTE asks the server to execute a prepared statement as identified by statement_id.
 *
 * It sends the values for the placeholders of the prepared statement (if it contained any) in
 * Binary Protocol Value form. The type of each parameter is made up of two bytes:
 *
 * the type as in enum_field_types
 * a flag byte which has the highest bit set if the type is unsigned [80]
 * The num_params used for this packet refers to num_params of the COM_STMT_PREPARE_OK of the
 * corresponding prepared statement.
 *
 * The server will use the first num_params (from prepare) parameter values to satisfy the
 * positional anonymous question mark parameters in the statement executed regardless of
 * whether they have names supplied or not. The rest num_remaining_attrs parameter values
 * will just be stored into the THD and if they have a name they can later be accessed as
 * query attributes. If any of the first num_params parameter values has a name supplied
 * they could then be accessed as a query attribute too. If supplied, parameter_count
 * will overwrite the num_params value by eventually adding a non-zero num_params_remaining
 * value to the original num_params.
 *
 * Returns
 * COM_STMT_EXECUTE Response
 */
internal fun MySQLConnection.sendStmtExecute(
   statementId: Int,
   flags: Int,
   iterationCount: Int,
   parameterCount: Int,
   nullBitmap: ByteArray? = null,
   newParamsBindFlag: Int,
   parameterType: Int,
   parameterName: String = "",
   parameterValues: ByteArray? = null,
) {
   send { out ->
      out.writeUByte(CommandType.STMT_EXECUTE.ordinal)


   }
}

/**
 * Fetches the requested amount of rows from a resultset produced by COM_STMT_EXECUTE
 *
 * Returns
 * COM_STMT_FETCH Response
 */
internal fun MySQLConnection.sendStmtFetch(
   /**
    * ID of the prepared statement to close
    */
   statementId: Int,
   /**
    * max number of rows to return
    */
   maxRows: Int
) {
   send { out ->
      out.writeUByte(CommandType.STMT_FETCH.ordinal)
      out.writeIntLE(statementId)
      out.writeIntLE(maxRows)
   }
}

/**
 * COM_STMT_CLOSE deallocates a prepared statement.
 *
 * No response packet is sent back to the client.
 *
 * Returns
 * None
 */
internal fun MySQLConnection.sendStmtClose(
   /**
    * ID of the prepared statement to close
    */
   statementId: Int
) {
   send { out ->
      out.writeUByte(CommandType.STMT_CLOSE.ordinal)
      out.writeIntLE(statementId)
   }
}

/**
 * COM_STMT_RESET resets the data of a prepared statement which was accumulated with
 * COM_STMT_SEND_LONG_DATA commands and closes the cursor if it was opened with COM_STMT_EXECUTE.
 *
 * The server will send a OK_Packet if the statement could be reset, a ERR_Packet if not.
 *
 * Returns
 * OK_Packet or a ERR_Packet
 */
internal fun MySQLConnection.sendStmtReset(
   /**
    * ID of the prepared statement to close
    */
   statementId: Int
) {
   send { out ->
      out.writeUByte(CommandType.STMT_RESET.ordinal)
      out.writeIntLE(statementId)
   }
}
