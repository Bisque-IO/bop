package bop.mysql.network

import bop.mysql.io.ByteArrayInputStream
import bop.mysql.io.ByteArrayOutputStream
import org.slf4j.Logger
import org.slf4j.LoggerFactory
import java.io.IOException
import java.net.Socket
import java.net.SocketException
import java.security.GeneralSecurityException
import java.security.cert.X509Certificate
import javax.net.ssl.*

class AuthenticationException : ServerException {
   constructor(
      message: String?,
      errorCode: Int,
      sqlState: String?
   ) : super(message, errorCode, sqlState)

   constructor(message: String?) : super(message, 0, "HY000")
}

class Authenticator(
   private val greetingPacket: GreetingPacket,
   private val channel: PacketChannel,
   private val schema: String?,
   private val username: String,
   private val password: String
) {
   private enum class AuthMethod {
      NATIVE,
      CACHING_SHA2
   }

   private var scramble: String = ""

   private val logger: Logger = LoggerFactory.getLogger(javaClass)

   private val SHA2_PASSWORD = "caching_sha2_password"
   private val MYSQL_NATIVE = "mysql_native_password"

   private var authMethod = AuthMethod.NATIVE

   init {
      this.scramble = greetingPacket.scramble
   }

   @Throws(IOException::class)
   fun authenticate() {
      logger.trace("Begin auth for $username")
      val collation = greetingPacket.serverCollation

      val authenticateCommand: Command?
      if (SHA2_PASSWORD == greetingPacket.pluginProvidedData) {
         authMethod = AuthMethod.CACHING_SHA2
         authenticateCommand =
            AuthenticateSHA2Command(schema, username, password, scramble, collation)
      } else {
         authMethod = AuthMethod.NATIVE
         authenticateCommand =
            AuthenticateSecurityPasswordCommand(schema, username, password, scramble, collation)
      }

      channel.write(authenticateCommand)
      readResult()
      logger.trace("Auth complete $username")
   }

   @Throws(IOException::class)
   private fun readResult() {
      val authenticationResult = channel.read()
      when (authenticationResult[0]) {
         0x00.toByte() ->                 // success
            return

         0xFF.toByte() -> {
            // error
            val bytes = authenticationResult.copyOfRange(1, authenticationResult.size)
            val errorPacket = ErrorPacket(bytes)
            throw AuthenticationException(
               errorPacket.errorMessage, errorPacket.errorCode, errorPacket.sqlState
            )
         }

         0xFE.toByte() -> {
            switchAuthentication(authenticationResult)
            return
         }

         else -> if (authMethod == AuthMethod.NATIVE) throw AuthenticationException("Unexpected authentication result (" + authenticationResult[0] + ")")
         else processCachingSHA2Result(authenticationResult)
      }
   }

   @Throws(IOException::class)
   private fun processCachingSHA2Result(authenticationResult: ByteArray) {
      if (authenticationResult.size < 2) throw AuthenticationException("caching_sha2_password response too short!")

      val stream = ByteArrayInputStream(authenticationResult)
      stream.readPackedInteger() // throw away length, always 1

      when (stream.read()) {
         0x03 -> {
            logger.trace("cached sha2 auth successful")
            // successful fast authentication
            readResult()
            return
         }

         0x04 -> {
            logger.trace("cached sha2 auth not successful, moving to full auth path")
            continueCachingSHA2Authentication()
         }
      }
   }

   @Throws(IOException::class)
   private fun continueCachingSHA2Authentication() {
      val buffer = ByteArrayOutputStream()
      if (channel.isSSL) {
         // over SSL we simply send the password in cleartext.

         buffer.writeZeroTerminatedString(password)

         val c: Command = ByteArrayCommand(buffer.toByteArray())
         channel.write(c)
         readResult()
      } else {
         // try to download an RSA key
         buffer.write(0x02)
         channel.write(ByteArrayCommand(buffer.toByteArray()))

         val stream = ByteArrayInputStream(channel.read())
         val result = stream.read()
         when (result) {
            0x01 -> {
               val rsaKey = ByteArray(stream.available())
               stream.read(rsaKey)

               logger.trace("received RSA key: $rsaKey")
               val c: Command =
                  AuthenticateSHA2RSAPasswordCommand(String(rsaKey), password, scramble)
               channel.write(c)

               readResult()
               return
            }

            else -> throw AuthenticationException("Unknown response fetching RSA key in caching_sha2_password auth: $result")
         }
      }
   }

   @Throws(IOException::class)
   private fun switchAuthentication(authenticationResult: ByteArray) {/*
            Azure-MySQL likes to tell us to switch authentication methods, even though
            we haven't advertised that we support any.  It uses this for some-odd
            reason to send the real password scramble.
        */
      val buffer = ByteArrayInputStream(authenticationResult)
      buffer.read(1)

      val authName = buffer.readZeroTerminatedString()
      if (MYSQL_NATIVE == authName) {
         authMethod = AuthMethod.NATIVE

         this.scramble = buffer.readZeroTerminatedString()

         val switchCommand: Command = AuthenticateNativePasswordCommand(scramble, password)
         channel.write(switchCommand)
      } else if (SHA2_PASSWORD == authName) {
         authMethod = AuthMethod.CACHING_SHA2

         this.scramble = buffer.readZeroTerminatedString()
         val authCommand: Command = AuthenticateSHA2Command(scramble, password)
         channel.write(authCommand)
      } else {
         throw AuthenticationException("unsupported authentication method: " + authName)
      }

      readResult()
   }
}

/**
 * @see [
 * Capability Flags](http://dev.mysql.com/doc/internals/en/capability-flags.html.packet-Protocol::CapabilityFlags)
 */
object ClientCapabilities {
   const val LONG_PASSWORD: Int =                  1 /* new more secure passwords */
   const val FOUND_ROWS: Int =                     1 shl 1 /* found instead of affected rows */
   const val LONG_FLAG: Int =                      1 shl 2 /* get all column flags */
   const val CONNECT_WITH_DB: Int =                1 shl 3 /* one can specify db on connect */
   const val NO_SCHEMA: Int =                      1 shl 4 /* don't allow database.table.column */
   const val COMPRESS: Int =                       1 shl 5 /* can use compression protocol */
   const val ODBC: Int =                           1 shl 6 /* odbc client */
   const val LOCAL_FILES: Int =                    1 shl 7 /* can use LOAD DATA LOCAL */
   const val IGNORE_SPACE: Int =                   1 shl 8 /* ignore spaces before '' */
   const val PROTOCOL_41: Int =                    1 shl 9 /* new 4.1 protocol */
   const val INTERACTIVE: Int =                    1 shl 10 /* this is an interactive client */
   const val SSL: Int =                            1 shl 11 /* switch to ssl after handshake */
   const val IGNORE_SIGPIPE: Int =                 1 shl 12 /* IGNORE sigpipes */
   const val TRANSACTIONS: Int =                   1 shl 13 /* client knows about transactions */
   const val RESERVED: Int =                       1 shl 14 /* old flag for 4.1 protocol  */
   const val SECURE_CONNECTION: Int =              1 shl 15 /* new 4.1 authentication */
   const val MULTI_STATEMENTS: Int =               1 shl 16 /* enable/disable multi-stmt support */
   const val MULTI_RESULTS: Int =                  1 shl 17 /* enable/disable multi-results */
   const val PS_MULTI_RESULTS: Int =               1 shl 18 /* multi-results in ps-protocol */
   const val PLUGIN_AUTH: Int =                    1 shl 19 /* client supports plugin authentication */
   const val CLIENT_CONNECT_ATTRS: Int = 1 shl 20
   const val CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA: Int = 1 shl 21
   const val PLUGIN_AUTH_LENENC_CLIENT_DATA: Int = 1 shl 21
   const val SSL_VERIFY_SERVER_CERT: Int =         1 shl 30
   const val REMEMBER_OPTIONS: Int =               1 shl 31
}

class IdentityVerificationException(message: String?) : SSLException(message)
open class ServerException(
   message: String?,
   /**
    * @see bop.mysql.ErrorCode
    *
    * @return error code
    */
   val errorCode: Int,
   val sqlState: String?
) : IOException(message)

interface SocketFactory {
   @Throws(SocketException::class)
   fun createSocket(): Socket?
}

/**
 * @see [* ssl-mode](https://dev.mysql.com/doc/refman/5.7/en/secure-connection-options.html.option_general_ssl-mode) for the original documentation.
 */
enum class SSLMode {
   /**
    * Establish a secure (encrypted) connection if the server supports secure connections.
    * Fall back to an unencrypted connection otherwise.
    */
   PREFERRED,

   /**
    * Establish an unencrypted connection.
    */
   DISABLED,

   /**
    * Establish a secure connection if the server supports secure connections.
    * The connection attempt fails if a secure connection cannot be established.
    */
   REQUIRED,

   /**
    * Like REQUIRED, but additionally verify the server TLS certificate against the configured Certificate Authority
    * (CA) certificates. The connection attempt fails if no valid matching CA certificates are found.
    */
   VERIFY_CA,

   /**
    * Like VERIFY_CA, but additionally verify that the server certificate matches the host to which the connection is
    * attempted.
    */
   VERIFY_IDENTITY
}

interface SSLSocketFactory {
   @Throws(SocketException::class)
   fun createSocket(socket: Socket): SSLSocket
}

open class DefaultSSLSocketFactory(val protocol: String = "TLSv1.2") : SSLSocketFactory {
   @Throws(SocketException::class)
   override fun createSocket(socket: Socket): SSLSocket {
      val sc: SSLContext?
      try {
         sc = SSLContext.getInstance(this.protocol)
         initSSLContext(sc)
      } catch (e: GeneralSecurityException) {
         throw SocketException(e.message)
      }
      try {
         return sc.socketFactory.createSocket(
            socket, socket.inetAddress.hostName, socket.port, true
         ) as SSLSocket
      } catch (e: IOException) {
         throw SocketException(e.message)
      }
   }

   @Throws(GeneralSecurityException::class)
   protected open fun initSSLContext(sc: SSLContext) {
      sc.init(null, null, null)
   }
}

class TLSHostnameVerifier : HostnameVerifier {
   override fun verify(
      hostname: String?,
      session: SSLSession
   ): Boolean {
      val checker = HostnameChecker.DEFAULT
      try {
         val peerCertificates = session.peerCertificates
         if (peerCertificates.size > 0 && peerCertificates[0] is X509Certificate) {
            val peerCertificate = peerCertificates[0] as X509Certificate
            try {
               checker.check(hostname, peerCertificate)
               return true
            } catch (ignored: SSLException) {
            }
         }
      } catch (ignored: SSLPeerUnverifiedException) {
      }
      return false
   }
}