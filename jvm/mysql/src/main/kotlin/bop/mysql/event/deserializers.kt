package bop.mysql.binlog.event.deserialization

import bop.mysql.binlog.MariadbGtidSet
import bop.mysql.event.AnnotateRowsEventData
import bop.mysql.event.BinlogCheckpointEventData
import bop.mysql.event.ByteArrayEventData
import bop.mysql.event.DeleteRowsEventData
import bop.mysql.event.Event
import bop.mysql.event.EventData
import bop.mysql.event.EventType
import bop.mysql.event.FormatDescriptionEventData
import bop.mysql.event.GtidEventData
import bop.mysql.event.IntVarEventData
import bop.mysql.event.MariadbGtidEventData
import bop.mysql.event.MariadbGtidListEventData
import bop.mysql.event.MySqlGtid
import bop.mysql.event.PreviousGtidSetEventData
import bop.mysql.event.QueryEventData
import bop.mysql.event.RotateEventData
import bop.mysql.event.RowsQueryEventData
import bop.mysql.event.TableMapEventData
import bop.mysql.event.TableMapEventMetadata
import bop.mysql.event.TransactionPayloadEventData
import bop.mysql.event.UpdateRowsEventData
import bop.mysql.event.WriteRowsEventData
import bop.mysql.event.XAPrepareEventData
import bop.mysql.event.XidEventData
import bop.mysql.io.ByteArrayInputStream
import com.github.luben.zstd.Zstd
import java.io.IOException
import java.io.Serializable
import java.lang.Double
import java.lang.Float
import java.math.BigDecimal
import java.nio.ByteBuffer
import java.sql.Date
import java.sql.Time
import java.sql.Timestamp
import java.util.*
import java.util.logging.Level
import java.util.logging.Logger
import kotlin.Array
import kotlin.Boolean
import kotlin.ByteArray
import kotlin.Int
import kotlin.IntArray
import kotlin.Long
import kotlin.String
import kotlin.Throws
import kotlin.arrayOfNulls
import kotlin.intArrayOf
import kotlin.math.pow

/**
 * Whole class is basically a mix of [open-replicator](https://code.google.com/p/open-replicator)'s
 * AbstractRowEventParser and MySQLUtils. Main purpose here is to ease rows deserialization.
 *
 * Current [ColumnType] to java type mapping is following:
 * <pre>
 * [ColumnType.TINY]: Integer
 * [ColumnType.SHORT]: Integer
 * [ColumnType.LONG]: Integer
 * [ColumnType.INT24]: Integer
 * [ColumnType.YEAR]: Integer
 * [ColumnType.ENUM]: Integer
 * [ColumnType.SET]: Long
 * [ColumnType.LONGLONG]: Long
 * [ColumnType.FLOAT]: Float
 * [ColumnType.DOUBLE]: Double
 * [ColumnType.BIT]: java.util.BitSet
 * [ColumnType.DATETIME]: java.util.Date
 * [ColumnType.DATETIME_V2]: java.util.Date
 * [ColumnType.NEWDECIMAL]: java.math.BigDecimal
 * [ColumnType.TIMESTAMP]: java.sql.Timestamp
 * [ColumnType.TIMESTAMP_V2]: java.sql.Timestamp
 * [ColumnType.DATE]: java.sql.Date
 * [ColumnType.TIME]: java.sql.Time
 * [ColumnType.TIME_V2]: java.sql.Time
 * [ColumnType.VARCHAR]: String
 * [ColumnType.VAR_STRING]: String
 * [ColumnType.STRING]: String
 * [ColumnType.BLOB]: byte[]
 * [ColumnType.GEOMETRY]: byte[]
 * </pre>
 *
 * At the moment [ColumnType.GEOMETRY] is unsupported.
 *
 * @param <T> event data this deserializer is responsible for </T> */
abstract class AbstractRowsEventDataDeserializer<T : EventData>(
   val tableMapEventByTableId: MutableMap<Long, TableMapEventData>
) : EventDataDeserializer<T> {
   private var deserializeDateAndTimeAsLong = false
   private var invalidDateAndTimeRepresentation: Long? = null
   private var microsecondsPrecision = false
   private var deserializeCharAndBinaryAsByteArray = false
   private var deserializeIntegerAsByteArray = false

   fun setDeserializeDateAndTimeAsLong(value: Boolean) {
      this.deserializeDateAndTimeAsLong = value
   }

   // value to return in case of 0000-00-00 00:00:00, 0000-00-00, etc.
   fun setInvalidDateAndTimeRepresentation(value: Long?) {
      this.invalidDateAndTimeRepresentation = value
   }

   fun setMicrosecondsPrecision(value: Boolean) {
      this.microsecondsPrecision = value
   }

   fun setDeserializeCharAndBinaryAsByteArray(value: Boolean) {
      this.deserializeCharAndBinaryAsByteArray = value
   }

   fun setDeserializeIntegerAsByteArray(deserializeIntegerAsByteArray: Boolean) {
      this.deserializeIntegerAsByteArray = deserializeIntegerAsByteArray
   }

   @Throws(IOException::class)
   protected fun deserializeRow(
      tableId: Long,
      includedColumns: BitSet,
      inputStream: ByteArrayInputStream
   ): Array<Serializable?> {
      val tableMapEvent: TableMapEventData = tableMapEventByTableId.get(tableId)!!
      if (tableMapEvent == null) {
         throw MissingTableMapEventException(
            "No TableMapEventData has been found for table id:" + tableId + ". Usually that means that you have started reading binary log 'within the logical event group'" + " (e.g. from WRITE_ROWS and not proceeding TABLE_MAP"
         )
      }
      val types = tableMapEvent.columnTypes
      val metadata = tableMapEvent.columnMetadata
      val result = arrayOfNulls<Serializable>(numberOfBitsSet(includedColumns))
      val nullColumns = inputStream.readBitSet(result.size, true)
      var i = 0
      var numberOfSkippedColumns = 0
      while (i < types!!.size) {
         if (!includedColumns.get(i)) {
            numberOfSkippedColumns++
            i++
            continue
         }
         val index = i - numberOfSkippedColumns
         if (!nullColumns.get(index)) {
            // mysql-5.6.24 sql/log_event.cc log_event_print_value (line 1980)
            var typeCode = types[i].toInt() and 0xFF
            val meta = metadata!![i]
            var length = 0
            if (typeCode == ColumnType.STRING.code) {
               if (meta >= 256) {
                  val meta0 = meta shr 8
                  val meta1 = meta and 0xFF
                  if ((meta0 and 0x30) != 0x30) {
                     typeCode = meta0 or 0x30
                     length = meta1 or (((meta0 and 0x30) xor 0x30) shl 4)
                  } else {
                     // mysql-5.6.24 sql/rpl_utility.h enum_field_types (line 278)
                     if (meta0 == ColumnType.ENUM.code || meta0 == ColumnType.SET.code) {
                        typeCode = meta0
                     }
                     length = meta1
                  }
               } else {
                  length = meta
               }
            }
            result[index] = deserializeCell(ColumnType.byCode(typeCode)!!, meta, length, inputStream)
         }
         i++
      }
      return result
   }

   @Throws(IOException::class)
   protected fun deserializeCell(
      type: ColumnType,
      meta: Int,
      length: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      when (type) {
         ColumnType.BIT -> return deserializeBit(meta, inputStream)
         ColumnType.TINY -> return deserializeTiny(inputStream)
         ColumnType.SHORT -> return deserializeShort(inputStream)
         ColumnType.INT24 -> return deserializeInt24(inputStream)
         ColumnType.LONG -> return deserializeLong(inputStream)
         ColumnType.LONGLONG -> return deserializeLongLong(inputStream)
         ColumnType.FLOAT -> return deserializeFloat(inputStream)
         ColumnType.DOUBLE -> return deserializeDouble(inputStream)
         ColumnType.NEWDECIMAL -> return deserializeNewDecimal(meta, inputStream)
         ColumnType.DATE -> return deserializeDate(inputStream)
         ColumnType.TIME -> return deserializeTime(inputStream)
         ColumnType.TIME_V2 -> return deserializeTimeV2(meta, inputStream)
         ColumnType.TIMESTAMP -> return deserializeTimestamp(inputStream)
         ColumnType.TIMESTAMP_V2 -> return deserializeTimestampV2(meta, inputStream)
         ColumnType.DATETIME -> return deserializeDatetime(inputStream)
         ColumnType.DATETIME_V2 -> return deserializeDatetimeV2(meta, inputStream)
         ColumnType.YEAR -> return deserializeYear(inputStream)
         ColumnType.STRING -> return deserializeString(length, inputStream)
         ColumnType.VARCHAR, ColumnType.VAR_STRING -> return deserializeVarString(meta, inputStream)
         ColumnType.BLOB -> return deserializeBlob(meta, inputStream)
         ColumnType.ENUM -> return deserializeEnum(length, inputStream)
         ColumnType.SET -> return deserializeSet(length, inputStream)
         ColumnType.GEOMETRY -> return deserializeGeometry(meta, inputStream)
         ColumnType.JSON -> return deserializeJson(meta, inputStream)
         else -> throw IOException("Unsupported type " + type)
      }
   }

   @Throws(IOException::class)
   protected fun deserializeBit(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable {
      val bitSetLength = (meta shr 8) * 8 + (meta and 0xFF)
      return inputStream.readBitSet(bitSetLength, false)
   }

   @Throws(IOException::class)
   protected fun deserializeTiny(inputStream: ByteArrayInputStream): Serializable? {
      if (deserializeIntegerAsByteArray) {
         return inputStream.read(1)
      }
      return (inputStream.readInteger(1).toByte()).toInt()
   }

   @Throws(IOException::class)
   protected fun deserializeShort(inputStream: ByteArrayInputStream): Serializable? {
      if (deserializeIntegerAsByteArray) {
         return inputStream.read(2)
      }
      return (inputStream.readInteger(2).toShort()).toInt()
   }

   @Throws(IOException::class)
   protected fun deserializeInt24(inputStream: ByteArrayInputStream): Serializable? {
      if (deserializeIntegerAsByteArray) {
         return inputStream.read(3)
      }
      return (inputStream.readInteger(3) shl 8) shr 8
   }

   @Throws(IOException::class)
   protected fun deserializeLong(inputStream: ByteArrayInputStream): Serializable? {
      if (deserializeIntegerAsByteArray) {
         return inputStream.read(4)
      }
      return inputStream.readInteger(4)
   }

   @Throws(IOException::class)
   protected fun deserializeLongLong(inputStream: ByteArrayInputStream): Serializable? {
      if (deserializeIntegerAsByteArray) {
         return inputStream.read(8)
      }
      return inputStream.readLong(8)
   }

   @Throws(IOException::class)
   protected fun deserializeFloat(inputStream: ByteArrayInputStream): Serializable {
      return Float.intBitsToFloat(inputStream.readInteger(4))
   }

   @Throws(IOException::class)
   protected fun deserializeDouble(inputStream: ByteArrayInputStream): Serializable {
      return Double.longBitsToDouble(inputStream.readLong(8))
   }

   @Throws(IOException::class)
   protected fun deserializeNewDecimal(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable {
      val precision = meta and 0xFF
      val scale = meta shr 8
      val x = precision - scale
      val ipd: Int = x / DIG_PER_DEC
      val fpd: Int = scale / DIG_PER_DEC
      val decimalLength: Int =
         (ipd shl 2) + DIG_TO_BYTES[x - ipd * DIG_PER_DEC] + (fpd shl 2) + DIG_TO_BYTES[scale - fpd * DIG_PER_DEC]
      return asBigDecimal(precision, scale, inputStream.read(decimalLength))
   }

   private fun castTimestamp(
      timestamp: Long?,
      fsp: Int
   ): Long? {
      if (microsecondsPrecision && timestamp != null && (timestamp != invalidDateAndTimeRepresentation)) {
         return timestamp * 1000 + fsp % 1000
      }
      return timestamp
   }

   @Throws(IOException::class)
   protected fun deserializeDate(inputStream: ByteArrayInputStream): Serializable? {
      var value = inputStream.readInteger(3)
      val day = value % 32
      value = value ushr 5
      val month = value % 16
      val year = value shr 4
      val timestamp = asUnixTime(year, month, day, 0, 0, 0, 0)
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, 0)
      }
      return if (timestamp != null) Date(timestamp) else null
   }

   @Throws(IOException::class)
   protected fun deserializeTime(inputStream: ByteArrayInputStream): Serializable? {
      val value = inputStream.readInteger(3)
      val split: IntArray = split(value.toLong(), 100, 3)
      val timestamp = asUnixTime(1970, 1, 1, split[2], split[1], split[0], 0)
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, 0)
      }
      return if (timestamp != null) Time(timestamp) else null
   }

   @Throws(IOException::class)
   protected fun deserializeTimeV2(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {/*
            (in big endian)

            1 bit sign (1= non-negative, 0= negative)
            1 bit unused (reserved for future extensions)
            10 bits hour (0-838)
            6 bits minute (0-59)
            6 bits second (0-59)

            (3 bytes in total)

            + fractional-seconds storage (size depends on meta)
        */
      val time: Long = bigEndianLong(inputStream.read(3), 0, 3)
      val fsp = deserializeFractionalSeconds(meta, inputStream)
      val timestamp = asUnixTime(
         1970, 1, 1, bitSlice(time, 2, 10, 24), bitSlice(time, 12, 6, 24), bitSlice(time, 18, 6, 24), fsp / 1000
      )
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, fsp)
      }
      return if (timestamp != null) Time(timestamp) else null
   }

   @Throws(IOException::class)
   protected fun deserializeTimestamp(inputStream: ByteArrayInputStream): Serializable? {
      val timestamp = inputStream.readLong(4) * 1000
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, 0)
      }
      return Timestamp(timestamp)
   }

   @Throws(IOException::class)
   protected fun deserializeTimestampV2(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      val millis: Long = bigEndianLong(inputStream.read(4), 0, 4)
      val fsp = deserializeFractionalSeconds(meta, inputStream)
      val timestamp = millis * 1000 + fsp / 1000
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, fsp)
      }
      return Timestamp(timestamp)
   }

   @Throws(IOException::class)
   protected fun deserializeDatetime(inputStream: ByteArrayInputStream): Serializable? {
      val split: IntArray = split(inputStream.readLong(8), 100, 6)
      val timestamp = asUnixTime(split[5], split[4], split[3], split[2], split[1], split[0], 0)
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, 0)
      }
      return if (timestamp != null) java.util.Date(timestamp) else null
   }

   @Throws(IOException::class)
   protected fun deserializeDatetimeV2(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {/*
            (in big endian)

            1 bit sign (1= non-negative, 0= negative)
            17 bits year*13+month (year 0-9999, month 0-12)
            5 bits day (0-31)
            5 bits hour (0-23)
            6 bits minute (0-59)
            6 bits second (0-59)

            (5 bytes in total)

            + fractional-seconds storage (size depends on meta)
        */
      val datetime: Long = bigEndianLong(inputStream.read(5), 0, 5)
      val yearMonth: Int = bitSlice(datetime, 1, 17, 40)
      val fsp = deserializeFractionalSeconds(meta, inputStream)
      val timestamp = asUnixTime(
         yearMonth / 13,
         yearMonth % 13,
         bitSlice(datetime, 18, 5, 40),
         bitSlice(datetime, 23, 5, 40),
         bitSlice(datetime, 28, 6, 40),
         bitSlice(datetime, 34, 6, 40),
         fsp / 1000
      )
      if (deserializeDateAndTimeAsLong) {
         return castTimestamp(timestamp, fsp)
      }
      return if (timestamp != null) java.util.Date(timestamp) else null
   }

   @Throws(IOException::class)
   protected fun deserializeYear(inputStream: ByteArrayInputStream): Serializable {
      return 1900 + inputStream.readInteger(1)
   }

   @Throws(IOException::class)
   protected fun deserializeString(
      length: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      // charset is not present in the binary log (meaning there is no way to distinguish between CHAR / BINARY)
      // as a result - return byte[] instead of an actual String
      val stringLength = if (length < 256) inputStream.readInteger(1) else inputStream.readInteger(2)
      if (deserializeCharAndBinaryAsByteArray) {
         return inputStream.read(stringLength)
      }
      return inputStream.readString(stringLength)
   }

   @Throws(IOException::class)
   protected fun deserializeVarString(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      val varcharLength = if (meta < 256) inputStream.readInteger(1) else inputStream.readInteger(2)
      if (deserializeCharAndBinaryAsByteArray) {
         return inputStream.read(varcharLength)
      }
      return inputStream.readString(varcharLength)
   }

   @Throws(IOException::class)
   protected fun deserializeBlob(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      val blobLength = inputStream.readInteger(meta)
      return inputStream.read(blobLength)
   }

   @Throws(IOException::class)
   protected fun deserializeEnum(
      length: Int,
      inputStream: ByteArrayInputStream
   ): Serializable {
      return inputStream.readInteger(length)
   }

   @Throws(IOException::class)
   protected fun deserializeSet(
      length: Int,
      inputStream: ByteArrayInputStream
   ): Serializable {
      return inputStream.readLong(length)
   }

   @Throws(IOException::class)
   protected fun deserializeGeometry(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Serializable? {
      val dataLength = inputStream.readInteger(meta)
      return inputStream.read(dataLength)
   }

   /**
    * Deserialize the `JSON` value on the input stream, and return MySQL's internal binary representation
    * of the JSON value. See [bop.mysql.event.JsonBinary] for
    * a utility to parse this binary representation into something more useful, including a string representation.
    *
    * @param meta the number of bytes in which the length of the JSON value is found first on the input stream
    * @param inputStream the stream containing the JSON value
    * @return the MySQL internal binary representation of the JSON value; may be null
    * @throws IOException if there is a problem reading the input stream
    */
   @Throws(IOException::class)
   protected fun deserializeJson(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): ByteArray? {
      val blobLength = inputStream.readInteger(meta)
      return inputStream.read(blobLength)
   }

   protected fun asUnixTime(
      year: Int,
      month: Int,
      day: Int,
      hour: Int,
      minute: Int,
      second: Int,
      millis: Int
   ): Long? {
      // https://dev.mysql.com/doc/refman/5.0/en/datetime.html
      if (year == 0 || month == 0 || day == 0) {
         return invalidDateAndTimeRepresentation
      }
      return UnixTime.from(year, month, day, hour, minute, second, millis)
   }

   @Throws(IOException::class)
   protected fun deserializeFractionalSeconds(
      meta: Int,
      inputStream: ByteArrayInputStream
   ): Int {
      val length = (meta + 1) / 2
      if (length > 0) {
         val fraction: Int = bigEndianInteger(inputStream.read(length), 0, length)
         return fraction * 100.0.pow((3 - length).toDouble()).toInt()
      }
      return 0
   }

   /**
    * Class for working with Unix time.
    */
   internal object UnixTime {
      private val YEAR_DAYS_BY_MONTH = intArrayOf(
         0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365
      )
      private val LEAP_YEAR_DAYS_BY_MONTH = intArrayOf(
         0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366
      )

      /**
       * Calendar::getTimeInMillis but magnitude faster for all dates starting from October 15, 1582
       * (Gregorian Calendar cutover).
       *
       * @param year year
       * @param month month [1..12]
       * @param day day [1..)
       * @param hour hour [0..23]
       * @param minute [0..59]
       * @param second [0..59]
       * @param millis [0..999]
       *
       * @return Unix time (number of seconds that have elapsed since 00:00:00 (UTC), Thursday,
       * 1 January 1970, not counting leap seconds)
       */
      // checkstyle, please ignore ParameterNumber for the next line
      fun from(
         year: Int,
         month: Int,
         day: Int,
         hour: Int,
         minute: Int,
         second: Int,
         millis: Int
      ): Long {
         if (year < 1582 || (year == 1582 && (month < 10 || (month == 10 && day < 15)))) {
            return fallbackToGC(year, month, day, hour, minute, second, millis)
         }
         var timestamp: Long = 0
         val numberOfLeapYears: Int = leapYears(1970, year)
         timestamp += 366L * 24 * 60 * 60 * numberOfLeapYears
         timestamp += 365L * 24 * 60 * 60 * (year - 1970 - numberOfLeapYears)
         val daysUpToMonth =
            (if (isLeapYear(year)) LEAP_YEAR_DAYS_BY_MONTH[month - 1] else YEAR_DAYS_BY_MONTH[month - 1]).toLong()
         timestamp += ((daysUpToMonth + day - 1) * 24 * 60 * 60) + (hour * 60 * 60) + (minute * 60) + (second)
         timestamp = timestamp * 1000 + millis
         return timestamp
      }

      // checkstyle, please ignore ParameterNumber for the next line
      private fun fallbackToGC(
         year: Int,
         month: Int,
         dayOfMonth: Int,
         hourOfDay: Int,
         minute: Int,
         second: Int,
         millis: Int
      ): Long {
         val c = Calendar.getInstance(TimeZone.getTimeZone("GMT"))
         c.set(Calendar.YEAR, year)
         c.set(Calendar.MONTH, month - 1)
         c.set(Calendar.DAY_OF_MONTH, dayOfMonth)
         c.set(Calendar.HOUR_OF_DAY, hourOfDay)
         c.set(Calendar.MINUTE, minute)
         c.set(Calendar.SECOND, second)
         c.set(Calendar.MILLISECOND, millis)
         return c.getTimeInMillis()
      }

      private fun leapYears(
         from: Int,
         end: Int
      ): Int {
         return leapYearsBefore(end) - leapYearsBefore(from + 1)
      }

      private fun leapYearsBefore(year: Int): Int {
         var year = year
         year--
         return (year / 4) - (year / 100) + (year / 400)
      }

      private fun isLeapYear(year: Int): Boolean {
         return year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
      }
   }

   companion object {
      private const val DIG_PER_DEC = 9
      private val DIG_TO_BYTES = intArrayOf(0, 1, 1, 2, 2, 3, 3, 4, 4, 4)

      private fun bitSlice(
         value: Long,
         bitOffset: Int,
         numberOfBits: Int,
         payloadSize: Int
      ): Int {
         val result = value shr payloadSize - (bitOffset + numberOfBits)
         return (result and ((1 shl numberOfBits) - 1).toLong()).toInt()
      }

      private fun numberOfBitsSet(bitSet: BitSet): Int {
         var result = 0
         var i = bitSet.nextSetBit(0)
         while (i >= 0) {
            result++
            i = bitSet.nextSetBit(i + 1)
         }
         return result
      }

      private fun split(
         value: Long,
         divider: Int,
         length: Int
      ): IntArray {
         var value = value
         val result = IntArray(length)
         for (i in 0..<length - 1) {
            result[i] = (value % divider).toInt()
            value /= divider.toLong()
         }
         result[length - 1] = value.toInt()
         return result
      }

      @JvmStatic
      fun asBigDecimal(
         precision: Int,
         scale: Int,
         value: ByteArray
      ): BigDecimal {
         val positive = (value[0].toInt() and 0x80) == 0x80
         value[0] = (value[0].toInt() xor 0x80).toByte()
         if (!positive) {
            for (i in value.indices) {
               value[i] = (value[i].toInt() xor 0xFF).toByte()
            }
         }
         val x = precision - scale
         val ipDigits: Int = x / DIG_PER_DEC
         val ipDigitsX: Int = x - ipDigits * DIG_PER_DEC
         val ipSize: Int = (ipDigits shl 2) + DIG_TO_BYTES[ipDigitsX]
         var offset: Int = DIG_TO_BYTES[ipDigitsX]
         var ip = if (offset > 0) BigDecimal.valueOf(bigEndianInteger(value, 0, offset).toLong()) else BigDecimal.ZERO
         while (offset < ipSize) {
            val i: Int = bigEndianInteger(value, offset, 4)
            ip = ip.movePointRight(DIG_PER_DEC).add(BigDecimal.valueOf(i.toLong()))
            offset += 4
         }
         var shift = 0
         var fp = BigDecimal.ZERO
         while (shift + DIG_PER_DEC <= scale) {
            val i: Int = bigEndianInteger(value, offset, 4)
            fp = fp.add(BigDecimal.valueOf(i.toLong()).movePointLeft(shift + DIG_PER_DEC))
            shift += DIG_PER_DEC
            offset += 4
         }
         if (shift < scale) {
            val i: Int = bigEndianInteger(value, offset, DIG_TO_BYTES[scale - shift])
            fp = fp.add(BigDecimal.valueOf(i.toLong()).movePointLeft(scale))
         }
         val result = ip.add(fp)
         return if (positive) result else result.negate()
      }

      private fun bigEndianInteger(
         bytes: ByteArray,
         offset: Int,
         length: Int
      ): Int {
         var result = 0
         for (i in offset..<(offset + length)) {
            val b = bytes[i]
            result = (result shl 8) or (if (b >= 0) b.toInt() else (b + 256))
         }
         return result
      }

      private fun bigEndianLong(
         bytes: ByteArray,
         offset: Int,
         length: Int
      ): Long {
         var result: Long = 0
         for (i in offset..<(offset + length)) {
            val b = bytes[i]
            result = (result shl 8) or (if (b >= 0) b.toInt() else (b + 256)).toLong()
         }
         return result
      }
   }
}

/**
 * Mariadb ANNOTATE_ROWS_EVENT Fields
 * <pre>
 * string&lt;EOF&gt; The SQL statement (not null-terminated)
</pre> *
 *
 * @author [Winger](mailto:winger2049@gmail.com)
 */
class AnnotateRowsEventDataDeserializer : EventDataDeserializer<AnnotateRowsEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): AnnotateRowsEventData {
      val event = AnnotateRowsEventData()
      event.rowsQuery = inputStream.readString(inputStream.available())
      return event
   }
}

class BinlogCheckpointEventDataDeserializer : EventDataDeserializer<BinlogCheckpointEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): BinlogCheckpointEventData {
      val eventData = BinlogCheckpointEventData()
      val length = inputStream.readInteger(4)
      eventData.logFileName = inputStream.readString(length)
      return eventData
   }
}

class ByteArrayEventDataDeserializer : EventDataDeserializer<ByteArrayEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): ByteArrayEventData {
      val eventData = ByteArrayEventData()
      eventData.data = inputStream.read(inputStream.available())
      return eventData
   }
}

class DeleteRowsEventDataDeserializer(
   tableMapEventByTableId: HashMap<Long, TableMapEventData>
) : AbstractRowsEventDataDeserializer<DeleteRowsEventData>(tableMapEventByTableId) {
   private var mayContainExtraInformation = false

   fun setMayContainExtraInformation(mayContainExtraInformation: Boolean): DeleteRowsEventDataDeserializer {
      this.mayContainExtraInformation = mayContainExtraInformation
      return this
   }

   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): DeleteRowsEventData {
      val eventData = DeleteRowsEventData()
      eventData.tableId = inputStream.readLong(6)
      inputStream.readInteger(2) // reserved
      if (mayContainExtraInformation) {
         val extraInfoLength = inputStream.readInteger(2)
         inputStream.skip((extraInfoLength - 2).toLong())
      }
      val numberOfColumns = inputStream.readPackedInteger()
      eventData.includedColumns = inputStream.readBitSet(numberOfColumns, true)
      eventData.rows = deserializeRows(eventData.tableId, eventData.includedColumns!!, inputStream)
      return eventData
   }

   @Throws(IOException::class)
   private fun deserializeRows(
      tableId: Long,
      includedColumns: BitSet,
      inputStream: ByteArrayInputStream
   ): MutableList<Array<Serializable?>> {
      val result: MutableList<Array<Serializable?>> = LinkedList<Array<Serializable?>>()
      while (inputStream.available() > 0) {
         result.add(deserializeRow(tableId, includedColumns, inputStream))
      }
      return result
   }
}

/**
 * @see [FORMAT_DESCRIPTION_EVENT](https://dev.mysql.com/doc/internals/en/format-description-event.html)
 */
class FormatDescriptionEventDataDeserializer : EventDataDeserializer<FormatDescriptionEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): FormatDescriptionEventData {
      val eventBodyLength = inputStream.available()
      val eventData = FormatDescriptionEventData()
      eventData.binlogVersion = inputStream.readInteger(2)
      eventData.serverVersion = inputStream.readString(50).trim { it <= ' ' }
      inputStream.skip(4) // redundant, present in a header
      eventData.headerLength = inputStream.readInteger(1)
      inputStream.skip((EventType.FORMAT_DESCRIPTION.ordinal - 1).toLong())
      eventData.dataLength = inputStream.readInteger(1)
      val checksumBlockLength = eventBodyLength - eventData.dataLength
      var checksumType: ChecksumType? = ChecksumType.NONE
      if (checksumBlockLength > 0) {
         inputStream.skip((inputStream.available() - checksumBlockLength).toLong())
         checksumType = ChecksumType.Companion.byOrdinal(inputStream.read())
      }
      eventData.checksumType = checksumType
      return eventData
   }
}

class GtidEventDataDeserializer : EventDataDeserializer<GtidEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): GtidEventData {
      val flags = inputStream.readInteger(1).toByte()
      val sourceIdMostSignificantBits: Long = readLongBigEndian(inputStream)
      val sourceIdLeastSignificantBits: Long = readLongBigEndian(inputStream)
      val transactionId = inputStream.readLong(8)

      val gtid = MySqlGtid(
         UUID(sourceIdMostSignificantBits, sourceIdLeastSignificantBits), transactionId
      )

      // MTR logical clock
      var lastCommitted: Long = 0
      var sequenceNumber: Long = 0
      // ImmediateCommitTimestamp/OriginalCommitTimestamp are introduced in MySQL-8.0.1, see:
      // https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-1.html
      var immediateCommitTimestamp: Long = 0
      var originalCommitTimestamp: Long = 0
      // Total transaction length (including this GTIDEvent), introduced in MySQL-8.0.2, sees:
      // https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-2.html
      var transactionLength: Long = 0
      // ImmediateServerVersion/OriginalServerVersion are introduced in MySQL-8.0.14, see
      // https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-14.html
      var immediateServerVersion = 0
      var originalServerVersion = 0

      // Logical timestamps - since MySQL 5.7.6
      if (inputStream.peek() == LOGICAL_TIMESTAMP_TYPECODE) {
         inputStream.skip(LOGICAL_TIMESTAMP_TYPECODE_LENGTH.toLong())
         lastCommitted = inputStream.readLong(LOGICAL_TIMESTAMP_LENGTH)
         sequenceNumber = inputStream.readLong(LOGICAL_TIMESTAMP_LENGTH)
         // Immediate and original commit timestamps are introduced in MySQL-8.0.1
         if (inputStream.available() >= IMMEDIATE_COMMIT_TIMESTAMP_LENGTH) {
            immediateCommitTimestamp = inputStream.readLong(IMMEDIATE_COMMIT_TIMESTAMP_LENGTH)
            // Check the MSB to determine how to populate the original commit timestamp
            if ((immediateCommitTimestamp and (1L shl ENCODED_COMMIT_TIMESTAMP_LENGTH)) != 0L) {
               immediateCommitTimestamp = immediateCommitTimestamp and (1L shl ENCODED_COMMIT_TIMESTAMP_LENGTH).inv()
               originalCommitTimestamp = inputStream.readLong(ORIGINAL_COMMIT_TIMESTAMP_LENGTH)
            } else {
               // Transaction originated in the previous server e.g., writer if directly connecting
               originalCommitTimestamp = immediateCommitTimestamp
            }
            // Total transaction length (including this GTIDEvent), introduced in MySQL-8.0.2
            if (inputStream.available() >= TRANSACTION_LENGTH_MIN_LENGTH) {
               transactionLength = inputStream.readPackedLong()
            }
            immediateServerVersion = UNDEFINED_SERVER_VERSION
            originalServerVersion = UNDEFINED_SERVER_VERSION
            // Immediate and original server versions are introduced in MySQL-8.0.14
            if (inputStream.available() >= IMMEDIATE_SERVER_VERSION_LENGTH) {
               immediateServerVersion = inputStream.readInteger(IMMEDIATE_SERVER_VERSION_LENGTH)
               // Check the MSB to determine how to populate an original server version
               if ((immediateServerVersion.toLong() and (1L shl ENCODED_SERVER_VERSION_LENGTH)) != 0L) {
                  immediateServerVersion =
                     immediateServerVersion and (1L shl ENCODED_SERVER_VERSION_LENGTH).inv().toInt()
                  originalServerVersion = inputStream.readInteger(ORIGINAL_SERVER_VERSION_LENGTH)
               } else {
                  originalServerVersion = immediateServerVersion
               }
            }
         }
      }

      return GtidEventData(
         gtid,
         flags,
         lastCommitted,
         sequenceNumber,
         immediateCommitTimestamp,
         originalCommitTimestamp,
         transactionLength,
         immediateServerVersion,
         originalServerVersion
      )
   }

   companion object {
      const val LOGICAL_TIMESTAMP_TYPECODE_LENGTH: Int = 1

      // Type code used before the logical timestamps.
      const val LOGICAL_TIMESTAMP_TYPECODE: Int = 2
      const val LOGICAL_TIMESTAMP_LENGTH: Int = 8

      // Length of immediate and original commit timestamps
      const val IMMEDIATE_COMMIT_TIMESTAMP_LENGTH: Int = 7
      const val ORIGINAL_COMMIT_TIMESTAMP_LENGTH: Int = 7

      // Use 7 bytes out of which 1 bit is used as a flag.
      const val ENCODED_COMMIT_TIMESTAMP_LENGTH: Int = 55
      const val TRANSACTION_LENGTH_MIN_LENGTH: Int = 1

      // Length of immediate and original server versions
      const val IMMEDIATE_SERVER_VERSION_LENGTH: Int = 4
      const val ORIGINAL_SERVER_VERSION_LENGTH: Int = 4

      // Use 4 bytes out of which 1 bit is used as a flag.
      const val ENCODED_SERVER_VERSION_LENGTH: Int = 31
      const val UNDEFINED_SERVER_VERSION: Int = 999999

      @Throws(IOException::class)
      private fun readLongBigEndian(input: ByteArrayInputStream): Long {
         var result: Long = 0
         for (i in 0..7) {
            result = ((result shl 8) or (input.read() and 0xff).toLong())
         }
         return result
      }
   }
}

class IntVarEventDataDeserializer : EventDataDeserializer<IntVarEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): IntVarEventData {
      val event = IntVarEventData()
      event.type = inputStream.readInteger(1)
      event.value = inputStream.readLong(8)
      return event
   }
}

/**
 * Mariadb GTID_EVENT Fields
 *
 * <pre>
 * uint8 GTID sequence
 * uint4 Replication Domain ID
 * uint1 Flags
 *
 * if flag &amp; FL_GROUP_COMMIT_ID
 * uint8 commit_id
 * else
 * uint6 0
 * </pre>
 *
 * @see [GTID_EVENT](https://mariadb.com/kb/en/gtid_event/) for the original doc
 */
class MariadbGtidEventDataDeserializer : EventDataDeserializer<MariadbGtidEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): MariadbGtidEventData {
      val event = MariadbGtidEventData()
      event.sequence = inputStream.readLong(8)
      event.domainId = inputStream.readInteger(4).toLong()
      event.flags = inputStream.readInteger(1)
      // Flags ignore
      return event
   }
}

/**
 * Mariadb GTID_LIST_EVENT Fields
 * <pre>
 * uint4 Number of GTIDs
 * GTID[0]
 * uint4 Replication Domain ID
 * uint4 Server_ID
 * uint8 GTID sequence ...
 * GTID[n]
 * </pre>
 *
 * @see [GTID_EVENT](https://mariadb.com/kb/en/gtid_event/) for the original doc
 */
class MariadbGtidListEventDataDeserializer : EventDataDeserializer<MariadbGtidListEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): MariadbGtidListEventData {
      val eventData = MariadbGtidListEventData()
      val gtidLength = inputStream.readInteger(4).toLong()
      val mariaGTIDSet = MariadbGtidSet()
      for (i in 0..<gtidLength) {
         val domainId = inputStream.readInteger(4).toLong()
         val serverId = inputStream.readInteger(4).toLong()
         val sequence = inputStream.readLong(8)
         mariaGTIDSet.add(MariadbGtidSet.MariaGtid(domainId, serverId, sequence))
      }
      eventData.mariaGTIDSet = mariaGTIDSet
      return eventData
   }
}

class NullEventDataDeserializer : EventDataDeserializer<EventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): EventData {
      return EventData.NULL
   }
}

class PreviousGtidSetDeserializer : EventDataDeserializer<PreviousGtidSetEventData> {
   @Throws(IOException::class)
   override fun deserialize(
      inputStream: ByteArrayInputStream
   ): PreviousGtidSetEventData {
      val nUuids = inputStream.readInteger(8)
      val gtids: Array<String> = Array(nUuids) {
         val uuid = formatUUID(inputStream.read(16))
         val nIntervals = inputStream.readInteger(8)
         val intervals: Array<String> = Array(nIntervals) {
            val start = inputStream.readLong(8)
            val end = inputStream.readLong(8)
            "$start-${end-1}"
         }
         String.format("%s:%s", uuid, join(intervals, ":"))
      }
      return PreviousGtidSetEventData(join(gtids, ","))
   }

   private fun formatUUID(bytes: ByteArray): String {
      return String.format(
         "%s-%s-%s-%s-%s",
         byteArrayToHex(bytes, 0, 4),
         byteArrayToHex(bytes, 4, 2),
         byteArrayToHex(bytes, 6, 2),
         byteArrayToHex(bytes, 8, 2),
         byteArrayToHex(bytes, 10, 6)
      )
   }

   companion object {
      private fun byteArrayToHex(
         a: ByteArray,
         offset: Int,
         len: Int
      ): String {
         val sb = StringBuilder()
         var idx = offset
         while (idx < (offset + len) && idx < a.size) {
            sb.append(String.format("%02x", a[idx].toInt() and 0xff))
            idx++
         }
         return sb.toString()
      }

      private fun join(
         values: Array<String>,
         separator: String = ""
      ): String {
         val sb = StringBuilder()
         for (i in values.indices) {
            if (i > 0) {
               sb.append(separator)
            }
            sb.append(values[i])
         }
         return sb.toString()
      }
   }
}

class QueryEventDataDeserializer : EventDataDeserializer<QueryEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): QueryEventData {
      val eventData = QueryEventData()
      eventData.threadId = inputStream.readLong(4)
      eventData.executionTime = inputStream.readLong(4)
      inputStream.skip(1) // length of the name of the database
      eventData.errorCode = inputStream.readInteger(2)
      inputStream.skip(inputStream.readInteger(2).toLong()) // status variables block
      eventData.database = inputStream.readZeroTerminatedString()
      eventData.sql = inputStream.readString(inputStream.available())
      return eventData
   }
}

class RotateEventDataDeserializer : EventDataDeserializer<RotateEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): RotateEventData {
      val eventData = RotateEventData()
      eventData.binlogPosition = inputStream.readLong(8)
      eventData.binlogFilename = inputStream.readString(inputStream.available())
      return eventData
   }
}

class RowsQueryEventDataDeserializer : EventDataDeserializer<RowsQueryEventData> {
   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): RowsQueryEventData {
      val eventData = RowsQueryEventData()
      inputStream.readInteger(1) // ignored
      eventData.query = inputStream.readString(inputStream.available())
      return eventData
   }
}

class TableMapEventDataDeserializer : EventDataDeserializer<TableMapEventData> {
   private val metadataDeserializer = TableMapEventMetadataDeserializer()

   @Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): TableMapEventData {
      val eventData = TableMapEventData()
      eventData.tableId = inputStream.readLong(6)
      inputStream.skip(3) // 2 bytes reserved for future use + 1 for the length of database name
      eventData.database = inputStream.readZeroTerminatedString()
      inputStream.skip(1) // table name
      eventData.table = inputStream.readZeroTerminatedString()
      val numberOfColumns = inputStream.readPackedInteger()
      eventData.columnTypes = inputStream.read(numberOfColumns)
      inputStream.readPackedInteger() // metadata length
      eventData.columnMetadata = readMetadata(inputStream, eventData.columnTypes!!)
      eventData.columnNullability = inputStream.readBitSet(numberOfColumns, true)
      val metadataLength = inputStream.available()
      var metadata: TableMapEventMetadata? = null
      if (metadataLength > 0) {
         metadata = metadataDeserializer.deserialize(
            ByteArrayInputStream(inputStream.read(metadataLength)),
            eventData.columnTypes!!.size,
            eventData.columnTypes!!
         )
      }
      eventData.eventMetadata = metadata
      return eventData
   }

   private fun numericColumnIndex(types: ByteArray): MutableList<Int> {
      val numericColumnIndexList: MutableList<Int> = ArrayList()
      for (i in types.indices) {
         when (ColumnType.Companion.byCode(types[i].toInt() and 0xff)) {
            ColumnType.TINY, ColumnType.SHORT, ColumnType.INT24, ColumnType.LONG, ColumnType.LONGLONG, ColumnType.NEWDECIMAL, ColumnType.FLOAT, ColumnType.DOUBLE, ColumnType.YEAR -> numericColumnIndexList.add(
               i
            )

            else -> {}
         }
      }
      return numericColumnIndexList
   }

   private fun numericColumnCount(types: ByteArray): Int {
      var count = 0
      for (i in types.indices) {
         when (ColumnType.Companion.byCode(types[i].toInt() and 0xff)) {
            ColumnType.TINY, ColumnType.SHORT, ColumnType.INT24, ColumnType.LONG, ColumnType.LONGLONG, ColumnType.NEWDECIMAL, ColumnType.FLOAT, ColumnType.DOUBLE, ColumnType.YEAR -> count++
            else -> {}
         }
      }
      return count
   }

   @Throws(IOException::class)
   private fun readMetadata(
      inputStream: ByteArrayInputStream,
      columnTypes: ByteArray
   ): IntArray {
      val metadata = IntArray(columnTypes.size)
      for (i in columnTypes.indices) {
         when (ColumnType.Companion.byCode(columnTypes[i].toInt() and 0xFF)) {
            ColumnType.FLOAT, ColumnType.DOUBLE, ColumnType.BLOB, ColumnType.JSON, ColumnType.GEOMETRY -> metadata[i] =
               inputStream.readInteger(1)

            ColumnType.BIT, ColumnType.VARCHAR, ColumnType.NEWDECIMAL -> metadata[i] = inputStream.readInteger(2)
            ColumnType.SET, ColumnType.ENUM, ColumnType.STRING -> metadata[i] =
               bigEndianInteger(inputStream.read(2), 0, 2)

            ColumnType.TIME_V2, ColumnType.DATETIME_V2, ColumnType.TIMESTAMP_V2 -> metadata[i] =
               inputStream.readInteger(1) // fsp (@see {@link ColumnType})
            else -> metadata[i] = 0
         }
      }
      return metadata
   }

   companion object {
      private fun bigEndianInteger(
         bytes: ByteArray,
         offset: Int,
         length: Int
      ): Int {
         var result = 0
         for (i in offset..<(offset + length)) {
            val b = bytes[i]
            result = (result shl 8) or (if (b >= 0) b.toInt() else (b + 256))
         }
         return result
      }
   }
}

class TableMapEventMetadataDeserializer {
   private val logger: Logger = Logger.getLogger(javaClass.getName())

   @kotlin.jvm.Throws(IOException::class)
   fun deserialize(
      inputStream: ByteArrayInputStream,
      nColumns: Int,
      columnTypes: ByteArray
   ): TableMapEventMetadata? {
      var remainingBytes = inputStream.available()
      if (remainingBytes <= 0) {
         return null
      }

      val result = TableMapEventMetadata()

      while (remainingBytes > 0) {
         val code = inputStream.readInteger(1)

         val fieldType = MetadataFieldType.Companion.byCode(code)
         if (fieldType == null) {
            throw IOException("Unsupported table metadata field type " + code)
         }
         if (MetadataFieldType.UNKNOWN_METADATA_FIELD_TYPE == fieldType) {
            if (logger.isLoggable(Level.FINE)) {
               logger.fine("Received metadata field of unknown type")
            }
            inputStream.enterBlock(remainingBytes)
            continue
         }

         val fieldLength = inputStream.readPackedInteger()

         remainingBytes = inputStream.available()
         inputStream.enterBlock(fieldLength)

         when (fieldType) {
            MetadataFieldType.SIGNEDNESS -> {
               var numericColumns = 0
               val bitSet = BitSet()
               run {
                  var i = 0
                  while (i < columnTypes.size) {
                     when (ColumnType.Companion.byCode(columnTypes[i].toInt() and 0xff)) {
                        ColumnType.TINY, ColumnType.SHORT, ColumnType.INT24, ColumnType.LONG, ColumnType.LONGLONG, ColumnType.NEWDECIMAL, ColumnType.FLOAT, ColumnType.DOUBLE, ColumnType.YEAR -> {
                           numericColumns++
                           bitSet.set(i)
                        }

                        else -> {}
                     }
                     i++
                  }
               }
               val signednessBitSet: BitSet = readBooleanList(inputStream, numericColumns)
               BitSet()

               var i = 0
               var j = 0
               while (i < columnTypes.size) {
                  if (bitSet.get(i))  // if is numeric
                     bitSet.set(i, signednessBitSet.get(j++)) // set signed-ness

                  i++
               }
               result.signedness = bitSet
            }

            MetadataFieldType.DEFAULT_CHARSET -> result.defaultCharset = readDefaultCharset(inputStream)
            MetadataFieldType.COLUMN_CHARSET -> result.columnCharsets = readIntegers(inputStream)
            MetadataFieldType.COLUMN_NAME -> result.columnNames = readColumnNames(inputStream)
            MetadataFieldType.SET_STR_VALUE -> result.setStrValues = readTypeValues(inputStream)
            MetadataFieldType.ENUM_STR_VALUE -> result.enumStrValues = readTypeValues(inputStream)
            MetadataFieldType.GEOMETRY_TYPE -> result.geometryTypes = readIntegers(inputStream)
            MetadataFieldType.SIMPLE_PRIMARY_KEY -> result.simplePrimaryKeys = readIntegers(inputStream)
            MetadataFieldType.PRIMARY_KEY_WITH_PREFIX -> result.primaryKeysWithPrefix = readIntegerPairs(inputStream)
            MetadataFieldType.ENUM_AND_SET_DEFAULT_CHARSET -> result.enumAndSetDefaultCharset =
               readDefaultCharset(inputStream)

            MetadataFieldType.ENUM_AND_SET_COLUMN_CHARSET -> result.enumAndSetColumnCharsets = readIntegers(inputStream)
            MetadataFieldType.VISIBILITY -> result.visibility = readBooleanList(inputStream, nColumns)
            else -> {
               inputStream.enterBlock(remainingBytes)
               throw IOException("Unsupported table metadata field type " + code)
            }
         }
         remainingBytes -= fieldLength
         inputStream.enterBlock(remainingBytes)
      }
      return result
   }


   private enum class MetadataFieldType(val code: Int) {
      SIGNEDNESS(1),  // Signedness of numeric colums
      DEFAULT_CHARSET(2),  // Charsets of character columns
      COLUMN_CHARSET(3),  // Charsets of character columns
      COLUMN_NAME(4),  // Names of columns
      SET_STR_VALUE(5),  // The string values of SET columns
      ENUM_STR_VALUE(6),  // The string values is ENUM columns
      GEOMETRY_TYPE(7),  // The real type of geometry columns
      SIMPLE_PRIMARY_KEY(8),  // The primary key without any prefix
      PRIMARY_KEY_WITH_PREFIX(9),  // The primary key with some prefix
      ENUM_AND_SET_DEFAULT_CHARSET(10),  // Charsets of ENUM and SET columns
      ENUM_AND_SET_COLUMN_CHARSET(11),  // Charsets of ENUM and SET columns
      VISIBILITY(12),  // Column visibility (8.0.23 and newer)
      UNKNOWN_METADATA_FIELD_TYPE(
         128
      ); // Returned with binlog-row-metadata=FULL from MySQL 8.0 in some cases

      companion object {
         private val INDEX_BY_CODE: MutableMap<Int?, MetadataFieldType>

         init {
            INDEX_BY_CODE = HashMap<Int?, MetadataFieldType>()
            for (fieldType in entries) {
               INDEX_BY_CODE.put(fieldType.code, fieldType)
            }
         }

         fun byCode(code: Int): MetadataFieldType {
            return INDEX_BY_CODE.get(code)!!
         }
      }
   }

   companion object {
      @kotlin.jvm.Throws(IOException::class)
      private fun readBooleanList(
         inputStream: ByteArrayInputStream,
         length: Int
      ): BitSet {
         val result = BitSet()
         // according to MySQL internals the amount of storage required for N columns is INT((N+7)/8) bytes
         val bytes = inputStream.read((length + 7) shr 3)
         for (i in 0..<length) {
            if ((bytes[i shr 3].toInt() and (1 shl (7 - (i % 8)))) != 0) {
               result.set(i)
            }
         }
         return result
      }

      @kotlin.jvm.Throws(IOException::class)
      private fun readDefaultCharset(inputStream: ByteArrayInputStream): TableMapEventMetadata.DefaultCharset {
         val result = TableMapEventMetadata.DefaultCharset()
         result.defaultCharsetCollation = inputStream.readPackedInteger()
         val charsetCollations: MutableMap<Int, Int> = readIntegerPairs(inputStream)
         if (!charsetCollations.isEmpty()) {
            result.charsetCollations = charsetCollations
         }
         return result
      }

      @kotlin.jvm.Throws(IOException::class)
      private fun readIntegers(inputStream: ByteArrayInputStream): MutableList<Int> {
         val result: MutableList<Int> = ArrayList<Int>()
         while (inputStream.available() > 0) {
            result.add(inputStream.readPackedInteger())
         }
         return result
      }

      @kotlin.jvm.Throws(IOException::class)
      private fun readColumnNames(inputStream: ByteArrayInputStream): MutableList<String> {
         val columnNames: MutableList<String> = ArrayList<String>()
         while (inputStream.available() > 0) {
            columnNames.add(inputStream.readLengthEncodedString())
         }
         return columnNames
      }

      @kotlin.jvm.Throws(IOException::class)
      private fun readTypeValues(inputStream: ByteArrayInputStream): MutableList<Array<String>> {
         val result: MutableList<Array<String>> = ArrayList<Array<String>>()
         while (inputStream.available() > 0) {
            val typeValues: MutableList<String> = ArrayList<String>()
            val valuesCount = inputStream.readPackedInteger()
            for (i in 0..<valuesCount) {
               typeValues.add(inputStream.readLengthEncodedString())
            }
            result.add(typeValues.toTypedArray<String>())
         }
         return result
      }

      @kotlin.jvm.Throws(IOException::class)
      private fun readIntegerPairs(inputStream: ByteArrayInputStream): MutableMap<Int, Int> {
         val result: MutableMap<Int, Int> = LinkedHashMap<Int, Int>()
         while (inputStream.available() > 0) {
            val columnIndex = inputStream.readPackedInteger()
            val columnCharset = inputStream.readPackedInteger()
            result.put(columnIndex, columnCharset)
         }
         return result
      }
   }
}

class TransactionPayloadEventDataDeserializer : EventDataDeserializer<TransactionPayloadEventData> {
   @kotlin.jvm.Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): TransactionPayloadEventData {
      val eventData: TransactionPayloadEventData = TransactionPayloadEventData()
      // Read the header fields from the event data
      while (inputStream.available() > 0) {
         var fieldType = 0
         var fieldLen = 0
         // Read the type of the field
         if (inputStream.available() >= 1) {
            fieldType = inputStream.readPackedInteger()
         }
         // We have reached the end of the Event Data Header
         if (fieldType == OTW_PAYLOAD_HEADER_END_MARK) {
            break
         }
         // Read the size of the field
         if (inputStream.available() >= 1) {
            fieldLen = inputStream.readPackedInteger()
         }
         when (fieldType) {
            OTW_PAYLOAD_SIZE_FIELD ->                     // Fetch the payload size
               eventData.payloadSize = inputStream.readPackedInteger()

            OTW_PAYLOAD_COMPRESSION_TYPE_FIELD ->                     // Fetch the compression type
               eventData.compressionType = inputStream.readPackedInteger()

            OTW_PAYLOAD_UNCOMPRESSED_SIZE_FIELD ->                     // Fetch the uncompressed size
               eventData.uncompressedSize = inputStream.readPackedInteger()

            else ->                     // Ignore unrecognized field
               inputStream.read(fieldLen)
         }
      }
      if (eventData.uncompressedSize == 0) {
         // Default the uncompressed to the payload size
         eventData.uncompressedSize = eventData.payloadSize
      }
      // set the payload to the rest of the input buffer
      eventData.payload = inputStream.read(eventData.payloadSize)

      // Decompress the payload
      val src = eventData.payload
      val dst = ByteBuffer.allocate(eventData.uncompressedSize).array()
      Zstd.decompressByteArray(dst, 0, dst.size, src, 0, src!!.size)

      // Read and store events from a decompressed byte array into input stream
      val decompressedEvents = ArrayList<Event>()
      val transactionPayloadEventDeserializer = EventDeserializer()
      val destinationInputStream = ByteArrayInputStream(dst)

      var internalEvent = transactionPayloadEventDeserializer.nextEvent(destinationInputStream)
      while (internalEvent != null) {
         decompressedEvents.add(internalEvent)
         internalEvent = transactionPayloadEventDeserializer.nextEvent(destinationInputStream)
      }

      eventData.uncompressedEvents = decompressedEvents

      return eventData
   }

   companion object {
      const val OTW_PAYLOAD_HEADER_END_MARK: Int = 0
      const val OTW_PAYLOAD_SIZE_FIELD: Int = 1
      const val OTW_PAYLOAD_COMPRESSION_TYPE_FIELD: Int = 2
      const val OTW_PAYLOAD_UNCOMPRESSED_SIZE_FIELD: Int = 3
   }
}

class UpdateRowsEventDataDeserializer(tableMapEventByTableId: MutableMap<Long, TableMapEventData>) :
   AbstractRowsEventDataDeserializer<UpdateRowsEventData>(tableMapEventByTableId) {
   private var mayContainExtraInformation = false

   fun setMayContainExtraInformation(mayContainExtraInformation: Boolean): UpdateRowsEventDataDeserializer {
      this.mayContainExtraInformation = mayContainExtraInformation
      return this
   }

   @kotlin.jvm.Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): UpdateRowsEventData {
      val eventData = UpdateRowsEventData()
      eventData.tableId = inputStream.readLong(6)
      inputStream.skip(2) // reserved
      if (mayContainExtraInformation) {
         val extraInfoLength = inputStream.readInteger(2)
         inputStream.skip((extraInfoLength - 2).toLong())
      }
      val numberOfColumns = inputStream.readPackedInteger()
      eventData.includedColumnsBeforeUpdate = inputStream.readBitSet(numberOfColumns, true)
      eventData.includedColumns = inputStream.readBitSet(numberOfColumns, true)
      eventData.rows = deserializeRows(eventData, inputStream)
      return eventData
   }

   @kotlin.jvm.Throws(IOException::class)
   private fun deserializeRows(
      eventData: UpdateRowsEventData,
      inputStream: ByteArrayInputStream
   ): MutableList<MutableMap.MutableEntry<Array<Serializable?>, Array<Serializable?>>> {
      val tableId = eventData.tableId
      val includedColumnsBeforeUpdate = eventData.includedColumnsBeforeUpdate
      val includedColumns = eventData.includedColumns
      val rows: MutableList<MutableMap.MutableEntry<Array<Serializable?>, Array<Serializable?>>> =
         ArrayList<MutableMap.MutableEntry<Array<Serializable?>, Array<Serializable?>>>()
      while (inputStream.available() > 0) {
         rows.add(
            AbstractMap.SimpleEntry<Array<Serializable?>, Array<Serializable?>>(
               deserializeRow(tableId, includedColumnsBeforeUpdate!!, inputStream),
               deserializeRow(tableId, includedColumns!!, inputStream)
            )
         )
      }
      return rows
   }
}

class WriteRowsEventDataDeserializer(tableMapEventByTableId: MutableMap<Long, TableMapEventData>) :
   AbstractRowsEventDataDeserializer<WriteRowsEventData>(tableMapEventByTableId) {
   private var mayContainExtraInformation = false

   fun setMayContainExtraInformation(mayContainExtraInformation: Boolean): WriteRowsEventDataDeserializer {
      this.mayContainExtraInformation = mayContainExtraInformation
      return this
   }

   @kotlin.jvm.Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): WriteRowsEventData {
      val eventData = WriteRowsEventData()
      eventData.tableId = inputStream.readLong(6)
      inputStream.skip(2) // reserved
      if (mayContainExtraInformation) {
         val extraInfoLength = inputStream.readInteger(2)
         inputStream.skip((extraInfoLength - 2).toLong())
      }
      val numberOfColumns = inputStream.readPackedInteger()
      eventData.includedColumns = inputStream.readBitSet(numberOfColumns, true)
      eventData.rows = deserializeRows(eventData.tableId, eventData.includedColumns!!, inputStream)
      return eventData
   }

   @kotlin.jvm.Throws(IOException::class)
   private fun deserializeRows(
      tableId: Long,
      includedColumns: BitSet,
      inputStream: ByteArrayInputStream
   ): MutableList<Array<Serializable?>> {
      val result: MutableList<Array<Serializable?>> = ArrayList<Array<Serializable?>>()
      while (inputStream.available() > 0) {
         result.add(deserializeRow(tableId, includedColumns, inputStream))
      }
      return result
   }
}

/**
 * https://github.com/mysql/mysql-server/blob/5.7/libbinlogevents/src/control_events.cpp#L590
 *
 * onePhase : boolean, 1byte
 * formatID : int, 4byte
 * gtridLength : int, 4byte
 * bqualLength : int, 4byte
 * data : String, gtrid + bqual, (gtridLength + bqualLength)byte
 */
class XAPrepareEventDataDeserializer : EventDataDeserializer<XAPrepareEventData> {
   @kotlin.jvm.Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): XAPrepareEventData {
      val xaPrepareEventData = XAPrepareEventData()
      xaPrepareEventData.isOnePhase = inputStream.read() != 0x00
      xaPrepareEventData.formatID = inputStream.readInteger(4)
      xaPrepareEventData.gtridLength = inputStream.readInteger(4)
      xaPrepareEventData.bqualLength = inputStream.readInteger(4)
      xaPrepareEventData.setEventData(
         inputStream.read(
            xaPrepareEventData.gtridLength + xaPrepareEventData.bqualLength
         )
      )

      return xaPrepareEventData
   }
}

class XidEventDataDeserializer : EventDataDeserializer<XidEventData> {
   @kotlin.jvm.Throws(IOException::class)
   override fun deserialize(inputStream: ByteArrayInputStream): XidEventData {
      val eventData = XidEventData()
      eventData.xid = inputStream.readLong(8)
      return eventData
   }
}
