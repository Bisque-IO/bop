package bop.mysql.binlog

import bop.mysql.event.Event
import bop.mysql.event.EventHeader
import bop.mysql.event.EventType
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference

class BinlogStats : BinlogClient.EventListener, BinlogClient.LifecycleListener {
   private val lastEventHeader = AtomicReference<EventHeader?>()
   private val timestampOfLastEvent = AtomicLong()
   private val totalNumberOfEventsSeen = AtomicLong()
   private val totalBytesReceived = AtomicLong()
   private val numberOfSkippedEvents = AtomicLong()
   private val numberOfDisconnects = AtomicLong()

   constructor()

   constructor(binlogClient: BinlogClient) {
      binlogClient.registerEventListener(this)
      binlogClient.registerLifecycleListener(this)
   }

   fun getLastEvent(): String? {
      val eventHeader = lastEventHeader.get()
      return if (eventHeader == null) null else (eventHeader.eventType.name + "/" + eventHeader.timestamp + " from server " + eventHeader.serverId)
   }

   fun getSecondsSinceLastEvent(): Long {
      val timestamp = timestampOfLastEvent.get()
      return if (timestamp == 0L) 0 else (this.currentTimeMillis - timestamp) / 1000
   }

   fun getSecondsBehindMaster(): Long {
      // because lastEventHeader and timestampOfLastEvent are not guarded by the common lock
      // we may get some "distorted" results, though shouldn't be a problem given the nature of the final value
      val timestamp = timestampOfLastEvent.get()
      val eventHeader = lastEventHeader.get()
      if (timestamp == 0L || eventHeader == null) {
         return -1
      }
      if (eventHeader.eventType === EventType.HEARTBEAT && eventHeader.timestamp == 0L) {
         return 0
      }
      return (timestamp - eventHeader.timestamp) / 1000
   }

   fun getTotalNumberOfEventsSeen(): Long {
      return totalNumberOfEventsSeen.get()
   }

   fun getTotalBytesReceived(): Long {
      return totalBytesReceived.get()
   }

   fun getNumberOfSkippedEvents(): Long {
      return numberOfSkippedEvents.get()
   }

   fun getNumberOfDisconnects(): Long {
      return numberOfDisconnects.get()
   }

   fun reset() {
      lastEventHeader.set(null)
      timestampOfLastEvent.set(0)
      totalNumberOfEventsSeen.set(0)
      totalBytesReceived.set(0)
      numberOfSkippedEvents.set(0)
      numberOfDisconnects.set(0)
   }

   override fun onEvent(event: Event) {
      val header = event.header
      lastEventHeader.set(header)
      timestampOfLastEvent.set(this.currentTimeMillis)
      totalNumberOfEventsSeen.getAndIncrement()
      totalBytesReceived.getAndAdd(header.headerLength + header.dataLength)
   }

   override fun onEventDeserializationFailure(
      client: BinlogClient,
      ex: Exception?
   ) {
      numberOfSkippedEvents.getAndIncrement()
      lastEventHeader.set(null)
      timestampOfLastEvent.set(this.currentTimeMillis)
      totalNumberOfEventsSeen.getAndIncrement()
   }

   override fun onDisconnect(client: BinlogClient) {
      numberOfDisconnects.getAndIncrement()
   }

   override fun onConnect(client: BinlogClient) {
   }

   override fun onCommunicationFailure(
      client: BinlogClient,
      ex: Exception?
   ) {
   }

   val currentTimeMillis: Long
      get() = System.currentTimeMillis()
}