package bop.mysql.binlog.event.deserialization

import bop.mysql.binlog.event.deserialization.EventDeserializer.EventDataWrapper.Deserializer
import bop.mysql.event.Event
import bop.mysql.event.EventData
import bop.mysql.event.EventHeader
import bop.mysql.event.EventHeaderV4
import bop.mysql.event.EventType
import bop.mysql.event.FormatDescriptionEventData
import bop.mysql.event.LRUCache
import bop.mysql.event.TableMapEventData
import bop.mysql.event.TransactionPayloadEventData
import bop.mysql.io.ByteArrayInputStream
import java.io.IOException
import java.util.*

class EventDeserializer {
   private val eventHeaderDeserializer: EventHeaderDeserializer<*>
   private val defaultEventDataDeserializer: EventDataDeserializer<*>
   private val eventDataDeserializers: MutableMap<EventType, EventDataDeserializer<*>>

   private var compatibilitySet: EnumSet<CompatibilityMode> =
      EnumSet.noneOf<CompatibilityMode>(CompatibilityMode::class.java)
   private var checksumLength = 0

   private val tableMapEventByTableId: LRUCache<Long, TableMapEventData>

   private var tableMapEventDataDeserializer: EventDataDeserializer<*>? = null
   private var formatDescEventDataDeserializer: EventDataDeserializer<*>? = null

   constructor(defaultEventDataDeserializer: EventDataDeserializer<*>) : this(
      EventHeaderV4Deserializer(), defaultEventDataDeserializer
   )

   constructor(
      eventHeaderDeserializer: EventHeaderDeserializer<*> = EventHeaderV4Deserializer(),
      defaultEventDataDeserializer: EventDataDeserializer<*> = NullEventDataDeserializer()
   ) {
      this.eventHeaderDeserializer = eventHeaderDeserializer
      this.defaultEventDataDeserializer = defaultEventDataDeserializer
      this.eventDataDeserializers = IdentityHashMap<EventType, EventDataDeserializer<*>>()
      this.tableMapEventByTableId = LRUCache<Long, TableMapEventData>(100, 0.75f, 10000)
      registerDefaultEventDataDeserializers()
      afterEventDataDeserializerSet(null)
   }

   constructor(
      eventHeaderDeserializer: EventHeaderDeserializer<*>,
      defaultEventDataDeserializer: EventDataDeserializer<*>,
      eventDataDeserializers: MutableMap<EventType, EventDataDeserializer<*>>,
      tableMapEventByTableId: LRUCache<Long, TableMapEventData>
   ) {
      this.eventHeaderDeserializer = eventHeaderDeserializer
      this.defaultEventDataDeserializer = defaultEventDataDeserializer
      this.eventDataDeserializers = eventDataDeserializers
      this.tableMapEventByTableId = tableMapEventByTableId
      afterEventDataDeserializerSet(null)
   }

   private fun registerDefaultEventDataDeserializers() {
      eventDataDeserializers.put(
         EventType.FORMAT_DESCRIPTION, FormatDescriptionEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.ROTATE, RotateEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.INTVAR, IntVarEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.QUERY, QueryEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.TABLE_MAP, TableMapEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.XID, XidEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.WRITE_ROWS, WriteRowsEventDataDeserializer(tableMapEventByTableId)
      )
      eventDataDeserializers.put(
         EventType.UPDATE_ROWS, UpdateRowsEventDataDeserializer(tableMapEventByTableId)
      )
      eventDataDeserializers.put(
         EventType.DELETE_ROWS, DeleteRowsEventDataDeserializer(tableMapEventByTableId)
      )
      eventDataDeserializers.put(
         EventType.EXT_WRITE_ROWS,
         WriteRowsEventDataDeserializer(tableMapEventByTableId).setMayContainExtraInformation(true)
      )
      eventDataDeserializers.put(
         EventType.EXT_UPDATE_ROWS,
         UpdateRowsEventDataDeserializer(tableMapEventByTableId).setMayContainExtraInformation(true)
      )
      eventDataDeserializers.put(
         EventType.EXT_DELETE_ROWS,
         DeleteRowsEventDataDeserializer(tableMapEventByTableId).setMayContainExtraInformation(true)
      )
      eventDataDeserializers.put(
         EventType.ROWS_QUERY, RowsQueryEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.GTID, GtidEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.PREVIOUS_GTIDS, PreviousGtidSetDeserializer()
      )
      eventDataDeserializers.put(
         EventType.XA_PREPARE, XAPrepareEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.ANNOTATE_ROWS, AnnotateRowsEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.MARIADB_GTID, MariadbGtidEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.BINLOG_CHECKPOINT, BinlogCheckpointEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.MARIADB_GTID_LIST, MariadbGtidListEventDataDeserializer()
      )
      eventDataDeserializers.put(
         EventType.TRANSACTION_PAYLOAD, TransactionPayloadEventDataDeserializer()
      )
   }

   fun setEventDataDeserializer(
      eventType: EventType,
      eventDataDeserializer: EventDataDeserializer<*>
   ) {
      ensureCompatibility(eventDataDeserializer)
      eventDataDeserializers.put(eventType, eventDataDeserializer)
      afterEventDataDeserializerSet(eventType)
   }

   private fun afterEventDataDeserializerSet(eventType: EventType?) {
      if (eventType == null || eventType == EventType.TABLE_MAP) {
         val eventDataDeserializer = getEventDataDeserializer(EventType.TABLE_MAP)
         if (eventDataDeserializer.javaClass != TableMapEventDataDeserializer::class.java && eventDataDeserializer.javaClass != Deserializer::class.java) {
            tableMapEventDataDeserializer = Deserializer(
               TableMapEventDataDeserializer(), eventDataDeserializer
            )
         } else {
            tableMapEventDataDeserializer = null
         }
      }
      if (eventType == null || eventType == EventType.FORMAT_DESCRIPTION) {
         val eventDataDeserializer = getEventDataDeserializer(EventType.FORMAT_DESCRIPTION)
         if (eventDataDeserializer.javaClass != FormatDescriptionEventDataDeserializer::class.java && eventDataDeserializer.javaClass != Deserializer::class.java) {
            formatDescEventDataDeserializer = Deserializer(
               FormatDescriptionEventDataDeserializer(), eventDataDeserializer
            )
         } else {
            formatDescEventDataDeserializer = null
         }
      }
   }

   /**
    * @param checksumType don't use this function.
    */
   @Deprecated(
      """resolved based on FORMAT_DESCRIPTION
	  """
   )
   fun setChecksumType(checksumType: ChecksumType) {
      this.checksumLength = checksumType.length
   }

   /**
    * @see CompatibilityMode
    *
    * @param first at least one CompatabilityMode
    * @param rest many modes
    */
   fun setCompatibilityMode(
      first: CompatibilityMode,
      vararg rest: CompatibilityMode?
   ) {
      this.compatibilitySet = EnumSet.of<CompatibilityMode?>(first, *rest)
      for (eventDataDeserializer in eventDataDeserializers.values) {
         ensureCompatibility(eventDataDeserializer)
      }
   }

   private fun ensureCompatibility(eventDataDeserializer: EventDataDeserializer<*>?) {
      if (eventDataDeserializer is AbstractRowsEventDataDeserializer<*>) {
         val deserializer = eventDataDeserializer
         val deserializeDateAndTimeAsLong =
            compatibilitySet.contains(CompatibilityMode.DATE_AND_TIME_AS_LONG) || compatibilitySet.contains(
               CompatibilityMode.DATE_AND_TIME_AS_LONG_MICRO
            )
         deserializer.setDeserializeDateAndTimeAsLong(deserializeDateAndTimeAsLong)
         deserializer.setMicrosecondsPrecision(
            compatibilitySet.contains(CompatibilityMode.DATE_AND_TIME_AS_LONG_MICRO)
         )
         if (compatibilitySet.contains(CompatibilityMode.INVALID_DATE_AND_TIME_AS_ZERO)) {
            deserializer.setInvalidDateAndTimeRepresentation(0L)
         }
         if (compatibilitySet.contains(CompatibilityMode.INVALID_DATE_AND_TIME_AS_NEGATIVE_ONE)) {
            require(deserializeDateAndTimeAsLong) {
               "INVALID_DATE_AND_TIME_AS_NEGATIVE_ONE requires " + "DATE_AND_TIME_AS_LONG or DATE_AND_TIME_AS_LONG_MICRO"
            }
            deserializer.setInvalidDateAndTimeRepresentation(-1L)
         }
         if (compatibilitySet.contains(CompatibilityMode.INVALID_DATE_AND_TIME_AS_MIN_VALUE)) {
            require(deserializeDateAndTimeAsLong) {
               "INVALID_DATE_AND_TIME_AS_MIN_VALUE requires " + "DATE_AND_TIME_AS_LONG or DATE_AND_TIME_AS_LONG_MICRO"
            }
            deserializer.setInvalidDateAndTimeRepresentation(Long.Companion.MIN_VALUE)
         }
         deserializer.setDeserializeCharAndBinaryAsByteArray(
            compatibilitySet.contains(CompatibilityMode.CHAR_AND_BINARY_AS_BYTE_ARRAY)
         )
         deserializer.setDeserializeIntegerAsByteArray(
            compatibilitySet.contains(CompatibilityMode.INTEGER_AS_BYTE_ARRAY)
         )
      }
   }

   /**
    * @return deserialized event or null in case of end-of-stream
    * @param inputStream input stream to fetch event from
    * @throws IOException if connection gets closed
    */
   @Throws(IOException::class)
   fun nextEvent(inputStream: ByteArrayInputStream): Event? {
      if (inputStream.peek() == -1) {
         return null
      }
      val eventHeader = eventHeaderDeserializer.deserialize(inputStream)
      val eventData: EventData
      when (eventHeader.eventType) {
         EventType.FORMAT_DESCRIPTION -> eventData =
            deserializeFormatDescriptionEventData(inputStream, eventHeader)

         EventType.TABLE_MAP -> eventData = deserializeTableMapEventData(inputStream, eventHeader)
         EventType.TRANSACTION_PAYLOAD -> eventData =
            deserializeTransactionPayloadEventData(inputStream, eventHeader)

         else -> {
            val eventDataDeserializer = getEventDataDeserializer(
               eventHeader.eventType
            )
            eventData = deserializeEventData(inputStream, eventHeader, eventDataDeserializer)
         }
      }
      return Event(eventHeader, eventData)
   }

   @Throws(EventDataDeserializationException::class)
   private fun deserializeFormatDescriptionEventData(
      inputStream: ByteArrayInputStream,
      eventHeader: EventHeader
   ): EventData {
      val eventDataDeserializer =
         (if (formatDescEventDataDeserializer != null) formatDescEventDataDeserializer else getEventDataDeserializer(
            EventType.FORMAT_DESCRIPTION
         ))!!
      val eventBodyLength = eventHeader.dataLength as Int
      var eventData: EventData
      try {
         inputStream.enterBlock(eventBodyLength)
         try {
            eventData = eventDataDeserializer.deserialize(inputStream)!!
            // https://dev.mysql.com/worklog/task/?id=2540#tabs-2540-4
            // +-----------+------------+-----------+------------------------+----------+
            // | Header    | Payload (dataLength)   | Checksum Type (1 byte) | Checksum |
            // +-----------+------------+-----------+------------------------+----------+
            //             |                    (eventBodyLength)                       |
            //             +------------------------------------------------------------+
            val formatDescriptionEvent: FormatDescriptionEventData?
            if (eventData is EventDataWrapper) {
               val eventDataWrapper = eventData
               formatDescriptionEvent = eventDataWrapper.internal as FormatDescriptionEventData?
               if (formatDescEventDataDeserializer != null) {
                  eventData = eventDataWrapper.external
               }
            } else {
               formatDescriptionEvent = eventData as FormatDescriptionEventData?
            }
            checksumLength = formatDescriptionEvent!!.checksumType!!.length
         } finally {
            inputStream.skipToTheEndOfTheBlock()
         }
      } catch (e: IOException) {
         throw EventDataDeserializationException(eventHeader, e)
      }
      return eventData
   }

   @Throws(IOException::class)
   fun deserializeTransactionPayloadEventData(
      inputStream: ByteArrayInputStream,
      eventHeader: EventHeader
   ): EventData {
      val eventDataDeserializer: EventDataDeserializer<*> =
         eventDataDeserializers.get(EventType.TRANSACTION_PAYLOAD)!!
      val eventData = deserializeEventData(inputStream, eventHeader, eventDataDeserializer)
      val transactionPayloadEventData = eventData as TransactionPayloadEventData

      /**
       * Handling for TABLE_MAP events withing the transaction payload event. This is to ensure that for the table map
       * events within the transaction payload, the target table id and the event gets added to the
       * tableMapEventByTableId map. This is map is later used while deserializing rows.
       */
      for (event in transactionPayloadEventData.uncompressedEvents!!) {
         if (event.header.eventType === EventType.TABLE_MAP && event.data != null) {
            val tableMapEvent = event.data as TableMapEventData
            tableMapEventByTableId.put(tableMapEvent.tableId, tableMapEvent)
         }
      }
      return eventData
   }

   @Throws(IOException::class)
   fun deserializeTableMapEventData(
      inputStream: ByteArrayInputStream,
      eventHeader: EventHeader
   ): EventData {
      val eventDataDeserializer =
         (if (tableMapEventDataDeserializer != null) tableMapEventDataDeserializer else getEventDataDeserializer(
            EventType.TABLE_MAP
         ))!!
      var eventData = deserializeEventData(inputStream, eventHeader, eventDataDeserializer)
      val tableMapEvent: TableMapEventData
      if (eventData is EventDataWrapper) {
         val eventDataWrapper = eventData
         tableMapEvent = eventDataWrapper.internal as TableMapEventData
         if (tableMapEventDataDeserializer != null) {
            eventData = eventDataWrapper.external
         }
      } else {
         tableMapEvent = eventData as TableMapEventData
      }
      tableMapEventByTableId.put(tableMapEvent.tableId, tableMapEvent)
      return eventData
   }

   @Throws(EventDataDeserializationException::class)
   private fun deserializeEventData(
      inputStream: ByteArrayInputStream,
      eventHeader: EventHeader,
      eventDataDeserializer: EventDataDeserializer<*>
   ): EventData {
      val eventBodyLength = eventHeader.dataLength as Int - checksumLength
      var eventData: EventData
      try {
         inputStream.enterBlock(eventBodyLength)
         try {
            eventData = eventDataDeserializer.deserialize(inputStream)!!
         } finally {
            inputStream.skipToTheEndOfTheBlock()
            inputStream.skip(checksumLength.toLong())
         }
      } catch (e: IOException) {
         throw EventDataDeserializationException(eventHeader, e)
      }
      return eventData
   }

   fun getEventDataDeserializer(eventType: EventType?): EventDataDeserializer<*> {
      val eventDataDeserializer = eventDataDeserializers.get(eventType)
      return (if (eventDataDeserializer != null) eventDataDeserializer else defaultEventDataDeserializer)!!
   }

   /**
    * @see CompatibilityMode.DATE_AND_TIME_AS_LONG
    *
    * @see CompatibilityMode.DATE_AND_TIME_AS_LONG_MICRO
    *
    * @see CompatibilityMode.INVALID_DATE_AND_TIME_AS_ZERO
    *
    * @see CompatibilityMode.INVALID_DATE_AND_TIME_AS_MIN_VALUE
    *
    * @see CompatibilityMode.CHAR_AND_BINARY_AS_BYTE_ARRAY
    */
   enum class CompatibilityMode {
      /**
       * Return DATETIME/DATETIME_V2/TIMESTAMP/TIMESTAMP_V2/DATE/TIME/TIME_V2 values as long|s
       * (number of milliseconds since the epoch (00:00:00 Coordinated Universal Time (UTC), Thursday, 1 January 1970,
       * not counting leap seconds)) (instead of java.util.Date/java.sql.Timestamp/java.sql.Date/new java.sql.Time).
       *
       *
       * This option is going to be enabled by default starting from mysql-binlog-connector-java@1.0.0.
       */
      DATE_AND_TIME_AS_LONG,

      /**
       * Same as [CompatibilityMode.DATE_AND_TIME_AS_LONG] but values are returned in microseconds.
       */
      DATE_AND_TIME_AS_LONG_MICRO,

      /**
       * Return 0 instead of null if year/month/day is 0.
       * Affects DATETIME/DATETIME_V2/DATE/TIME/TIME_V2.
       */
      INVALID_DATE_AND_TIME_AS_ZERO,

      /**
       * Return -1 instead of null if year/month/day is 0.
       * Affects DATETIME/DATETIME_V2/DATE/TIME/TIME_V2.
       *
       */
      @Deprecated("")
      INVALID_DATE_AND_TIME_AS_NEGATIVE_ONE,

      /**
       * Return Long.MIN_VALUE instead of null if year/month/day is 0.
       * Affects DATETIME/DATETIME_V2/DATE/TIME/TIME_V2.
       */
      INVALID_DATE_AND_TIME_AS_MIN_VALUE,

      /**
       * Return CHAR/VARCHAR/BINARY/VARBINARY values as byte[]|s (instead of String|s).
       *
       *
       * This option is going to be enabled by default starting from mysql-binlog-connector-java@1.0.0.
       */
      CHAR_AND_BINARY_AS_BYTE_ARRAY,

      /**
       * Return TINY/SHORT/INT24/LONG/LONGLONG values as byte[]|s (instead of int|s).
       */
      INTEGER_AS_BYTE_ARRAY
   }

   /**
    * Enwraps internal [EventData] if custom [EventDataDeserializer] is provided (for internally used
    * events only).
    */
   data class EventDataWrapper(
      val internal: EventData,
      val external: EventData
   ) : EventData {
      /**
       * [EventDeserializer.EventDataWrapper] deserializer.
       */
      class Deserializer(
         val internal: EventDataDeserializer<*>,
         val external: EventDataDeserializer<*>
      ) : EventDataDeserializer<EventData> {
         @Throws(IOException::class)
         override fun deserialize(inputStream: ByteArrayInputStream): EventData {
            val bytes = inputStream.read(inputStream.available())
            val internalEventData = internal.deserialize(ByteArrayInputStream(bytes))
            val externalEventData = external.deserialize(ByteArrayInputStream(bytes))
            return EventDataWrapper(internalEventData, externalEventData!!)
         }
      }

      companion object {
         @JvmStatic
         fun internal(eventData: EventData): EventData {
            return if (eventData is EventDataWrapper) eventData.internal else eventData
         }
      }
   }
}

/**
 * @param <T> event data this deserializer is responsible for
 */
interface EventDataDeserializer<T : EventData> {
   @Throws(IOException::class)
   fun deserialize(inputStream: ByteArrayInputStream): T
}

/**
 * @param <T> event header this deserializer is responsible for
 */
interface EventHeaderDeserializer<T : EventHeader> {
   @Throws(IOException::class)
   fun deserialize(inputStream: ByteArrayInputStream): T
}

class EventHeaderV4Deserializer : EventHeaderDeserializer<EventHeaderV4> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): EventHeaderV4 {
      val header = EventHeaderV4()
      header.timestamp = inputStream.readLong(4) * 1000L
      header.eventType = getEventType(inputStream.readInteger(1))
      header.serverId = inputStream.readLong(4)
      header.eventLength = inputStream.readLong(4)
      header.nextPosition = inputStream.readLong(4)
      header.flags = inputStream.readInteger(2)
      return header
   }

   companion object {
      fun getEventType(ordinal: Int): EventType =
         EventType.byEventNumber(ordinal) ?: EventType.UNKNOWN
   }
}

/**
 * @see  [MySQL --binlog-checksum option](https://dev.mysql.com/doc/refman/5.6/en/replication-options-binary-log.html.option_mysqld_binlog-checksum) *
 */
enum class ChecksumType(
   val length: Int
) {
   NONE(0),

   CRC32(4);

   companion object {
      private val VALUES: Array<ChecksumType?> = entries.toTypedArray()

      @JvmStatic
      fun byOrdinal(ordinal: Int): ChecksumType? {
         return VALUES[ordinal]
      }
   }
}

enum class ColumnType(val code: Int) {
   DECIMAL(0),
   TINY(1),
   SHORT(2),
   LONG(3),
   FLOAT(4),
   DOUBLE(5),
   NULL(6),
   TIMESTAMP(7),
   LONGLONG(8),
   INT24(9),
   DATE(10),
   TIME(11),
   DATETIME(12),
   YEAR(13),
   NEWDATE(14),
   VARCHAR(15),
   BIT(16),

   // (TIMESTAMP|DATETIME|TIME)_V2 data types appeared in MySQL 5.6.4
   // @see http://dev.mysql.com/doc/internals/en/date-and-time-data-type-representation.html
   TIMESTAMP_V2(17),
   DATETIME_V2(18),
   TIME_V2(19),
   JSON(245),
   NEWDECIMAL(246),
   ENUM(247),
   SET(248),
   TINY_BLOB(249),
   MEDIUM_BLOB(250),
   LONG_BLOB(251),
   BLOB(252),
   VAR_STRING(253),
   STRING(254),
   GEOMETRY(255);

   companion object {
      private val INDEX_BY_CODE = HashMap<Int, ColumnType>()

      init {
         for (columnType in entries) {
            INDEX_BY_CODE.put(columnType.code, columnType)
         }
      }

      @JvmStatic
      fun byCode(code: Int): ColumnType? {
         return INDEX_BY_CODE.get(code)
      }
   }
}