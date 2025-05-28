package bop.mysql.event

import bop.mysql.binlog.event.deserialization.AbstractRowsEventDataDeserializer
import bop.mysql.binlog.event.deserialization.ColumnType
import bop.mysql.io.ByteArrayInputStream
import java.io.IOException
import java.math.BigDecimal
import java.math.BigInteger
import java.nio.charset.Charset
import java.util.*
import kotlin.math.abs

/**
 * Utility to parse the binary-encoded value of a MySQL `JSON` type, translating the encoded representation into
 * method calls on a supplied [JsonFormatter] implementation.
 *
 * <h2>Binary Format</h2>
 *
 * Each JSON value (scalar, object or array) has a one byte type identifier followed by the actual value.
 *
 * <h3>Scalar</h3>
 *
 * The binary value may contain a single scalar that is one of:
 *
 *  * null
 *  * boolean
 *  * int16
 *  * int32
 *  * int64
 *  * uint16
 *  * uint32
 *  * uint64
 *  * double
 *  * string
 *  * `DATE` as a string of the form `YYYY-MM-DD` where `YYYY` can be positive or negative
 *  * `TIME` as a string of the form `HH-MM-SS` where `HH` can be positive or negative
 *  * `DATETIME` as a string of the form `YYYY-MM-DD HH-mm-SS.ssssss` where `YYYY` can be positive or
 * negative
 *  * `TIMESTAMP` as the number of microseconds past epoch (January 1, 1970), or if negative the number of
 * microseconds before epoch (January 1, 1970)
 *  * any other MySQL value encoded as an opaque binary value
 *
 *
 * <h3>JSON Object</h3>
 *
 * If the value is a JSON object, its binary representation will have a header that contains:
 *
 *  * the member count
 *  * the size of the binary value in bytes
 *  * a list of pointers to each key
 *  * a list of pointers to each value
 *
 *
 * The actual keys and values will come after the header, in the same order as in the header.
 *
 * <h3>JSON Array</h3>
 *
 * If the value is a JSON array, the binary representation will have a header with
 *
 *  * the element count
 *  * the size of the binary value in bytes
 *  * a list of pointers to each value
 *
 * followed by the actual values, in the same order as in the header.
 *
 * <h2>Grammar</h2>
 * The grammar of the binary representation of JSON objects are defined in the MySQL codebase in the
 * [json_binary.h](https://github.com/mysql/mysql-server/blob/5.7/sql/json_binary.h) file:
 * <pre>
 * doc ::= type value
 * type ::=
 * 0x00 |  // small JSON object
 * 0x01 |  // large JSON object
 * 0x02 |  // small JSON array
 * 0x03 |  // large JSON array
 * 0x04 |  // literal (true/false/null)
 * 0x05 |  // int16
 * 0x06 |  // uint16
 * 0x07 |  // int32
 * 0x08 |  // uint32
 * 0x09 |  // int64
 * 0x0a |  // uint64
 * 0x0b |  // double
 * 0x0c |  // utf8mb4 string
 * 0x0f    // custom data (any MySQL data type)
 * value ::=
 * object  |
 * array   |
 * literal |
 * number  |
 * string  |
 * custom-data
 * object ::= element-count size key-entry* value-entry* key* value*
 * array ::= element-count size value-entry* value*
 * // number of members in object or number of elements in array
 * element-count ::=
 * uint16 |  // if used in small JSON object/array
 * uint32    // if used in large JSON object/array
 * // number of bytes in the binary representation of the object or array
 * size ::=
 * uint16 |  // if used in small JSON object/array
 * uint32    // if used in large JSON object/array
 * key-entry ::= key-offset key-length
 * key-offset ::=
 * uint16 |  // if used in small JSON object
 * uint32    // if used in large JSON object
 * key-length ::= uint16    // key length must be less than 64KB
 * value-entry ::= type offset-or-inlined-value
 * // This field holds either the offset to where the value is stored,
 * // or the value itself if it is small enough to be inlined (that is,
 * // if it is a JSON literal or a small enough [u]int).
 * offset-or-inlined-value ::=
 * uint16 |   // if used in small JSON object/array
 * uint32     // if used in large JSON object/array
 * key ::= utf8mb4-data
 * literal ::=
 * 0x00 |   // JSON null literal
 * 0x01 |   // JSON true literal
 * 0x02 |   // JSON false literal
 * number ::=  ....  // little-endian format for [u]int(16|32|64), whereas
 * // double is stored in a platform-independent, eight-byte
 * // format using float8store()
 * string ::= data-length utf8mb4-data
 * custom-data ::= custom-type data-length binary-data
 * custom-type ::= uint8   // type identifier that matches the
 * // internal enum_field_types enum
 * data-length ::= uint8*  // If the high bit of a byte is 1, the length
 * // field is continued in the next byte,
 * // otherwise it is the last byte of the length
 * // field. So we need 1 byte to represent
 * // lengths up to 127, 2 bytes to represent
 * // lengths up to 16383, and so on...
</pre> *
 *
 * @author [Randall Hauch](mailto:rhauch@gmail.com)
 */
class JsonBinary(private val reader: ByteArrayInputStream) {
   constructor(bytes: ByteArray) : this(ByteArrayInputStream(bytes))

   init {
      this.reader.mark(Int.Companion.MAX_VALUE)
   }

   val string: String
      get() {
         val handler = JsonStringFormatter()
         try {
            parse(handler)
         } catch (e: IOException) {
            throw RuntimeException(e)
         }
         return handler.string
      }

   @Throws(IOException::class)
   fun parse(formatter: JsonFormatter) {
      parse(readValueType(), formatter)
   }

   @Throws(IOException::class)
   fun parse(
      type: ValueType,
      formatter: JsonFormatter
   ) {
      when (type) {
         ValueType.SMALL_DOCUMENT -> parseObject(true, formatter)
         ValueType.LARGE_DOCUMENT -> parseObject(false, formatter)
         ValueType.SMALL_ARRAY -> parseArray(true, formatter)
         ValueType.LARGE_ARRAY -> parseArray(false, formatter)
         ValueType.LITERAL -> parseBoolean(formatter)
         ValueType.INT16 -> parseInt16(formatter)
         ValueType.UINT16 -> parseUInt16(formatter)
         ValueType.INT32 -> parseInt32(formatter)
         ValueType.UINT32 -> parseUInt32(formatter)
         ValueType.INT64 -> parseInt64(formatter)
         ValueType.UINT64 -> parseUInt64(formatter)
         ValueType.DOUBLE -> parseDouble(formatter)
         ValueType.STRING -> parseString(formatter)
         ValueType.CUSTOM -> parseOpaque(formatter)
         else -> throw IOException(
            "Unknown type value '" + asHex(type.code) + "' in first byte of a JSON value"
         )
      }
   }

   /**
    * Parse a JSON object.
    *
    *
    * The grammar of the binary representation of JSON objects are defined in the MySQL code base in the
    * [json_binary.h](https://github.com/mysql/mysql-server/blob/5.7/sql/json_binary.h) file:
    * <h3>Grammar</h3>
    *
    * <pre>
    * value ::=
    * object  |
    * array   |
    * literal |
    * number  |
    * string  |
    * custom-data
    * object ::= element-count size key-entry* value-entry* key* value*
    * // number of members in object or number of elements in array
    * element-count ::=
    * uint16 |  // if used in small JSON object/array
    * uint32    // if used in large JSON object/array
    * // number of bytes in the binary representation of the object or array
    * size ::=
    * uint16 |  // if used in small JSON object/array
    * uint32    // if used in large JSON object/array
    * key-entry ::= key-offset key-length
    * key-offset ::=
    * uint16 |  // if used in small JSON object
    * uint32    // if used in large JSON object
    * key-length ::= uint16    // key length must be less than 64KB
    * value-entry ::= type offset-or-inlined-value
    * // This field holds either the offset to where the value is stored,
    * // or the value itself if it is small enough to be inlined (that is,
    * // if it is a JSON literal or a small enough [u]int).
    * offset-or-inlined-value ::=
    * uint16 |   // if used in small JSON object/array
    * uint32     // if used in large JSON object/array
    * key ::= utf8mb4-data
    * literal ::=
    * 0x00 |   // JSON null literal
    * 0x01 |   // JSON true literal
    * 0x02 |   // JSON false literal
    * number ::=  ....  // little-endian format for [u]int(16|32|64), whereas
    * // double is stored in a platform-independent, eight-byte
    * // format using float8store()
    * string ::= data-length utf8mb4-data
    * custom-data ::= custom-type data-length binary-data
    * custom-type ::= uint8   // type identifier that matches the
    * // internal enum_field_types enum
    * data-length ::= uint8*  // If the high bit of a byte is 1, the length
    * // field is continued in the next byte,
    * // otherwise it is the last byte of the length
    * // field. So we need 1 byte to represent
    * // lengths up to 127, 2 bytes to represent
    * // lengths up to 16383, and so on...
    * </pre>
    *
    * @param small `true` if the object being read is "small", or `false` otherwise
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseObject(
      small: Boolean,
      formatter: JsonFormatter
   ) {
      // this is terrible, but without a decent seekable InputStream the other way seemed like
      // a full-on rewrite
      val objectOffset = this.reader.position

      // Read the header ...
      val numElements = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "number of elements in")
      val numBytes = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "size of")
      val valueSize = if (small) 2 else 4

      // Read each key-entry, consisting of the offset and length of each key ...
      val keys: Array<KeyEntry> = Array(numElements) {
         KeyEntry(
            readUnsignedIndex(numBytes, small, "key offset in"), readUInt16()
         )
      }

      // Read each key value value-entry
      val entries: Array<ValueEntry> = Array(numElements) { i ->
         // Parse the value ...
         val type = readValueType()
         var entry: ValueEntry?
         when (type) {
            ValueType.LITERAL -> {
               entry = ValueEntry(type).setValue(readLiteral())
               reader.skip((valueSize - 1).toLong())
            }

            ValueType.INT16 -> {
               entry = ValueEntry(type).setValue(readInt16())
               reader.skip((valueSize - 2).toLong())
            }

            ValueType.UINT16 -> {
               entry = ValueEntry(type).setValue(readUInt16())
               reader.skip((valueSize - 2).toLong())
            }

            ValueType.INT32 -> {
               if (!small) {
                  entry = ValueEntry(type).setValue(readInt32())
               } else if (!small) {
                  entry = ValueEntry(type).setValue(readUInt32())
               } else {
                  // It is an offset, not a value ...
                  val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
                  if (offset >= numBytes) {
                     throw IOException(
                        "The offset for the value in the JSON binary document is " + offset + ", which is larger than the binary form of the JSON document (" + numBytes + " bytes)"
                     )
                  }
                  entry = ValueEntry(type, offset)
               }
            }

            ValueType.UINT32 -> {
               if (!small) {
                  entry = ValueEntry(type).setValue(readUInt32())
               } else {
                  val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
                  if (offset >= numBytes) {
                     throw IOException(
                        "The offset for the value in the JSON binary document is " + offset + ", which is larger than the binary form of the JSON document (" + numBytes + " bytes)"
                     )
                  }
                  entry = ValueEntry(type, offset)
               }
            }

            else -> {
               val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
               if (offset >= numBytes) {
                  throw IOException(
                     "The offset for the value in the JSON binary document is " + offset + ", which is larger than the binary form of the JSON document (" + numBytes + " bytes)"
                  )
               }
               entry = ValueEntry(type, offset)
            }
         }
         entry
      }

      // Read each key ...
      for (i in 0..<numElements) {
         val skipBytes = keys[i].index + objectOffset - reader.position
         // Skip to a start of a field name if the current position does not point to it
         // This can happen for MySQL 8
         if (skipBytes != 0) {
            reader.fastSkip(skipBytes.toLong())
         }
         keys[i].name = reader.readString(keys[i].length)
      }

      // Now parse the values ...
      formatter.beginObject(numElements)
      for (i in 0..<numElements) {
         if (i != 0) {
            formatter.nextEntry()
         }
         formatter.name(keys[i].name ?: "")
         val entry = entries[i]
         if (entry.resolved) {
            val value = entry.value
            if (value == null) {
               formatter.valueNull()
            } else if (value is Boolean) {
               formatter.value(value)
            } else if (value is Int) {
               formatter.value(value)
            }
         } else {
            // Parse the value ...
            this.reader.reset()
            this.reader.fastSkip((objectOffset + entry.index).toLong())
            parse(entry.type, formatter)
         }
      }
      formatter.endObject()
   }

   /**
    * Parse a JSON array.
    *
    *
    * The grammar of the binary representation of JSON objects are defined in the MySQL code base in the
    * [json_binary.h](https://github.com/mysql/mysql-server/blob/5.7/sql/json_binary.h) file, and are:
    * <h3>Grammar</h3>
    *
    * <h3>Grammar</h3>
    *
    * <pre>
    * value ::=
    * object  |
    * array   |
    * literal |
    * number  |
    * string  |
    * custom-data
    * array ::= element-count size value-entry* value*
    * // number of members in object or number of elements in array
    * element-count ::=
    * uint16 |  // if used in small JSON object/array
    * uint32    // if used in large JSON object/array
    * // number of bytes in the binary representation of the object or array
    * size ::=
    * uint16 |  // if used in small JSON object/array
    * uint32    // if used in large JSON object/array
    * value-entry ::= type offset-or-inlined-value
    * // This field holds either the offset to where the value is stored,
    * // or the value itself if it is small enough to be inlined (that is,
    * // if it is a JSON literal or a small enough [u]int).
    * offset-or-inlined-value ::=
    * uint16 |   // if used in small JSON object/array
    * uint32     // if used in large JSON object/array
    * key ::= utf8mb4-data
    * literal ::=
    * 0x00 |   // JSON null literal
    * 0x01 |   // JSON true literal
    * 0x02 |   // JSON false literal
    * number ::=  ....  // little-endian format for [u]int(16|32|64), whereas
    * // double is stored in a platform-independent, eight-byte
    * // format using float8store()
    * string ::= data-length utf8mb4-data
    * custom-data ::= custom-type data-length binary-data
    * custom-type ::= uint8   // type identifier that matches the
    * // internal enum_field_types enum
    * data-length ::= uint8*  // If the high bit of a byte is 1, the length
    * // field is continued in the next byte,
    * // otherwise it is the last byte of the length
    * // field. So we need 1 byte to represent
    * // lengths up to 127, 2 bytes to represent
    * // lengths up to 16383, and so on...
    *
    *
    * @param small `true` if the object being read is "small", or `false` otherwise
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   // checkstyle, please ignore MethodLength for the next line
   @Throws(IOException::class)
   fun parseArray(
      small: Boolean,
      formatter: JsonFormatter
   ) {
      val arrayOffset = this.reader.position

      // Read the header ...
      val numElements = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "number of elements in")
      val numBytes = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "size of")
      val valueSize = if (small) 2 else 4

      // Read each key value value-entry
      val entries: Array<ValueEntry> = Array(numElements) { i ->
         // Parse the value ...
         val type = readValueType()
         var entry: ValueEntry?
         when (type) {
            ValueType.LITERAL -> {
               entry = ValueEntry(type).setValue(readLiteral())
               reader.skip((valueSize - 1).toLong())
            }

            ValueType.INT16 -> {
               entry = ValueEntry(type).setValue(readInt16())
               reader.skip((valueSize - 2).toLong())
            }

            ValueType.UINT16 -> {
               entry = ValueEntry(type).setValue(readUInt16())
               reader.skip((valueSize - 2).toLong())
            }

            ValueType.INT32 -> {
               if (!small) {
                  entry = ValueEntry(type).setValue(readInt32())
               } else if (!small) {
                  entry = ValueEntry(type).setValue(readUInt32())
               } else {
                  // It is an offset, not a value ...
                  val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
                  if (offset >= numBytes) {
                     throw IOException(
                        "The offset for the value in the JSON binary document is $offset, which is larger than the binary form of the JSON document ($numBytes bytes)"
                     )
                  }
                  entry = ValueEntry(type, offset)
               }
            }

            ValueType.UINT32 -> {
               if (!small) {
                  entry = ValueEntry(type).setValue(readUInt32())
               } else {
                  val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
                  if (offset >= numBytes) {
                     throw IOException(
                        "The offset for the value in the JSON binary document is $offset, which is larger than the binary form of the JSON document ($numBytes bytes)"
                     )
                  }
                  entry = ValueEntry(type, offset)
               }
            }

            else -> {
               val offset = readUnsignedIndex(Int.Companion.MAX_VALUE, small, "value offset in")
               if (offset >= numBytes) {
                  throw IOException(
                     "The offset for the value in the JSON binary document is $offset, which is larger than the binary form of the JSON document ($numBytes bytes)"
                  )
               }
               entry = ValueEntry(type, offset)
            }
         }
         entry
      }

      // Now parse the values ...
      formatter.beginArray(numElements)
      for (i in 0..<numElements) {
         if (i != 0) {
            formatter.nextEntry()
         }
         val entry = entries[i]
         if (entry.resolved) {
            val value = entry.value
            if (value == null) {
               formatter.valueNull()
            } else if (value is Boolean) {
               formatter.value(value)
            } else if (value is Int) {
               formatter.value(value)
            }
         } else {
            // Parse the value ...
            this.reader.reset()
            this.reader.fastSkip((arrayOffset + entry.index).toLong())

            parse(entry.type, formatter)
         }
      }
      formatter.endArray()
   }

   /**
    * Parse a literal value that is either null, `true`, or `false`.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseBoolean(formatter: JsonFormatter) {
      val literal = readLiteral()
      if (literal == null) {
         formatter.valueNull()
      } else {
         formatter.value(literal)
      }
   }

   /**
    * Parse a 2 byte integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseInt16(formatter: JsonFormatter) {
      val value = readInt16()
      formatter.value(value)
   }

   /**
    * Parse a 2 byte unsigned integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseUInt16(formatter: JsonFormatter) {
      val value = readUInt16()
      formatter.value(value)
   }

   /**
    * Parse a 4 byte integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseInt32(formatter: JsonFormatter) {
      val value = readInt32()
      formatter.value(value)
   }

   /**
    * Parse a 4 byte unsigned integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseUInt32(formatter: JsonFormatter) {
      val value = readUInt32()
      formatter.value(value)
   }

   /**
    * Parse a 8 byte integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseInt64(formatter: JsonFormatter) {
      val value = readInt64()
      formatter.value(value)
   }

   /**
    * Parse a 8 byte unsigned integer value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseUInt64(formatter: JsonFormatter) {
      val value = readUInt64()
      formatter.value(value)
   }

   /**
    * Parse a 8 byte double value.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseDouble(formatter: JsonFormatter) {
      val rawValue = readInt64()
      val value = Double.fromBits(rawValue)
      formatter.value(value)
   }

   /**
    * Parse the length and value of a string stored in MySQL's "utf8mb" character set (which equates to Java's
    * UTF-8 character set. The length is a [variable length integer][.readVariableInt] length of the string.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseString(formatter: JsonFormatter) {
      val length = readVariableInt()
      val value = String(reader.read(length), UTF_8)
      formatter.value(value)
   }

   /**
    * Parse an opaque type. Specific types such as [DATE][.parseDate],
    * [TIME][.parseTime], and [DATETIME][.parseDatetime] values are
    * stored as opaque types, though they are to be unpacked. TIMESTAMPs are also stored as opaque types, but
    * [converted](https://github.com/mysql/mysql-server/blob/5.7/sql/sql_time.cc#L1624) by MySQL to
    * `DATETIME` prior to storage.
    * Other MySQL types are stored as opaque types and passed on to the formatter as opaque values.
    *
    *
    * See the [
 * MySQL source code](https://github.com/mysql/mysql-server/blob/e0e0ae2ea27c9bb76577664845507ef224d362e4/sql/json_binary.cc#L1034) for the logic used in this method.
    * <h3>Grammar</h3>
    *
    * <pre>
    * custom-data ::= custom-type data-length binary-data
    * custom-type ::= uint8   // type identifier that matches the
    * // internal enum_field_types enum
    * data-length ::= uint8*  // If the high bit of a byte is 1, the length
    * // field is continued in the next byte,
    * // otherwise it is the last byte of the length
    * // field. So we need 1 byte to represent
    * // lengths up to 127, 2 bytes to represent
    * // lengths up to 16383, and so on...
   </pre> *
    *
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseOpaque(formatter: JsonFormatter) {
      // Read the custom type, which should be a standard ColumnType ...
      val customType = reader.read()
      val type = ColumnType.Companion.byCode(customType)
      if (type == null) {
         throw IOException(
            "Unknown type '" + asHex(customType) + "' in first byte of a JSON opaque value"
         )
      }
      // Read the data length ...
      val length = readVariableInt()

      when (type) {
         ColumnType.DECIMAL, ColumnType.NEWDECIMAL ->                 // See 'Json_decimal::convert_from_binary'
            // https://github.com/mysql/mysql-server/blob/5.7/sql/json_dom.cc#L1625
            parseDecimal(length, formatter)

         ColumnType.DATE -> parseDate(formatter)
         ColumnType.TIME, ColumnType.TIME_V2 -> parseTime(formatter)
         ColumnType.DATETIME, ColumnType.DATETIME_V2, ColumnType.TIMESTAMP, ColumnType.TIMESTAMP_V2 -> parseDatetime(
            formatter
         )

         else -> parseOpaqueValue(type, length, formatter)
      }
   }

   /**
    * Parse a `DATE` value, which is stored using the same format as `DATETIME`:
    * 5 bytes + fractional-seconds storage. However, the hour, minute, second, and fractional seconds are ignored.
    *
    *
    * The non-fractional part is 40 bits:
    *
    * <pre>
    * 1 bit  sign           (1= non-negative, 0= negative)
    * 17 bits year*13+month  (year 0-9999, month 0-12)
    * 5 bits day            (0-31)
    * 5 bits hour           (0-23)
    * 6 bits minute         (0-59)
    * 6 bits second         (0-59)
   </pre> *
    *
    * The fractional part is typically dependent upon the *fsp* (i.e., fractional seconds part) defined by
    * a column, but in the case of JSON it is always 3 bytes.
    *
    *
    * The format of all temporal values is outlined in the [MySQL documentation](https://dev.mysql.com/doc/internals/en/date-and-time-data-type-representation.html),
    * although since the MySQL `JSON` type is only available in 5.7, only version 2 of the date-time formats
    * are necessary.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseDate(formatter: JsonFormatter) {
      val raw = readInt64()
      val value = raw shr 24
      val yearMonth = (value shr 22).toInt() % (1 shl 17) // 17 bits starting at 22nd
      val year = yearMonth / 13
      val month = yearMonth % 13
      val day = (value shr 17).toInt() % (1 shl 5) // 5 bits starting at 17th
      formatter.valueDate(year, month, day)
   }

   /**
    * Parse a `TIME` value, which is stored using the same format as `DATETIME`:
    * 5 bytes + fractional-seconds storage. However, the year, month, and day values are ignored
    *
    *
    * The non-fractional part is 40 bits:
    *
    * <pre>
    * 1 bit  sign           (1= non-negative, 0= negative)
    * 17 bits year*13+month  (year 0-9999, month 0-12)
    * 5 bits day            (0-31)
    * 5 bits hour           (0-23)
    * 6 bits minute         (0-59)
    * 6 bits second         (0-59)
   </pre> *
    *
    * The fractional part is typically dependent upon the *fsp* (i.e., fractional seconds part) defined by
    * a column, but in the case of JSON it is always 3 bytes.
    *
    *
    * The format of all temporal values is outlined in the [MySQL documentation](https://dev.mysql.com/doc/internals/en/date-and-time-data-type-representation.html),
    * although since the MySQL `JSON` type is only available in 5.7, only version 2 of the date-time formats
    * are necessary.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseTime(formatter: JsonFormatter) {
      val raw = readInt64()
      val value = raw shr 24
      val negative = value < 0L
      var hour = (value shr 12).toInt() % (1 shl 10) // 10 bits starting at 12th
      val min = (value shr 6).toInt() % (1 shl 6) // 6 bits starting at 6th
      val sec = value.toInt() % (1 shl 6) // 6 bits starting at 0th
      if (negative) {
         hour *= -1
      }
      val microSeconds = (raw % (1 shl 24)).toInt()
      formatter.valueTime(hour, min, sec, microSeconds)
   }

   /**
    * Parse a `DATETIME` value, which is stored as 5 bytes + fractional-seconds storage.
    *
    *
    * The non-fractional part is 40 bits:
    *
    * <pre>
    * 1 bit  sign           (1= non-negative, 0= negative)
    * 17 bits year*13+month  (year 0-9999, month 0-12)
    * 5 bits day            (0-31)
    * 5 bits hour           (0-23)
    * 6 bits minute         (0-59)
    * 6 bits second         (0-59)
   </pre> *
    *
    * The sign bit is always 1. A value of 0 (negative) is reserved. The fractional part is typically dependent upon
    * the *fsp* (i.e., fractional seconds part) defined by a column, but in the case of JSON it is always 3 bytes.
    * Unlike the documentation, however, the 8 byte value is in *little-endian* form.
    *
    *
    * The format of all temporal values is outlined in the [MySQL documentation](https://dev.mysql.com/doc/internals/en/date-and-time-data-type-representation.html),
    * although since the MySQL `JSON` type is only available in 5.7, only version 2 of the date-time formats
    * are necessary.
    *
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseDatetime(formatter: JsonFormatter) {
      val raw = readInt64()
      val value = raw shr 24
      val yearMonth = (value shr 22).toInt() % (1 shl 17) // 17 bits starting at 22nd
      val year = yearMonth / 13
      val month = yearMonth % 13
      val day = (value shr 17).toInt() % (1 shl 5) // 5 bits starting at 17th
      val hour = (value shr 12).toInt() % (1 shl 5) // 5 bits starting at 12th
      val min = (value shr 6).toInt() % (1 shl 6) // 6 bits starting at 6th
      val sec = (value % (1 shl 6)).toInt() // 6 bits starting at 0th
      val microSeconds = (raw % (1 shl 24)).toInt()
      formatter.valueDatetime(year, month, day, hour, min, sec, microSeconds)
   }

   /**
    * Parse a `DECIMAL` value. The first two bytes are the precision and scale, followed by the binary
    * representation of the decimal itself.
    *
    * @param length the length of the complete binary representation
    * @param formatter the formatter to be notified of the parsed value; may not be null
    * @throws IOException if there is a problem reading the JSON value
    */
   @Throws(IOException::class)
   protected fun parseDecimal(
      length: Int,
      formatter: JsonFormatter
   ) {
      // First two bytes are the precision and scale ...
      val precision = reader.read()
      val scale = reader.read()

      // Followed by the binary representation (see `my_decimal_get_binary_size`)
      val decimalLength = length - 2
      val dec = AbstractRowsEventDataDeserializer.Companion.asBigDecimal(precision, scale, reader.read(decimalLength))
      formatter.value(dec)
   }

   @Throws(IOException::class)
   protected fun parseOpaqueValue(
      type: ColumnType?,
      length: Int,
      formatter: JsonFormatter
   ) {
      formatter.valueOpaque(type, reader.read(length))
   }

   @Throws(IOException::class)
   protected fun readFractionalSecondsInMicroseconds(): Int {
      return readBigEndianLong(3).toInt()
   }

   @Throws(IOException::class)
   protected fun readBigEndianLong(numBytes: Int): Long {
      val bytes = reader.read(numBytes)
      var result: Long = 0
      for (i in 0..<numBytes) {
         val b = bytes[i].toInt() and 0xFF
         result = (result shl 8) or b.toLong()
      }
      return result
   }

   @Throws(IOException::class)
   protected fun readUnsignedIndex(
      maxValue: Int,
      isSmall: Boolean,
      desc: String?
   ): Int {
      val result = if (isSmall) readUInt16().toLong() else readUInt32()
      if (result > maxValue) {
         throw IOException(
            "The " + desc + " the JSON document is " + result + " and is too big for the binary form of the document (" + maxValue + ")"
         )
      }
      if (result > Int.Companion.MAX_VALUE) {
         throw IOException("The " + desc + " the JSON document is " + result + " and is too big to be used")
      }
      return result.toInt()
   }

   @Throws(IOException::class)
   protected fun readInt16(): Int {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read()
      return (b2 shl 8 or b1).toShort().toInt()
   }

   @Throws(IOException::class)
   protected fun readUInt16(): Int {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read() and 0xFF
      return (b2 shl 8 or b1) and 0xFFFF
   }

   @Throws(IOException::class)
   protected fun readInt24(): Int {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read() and 0xFF
      val b3 = reader.read()
      return b3 shl 16 or (b2 shl 8) or b1
   }

   @Throws(IOException::class)
   protected fun readInt32(): Int {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read() and 0xFF
      val b3 = reader.read() and 0xFF
      val b4 = reader.read()
      return b4 shl 24 or (b3 shl 16) or (b2 shl 8) or b1
   }

   @Throws(IOException::class)
   protected fun readUInt32(): Long {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read() and 0xFF
      val b3 = reader.read() and 0xFF
      val b4 = reader.read() and 0xFF
      return ((b4 shl 24) or (b3 shl 16) or (b2 shl 8) or b1).toLong() and 0xFFFFFFFFL
   }

   @Throws(IOException::class)
   protected fun readInt64(): Long {
      val b1 = reader.read() and 0xFF
      val b2 = reader.read() and 0xFF
      val b3 = reader.read() and 0xFF
      val b4 = (reader.read() and 0xFF).toLong()
      val b5 = (reader.read() and 0xFF).toLong()
      val b6 = (reader.read() and 0xFF).toLong()
      val b7 = (reader.read() and 0xFF).toLong()
      val b8 = reader.read().toLong()
      return b8 shl 56 or (b7 shl 48) or (b6 shl 40) or (b5 shl 32) or (b4 shl 24) or (b3 shl 16).toLong() or (b2 shl 8).toLong() or b1.toLong()
   }

   @Throws(IOException::class)
   protected fun readUInt64(): BigInteger {
      val bigEndian = ByteArray(8)
      for (i in 8 downTo 1) {
         bigEndian[i - 1] = (reader.read() and 0xFF).toByte()
      }
      return BigInteger(1, bigEndian)
   }

   /**
    * Read a variable-length integer value.
    *
    *
    * If the high bit of a byte is 1, the length field is continued in the next byte, otherwise it is the last
    * byte of the length field. So we need 1 byte to represent lengths up to 127, 2 bytes to represent lengths up
    * to 16383, and so on...
    *
    * @return the integer value
    * @throws IOException if we don't encounter an end-of-int marker
    */
   @Throws(IOException::class)
   protected fun readVariableInt(): Int {
      var length = 0
      for (i in 0..4) {
         val b = reader.read().toByte()
         length = length or ((b.toInt() and 0x7F) shl (7 * i))
         if ((b.toInt() and 0x80) == 0) {
            return length
         }
      }
      throw IOException("Unexpected byte sequence ($length)")
   }

   @Throws(IOException::class)
   protected fun readLiteral(): Boolean? {
      val b = reader.read().toByte()
      if (b.toInt() == 0x00) {
         return null
      } else if (b.toInt() == 0x01) {
         return true
      } else if (b.toInt() == 0x02) {
         return false
      }
      throw IOException("Unexpected value '" + asHex(b) + "' for literal")
   }

   @Throws(IOException::class)
   protected fun readValueType(): ValueType {
      val b = reader.read().toByte()
      val result: ValueType = ValueType.Companion.byCode(b.toInt())!!
      if (result == null) {
         throw IOException("Unknown value type code '" + String.format("%02X", b.toInt()) + "'")
      }
      return result
   }

   /**
    * Class used internally to hold key entry information.
    */
   protected class KeyEntry(
      val index: Int,
      val length: Int
   ) {
      var name: String? = null

      fun setKey(key: String?): KeyEntry {
         this.name = key
         return this
      }
   }

   /**
    * Class used internally to hold value entry information.
    */
   protected class ValueEntry {
      val type: ValueType
      val index: Int
      var value: Any? = null
      var resolved: Boolean = false

      constructor(type: ValueType) {
         this.type = type
         this.index = 0
      }

      constructor(
         type: ValueType,
         index: Int
      ) {
         this.type = type
         this.index = index
      }

      fun setValue(value: Any?): ValueEntry {
         this.value = value
         this.resolved = true
         return this
      }
   }

   companion object {
      private val UTF_8: Charset = Charset.forName("UTF-8")

      /**
       * Parse the MySQL binary representation of a `JSON` value and return the JSON string representation.
       *
       *
       * This method is equivalent to [.parse] using the [JsonStringFormatter].
       *
       * @param bytes the binary representation; may not be null
       * @return the JSON string representation; never null
       * @throws IOException if there is a problem reading or processing the binary representation
       */
      @Throws(IOException::class)
      fun parseAsString(bytes: ByteArray): String {/* check for mariaDB-format JSON strings inside columns marked JSON */
         if (isJSONString(bytes)) {
            return String(bytes)
         }
         val handler = JsonStringFormatter()
         parse(bytes, handler)
         return handler.string
      }

      private fun isJSONString(bytes: ByteArray): Boolean {
         return bytes[0] > 0x0f
      }

      /**
       * Parse the MySQL binary representation of a `JSON` value and call the supplied [JsonFormatter]
       * for the various components of the value.
       *
       * @param bytes the binary representation; may not be null
       * @param formatter the formatter that will be called as the binary representation is parsed; may not be null
       * @throws IOException if there is a problem reading or processing the binary representation
       */
      @Throws(IOException::class)
      fun parse(
         bytes: ByteArray,
         formatter: JsonFormatter
      ) {
         JsonBinary(bytes).parse(formatter)
      }

      protected fun asHex(b: Byte): String {
         return String.format("%02X ", b)
      }

      protected fun asHex(value: Int): String {
         return Integer.toHexString(value)
      }
   }
}

/**
 * Handle the various actions involved when [JsonBinary.parse] a JSON binary
 * value.
 *
 * @author [Randall Hauch](mailto:rhauch@gmail.com)
 */
interface JsonFormatter {
   /**
    * Prepare to receive the name-value pairs in a JSON object.
    *
    * @param numElements the number of name-value pairs (or elements)
    */
   fun beginObject(numElements: Int)

   /**
    * Prepare to receive the value pairs that in a JSON array.
    *
    * @param numElements the number of array elements
    */
   fun beginArray(numElements: Int)

   /**
    * Complete the previously-started JSON object.
    */
   fun endObject()

   /**
    * Complete the previously-started JSON array.
    */
   fun endArray()

   /**
    * Receive the name of an element in a JSON object.
    *
    * @param name the element's name; never null
    */
   fun name(name: String)

   /**
    * Receive the string value of an element in a JSON object.
    *
    * @param value the element's value; never null
    */
   fun value(value: String)

   /**
    * Receive the integer value of an element in a JSON object.
    *
    * @param value the element's value
    */
   fun value(value: Int)

   /**
    * Receive the long value of an element in a JSON object.
    *
    * @param value the element's value
    */
   fun value(value: Long)

   /**
    * Receive the double value of an element in a JSON object.
    *
    * @param value the element's value
    */
   fun value(value: Double)

   /**
    * Receive the [BigInteger] value of an element in a JSON object.
    *
    * @param value the element's value; never null
    */
   fun value(value: BigInteger)

   /**
    * Receive the [BigDecimal] value of an element in a JSON object.
    *
    * @param value the element's value; never null
    */
   fun value(value: BigDecimal)

   /**
    * Receive the boolean value of an element in a JSON object.
    *
    * @param value the element's value
    */
   fun value(value: Boolean)

   /**
    * Receive a null value of an element in a JSON object.
    */
   fun valueNull()

   /**
    * Receive the year value of an element in a JSON object.
    *
    * @param year the year number that makes up the element's value
    */
   fun valueYear(year: Int)

   /**
    * Receive the date value of an element in a JSON object.
    *
    * @param year the positive or negative year in the element's date value
    * @param month the month (0-12) in the element's date value
    * @param day the day of the month (0-31) in the element's date value
    */
   fun valueDate(
      year: Int,
      month: Int,
      day: Int
   )

   /**
    * Receive the date and time value of an element in a JSON object.
    *
    * @param year the positive or negative year in the element's date value
    * @param month the month (0-12) in the element's date value
    * @param day the day of the month (0-31) in the element's date value
    * @param hour the hour of the day (0-24) in the element's time value
    * @param min the minutes of the hour (0-60) in the element's time value
    * @param sec the seconds of the minute (0-60) in the element's time value
    * @param microSeconds the number of microseconds in the element's time value
    */
   // checkstyle, please ignore ParameterNumber for the next line
   fun valueDatetime(
      year: Int,
      month: Int,
      day: Int,
      hour: Int,
      min: Int,
      sec: Int,
      microSeconds: Int
   )

   /**
    * Receive the time value of an element in a JSON object.
    *
    * @param hour the hour of the day (0-24) in the element's time value
    * @param min the minutes of the hour (0-60) in the element's time value
    * @param sec the seconds of the minute (0-60) in the element's time value
    * @param microSeconds the number of microseconds in the element's time value
    */
   fun valueTime(
      hour: Int,
      min: Int,
      sec: Int,
      microSeconds: Int
   )

   /**
    * Receive the timestamp value of an element in a JSON object.
    *
    * @param secondsPastEpoch the number of seconds past epoch (January 1, 1970) in the element's timestamp value
    * @param microSeconds the number of microseconds in the element's time value
    */
   fun valueTimestamp(
      secondsPastEpoch: Long,
      microSeconds: Int
   )

   /**
    * Receive an opaque value of an element in a JSON object.
    *
    * @param type the column type for the value; may not be null
    * @param value the binary representation for the element's value
    */
   fun valueOpaque(
      type: ColumnType?,
      value: ByteArray?
   )

   /**
    * Called after an entry signaling that another entry will be signaled.
    */
   fun nextEntry()
}

/**
 * A [JsonFormatter] implementation that creates a JSON string representation.
 *
 * @author [Randall Hauch](mailto:rhauch@gmail.com)
 */
class JsonStringFormatter : JsonFormatter {
   private val sb: StringBuilder

   constructor() {
      this.sb = StringBuilder()
   }

   /**
    * Constructs a JsonFormatter with the given initial capacity.
    * @param capacity the initial capacity. Should be a positive number.
    */
   constructor(capacity: Int) {
      this.sb = StringBuilder(capacity)
   }

   override fun toString(): String {
      return this.string
   }

   val string: String
      get() = sb.toString()

   override fun beginObject(numElements: Int) {
      sb.append('{')
   }

   override fun beginArray(numElements: Int) {
      sb.append('[')
   }

   override fun endObject() {
      sb.append('}')
   }

   override fun endArray() {
      sb.append(']')
   }

   override fun name(name: String) {
      sb.append('"')
      appendString(name)
      sb.append("\":")
   }

   override fun value(value: String) {
      sb.append('"')
      appendString(value)
      sb.append('"')
   }

   override fun value(value: Int) {
      sb.append(value)
   }

   override fun value(value: Long) {
      sb.append(value)
   }

   override fun value(value: Double) {
      // Double's toString method will result in scientific notation and loss of precision
      val str = value.toString()
      if (str.contains("E")) {
         value(BigDecimal(value))
      } else {
         sb.append(str)
      }
   }

   override fun value(value: BigInteger) {
      // Using the BigInteger.toString() method will result in scientific notation, so instead ...
      value(BigDecimal(value))
   }

   override fun value(value: BigDecimal) {
      // Using the BigInteger.toString() method will result in scientific notation, so instead ...
      sb.append(value.toPlainString())
   }

   override fun value(value: Boolean) {
      sb.append(value.toString())
   }

   override fun valueNull() {
      sb.append("null")
   }

   override fun valueYear(year: Int) {
      sb.append(year)
   }

   override fun valueDate(
      year: Int,
      month: Int,
      day: Int
   ) {
      sb.append('"')
      appendDate(year, month, day)
      sb.append('"')
   }

   // checkstyle, please ignore ParameterNumber for the next line
   override fun valueDatetime(
      year: Int,
      month: Int,
      day: Int,
      hour: Int,
      min: Int,
      sec: Int,
      microSeconds: Int
   ) {
      sb.append('"')
      appendDate(year, month, day)
      sb.append(' ')
      appendTime(hour, min, sec, microSeconds)
      sb.append('"')
   }

   override fun valueTime(
      hour: Int,
      min: Int,
      sec: Int,
      microSeconds: Int
   ) {
      var hour = hour
      sb.append('"')
      if (hour < 0) {
         sb.append('-')
         hour = abs(hour)
      }
      appendTime(hour, min, sec, microSeconds)
      sb.append('"')
   }

   override fun valueTimestamp(
      secondsPastEpoch: Long,
      microSeconds: Int
   ) {
      sb.append(secondsPastEpoch)
      appendSixDigitUnsignedInt(microSeconds, false)
   }

   override fun valueOpaque(
      type: ColumnType?,
      value: ByteArray?
   ) {
      sb.append('"')
      sb.append(Base64.getEncoder().encodeToString(value))
      sb.append('"')
   }

   override fun nextEntry() {
      sb.append(',')
   }

   /**
    * Append a string by escaping any characters that must be escaped.
    *
    * @param original the string to be written; may not be null
    */
   protected fun appendString(original: String) {
      var i = 0
      val len = original.length
      while (i < len) {
         val c = original[i]
         val ch = c.code
         if (ch < 0 || ch >= ESCAPES.size || ESCAPES[ch] == 0) {
            sb.append(c)
            ++i
            continue
         }
         val escape: Int = ESCAPES[ch]
         if (escape > 0) { // 2-char escape, fine
            sb.append('\\')
            sb.append(escape.toChar())
         } else {
            unicodeEscape(ch)
         }
         ++i
      }
   }

   /**
    * Append a generic Unicode escape (e.g., `\\uXXXX`) for given character.
    *
    * @param charToEscape the character to escape
    */
   private fun unicodeEscape(charToEscape: Int) {
      var charToEscape = charToEscape
      sb.append('\\')
      sb.append('u')
      if (charToEscape > 0xFF) {
         val hi = (charToEscape shr 8) and 0xFF
         sb.append(HEX_CODES[hi shr 4])
         sb.append(HEX_CODES[hi and 0xF])
         charToEscape = charToEscape and 0xFF
      } else {
         sb.append('0')
         sb.append('0')
      }
      // We know it's a control char, so only the last 2 chars are non-0
      sb.append(HEX_CODES[charToEscape shr 4])
      sb.append(HEX_CODES[charToEscape and 0xF])
   }

   protected fun appendTwoDigitUnsignedInt(value: Int) {
      assert(value >= 0)
      assert(value < 100)
      if (value < 10) {
         sb.append("0").append(value)
      } else {
         sb.append(value)
      }
   }

   protected fun appendFourDigitUnsignedInt(value: Int) {
      if (value < 10) {
         sb.append("000").append(value)
      } else if (value < 100) {
         sb.append("00").append(value)
      } else if (value < 1000) {
         sb.append("0").append(value)
      } else {
         sb.append(value)
      }
   }

   protected fun appendSixDigitUnsignedInt(
      value: Int,
      trimTrailingZeros: Boolean
   ) {
      var value = value
      assert(value > 0)
      assert(value < 1000000)
      // Add prefixes if necessary ...
      if (value < 10) {
         sb.append("00000")
      } else if (value < 100) {
         sb.append("0000")
      } else if (value < 1000) {
         sb.append("000")
      } else if (value < 10000) {
         sb.append("00")
      } else if (value < 100000) {
         sb.append("0")
      }
      if (trimTrailingZeros) {
         // Remove any trailing 0's ...
         for (i in 0..5) {
            if (value % 10 == 0) {
               value /= 10
            }
         }
         sb.append(value)
      }
   }

   fun appendDate(
      year: Int,
      month: Int,
      day: Int
   ) {
      var year = year
      if (year < 0) {
         sb.append('-')
         year = abs(year)
      }
      appendFourDigitUnsignedInt(year)
      sb.append('-')
      appendTwoDigitUnsignedInt(month)
      sb.append('-')
      appendTwoDigitUnsignedInt(day)
   }

   fun appendTime(
      hour: Int,
      min: Int,
      sec: Int,
      microSeconds: Int
   ) {
      appendTwoDigitUnsignedInt(hour)
      sb.append(':')
      appendTwoDigitUnsignedInt(min)
      sb.append(':')
      appendTwoDigitUnsignedInt(sec)
      if (microSeconds != 0) {
         sb.append('.')
         appendSixDigitUnsignedInt(microSeconds, true)
      }
   }

   companion object {
      /**
       * Value used for lookup tables to indicate that matching characters
       * do not need to be escaped.
       */
      private const val ESCAPE_NONE = 0

      /**
       * Value used for lookup tables to indicate that matching characters
       * are to be escaped using standard escaping; for JSON this means
       * (for example) using the "backslash-u" escape method.
       */
      private val ESCAPE_GENERIC = -1

      /**
       * A lookup table that determines which of the first 128 Unicode code points (single-byte UTF-8 characters)
       * must be escaped. A value of '0' means no escaping is required; positive values must be escaped with a
       * preceding backslash; and negative values that generic escaping (e.g., `\\uXXXX`).
       */
      private val ESCAPES: IntArray

      init {
         val escape = IntArray(128)
         // Generic escape for control characters ...
         for (i in 0..31) {
            escape[i] = ESCAPE_GENERIC
         }
         // Backslash escape for other specific characters ...
         escape['"'.code] = '"'.code
         escape['\\'.code] = '\\'.code
         // Escaping of slash is optional, so let's not add it
         escape[0x08] = 'b'.code
         escape[0x09] = 't'.code
         escape[0x0C] = 'f'.code
         escape[0x0A] = 'n'.code
         escape[0x0D] = 'r'.code
         ESCAPES = escape
      }

      private val HEX_CODES = "0123456789ABCDEF".toCharArray()
   }
}

/**
 * The set of values that can be used within a MySQL JSON value.
 *
 *
 * These values are defined in the MySQL codebase in the
 * [json_binary.h](https://github.com/mysql/mysql-server/blob/5.7/sql/json_binary.h) file, and are:
 *
 * <pre>
 * type ::=
 * 0x00 |  // small JSON object
 * 0x01 |  // large JSON object
 * 0x02 |  // small JSON array
 * 0x03 |  // large JSON array
 * 0x04 |  // literal (true/false/null)
 * 0x05 |  // int16
 * 0x06 |  // uint16
 * 0x07 |  // int32
 * 0x08 |  // uint32
 * 0x09 |  // int64
 * 0x0a |  // uint64
 * 0x0b |  // double
 * 0x0c |  // utf8mb4 string
 * 0x0f    // custom data (any MySQL data type)
 * </pre>
 *
 */
enum class ValueType(
   val code: Int
) {
   SMALL_DOCUMENT(0x00),
   LARGE_DOCUMENT(0x01),
   SMALL_ARRAY(0x02),
   LARGE_ARRAY(0x03),
   LITERAL(0x04),
   INT16(0x05),
   UINT16(0x06),
   INT32(0x07),
   UINT32(0x08),
   INT64(0x09),
   UINT64(0x0a),
   DOUBLE(0x0b),
   STRING(0x0c),
   CUSTOM(0x0f);

   companion object {
      private val TYPE_BY_CODE = HashMap<Int, ValueType>()

      init {
         for (type in entries) {
            TYPE_BY_CODE.put(type.code, type)
         }
      }

      @JvmStatic
      fun byCode(code: Int): ValueType? {
         return TYPE_BY_CODE[code]
      }
   }
}