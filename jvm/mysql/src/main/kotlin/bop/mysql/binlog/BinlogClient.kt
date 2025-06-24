package bop.mysql.binlog

import bop.concurrent.SpinLock
import bop.mysql.MySQLVersion
import bop.mysql.MySQLVersion.Companion.parse
import bop.mysql.binlog.MariadbGtidSet.Companion.isMariaGtidSet
import bop.mysql.binlog.event.deserialization.*
import bop.mysql.binlog.event.deserialization.EventDeserializer.EventDataWrapper
import bop.mysql.binlog.event.deserialization.EventDeserializer.EventDataWrapper.Companion.internal
import bop.mysql.binlog.event.deserialization.EventDeserializer.EventDataWrapper.Deserializer
import bop.mysql.event.AnnotateRowsEventData
import bop.mysql.event.Event
import bop.mysql.event.EventHeaderV4
import bop.mysql.event.EventType
import bop.mysql.event.GtidEventData
import bop.mysql.event.MariadbGtidEventData
import bop.mysql.event.MariadbGtidListEventData
import bop.mysql.event.QueryEventData
import bop.mysql.event.RotateEventData
import bop.mysql.io.ByteArrayInputStream
import bop.mysql.network.Authenticator
import bop.mysql.network.ClientCapabilities
import bop.mysql.network.Command
import bop.mysql.network.DefaultSSLSocketFactory
import bop.mysql.network.DumpBinaryLogCommand
import bop.mysql.network.DumpBinaryLogGtidCommand
import bop.mysql.network.ErrorPacket
import bop.mysql.network.GreetingPacket
import bop.mysql.network.Packet
import bop.mysql.network.PacketChannel
import bop.mysql.network.PingCommand
import bop.mysql.network.QueryCommand
import bop.mysql.network.ResultSetRowPacket
import bop.mysql.network.SSLMode
import bop.mysql.network.SSLRequestCommand
import bop.mysql.network.SSLSocketFactory
import bop.mysql.network.ServerException
import bop.mysql.network.SocketFactory
import bop.mysql.network.TLSHostnameVerifier
import org.slf4j.Logger
import org.slf4j.LoggerFactory
import java.io.EOFException
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketException
import java.security.GeneralSecurityException
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.*
import java.util.concurrent.*
import java.util.concurrent.atomic.AtomicReference
import java.util.concurrent.locks.Lock
import java.util.concurrent.locks.ReentrantLock
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager
import kotlin.concurrent.Volatile
import kotlin.concurrent.withLock

/**
 * MySQL replication stream client.
 */
class BinlogClient(
   /**
    * mysql server hostname
    */
   val hostname: String,

   /**
    * mysql server port
    */
   val port: Int,

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
   val password: String = ""
) {
   val logger: Logger = LoggerFactory.getLogger(javaClass)

   /**
    * @param isBlocking blocking mode. If set to false - BinaryLogClient will disconnect after the last event.
    */
   var isBlocking: Boolean = true

   /**
    * @return server id (65535 by default)
    * @see .setServerId
    */
   var serverId: Long = 65535
      /**
       * @param serverId server id (in the range from 1 to 2^32 - 1). This value MUST be unique across whole replication
       * group (that is, different from any other server id being used by any master or slave). Keep in mind that each
       * binary log client (mysql-binlog-connector-java/BinaryLogClient, mysqlbinlog, etc) should be treated as a
       * simplified slave and thus MUST also use a different server id.
       * @see .getServerId
       */
      set

   /**
    * @return binary log filename, nullable (and null be default). Note that this value is automatically tracked by
    * the client and thus is subject to change (in response to [bop.mysql.event.EventType.ROTATE], for example).
    * @see .setBinlogFilename
    */

   @Volatile
   var binlogFilename: String? = null
      /**
       * @param binlogFilename binary log filename.
       * Special values are:
       *
       *  * null, which turns on automatic resolution (resulting in the last known binlog and position). This is what
       * happens by default when you don't specify binary log filename explicitly.
       *  * "" (empty string), which instructs server to stream events starting from the oldest known binlog.
       *
       * @see .getBinlogFilename
       */
      set

   /**
    * @return binary log position of the next event, 4 by default (which is a position of first event). Note that this
    * value changes with each incoming event.
    * @see .setBinlogPosition
    */
   @Volatile
   var binlogPosition: Long = 4
      /**
       * @param binlogPosition binary log position. Any value less than 4 gets automatically adjusted to 4 on connect.
       * @see .getBinlogPosition
       */
      set

   /**
    * @return thread id
    */
   @Volatile
   var connectionId: Long = 0
      private set

   private var sslMode = SSLMode.DISABLED
   private var useNonGracefulDisconnect = false

   var gtidSet: GtidSet = GtidSet.NULL
   val gtidSetAccessLock = SpinLock()

   /**
    * @see .setGtidSetFallbackToPurged
    * @return whether gtid_purged is used as a fallback
    */
   var isGtidSetFallbackToPurged: Boolean = false
      /**
       * @param isGtidSetFallbackToPurged true if gtid_purged should be used as a fallback when gtidSet is set to "" and
       * MySQL server has purged some of the binary logs, false otherwise (default).
       */
      set

   private var gtidEnabled = false

   /**
    * @see .setUseBinlogFilenamePositionInGtidMode
    * @return value of useBinlogFilenamePostionInGtidMode
    */
   var isUseBinlogFilenamePositionInGtidMode: Boolean = false
      /**
       * @param isUseBinlogFilenamePositionInGtidMode true if MySQL server should start streaming events from a given
       * [.getBinlogFilename] and [.getBinlogPosition] instead of "the oldest known binlog" when
       * [.getGtidSet] is set, false otherwise (default).
       */
      set

   var gtid: Any? = null

   private var tx = false

   private var eventDeserializer = EventDeserializer()

   private val eventListeners: MutableList<EventListener> = CopyOnWriteArrayList<EventListener>()
   private val lifecycleListeners: MutableList<LifecycleListener> = CopyOnWriteArrayList<LifecycleListener>()

   private var socketFactory: SocketFactory? = null
   private var sslSocketFactory: SSLSocketFactory? = null

   @Volatile
   var channel: PacketChannel = PacketChannel.NULL

   /**
    * @return true if a client is connected, false otherwise
    */
   @Volatile
   var isConnected: Boolean = false
      private set

   @Volatile
   var masterServerId: Long = -1
      private set

   private var threadFactory: ThreadFactory? = null

   /**
    * @return true if the "keep alive" thread should be automatically started (default), false otherwise.
    * @see .setKeepAlive
    */
   var isKeepAlive: Boolean = true
      /**
       * @param isKeepAlive true if the "keep alive" thread should be automatically started (recommended and true by default),
       * false otherwise.
       */
      set

   /**
    * @return "keep alive" interval in milliseconds, 1 minute by default.
    * @see .setKeepAliveInterval
    */
   var keepAliveInterval: Long = TimeUnit.MINUTES.toMillis(1)
      /**
       * @param keepAliveInterval "keep alive" interval in milliseconds.
       */
      set

   /**
    * @return heartbeat period in milliseconds (0 if not set (default)).
    */
   var heartbeatInterval: Long = 0
      /**
       * @param heartbeatInterval heartbeat period in milliseconds.
       *
       *
       * If set (recommended)
       *
       *  *  HEARTBEAT event will be emitted every "heartbeatInterval".
       *  *  if [.setKeepAlive] is on, then the keepAlive thread will attempt to reconnect if no
       * HEARTBEAT events were received within [.setKeepAliveInterval] (instead of trying to send
       * PING every [.setKeepAliveInterval], which is fundamentally flawed -
       * https://github.com/shyiko/mysql-binlog-connector-java/issues/118).
       *
       * Note that when used together with keepAlive heartbeatInterval MUST be set less than keepAliveInterval.
       *
       * @see .getHeartbeatInterval
       */
      set

   @Volatile
   private var eventLastSeen: Long = 0

   /**
    * @return "keep alive" connect timeout in milliseconds.
    */
   var keepAliveConnectTimeout: Long = TimeUnit.SECONDS.toMillis(3)

   @Volatile
   private var keepAliveThreadExecutor: ExecutorService? = null

   private val connectLock: Lock = ReentrantLock()
   private val keepAliveThreadExecutorLock: Lock = ReentrantLock()
   var isUseSendAnnotateRowsEvent: Boolean = false

   private var databaseVersion: MySQLVersion = MySQLVersion(-1, 0, 0, "", "", "")
   /**
    * @return the configured MariaDB slave compatibility level, defaults to 4.
    */
   var mariaDbSlaveCapability: Int = 4
      /**
       * Set the client's MariaDB slave compatibility level. This only applies when connecting to MariaDB.
       *
       * @param mariaDbSlaveCapability the expected compatibility level
       */
      set

   fun setUseNonGracefulDisconnect(useNonGracefulDisconnect: Boolean) {
      this.useNonGracefulDisconnect = useNonGracefulDisconnect
   }

   /**
    * @return GTID set. Note that this value changes with each received GTID event (provided client is in GTID mode).
    * @see .setGtidSet
    */
   fun getGtidSet(): String? {
      gtidSetAccessLock.withLock {
         return if (gtidSet != GtidSet.NULL) gtidSet.toString() else null
      }
   }

   /**
    * @param gtidStr GTID set string (can be an empty string).
    *
    * NOTE #1: Any value but null will switch BinaryLogClient into a GTID mode (this will also set binlogFilename
    * to "" (provided it's null) forcing MySQL to send events starting from the oldest known binlog (keep in mind
    * that connection will fail if gtid_purged is anything but empty (unless
    * [.setGtidSetFallbackToPurged] is set to true))).
    *
    * NOTE #2: GTID set is automatically updated with each incoming GTID event (provided GTID mode is on).
    * @see .getGtidSet
    * @see .setGtidSetFallbackToPurged
    */
   fun setGtidSet(gtidStr: String?) {
      if (gtidStr == null) return

      this.gtidEnabled = true

      if (this.binlogFilename == null) {
         this.binlogFilename = ""
      }

      gtidSetAccessLock.withLock {
         if (gtidStr != "") {
            if (isMariaGtidSet(gtidStr)) {
               this.gtidSet = MariadbGtidSet(gtidStr)
            } else {
               this.gtidSet = GtidSet(gtidStr)
            }
         }
      }
   }

   /**
    * @param eventDeserializer custom event deserializer
    */
   fun setEventDeserializer(eventDeserializer: EventDeserializer) {
      requireNotNull(eventDeserializer) { "Event deserializer cannot be NULL" }
      this.eventDeserializer = eventDeserializer
   }

   /**
    * @param socketFactory custom socket factory. If not provided, socket will be created with "new Socket()".
    */
   fun setSocketFactory(socketFactory: SocketFactory?) {
      this.socketFactory = socketFactory
   }

   /**
    * @param sslSocketFactory custom ssl socket factory
    */
   fun setSslSocketFactory(sslSocketFactory: SSLSocketFactory?) {
      this.sslSocketFactory = sslSocketFactory
   }

   /**
    * @param threadFactory custom thread factory. If not provided, threads will be created using simple "new Thread()".
    */
   fun setThreadFactory(threadFactory: ThreadFactory?) {
      this.threadFactory = threadFactory
   }


   val mariaDB: Boolean
      /**
       * @return true/false depending on whether we've connected to MariaDB.  NULL if not connected.
       */
      get() = databaseVersion.isMariaDb

   /**
    * Connect to the replication stream. Note that this method blocks until disconnected.
    * @throws AuthenticationException if authentication fails
    * @throws ServerException if MySQL server responds with an error
    * @throws IOException if anything goes wrong while trying to connect
    * @throws IllegalStateException if binary log client is already connected
    */
   @Throws(IOException::class, IllegalStateException::class)
   fun connect() {
      check(connectLock.tryLock()) { "BinaryLogClient is already connected" }
      var notifyWhenDisconnected = false
      try {
         var cancelDisconnect: Callable<*>? = null
         try {
            try {
               val start = System.currentTimeMillis()
               channel = openChannel()
//               if (this.keepAliveConnectTimeout > 0 && !this.isKeepAliveThreadRunning) {
//                  cancelDisconnect = scheduleDisconnectIn(
//                     this.keepAliveConnectTimeout - (System.currentTimeMillis() - start)
//                  )
//               }
               if (channel.inputStream.peek() == -1) {
                  throw EOFException()
               }
            } catch (e: IOException) {
               throw IOException(
                  "Failed to connect to MySQL on $hostname:$port. Please make sure it's running.", e
               )
            }
            val greetingPacket = receiveGreeting()

            logger.info("Connected to $hostname:$port -> $greetingPacket")
            resolveDatabaseVersion(greetingPacket)
            tryUpgradeToSSL(greetingPacket)

            Authenticator(greetingPacket, channel, schema, username, password).authenticate()
            channel.authenticationComplete()

            connectionId = greetingPacket.threadId
            if ("" == binlogFilename) {
               setupGtidSet()
            }
            if (binlogFilename == null) {
               fetchBinlogFilenameAndPosition()
            }
            if (binlogPosition < 4) {
               if (logger.isWarnEnabled) {
                  logger.warn("Binary log position adjusted from $binlogPosition to 4")
               }
               binlogPosition = 4
            }
            setupConnection()
            gtid = null
            tx = false
            requestBinaryLogStream()
         } catch (e: IOException) {
            disconnectChannel()
            throw e
         } finally {
            if (cancelDisconnect != null) {
               try {
                  cancelDisconnect.call()
               } catch (e: Exception) {
                  if (logger.isWarnEnabled) {
                     logger.warn(
                        "\"" + e.message + "\" was thrown while canceling scheduled disconnect call"
                     )
                  }
               }
            }
         }
         this.isConnected = true
         notifyWhenDisconnected = true
         if (logger.isInfoEnabled) {
            val position: String?
            gtidSetAccessLock.withLock {
               position = if (gtidSet != GtidSet.NULL) gtidSet.toString() else "$binlogFilename/$binlogPosition"
            }
            logger.info(
               "Connected to " + hostname + ":" + port + " at " + position + " (" + (if (this.isBlocking) "sid:" + serverId + ", " else "") + "cid:" + connectionId + ")"
            )
         }
         for (lifecycleListener in lifecycleListeners) {
            lifecycleListener.onConnect(this)
         }
         if (this.isKeepAlive && !this.isKeepAliveThreadRunning) {
            spawnKeepAliveThread()
         }
         ensureEventDataDeserializer(EventType.ROTATE, RotateEventDataDeserializer::class.java)
         gtidSetAccessLock.withLock {
            if (this.gtidEnabled) {
               ensureGtidEventDataDeserializer()
            }
         }
         listenForEventPackets()
      } finally {
         connectLock.unlock()
         if (notifyWhenDisconnected) {
            for (lifecycleListener in lifecycleListeners) {
               lifecycleListener.onDisconnect(this)
            }
         }
      }
   }

   private fun resolveDatabaseVersion(packet: GreetingPacket) {
      this.databaseVersion = parse(packet.serverVersion ?: "")
      logger.info("Database version: " + this.databaseVersion)
   }

   /**
    * Apply additional options for connection before requesting binlog stream.
    */
   @Throws(IOException::class)
   protected fun setupConnection() {
      val checksumType = fetchBinlogChecksum()
      if (checksumType != ChecksumType.NONE) {
         confirmSupportOfChecksum(checksumType)
      }
      setMasterServerId()
      if (heartbeatInterval > 0) {
         enableHeartbeat()
      }
   }

   @Throws(IOException::class)
   private fun openChannel(): PacketChannel {
      val socket = if (socketFactory != null) socketFactory!!.createSocket()!! else Socket()
      socket.connect(InetSocketAddress(hostname, port), keepAliveConnectTimeout.toInt())
      return PacketChannel(socket)
   }

   private fun scheduleDisconnectIn(timeout: Long): Callable<*> {
      val self = this
      val connectLatch = CountDownLatch(1)
      val thread = newNamedThread(object : Runnable {
         override fun run() {
            try {
               connectLatch.await(timeout, TimeUnit.MILLISECONDS)
            } catch (e: InterruptedException) {
               if (logger.isWarnEnabled) {
                  logger.warn(e.message)
               }
            }
            if (connectLatch.count != 0L) {
               if (logger.isWarnEnabled) {
                  logger.warn(
                     "Failed to establish connection in " + timeout + "ms. " + "Forcing disconnect."
                  )
               }
               try {
                  self.disconnectChannel()
               } catch (e: IOException) {
                  if (logger.isWarnEnabled) {
                     logger.warn(e.message)
                  }
               }
            }
         }
      }, "blc-disconnect-" + hostname + ":" + port)
      thread.start()
      return object : Callable<Any?> {
         @Throws(Exception::class)
         override fun call(): Any? {
            connectLatch.countDown()
            thread.join()
            return null
         }
      }
   }

   @Throws(IOException::class)
   private fun checkError(packet: ByteArray) {
      if (packet[0] == 0xFF.toByte() /* error */) {
         val bytes = packet.copyOfRange(1, packet.size)
         val errorPacket = ErrorPacket(bytes)
         throw ServerException(
            errorPacket.errorMessage, errorPacket.errorCode, errorPacket.sqlState
         )
      }
   }

   @Throws(IOException::class)
   private fun receiveGreeting(): GreetingPacket {
      val initialHandshakePacket = channel.read()
      checkError(initialHandshakePacket)

      return GreetingPacket.parse(initialHandshakePacket)
   }

   @Throws(IOException::class)
   private fun tryUpgradeToSSL(greetingPacket: GreetingPacket): Boolean {
      val collation = greetingPacket.serverCollation

      if (sslMode != SSLMode.DISABLED) {
         val serverSupportsSSL = (greetingPacket.serverCapabilities and ClientCapabilities.SSL) != 0
         if (!serverSupportsSSL && (sslMode == SSLMode.REQUIRED || sslMode == SSLMode.VERIFY_CA || sslMode == SSLMode.VERIFY_IDENTITY)) {
            throw IOException("MySQL server does not support SSL")
         }
         if (serverSupportsSSL) {
            val sslRequestCommand = SSLRequestCommand()
            sslRequestCommand.setCollation(collation)
            channel.write(sslRequestCommand)
            val sslSocketFactory =
               (if (this.sslSocketFactory != null) this.sslSocketFactory else if (sslMode == SSLMode.REQUIRED || sslMode == SSLMode.PREFERRED) DEFAULT_REQUIRED_SSL_MODE_SOCKET_FACTORY else DEFAULT_VERIFY_CA_SSL_MODE_SOCKET_FACTORY)!!
            channel.upgradeToSSL(
               sslSocketFactory, if (sslMode == SSLMode.VERIFY_IDENTITY) TLSHostnameVerifier() else null
            )
            logger.info("SSL enabled")
            return true
         }
      }
      return false
   }

   @Throws(IOException::class)
   private fun enableHeartbeat() {
      channel.write(QueryCommand("set @master_heartbeat_period=" + heartbeatInterval * 1000000))
      val statementResult = channel.read()!!
      checkError(statementResult)
   }

   @Throws(IOException::class)
   private fun setMasterServerId() {
      channel.write(QueryCommand("select @@server_id"))
      val resultSet = readResultSet()
      this.masterServerId = resultSet[0].getValue(0)!!.toLong()
   }

   @Throws(IOException::class)
   protected fun requestBinaryLogStream() {
      val serverId = if (this.isBlocking) this.serverId else 0 // http://bugs.mysql.com/bug.php?id=71178
      if (this.databaseVersion.isMariaDb) requestBinaryLogStreamMaria(serverId)
      else requestBinaryLogStreamMysql(serverId)
   }

   @Throws(IOException::class)
   private fun requestBinaryLogStreamMysql(serverId: Long) {
      val dumpBinaryLogCommand: Command
      gtidSetAccessLock.withLock {
         dumpBinaryLogCommand = if (this.gtidEnabled) {
            DumpBinaryLogGtidCommand(
               serverId,
               if (this.isUseBinlogFilenamePositionInGtidMode) binlogFilename else "",
               if (this.isUseBinlogFilenamePositionInGtidMode) binlogPosition else 4,
               gtidSet
            )
         } else {
            DumpBinaryLogCommand(serverId, binlogFilename, binlogPosition)
         }
      }
      channel.write(dumpBinaryLogCommand)
   }

   @Throws(IOException::class)
   private fun requestBinaryLogStreamMaria(serverId: Long) {/*
            https://jira.mariadb.org/browse/MDEV-225
      */
      channel.write(QueryCommand("SET @mariadb_slave_capability=$mariaDbSlaveCapability"))
      checkError(channel.read())
      val dumpBinaryLogCommand: Command

      gtidSetAccessLock.withLock {
         if (this.gtidEnabled) {
            logger.info(gtidSet.toString())
            channel.write(QueryCommand("SET @slave_connect_state = '$gtidSet'"))
            checkError(channel.read())
            channel.write(QueryCommand("SET @slave_gtid_strict_mode = 0"))
            checkError(channel.read())
            channel.write(QueryCommand("SET @slave_gtid_ignore_duplicates = 0"))
            checkError(channel.read())
            dumpBinaryLogCommand = DumpBinaryLogCommand(serverId, "", 0L, this.isUseSendAnnotateRowsEvent)
         } else {
            dumpBinaryLogCommand = DumpBinaryLogCommand(
               serverId, binlogFilename, binlogPosition, this.isUseSendAnnotateRowsEvent
            )
         }
      }
      channel.write(dumpBinaryLogCommand)
   }

   private fun ensureEventDataDeserializer(
      eventType: EventType,
      eventDataDeserializerClass: Class<out EventDataDeserializer<*>?>
   ) {
      val eventDataDeserializer = eventDeserializer.getEventDataDeserializer(eventType)
      if (eventDataDeserializer.javaClass != eventDataDeserializerClass && eventDataDeserializer.javaClass != Deserializer::class.java) {
         val internalEventDataDeserializer: EventDataDeserializer<*>?
         try {
            internalEventDataDeserializer = eventDataDeserializerClass.newInstance()
         } catch (e: Exception) {
            throw RuntimeException(e)
         }
         eventDeserializer.setEventDataDeserializer(
            eventType, Deserializer(
               internalEventDataDeserializer, eventDataDeserializer
            )
         )
      }
   }

   private fun ensureGtidEventDataDeserializer() {
      ensureEventDataDeserializer(EventType.GTID, GtidEventDataDeserializer::class.java)
      ensureEventDataDeserializer(EventType.QUERY, QueryEventDataDeserializer::class.java)
      ensureEventDataDeserializer(EventType.ANNOTATE_ROWS, AnnotateRowsEventDataDeserializer::class.java)
      ensureEventDataDeserializer(EventType.MARIADB_GTID, MariadbGtidEventDataDeserializer::class.java)
      ensureEventDataDeserializer(EventType.MARIADB_GTID_LIST, MariadbGtidListEventDataDeserializer::class.java)
   }

   private fun spawnKeepAliveThread() {
      val threadExecutor = Executors.newSingleThreadExecutor { runnable ->
         newNamedThread(
            runnable, "blc-keepalive-$hostname:$port"
         )
      }
      try {
         keepAliveThreadExecutorLock.lock()
         threadExecutor.submit(object : Runnable {
            override fun run() {
               while (!threadExecutor.isShutdown) {
                  try {
                     Thread.sleep(keepAliveInterval)
                  } catch (e: InterruptedException) {
                     // expected in case of disconnect
                  }
                  if (threadExecutor.isShutdown) {
                     logger.info("threadExecutor is shut down, terminating keepalive thread")
                     return
                  }
                  var connectionLost = false
                  if (heartbeatInterval > 0) {
                     connectionLost = System.currentTimeMillis() - eventLastSeen > keepAliveInterval
                  } else {
                     try {
                        channel.write(PingCommand())
                     } catch (e: IOException) {
                        connectionLost = true
                     }
                  }
                  if (connectionLost) {
                     logger.info("Keepalive: Trying to restore lost connection to $hostname:$port")
                     try {
                        terminateConnect(useNonGracefulDisconnect)
                        connect(keepAliveConnectTimeout)
                     } catch (ce: Exception) {
                        logger.warn(
                           "keepalive: Failed to restore connection to $hostname:$port. Next attempt in $keepAliveInterval ms"
                        )
                     }
                  }
               }
            }
         })
         keepAliveThreadExecutor = threadExecutor
      } finally {
         keepAliveThreadExecutorLock.unlock()
      }
   }

   private fun newNamedThread(
      runnable: Runnable,
      threadName: String
   ): Thread {
      val thread = if (threadFactory == null) Thread(runnable) else threadFactory!!.newThread(runnable)
      thread.setName(threadName)
      return thread
   }

   val isKeepAliveThreadRunning: Boolean
      get() {
         keepAliveThreadExecutorLock.withLock {
            return keepAliveThreadExecutor != null && !keepAliveThreadExecutor!!.isShutdown
         }
      }

   /**
    * Connect to the replication stream in a separate thread.
    * @param timeout timeout in milliseconds
    * @throws AuthenticationException if authentication fails
    * @throws ServerException if MySQL server responds with an error
    * @throws IOException if anything goes wrong while trying to connect
    * @throws TimeoutException if a client was unable to connect within given time limit
    */
   @Throws(IOException::class, TimeoutException::class)
   fun connect(timeout: Long) {
      val countDownLatch = CountDownLatch(1)
      val connectListener: AbstractLifecycleListener = object : AbstractLifecycleListener() {
         override fun onConnect(client: BinlogClient) {
            countDownLatch.countDown()
         }
      }
      registerLifecycleListener(connectListener)
      val exceptionReference = AtomicReference<IOException?>()
      val runnable: Runnable = object : Runnable {
         override fun run() {
            try {
               this@BinlogClient.keepAliveConnectTimeout = timeout
               connect()
            } catch (e: IOException) {
               exceptionReference.set(e)
               countDownLatch.countDown() // making sure we don't end up waiting the whole "timeout"
            } catch (e: Exception) {
               exceptionReference.set(IOException(e)) // method is asynchronous, catch all exceptions so that they are not lost
               countDownLatch.countDown() // making sure we don't end up waiting the whole "timeout"
            }
         }
      }
      newNamedThread(runnable, "blc-$hostname:$port").start()
      var started = false
      try {
         started = countDownLatch.await(timeout, TimeUnit.MILLISECONDS)
      } catch (e: InterruptedException) {
         if (logger.isWarnEnabled) {
            logger.warn(e.message)
         }
      }
      unregisterLifecycleListener(connectListener)
      val e = exceptionReference.get()
      if (e != null) {
         throw e
      }
      if (!started) {
         try {
            terminateConnect()
         } finally {
            throw TimeoutException("BinaryLogClient was unable to connect in " + timeout + "ms")
         }
      }
   }

   @Throws(IOException::class)
   private fun fetchGtidPurged(): String {
      channel.write(QueryCommand("show global variables like 'gtid_purged'"))
      val resultSet = readResultSet()
      if (resultSet.isNotEmpty()) {
         return resultSet[0].getValue(1)!!.uppercase(Locale.getDefault())
      }
      return ""
   }

   @Throws(IOException::class)
   private fun setupGtidSet() {
      if (!this.gtidEnabled) return

      gtidSetAccessLock.withLock {
         if (this.databaseVersion.isMariaDb) {
            if (gtidSet == GtidSet.NULL) {
               gtidSet = MariadbGtidSet("")
            } else if (gtidSet !is MariadbGtidSet) {
               throw RuntimeException("Connected to MariaDB but given a mysql GTID set!")
            }
         } else {
            if (gtidSet == GtidSet.NULL && this.isGtidSetFallbackToPurged) {
               gtidSet = GtidSet(fetchGtidPurged())
            } else if (gtidSet == GtidSet.NULL) {
               gtidSet = GtidSet("")
            } else if (gtidSet is MariadbGtidSet) {
               throw RuntimeException("Connected to Mysql but given a MariaDB GTID set!")
            }
         }
      }
   }

   @Throws(IOException::class)
   private fun fetchBinlogFilenameAndPosition() {
      if (!databaseVersion.isMariaDb && databaseVersion.isGreaterThanOrEqualTo(8, 4)) {
         channel.write(QueryCommand("show binary log status"))
      } else if (!databaseVersion.isMariaDb && databaseVersion.isGreaterThanOrEqualTo(8, 0, 22)) {
         channel.write(QueryCommand("show replica status"))
      } else {
         channel.write(QueryCommand("show master status"))
      }
      val resultSet = readResultSet()
      if (resultSet.isEmpty()) {
         throw IOException("Failed to determine binlog filename/position")
      }
      val resultSetRow = resultSet[0]
      binlogFilename = resultSetRow.getValue(0)
      binlogPosition = resultSetRow.getValue(1)!!.toLong()
   }

   @Throws(IOException::class)
   private fun fetchBinlogChecksum(): ChecksumType {
      channel.write(QueryCommand("show global variables like 'binlog_checksum'"))
      val resultSet = readResultSet()
      if (resultSet.isEmpty()) {
         return ChecksumType.NONE
      }
      return ChecksumType.valueOf(resultSet[0].getValue(1)!!.uppercase(Locale.getDefault()))
   }

   @Throws(IOException::class)
   private fun confirmSupportOfChecksum(checksumType: ChecksumType) {
      channel.write(QueryCommand("set @master_binlog_checksum= @@global.binlog_checksum"))
      val statementResult = channel.read()
      checkError(statementResult)
      eventDeserializer.setChecksumType(checksumType)
   }

   @Throws(IOException::class)
   private fun listenForEventPackets() {
      val inputStream = channel.inputStream
      var completeShutdown = false
      try {
         while (inputStream.peek() != -1) {
            val packetLength = inputStream.readInteger(3)
            inputStream.skip(1) // 1 byte for sequence
            val marker = inputStream.read()
            if (marker == 0xFF) {
               val errorPacket = ErrorPacket(inputStream.read(packetLength - 1))
               throw ServerException(
                  errorPacket.errorMessage, errorPacket.errorCode, errorPacket.sqlState
               )
            }
            if (marker == 0xFE && !this.isBlocking) {
               completeShutdown = true
               break
            }
            val event: Event?
            try {
               event = eventDeserializer.nextEvent(
                  if (packetLength == MAX_PACKET_LENGTH) ByteArrayInputStream(
                     readPacketSplitInChunks(
                        inputStream, packetLength - 1
                     )
                  ) else inputStream
               )
               if (event == null) {
                  throw EOFException()
               }
            } catch (e: Exception) {
               val cause = if (e is EventDataDeserializationException) e.cause else e
               if (cause is EOFException || cause is SocketException) {
                  throw e
               }
               if (isConnected) {
                  for (lifecycleListener in lifecycleListeners) {
                     lifecycleListener.onEventDeserializationFailure(this, e)
                  }
               }
               continue
            }
            if (isConnected) {
               eventLastSeen = System.currentTimeMillis()
               updateGtidSet(event)
               notifyEventListeners(event)
               updateClientBinlogFilenameAndPosition(event)
            }
         }
      } catch (e: Exception) {
         if (isConnected) {
            for (lifecycleListener in lifecycleListeners) {
               lifecycleListener.onCommunicationFailure(this, e)
            }
         }
      } finally {
         if (isConnected) {
            if (completeShutdown) {
               disconnect() // initiate a complete shutdown sequence (which includes keep alive thread)
            } else {
               disconnectChannel()
            }
         }
      }
   }

   @Throws(IOException::class)
   private fun readPacketSplitInChunks(
      inputStream: ByteArrayInputStream,
      packetLength: Int
   ): ByteArray {
      var result = inputStream.read(packetLength)
      var chunkLength: Int
      do {
         chunkLength = inputStream.readInteger(3)
         inputStream.skip(1) // 1 byte for sequence
         result = result.copyOf(result.size + chunkLength)
         inputStream.fill(result, result.size - chunkLength, chunkLength)
      } while (chunkLength == Packet.MAX_LENGTH)
      return result
   }

   private fun updateClientBinlogFilenameAndPosition(event: Event) {
      val eventHeader = event.header
      val eventType: EventType? = eventHeader.eventType
      if (eventType == EventType.ROTATE) {
         val rotateEventData = internal(event.data) as RotateEventData?
         binlogFilename = rotateEventData!!.binlogFilename
         binlogPosition = rotateEventData.binlogPosition
      } else  // do not update binlogPosition on TABLE_MAP so that in case of reconnect (using a different instance of
      // client) table mapping cache could be reconstructed before hitting row mutation event
         if (eventType != EventType.TABLE_MAP && eventHeader is EventHeaderV4) {
            val trackableEventHeader = eventHeader
            val nextBinlogPosition = trackableEventHeader.nextPosition
            if (nextBinlogPosition > 0) {
               binlogPosition = nextBinlogPosition
            }
         }
   }

   fun updateGtidSet(event: Event) {
      gtidSetAccessLock.withLock {
         if (gtidSet == GtidSet.NULL) {
            return
         }
      }
      val eventHeader = event.header
      when (eventHeader.eventType) {
         EventType.GTID -> {
            val gtidEventData = internal(event.data) as GtidEventData?
            gtid = gtidEventData!!.mySqlGtid
         }

         EventType.XID -> {
            commitGtid()
            tx = false
         }

         EventType.QUERY -> {
            val queryEventData = internal(event.data) as QueryEventData?
            val sql = queryEventData?.sql
            if (sql != null) {
               commitGtid(sql)
            }
         }

         EventType.ANNOTATE_ROWS -> {
            val annotateRowsEventData = internal(event.data) as AnnotateRowsEventData?
            val sql = annotateRowsEventData?.rowsQuery
            if (sql != null) {
               commitGtid(sql)
            }
         }

         EventType.MARIADB_GTID -> {
            val mariadbGtidEventData = internal(event.data) as MariadbGtidEventData?
            mariadbGtidEventData!!.serverId = eventHeader.serverId
            gtid = mariadbGtidEventData.toString()
         }

         EventType.MARIADB_GTID_LIST -> {
            val mariadbGtidListEventData = internal(event.data) as MariadbGtidListEventData?
            gtid = mariadbGtidListEventData!!.mariaGTIDSet.toString()
         }

         else -> {}
      }
   }

   private fun commitGtid(sql: String?) {
      if ("BEGIN" == sql) {
         tx = true
      } else if ("COMMIT" == sql || "ROLLBACK" == sql) {
         commitGtid()
         tx = false
      } else if (!tx) {
         // auto-commit query, likely DDL
         commitGtid()
      }
   }

   private fun commitGtid() {
      if (gtid != null) {
         gtidSetAccessLock.withLock {
            if (gtidSet == GtidSet.NULL) {
               gtidSet = GtidSet("")
            }
            gtidSet.addGtid(gtid)
         }
      }
   }

   @Throws(IOException::class)
   private fun readResultSet(): Array<ResultSetRowPacket> {
      val resultSet: MutableList<ResultSetRowPacket> = LinkedList<ResultSetRowPacket>()
      val statementResult = channel.read()
      checkError(statementResult)

      while ((channel.read())[0] != 0xFE.toByte() /* eof */) { /* skip */
      }
      var bytes: ByteArray?
      while ((channel.read().also { bytes = it })[0] != 0xFE.toByte() /* eof */) {
         checkError(bytes!!)
         resultSet.add(ResultSetRowPacket(bytes))
      }
      return resultSet.toTypedArray<ResultSetRowPacket>()
   }

   /**
    * @return registered event listeners
    */
   fun getEventListeners(): MutableList<EventListener?> {
      return Collections.unmodifiableList<EventListener?>(eventListeners)
   }

   /**
    * Register event listener. Note that multiple event listeners will be called in order they
    * where registered.
    * @param eventListener event listener
    */
   fun registerEventListener(eventListener: EventListener?) {
      eventListeners.add(eventListener!!)
   }

   /**
    * Unregister all event listener of specific type.
    * @param listenerClass event listener class to unregister
    */
   fun unregisterEventListener(listenerClass: Class<out EventListener?>) {
      for (eventListener in eventListeners) {
         if (listenerClass.isInstance(eventListener)) {
            eventListeners.remove(eventListener)
         }
      }
   }

   /**
    * Unregister single event listener.
    * @param eventListener event listener to unregister
    */
   fun unregisterEventListener(eventListener: EventListener?) {
      eventListeners.remove(eventListener)
   }

   private fun notifyEventListeners(event: Event) {
      var event = event
      if (event.data is EventDataWrapper) {
         event = Event(event.header, event.data.external)
      }
      for (eventListener in eventListeners) {
         try {
            eventListener.onEvent(event)
         } catch (e: Exception) {
            if (logger.isWarnEnabled) {
               logger.warn("$eventListener choked on $event", e)
            }
         }
      }
   }

   /**
    * @return registered lifecycle listeners
    */
   fun getLifecycleListeners(): List<LifecycleListener> {
      return lifecycleListeners
   }

   /**
    * Register a lifecycle listener. Note that multiple lifecycle listeners will be called in order they
    * where registered.
    * @param lifecycleListener lifecycle listener to register
    */
   fun registerLifecycleListener(lifecycleListener: LifecycleListener) {
      lifecycleListeners.add(lifecycleListener)
   }

   /**
    * Unregister all lifecycle listener of specific type.
    * @param listenerClass lifecycle listener class to unregister
    */
   fun unregisterLifecycleListener(listenerClass: Class<out LifecycleListener?>) {
      for (lifecycleListener in lifecycleListeners) {
         if (listenerClass.isInstance(lifecycleListener)) {
            lifecycleListeners.remove(lifecycleListener)
         }
      }
   }

   /**
    * Unregister single lifecycle listener.
    * @param eventListener lifecycle listener to unregister
    */
   fun unregisterLifecycleListener(eventListener: LifecycleListener?) {
      lifecycleListeners.remove(eventListener)
   }

   /**
    * Disconnect from the replication stream.
    * Note that this does not cause binlogFilename/binlogPosition to be cleared out.
    * As a result, the following [.connect] resumes client from where it left off.
    */
   @Throws(IOException::class)
   fun disconnect() {
      terminateKeepAliveThread()
      terminateConnect()
   }

   private fun terminateKeepAliveThread() {
      try {
         keepAliveThreadExecutorLock.lock()
         val keepAliveThreadExecutor = this.keepAliveThreadExecutor
         if (keepAliveThreadExecutor == null) {
            return
         }
         keepAliveThreadExecutor.shutdownNow()
      } finally {
         keepAliveThreadExecutorLock.unlock()
      }
      while (!awaitTerminationInterruptibly(
            keepAliveThreadExecutor!!,
            Long.Companion.MAX_VALUE,
            TimeUnit.NANOSECONDS
         )
      ) {
         // ignore
      }
   }

   @Throws(IOException::class)
   private fun terminateConnect(force: Boolean = false) {
      do {
         disconnectChannel(force)
      } while (!tryLockInterruptibly(connectLock, 1000, TimeUnit.MILLISECONDS))
      connectLock.unlock()
   }

   @Throws(IOException::class)
   private fun disconnectChannel(force: Boolean = false) {
      this.isConnected = false
      if (channel != null && channel.isOpen) {
         if (force) {
            channel.setShouldUseSoLinger0()
         }
         channel.close()
      }
   }

   /**
    * [BinlogClient]'s event listener.
    */
   interface EventListener {
      fun onEvent(event: Event)
   }

   /**
    * [BinlogClient]'s lifecycle listener.
    */
   interface LifecycleListener {
      /**
       * Called once client has successfully logged in but before started to receive binlog events.
       * @param client the client that logged in
       */
      fun onConnect(client: BinlogClient)

      /**
       * It's guarantied to be called before [.onDisconnect]) in case of
       * communication failure.
       * @param client the client that triggered the communication failure
       * @param ex The exception that triggered the communication failutre
       */
      fun onCommunicationFailure(
         client: BinlogClient,
         ex: Exception?
      )

      /**
       * Called in case of failed event deserialization. Note this type of error does NOT cause client to
       * disconnect. If you wish to stop receiving events you'll need to fire client.disconnect() manually.
       * @param client the client that failed event deserialization
       * @param ex The exception that triggered the failutre
       */
      fun onEventDeserializationFailure(
         client: BinlogClient,
         ex: Exception?
      )

      /**
       * Called upon disconnect (regardless of the reason).
       * @param client the client that disconnected
       */
      fun onDisconnect(client: BinlogClient)
   }

   /**
    * Default (no-op) implementation of [LifecycleListener].
    */
   abstract class AbstractLifecycleListener : LifecycleListener {
      override fun onConnect(client: BinlogClient) {}

      override fun onCommunicationFailure(
         client: BinlogClient,
         ex: Exception?
      ) {
      }

      override fun onEventDeserializationFailure(
         client: BinlogClient,
         ex: Exception?
      ) {
      }

      override fun onDisconnect(client: BinlogClient) {}
   }

   companion object {
      val DEFAULT_REQUIRED_SSL_MODE_SOCKET_FACTORY: SSLSocketFactory = object : DefaultSSLSocketFactory() {
         @Throws(GeneralSecurityException::class)
         override fun initSSLContext(sc: SSLContext) {
            sc.init(null, arrayOf<TrustManager>(object : X509TrustManager {
               @Throws(CertificateException::class)
               override fun checkClientTrusted(
                  x509Certificates: Array<X509Certificate?>?,
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
                  return arrayOfNulls<X509Certificate>(0)
               }
            }), null)
         }
      }

      val DEFAULT_VERIFY_CA_SSL_MODE_SOCKET_FACTORY: SSLSocketFactory = DefaultSSLSocketFactory()

      // https://dev.mysql.com/doc/internals/en/sending-more-than-16mbyte.html
      private const val MAX_PACKET_LENGTH = 16777215

      private fun awaitTerminationInterruptibly(
         executorService: ExecutorService,
         timeout: Long,
         unit: TimeUnit
      ): Boolean {
         return try {
            executorService.awaitTermination(timeout, unit)
         } catch (e: InterruptedException) {
            false
         }
      }

      private fun tryLockInterruptibly(
         lock: Lock,
         time: Long,
         unit: TimeUnit
      ): Boolean {
         return try {
            lock.tryLock(time, unit)
         } catch (e: InterruptedException) {
            false
         }
      }
   }
}
