package bop.mysql.network

import bop.mysql.io.BufferedSocketInputStream
import bop.mysql.io.ByteArrayInputStream
import bop.mysql.io.ByteArrayOutputStream
import bop.mysql.MySQLBuffer
import java.io.IOException
import java.net.Socket
import java.net.SocketException
import java.nio.channels.Channel
import java.util.*
import javax.net.ssl.HostnameVerifier

interface Packet {
   companion object {
      // https://dev.mysql.com/doc/internals/en/sending-more-than-16mbyte.html
      const val MAX_LENGTH: Int = 16777215
   }
}

class ErrorPacket(bytes: ByteArray) : Packet {
   var errorCode: Int
   var sqlState: String
      private set
   var errorMessage: String

   init {
      val buffer = MySQLBuffer.wrap(bytes)
      this.errorCode = buffer.readIntLE(2)
      if (buffer.peek() == '#'.code) {
         // marker of the SQL State
         buffer.skip(1)
         this.sqlState = buffer.readString(5)
      } else {
         this.sqlState = ""
      }
      this.errorMessage = buffer.readString(buffer.available)
   }
}

data class GreetingPacket(
   val protocolVersion: Int,
   val serverVersion: String?,
   val threadId: Long,
   val scramble: String,
   val serverCapabilities: Int,
   val serverCollation: Int,
   val serverStatus: Int,
   val pluginProvidedData: String = ""
) : Packet {
   companion object {
      fun parse(bytes: ByteArray): GreetingPacket {
         val buffer = ByteArrayInputStream(bytes)
         val protocolVersion = buffer.readInteger(1)
         val serverVersion = buffer.readZeroTerminatedString()
         val threadId = buffer.readLong(4)
         val scramblePrefix = buffer.readZeroTerminatedString()
         val serverCapabilities = buffer.readInteger(2)
         val serverCollation = buffer.readInteger(1)
         val serverStatus = buffer.readInteger(2)
         buffer.skip(13) // reserved
         val scramble = scramblePrefix + buffer.readZeroTerminatedString()
         val pluginProvidedData = if (buffer.available() > 0) {
            buffer.readZeroTerminatedString()
         } else {
            ""
         }
         return GreetingPacket(
            protocolVersion,
            serverVersion,
            threadId,
            scramble,
            serverCapabilities,
            serverCollation,
            serverStatus,
            pluginProvidedData
         )
      }
   }
}

class PacketChannel(private var socket: Socket) : Channel {
   private var packetNumber = 0
   private var authenticationComplete = false
   var isSSL: Boolean = false
      private set
   var inputStream: ByteArrayInputStream
      private set
   var outputStream: ByteArrayOutputStream
      private set
   private var shouldUseSoLinger0 = false

   constructor(
      hostname: String?,
      port: Int
   ) : this(Socket(hostname, port))

   init {
      if (socket.isConnected) {
         this.inputStream = ByteArrayInputStream(BufferedSocketInputStream(socket.getInputStream()))
         this.outputStream = ByteArrayOutputStream(socket.getOutputStream())
      } else {
         this.inputStream = ByteArrayInputStream(ByteArray(0))
         this.outputStream = ByteArrayOutputStream()
      }
   }

   fun authenticationComplete() {
      authenticationComplete = true
   }

   @Throws(IOException::class)
   fun read(): ByteArray {
      val length = inputStream.readInteger(3)
      val sequence = inputStream.read() // sequence
      if (sequence != packetNumber++) {
         throw IOException("unexpected sequence #$sequence")
      }
      return inputStream.read(length)
   }

   @Throws(IOException::class)
   fun write(command: Command) {
      val body = command.toByteArray()
      val buffer = ByteArrayOutputStream()
      buffer.writeInteger(body.size, 3) // packet length

      // see https://dev.mysql.com/doc/dev/mysql-server/8.0.11/page_protocol_basic_packets.html#sect_protocol_basic_packets_sequence_id
      // we only have to maintain a sequence number in the authentication phase.
      // what the point is, I do not know
      if (authenticationComplete) {
         packetNumber = 0
      }

      buffer.writeInteger(packetNumber++, 1)

      buffer.write(body, 0, body.size)
      outputStream.write(buffer.toByteArray())
      // though it has no effect in case of default (underlying) output stream (SocketOutputStream),
      // it may be necessary in case of non-default one
      outputStream.flush()
   }

   @Throws(IOException::class)
   fun upgradeToSSL(
      sslSocketFactory: SSLSocketFactory,
      hostnameVerifier: HostnameVerifier?
   ) {
      val sslSocket = sslSocketFactory.createSocket(this.socket)
      sslSocket.startHandshake()
      socket = sslSocket
      inputStream = ByteArrayInputStream(sslSocket.getInputStream())
      outputStream = ByteArrayOutputStream(sslSocket.getOutputStream())
      if (hostnameVerifier != null && !hostnameVerifier.verify(
            sslSocket.getInetAddress().getHostName(), sslSocket.session
         )
      ) {
         throw IdentityVerificationException(
            "\"" + sslSocket.getInetAddress().getHostName() + "\" identity was not confirmed"
         )
      }
      isSSL = true
   }

   fun setShouldUseSoLinger0() {
      shouldUseSoLinger0 = true
   }

   override fun isOpen(): Boolean {
      return !socket.isClosed
   }

   @Throws(IOException::class)
   override fun close() {
      if (shouldUseSoLinger0) {
         try {
            socket.setSoLinger(true, 0)
         } catch (e: SocketException) {
            // ignore
         }
      }
      try {
         socket.shutdownInput() // for socketInputStream.setEOF(true)
      } catch (e: Exception) {
         // ignore
      }
      try {
         socket.shutdownOutput()
      } catch (e: Exception) {
         // ignore
      }
      socket.close()
      shouldUseSoLinger0 = false
   }

   companion object {
      val NULL = PacketChannel(Socket())
   }
}

class ResultSetRowPacket(bytes: ByteArray) : Packet {
   val values: Array<String?>

   init {
      val buffer = ByteArrayInputStream(bytes)
      val values: MutableList<String?> = LinkedList<String?>()
      while (buffer.available() > 0) {
         values.add(buffer.readLengthEncodedString())
      }
      this.values = values.toTypedArray<String?>()
   }

   fun getValue(index: Int): String? {
      return values[index]
   }

   fun size(): Int {
      return values.size
   }
}