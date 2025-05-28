package bop.mysql.event

import bop.mysql.binlog.MariadbGtidSet
import bop.mysql.binlog.event.deserialization.ChecksumType
import java.io.Serializable
import java.util.BitSet
import java.util.UUID

interface EventHeader : Serializable {
   val timestamp: Long
   val eventType: EventType
   val serverId: Long
   val headerLength: Long
   val dataLength: Long
}

interface EventData : Serializable {
   companion object {
      val NULL = object : EventData {}
   }
}

data class Event(val header: EventHeader, val data: EventData) : Serializable

data class EventHeaderV4(
   // v1 (MySQL 3.23)
   override var timestamp: Long = 0,

   override var eventType: EventType = EventType.UNKNOWN,

   override var serverId: Long = 0,

   var eventLength: Long = 0,

// v3 (MySQL 4.0.2-4.1)

   var nextPosition: Long = 0,

   var flags: Int = 0

) : EventHeader {

   val position: Long
      get() = nextPosition - eventLength

   override val headerLength: Long
      get() = 19

   override val dataLength: Long
      get() = this.eventLength - headerLength

}

/**
 * Mariadb ANNOTATE_ROWS_EVENT events accompany row events and describe the query which caused the row event
 * Enable this with --binlog-annotate-row-events (default on from MariaDB 10.2.4).
 * In the binary log, each Annotate_rows event precedes the corresponding Table map event.
 * Note the master server sends ANNOTATE_ROWS_EVENT events only if the Slave server connects
 * with the BINLOG_SEND_ANNOTATE_ROWS_EVENT flag (value is 2) in the COM_BINLOG_DUMP Slave Registration phase.
 *
 * @see [ANNOTATE_ROWS_EVENT](https://mariadb.com/kb/en/annotate_rows_event/) for the original doc
 */
data class AnnotateRowsEventData(var rowsQuery: String = "") : EventData

data class BinlogCheckpointEventData(var logFileName: String = "") : EventData

data class ByteArrayEventData(
   var data: ByteArray = byteArrayOf()
) : EventData {
   override fun equals(other: Any?): Boolean {
      if (this === other) return true
      if (javaClass != other?.javaClass) return false

      other as ByteArrayEventData

      return data.contentEquals(other.data)
   }

   override fun hashCode(): Int {
      return data.contentHashCode()
   }
}

data class DeleteRowsEventData(
   @JvmField
   var tableId: Long = 0,
   @JvmField
   var includedColumns: BitSet? = null,
   /**
    * @see bop.mysql.binlog.event.deserialization.AbstractRowsEventDataDeserializer
    */
   var rows: List<Array<Serializable?>> = emptyList()
) : EventData

data class FormatDescriptionEventData(
   var binlogVersion: Int = 0,
   var serverVersion: String = "",
   var headerLength: Int = 0,
   var dataLength: Int = 0,
   var checksumType: ChecksumType? = null
) : EventData

class GtidEventData : EventData {
   private var gtid: MySqlGtid? = null

   @set:Deprecated("")
   var flags: Byte = 0
   var lastCommitted: Long = 0
      private set
   var sequenceNumber: Long = 0
      private set
   var immediateCommitTimestamp: Long = 0
      private set
   var originalCommitTimestamp: Long = 0
      private set
   var transactionLength: Long = 0
      private set
   var immediateServerVersion: Int = 0
      private set
   var originalServerVersion: Int = 0
      private set

   @Deprecated("")
   constructor()

   constructor(
      gtid: MySqlGtid,
      flags: Byte,
      lastCommitted: Long,
      sequenceNumber: Long,
      immediateCommitTimestamp: Long,
      originalCommitTimestamp: Long,
      transactionLength: Long,
      immediateServerVersion: Int,
      originalServerVersion: Int
   ) {
      this.gtid = gtid
      this.flags = flags
      this.lastCommitted = lastCommitted
      this.sequenceNumber = sequenceNumber
      this.immediateCommitTimestamp = immediateCommitTimestamp
      this.originalCommitTimestamp = originalCommitTimestamp
      this.transactionLength = transactionLength
      this.immediateServerVersion = immediateServerVersion
      this.originalServerVersion = originalServerVersion
   }

   @Deprecated("")
   fun getGtid(): String? {
      return gtid.toString()
   }

   @Deprecated("")
   fun setGtid(gtid: String) {
      this.gtid = MySqlGtid.fromString(gtid)
   }

   val mySqlGtid: MySqlGtid
      get() = gtid!!

   override fun toString(): String {
      val sb = StringBuilder()
      sb.append("GtidEventData")
      sb.append("{flags=").append(flags.toInt()).append(", gtid='").append(gtid).append('\'')
      sb.append(", last_committed='").append(lastCommitted).append('\'')
      sb.append(", sequence_number='").append(sequenceNumber).append('\'')
      if (immediateCommitTimestamp != 0L) {
         sb.append(", immediate_commit_timestamp='").append(immediateCommitTimestamp).append('\'')
         sb.append(", original_commit_timestamp='").append(originalCommitTimestamp).append('\'')
      }
      if (transactionLength != 0L) {
         sb.append(", transaction_length='").append(transactionLength).append('\'')
         if (immediateServerVersion != 0) {
            sb.append(", immediate_server_version='").append(immediateServerVersion).append('\'')
            sb.append(", original_server_version='").append(originalServerVersion).append('\'')
         }
      }
      sb.append('}')
      return sb.toString()
   }

   companion object {
      const val COMMIT_FLAG: Byte = 1
   }
}

data class IntVarEventData(
   /**
    * Type indicating whether the value is meant to be used for the LAST_INSERT_ID() invocation (should be equal 1) or
    * AUTO_INCREMENT column (should be equal 2).
    */
   var type: Int = 0,
   var value: Long = 0
) : EventData

/**
 * MariaDB and MySQL have different GTID implementations, and that these are not compatible with each other.
 *
 * @see [GTID_EVENT](https://mariadb.com/kb/en/gtid_event/) for the original doc
 */
data class MariadbGtidEventData(
   var sequence: Long = 0,
   var domainId: Long = 0,
   var serverId: Long = 0,
   var flags: Int = 0
) : EventData {
   companion object {
      const val FL_STANDALONE: Int = 1
      const val FL_GROUP_COMMIT_ID: Int = 2
      const val FL_TRANSACTIONAL: Int = 4
      const val FL_ALLOW_PARALLEL: Int = 8
      const val FL_WAITED: Int = 16
      const val FL_DDL: Int = 32
   }
}

/**
 * Logged in every binlog to record the current replication state
 *
 * @see [GTID_LIST_EVENT](https://mariadb.com/kb/en/gtid_list_event/) for the original doc
 */
data class MariadbGtidListEventData(
   var mariaGTIDSet: MariadbGtidSet? = null
) : EventData

data class MySqlGtid(val serverId: UUID, val transactionId: Long) {
   override fun toString() = "$serverId:$transactionId"

   companion object {
      fun fromString(gtid: String): MySqlGtid {
         val split = gtid.split(":".toRegex()).dropLastWhile { it.isEmpty() }.toTypedArray()
         val sourceId = split[0]
         val transactionId = split[1].toLong()
         return MySqlGtid(UUID.fromString(sourceId), transactionId)
      }
   }
}

data class PreviousGtidSetEventData(var gtidSet: String) : EventData
data class QueryEventData(
   var threadId: Long = 0,
   var executionTime: Long = 0,
   var errorCode: Int = 0,
   var database: String = "",
   var sql: String = ""
) : EventData

data class RotateEventData(
   var binlogFilename: String = "",
   var binlogPosition: Long = 0
) : EventData

data class RowsQueryEventData(
   var query: String = ""
) : EventData

data class TableMapEventData(
   var tableId: Long = 0,
   var database: String = "",
   var table: String = "",
   var columnTypes: ByteArray? = null,
   var columnMetadata: IntArray? = null,
   var columnNullability: BitSet? = null,
   var eventMetadata: TableMapEventMetadata? = null
) : EventData {
   override fun equals(other: Any?): Boolean {
      if (this === other) return true
      if (javaClass != other?.javaClass) return false

      other as TableMapEventData

      if (tableId != other.tableId) return false
      if (database != other.database) return false
      if (table != other.table) return false
      if (!columnTypes.contentEquals(other.columnTypes)) return false
      if (!columnMetadata.contentEquals(other.columnMetadata)) return false
      if (columnNullability != other.columnNullability) return false
      if (eventMetadata != other.eventMetadata) return false

      return true
   }

   override fun hashCode(): Int {
      var result = tableId.hashCode()
      result = 31 * result + database.hashCode()
      result = 31 * result + table.hashCode()
      result = 31 * result + columnTypes.contentHashCode()
      result = 31 * result + columnMetadata.contentHashCode()
      result = 31 * result + (columnNullability?.hashCode() ?: 0)
      result = 31 * result + (eventMetadata?.hashCode() ?: 0)
      return result
   }

}

class TableMapEventMetadata(
   var signedness: BitSet? = null,

   var defaultCharset: DefaultCharset? = null,
   var columnCharsets: MutableList<Int>? = null,
   var columnNames: MutableList<String>? = null,
   var setStrValues: MutableList<Array<String>>? = null,
   var enumStrValues: MutableList<Array<String>>? = null,
   var geometryTypes: MutableList<Int>? = null,
   var simplePrimaryKeys: MutableList<Int>? = null,
   var primaryKeysWithPrefix: MutableMap<Int, Int>? = null,

   var enumAndSetDefaultCharset: DefaultCharset? = null,
   var enumAndSetColumnCharsets: MutableList<Int>? = null,

   var visibility: BitSet? = null
) : EventData {


   override fun toString(): String {
      val sb = StringBuilder()
      sb.append("TableMapEventMetadata")
      sb.append("{signedness=").append(signedness)
      sb.append(", defaultCharset=").append(if (defaultCharset == null) "null" else defaultCharset)

      sb.append(", columnCharsets=").append(if (columnCharsets == null) "null" else "")
      appendList(sb, columnCharsets)

      sb.append(", columnNames=").append(if (columnNames == null) "null" else "")
      appendList(sb, columnNames)

      sb.append(", setStrValues=").append(if (setStrValues == null) "null" else "")
      run {
         var i = 0
         while (setStrValues != null && i < setStrValues!!.size) {
            sb.append(if (i == 0) "" else ", ").append(join(", ", *setStrValues!!.get(i)!!))
            ++i
         }
      }

      sb.append(", enumStrValues=").append(if (enumStrValues == null) "null" else "")
      var i = 0
      while (enumStrValues != null && i < enumStrValues!!.size) {
         sb.append(if (i == 0) "" else ", ").append(join(", ", *enumStrValues!!.get(i)!!))
         ++i
      }

      sb.append(", geometryTypes=").append(if (geometryTypes == null) "null" else "")
      appendList(sb, geometryTypes)

      sb.append(", simplePrimaryKeys=").append(if (simplePrimaryKeys == null) "null" else "")
      appendList(sb, simplePrimaryKeys)

      sb.append(", primaryKeysWithPrefix=").append(if (primaryKeysWithPrefix == null) "null" else "")
      appendMap(sb, primaryKeysWithPrefix)

      sb.append(", enumAndSetDefaultCharset=")
         .append(if (enumAndSetDefaultCharset == null) "null" else enumAndSetDefaultCharset)

      sb.append(", enumAndSetColumnCharsets=").append(if (enumAndSetColumnCharsets == null) "null" else "")
      appendList(sb, enumAndSetColumnCharsets)

      sb.append(",visibility=").append(visibility)

      sb.append('}')
      return sb.toString()
   }

   class DefaultCharset(
      var defaultCharsetCollation: Int = 0,
      var charsetCollations: MutableMap<Int, Int>? = null
   ) : Serializable {
      override fun toString(): String {
         val sb = StringBuilder()
         sb.append(defaultCharsetCollation)
         sb.append(", charsetCollations=").append(if (charsetCollations == null) "null" else "")
         appendMap(sb, charsetCollations)
         return sb.toString()
      }
   }

   companion object {
      private fun join(
         delimiter: CharSequence?,
         vararg elements: CharSequence?
      ): String {
         if (elements.isEmpty()) {
            return ""
         }

         val sb = StringBuilder()
         sb.append(elements[0])

         for (i in 1..<elements.size) {
            sb.append(delimiter).append(elements[i])
         }
         return sb.toString()
      }

      private fun appendList(
         sb: StringBuilder,
         elements: MutableList<*>?
      ) {
         if (elements == null) {
            return
         }

         for (i in elements.indices) {
            sb.append(if (i == 0) "" else ", ").append(elements.get(i))
         }
      }

      private fun appendMap(
         sb: StringBuilder,
         map: MutableMap<*, *>?
      ) {
         if (map == null) {
            return
         }

         var entryCount = 0
         for (entry in map.entries) {
            sb.append(if (entryCount++ == 0) "" else ", ").append(entry.key.toString() + ": " + entry.value)
         }
      }
   }
}

data class TransactionPayloadEventData(
   var payloadSize: Int = 0,
   var uncompressedSize: Int = 0,
   var compressionType: Int = 0,
   var payload: ByteArray? = null,
   var uncompressedEvents: ArrayList<Event>? = null
) : EventData {
   override fun equals(other: Any?): Boolean {
      if (this === other) return true
      if (javaClass != other?.javaClass) return false

      other as TransactionPayloadEventData

      if (payloadSize != other.payloadSize) return false
      if (uncompressedSize != other.uncompressedSize) return false
      if (compressionType != other.compressionType) return false
      if (!payload.contentEquals(other.payload)) return false
      if (uncompressedEvents != other.uncompressedEvents) return false

      return true
   }

   override fun hashCode(): Int {
      var result = payloadSize
      result = 31 * result + uncompressedSize
      result = 31 * result + compressionType
      result = 31 * result + (payload?.contentHashCode() ?: 0)
      result = 31 * result + (uncompressedEvents?.hashCode() ?: 0)
      return result
   }
}

data class UpdateRowsEventData(
   var tableId: Long = 0L,
   var includedColumnsBeforeUpdate: BitSet? = null,
   var includedColumns: BitSet? = null,
   /**
    * @see bop.mysql.binlog.event.deserialization.AbstractRowsEventDataDeserializer
    */
   var rows: MutableList<MutableMap.MutableEntry<Array<Serializable?>, Array<Serializable?>>>? = null
) : EventData {
   override fun toString(): String {
      val sb = StringBuilder()
      sb.append("UpdateRowsEventData")
      sb.append("(tableId=").append(tableId)
      sb.append(", includedColumnsBeforeUpdate=").append(includedColumnsBeforeUpdate)
      sb.append(", includedColumns=").append(includedColumns)
      sb.append(", rows=[")
      rows?.forEach { row ->
         sb.append("\n    ").append("{before=").append(row.key.contentToString()).append(", after=")
            .append(row.value.contentToString()).append("},")
      }
      if (!rows.isNullOrEmpty()) {
         sb.replace(sb.length - 1, sb.length, "\n")
      }
      sb.append("])")
      return sb.toString()
   }
}

data class WriteRowsEventData(
   var tableId: Long = 0L,
   var includedColumns: BitSet? = null,
   /**
    * @see bop.mysql.binlog.event.deserialization.AbstractRowsEventDataDeserializer
    */
   var rows: MutableList<Array<Serializable?>>? = null
) : EventData

val EMPTY_BYTE_ARRAY = ByteArray(0)

data class XAPrepareEventData(
   var isOnePhase: Boolean = false,
   var formatID: Int = 0,
   var gtridLength: Int = 0,
   var bqualLength: Int = 0,
   var data: ByteArray = EMPTY_BYTE_ARRAY,
   var gtrid: String = "",
   var bqual: String = ""
) : EventData {

   fun setEventData(data: ByteArray) {
      this.data = data
      gtrid = String(data, 0, gtridLength)
      bqual = String(data, gtridLength, bqualLength)
   }

   override fun equals(other: Any?): Boolean {
      if (this === other) return true
      if (javaClass != other?.javaClass) return false

      other as XAPrepareEventData

      if (isOnePhase != other.isOnePhase) return false
      if (formatID != other.formatID) return false
      if (gtridLength != other.gtridLength) return false
      if (bqualLength != other.bqualLength) return false
      if (!data.contentEquals(other.data)) return false
      if (gtrid != other.gtrid) return false
      if (bqual != other.bqual) return false

      return true
   }

   override fun hashCode(): Int {
      var result = isOnePhase.hashCode()
      result = 31 * result + formatID
      result = 31 * result + gtridLength
      result = 31 * result + bqualLength
      result = 31 * result + (data?.contentHashCode() ?: 0)
      result = 31 * result + gtrid.hashCode()
      result = 31 * result + bqual.hashCode()
      return result
   }
}

data class XidEventData(
   var xid: Long = 0L
) : EventData
