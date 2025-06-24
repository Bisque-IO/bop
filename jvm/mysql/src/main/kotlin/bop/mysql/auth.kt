package bop.mysql

import bop.mysql.MySQLConnection.Companion.LOG
import bop.mysql.network.*
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager

const val SHA2_PASSWORD = "caching_sha2_password"
const val MYSQL_NATIVE = "mysql_native_password"

enum class AuthMethod {
   NATIVE,
   CACHING_SHA2
}

data class Greeting(
   var protocolVersion: Int = 0,
   var serverVersion: String = "",
   var threadId: Long = 0L,
   var scramble: String = "",
   var serverCapabilities: Int = 0,
   var serverCollation: Int = 0,
   var serverStatus: Int = 0,
   var pluginProvidedData: String = ""
)

internal fun MySQLConnection.tryUpgradeToSSL(): Boolean {
   val collation = greeting.serverCollation
   if (sslMode == SSLMode.DISABLED) {
      LOG.info("SSL is disabled")
      return false
   }

   val serverSupportsSSL = (greeting.serverCapabilities and ClientCapabilities.CLIENT_SSL) != 0
   if (!serverSupportsSSL && (sslMode == SSLMode.REQUIRED || sslMode == SSLMode.VERIFY_CA || sslMode == SSLMode.VERIFY_IDENTITY)) {
      throw IOException("MySQL server does not support SSL")
   }
   if (!serverSupportsSSL) {
      LOG.warn("SSL is not supported by the server")
      return false
   }

   sendSSLRequest(
      collation = collation
   )

   val sslSocketFactory = if (this.sslSocketFactory != null) this.sslSocketFactory
   else if (sslMode == SSLMode.REQUIRED || sslMode == SSLMode.PREFERRED) DEFAULT_REQUIRED_SSL_MODE_SOCKET_FACTORY
   else DEFAULT_VERIFY_CA_SSL_MODE_SOCKET_FACTORY

   val hostnameVerifier = if (this.hostnameVerifier != null) this.hostnameVerifier
   else if (sslMode == SSLMode.VERIFY_IDENTITY) TLSHostnameVerifier()
   else null

   val sslSocket = sslSocketFactory!!.createSocket(socket)
   sslSocket.startHandshake()
   inputStream = sslSocket.inputStream
   outputStream = sslSocket.outputStream
   socket = sslSocket
   if (hostnameVerifier != null && !hostnameVerifier.verify(hostname, sslSocket.session)) {
      throw IdentityVerificationException(
         "\"" + sslSocket.getInetAddress().getHostName() + "\" identity was not confirmed"
      )
   }
   isSSL = true

   LOG.info("SSL enabled")
   return true
}

internal fun MySQLConnection.authenticate() {
   scramble = greeting.scramble
   val collation = greeting.serverCollation

   if (greeting.pluginProvidedData == SHA2_PASSWORD) {
      authMethod = AuthMethod.CACHING_SHA2
      sendAuthenticateSHA2(
         schema = schema,
         username = username,
         password = password,
         scramble = scramble,
         collation = collation
      )
   } else {
      authMethod = AuthMethod.NATIVE
      sendAuthenticateSecurityPassword(
         schema = schema,
         username = username,
         password = password,
         salt = scramble,
         collation = collation
      )
   }

   readAuthResult()
}

internal fun MySQLConnection.switchAuthentication() {
   // Azure-MySQL likes to tell us to switch authentication methods, even though
   // we haven't advertised that we support any.  It uses this for some-odd
   // reason to send the real password scramble.
   packet.readByte()

   val authName = packet.readCString()
   if (MYSQL_NATIVE == authName) {
      authMethod = AuthMethod.NATIVE
      this.scramble = packet.readCString()
      sendAuthenticateNativePassword(password, scramble)
   } else if (SHA2_PASSWORD == authName) {
      authMethod = AuthMethod.CACHING_SHA2
      this.scramble = packet.readCString()
      sendAuthenticateSHA2(password = password, scramble = scramble, rawPassword = true)
   } else {
      throw AuthenticationException("unsupported authentication method: $authName")
   }

   readAuthResult()
}

internal fun MySQLConnection.readAuthResult() {
   readPacket()

   when (val b = packet.readUByte()) {
      0x00 -> return

      0xFF -> {
         // error
         val errorPacket = MySQLError.parse(packet)
         throw AuthenticationException(
            errorPacket.errorMessage, errorPacket.errorCode, errorPacket.sqlState
         )
      }

      0xFE -> {
         switchAuthentication()
         return
      }

      else -> if (authMethod == AuthMethod.NATIVE) throw AuthenticationException(
         "Unexpected authentication result ($b)"
      )

      else processCachingSHA2Result()
   }
}

internal fun MySQLConnection.processCachingSHA2Result() {
   if (packet.size < 2) throw AuthenticationException("caching_sha2_password response too short!")

   packet.readPackedInteger() // throw away length, always 1

   when (packet.readUByte()) {
      0x03 -> {
         LOG.trace("cached sha2 auth successful")
         // successful fast authentication
         readAuthResult()
         return
      }

      0x04 -> {
         LOG.trace("cached sha2 auth not successful, moving to full auth path")
         continueCachingSHA2Authentication()
      }
   }
}

internal fun MySQLConnection.continueCachingSHA2Authentication() {
   if (isSSL) {
      // over SSL we simply send the password in cleartext.
      send(toCStringBytes(password))
      readAuthResult()
   } else {
      // try to download an RSA key
      send(0x02)
      readPacket()

      when (val result = packet.readUByte()) {
         0x01 -> {
            val rsaKey = packet.readBytes(packet.available)

            LOG.trace("received RSA key: $rsaKey")
            sendAuthenticateSHA2RSAPassword(
               rsaKey = String(rsaKey), password = password, scramble = scramble
            )
            readAuthResult()
            return
         }

         else -> throw AuthenticationException("Unknown response fetching RSA key in caching_sha2_password auth: $result")
      }
   }
}

val DEFAULT_REQUIRED_SSL_MODE_SOCKET_FACTORY: SSLSocketFactory =
   object : DefaultSSLSocketFactory() {
      @Throws(GeneralSecurityException::class)
      override fun initSSLContext(sc: SSLContext) {
         sc.init(null, arrayOf<TrustManager>(object : X509TrustManager {
            @Throws(CertificateException::class)
            override fun checkClientTrusted(
               x509Certificates: Array<X509Certificate>?,
               s: String?
            ) {
            }

            @Throws(CertificateException::class)
            override fun checkServerTrusted(
               x509Certificates: Array<X509Certificate?>?,
               s: String?
            ) {
            }

            override fun getAcceptedIssuers(): Array<X509Certificate?> {
               return arrayOfNulls(0)
            }
         }), null)
      }
   }

val DEFAULT_VERIFY_CA_SSL_MODE_SOCKET_FACTORY: SSLSocketFactory = DefaultSSLSocketFactory()
