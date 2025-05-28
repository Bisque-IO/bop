package bop.mysql

import bop.concurrent.SpinLock
import bop.mysql.binlog.GtidSet
import bop.mysql.network.AuthenticationException
import bop.mysql.network.SSLMode
import bop.mysql.network.SSLSocketFactory
import bop.mysql.network.SocketFactory
import org.slf4j.Logger
import org.slf4j.LoggerFactory
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.HostnameVerifier
import kotlin.concurrent.withLock

class MySQLConnection(
   /**
    * mysql server hostname
    */
   val hostname: String = "localhost",

   /**
    * mysql server port
    */
   val port: Int = 3306,

   /**
    * database name. Note that this parameter has nothing to do with event filtering. It's
    * used only during the authentication.
    */
   val schema: String? = null,

   /**
    * auth username
    */
   val username: String = "",

   /**
    * auth password
    */
   val password: String = "",

   /**
    * server id (in the range from 1 to 2^32 - 1). This value MUST be unique across whole replication
    * group (that is, different from any other server id being used by any master or slave). Keep in mind that each
    * binary log client (mysql-binlog-connector-java/BinaryLogClient, mysqlbinlog, etc) should be treated as a
    * simplified slave and thus MUST also use a different server id.
    * @see .getServerId
    */
   var serverId: Long = 65535,

   /**
    * binary log filename, nullable (and null be default). Note that this value is automatically tracked by
    * the client and thus is subject to change (in response to [EventType.ROTATE], for example).
    *
    *  * null, which turns on automatic resolution (resulting in the last known binlog and position). This is what
    * happens by default when you don't specify the binary log filename explicitly.
    *  * "" (empty string), which instructs the server to stream events starting from the oldest known binlog.
    */
   @Volatile
   var binlogFilename: String? = null,

   /**
    * @return binary log position of the next event, 4 by default (which is a position of first event). Note that this
    * value changes with each incoming event.
    *
    * Any value less than 4 gets automatically adjusted to 4 on connect.
    */
   @Volatile
   var binlogPosition: Long = 4L,

   var keepAlive: Boolean = true,

   var keepAliveInterval: Long = TimeUnit.SECONDS.toMillis(5),

   /**
    * heartbeat period in milliseconds (0 if not set (default)).
    *
    * If set (recommended)
    *
    *  *  HEARTBEAT event will be emitted every "heartbeatInterval".
    *  *  if [.setKeepAlive] is on, then the keepAlive thread will attempt to reconnect if no
    * HEARTBEAT events were received within [.setKeepAliveInterval] (instead of trying to send
    * PING every [.setKeepAliveInterval], which is fundamentally flawed
    *
    * Note that when used together with keepAlive heartbeatInterval MUST be set less than keepAliveInterval.
    *
    * @see .getHeartbeatInterval
    */
   var heartbeatInterval: Long = TimeUnit.SECONDS.toMillis(0),

   var sslMode: SSLMode = SSLMode.DISABLED,

   /**
    * if gtid_purged should be used as a fallback when gtidSet is set to "" and
    * MySQL server has purged some of the binary logs, false otherwise (default).
    *
    * @return whether gtid_purged is used as a fallback
    */
   var isGtidSetFallbackToPurged: Boolean = false,

   /**
    * @param isUseBinlogFilenamePositionInGtidMode true if MySQL server should start streaming events from a given
    * [.getBinlogFilename] and [.getBinlogPosition] instead of "the oldest known binlog" when
    * [.getGtidSet] is set, false otherwise (default).
    */
   var isUseBinlogFilenamePositionInGtidMode: Boolean = false,

   var socketFactory: SocketFactory? = null,
   var sslSocketFactory: SSLSocketFactory? = null,
   var hostnameVerifier: HostnameVerifier? = null
) {
   @Volatile
   var connectionId: Long = 0
      internal set

   var gtidSet = GtidSet.NULL
   var gtidSetAccessLock = SpinLock()
   internal var gtidEnabled = false
   var gtid: Any? = null


   @Volatile
   var masterServerId = -1L
      internal set

   internal var tx = false

   @Volatile
   internal var eventLastSeen: Long = 0L
   internal val connectLock = SpinLock()

   var isUseSendAnnotateRowsEvent: Boolean = false

   var databaseVersion: MySQLVersion = MySQLVersion(
      -1, 0, 0, "", "", ""
   )
      private set

   val mariaDb: Boolean
      get() = databaseVersion.isMariaDb

   var socket: Socket = NULL_SOCKET
      internal set

   @Volatile
   var isConnected = socket.isConnected
      internal set

   @Volatile
   internal var connecting: Boolean = false

   internal var packetSize: Int = 0
   internal var packetHeader: Int = 0
   internal var packetWarnings: Int = 0
   internal var packetStatusFlags: Int = 0
   internal var affectedRows: Long = 0
   internal var lastInsertId: Long = 0
   internal var statusFlags: Int = 0
   internal var warnings: Int = 0
   internal var packetSequence: UByte = 0.toUByte()
   internal var packet = MySQLBuffer.allocate(MAX_PACKET_LENGTH)
   
   internal var command: Int = 0
   internal var sendPacket = MySQLBuffer.allocate(65536)

   internal var inputStream: InputStream = InputStream.nullInputStream()
   internal var outputStream: OutputStream = OutputStream.nullOutputStream()

   internal var greeting = Greeting()
   internal var clientCapabilities: Int = 0

   // authentication
   internal var scramble = ""
   internal var authMethod = AuthMethod.NATIVE
   internal var authenticationComplete = false

   // result set
   internal val resultSet = ResultSet()

   var isSSL: Boolean = false
      internal set

   val packetCounter = AtomicLong(0L)
   val rxBytes = AtomicLong(0L)
   val txBytes = AtomicLong(0L)
   val rowCounter = AtomicLong(0L)
   val pingCounter = AtomicLong(0L)

   /**
    *
    */
   fun connect(timeout: Int = 3000) {
      val timeout = if (timeout < 2000) 2000 else timeout
      connectLock.withLock {
         if (isConnected) {
            throw IllegalStateException("already connected")
         }
         if (connecting) {
            throw IllegalStateException("currently connecting")
         }

         connecting = true
      }

      try {
         socket = socketFactory?.createSocket() ?: Socket()
//         socket.keepAlive = false
         socket.tcpNoDelay = true
         socket.receiveBufferSize = 1024 * 1024
         socket.sendBufferSize = 1024 * 1024
//         socket.soTimeout = 1000
         socket.connect(InetSocketAddress(hostname, port), timeout)

//         inputStream = socket.inputStream
         this.inputStream = socket.inputStream
         this.outputStream = socket.outputStream

         // connected
         readPacket()

         // receive greeting
         greeting.protocolVersion = packet.readUByte()
         greeting.serverVersion = packet.readCString()
         greeting.threadId = packet.readIntLE().toUInt().toLong()
         val scramblePrefix = packet.readCString()
         greeting.serverCapabilities = packet.readCharLE().code
         greeting.serverCollation = packet.readUByte()
         greeting.serverStatus = packet.readCharLE().code
         packet.skip(13)
         greeting.scramble = scramblePrefix + packet.readCString()
         greeting.pluginProvidedData = if (packet.available > 0) {
            packet.readCString()
         } else {
            ""
         }

         databaseVersion = MySQLVersion.parse(greeting.serverVersion)

         LOG.info("connected with greeting: $greeting")
         LOG.info("authenticating via: ${greeting.pluginProvidedData}")

         tryUpgradeToSSL()

         authenticate()
         authenticationComplete = true
         LOG.info("authenticated: $username")

         connectionId = greeting.threadId
         if (binlogFilename == "") {
            setupGtidSet()
         } else if (binlogFilename == null) {
            fetchBinlogFilenameAndPosition()
         }
      } finally {
         connecting = false
      }
   }

   fun close() {
      runCatching {
         socket.shutdownInput()
      }
      runCatching {
         socket.shutdownOutput()
      }
      runCatching {
         socket.close()
      }
      socket = NULL_SOCKET
      isConnected = false
      connecting = false
      LOG.info("closed")
   }

   internal fun readPacket() {
      var offset = 0
      while (offset < 4) {
         val read = inputStream.read(packet.base as ByteArray, offset, 4 - offset)
         if (read == -1) throw EOFException("Stream ended before reading 4 byte packet header")
         offset += read
      }
      packet.position = 0
      packet.size = 4

      packetSize = packet.readInt24LE()
      val sequence = packet.readUByte()

      if (sequence.toUByte() != packetSequence++) {
         throw IOException("unexpected sequence: #$sequence")
      }

      packet.extend(0, packetSize)
      packet.position = 0
      packet.size = 0

      offset = 0

      while (offset < packetSize) {
         val read = inputStream.read(packet.base as ByteArray, offset, packetSize - offset)
         if (read == -1) throw EOFException("Stream ended before reading packet")
         offset += read
         packet.size += read
      }

      packetHeader = packet.getUByte(0)
   }

   internal fun readOKorEOFDetails() {
      if (packetHeader == 0x00 && packet.size >= 7) {
         packet.readUByte()
         affectedRows = packet.readPackedNumber()
         lastInsertId = packet.readPackedNumber()

         if (clientCapabilities and ClientCapabilities.CLIENT_PROTOCOL_41 != 0) {
            statusFlags = packet.readCharLE().code
            warnings = packet.readCharLE().code
         } else if (clientCapabilities and ClientCapabilities.CLIENT_TRANSACTIONS != 0) {
            statusFlags = packet.readCharLE().code
            warnings = 0
         } else {
            statusFlags = 0
            warnings = 0
         }
      } else if (packetHeader == 0xFE && packet.size < 8) {
         packet.readUByte()
         if (clientCapabilities and ClientCapabilities.CLIENT_PROTOCOL_41 != 0) {
            warnings = packet.readCharLE().code
            statusFlags = packet.readCharLE().code
         } else {
            statusFlags = 0
            warnings = 0
         }
      }
   }

   fun setupGtidSet() {

   }

   internal fun fetchBinlogFilenameAndPosition() {
      if (!databaseVersion.isMariaDb && databaseVersion.isGreaterThanOrEqualTo(8, 4)) {
         sendQuery("SHOW BINARY LOG STATUS")
      } else if (!databaseVersion.isMariaDb && databaseVersion.isGreaterThanOrEqualTo(8, 0, 22)) {
         sendQuery("SHOW REPLICA STATUS")
      } else {
         sendQuery("SHOW MASTER STATUS")
      }
      val resultSet = readResultSet()
      if (resultSet.isEmpty()) {
         throw IOException("Failed to determine binlog filename/position")
      }
      val row = resultSet[0]
      binlogFilename = row[0].toString()
      binlogPosition = row[1].toString().toLong()

      LOG.info("binlog filename: $binlogFilename   position: $binlogPosition")
   }

   internal fun checkError() {
      if (packetHeader == 0xFF) {
         val errorPacket = MySQLError.parse(packet)
         throw AuthenticationException(
            errorPacket.errorMessage, errorPacket.errorCode, errorPacket.sqlState
         )
      }
   }

   companion object {
      val LOG: Logger = LoggerFactory.getLogger(MySQLConnection::class.java)!!
   }
}


val EMPTY_BYTES: ByteArray = ByteArray(0)