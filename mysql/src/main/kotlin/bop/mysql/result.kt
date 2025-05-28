package bop.mysql

import java.io.IOException

internal fun MySQLConnection.readResultSet(): ResultSet {
   resultSet.clear()
   readPacket()
   checkError()

//      val metadataFollows = if (clientCapabilities and ClientCapabilities.CLIENT_OPTIONAL_RESULTSET_METADATA != 0)
//         packet.readUByte()
//      else
//         0

//   val columnCount = packet.readPackedNumber()

   resultSet.columnCount = packet.readPackedNumber().toInt()

   for (i in 0 until resultSet.columnCount) {
      readPacket()

      // EOF?
      if (packetHeader == 0x00 || packetHeader == 0xFE) {
         break
      }

      val col = resultSet.columns[i]
      col.reset(resultSet.columnBuffer, resultSet.columnBuffer.size, packet.size)
      resultSet.columnBuffer.appendFrom(packet, packet.size)

      val name = col.name

      checkError()
   }

   if (clientCapabilities and ClientCapabilities.CLIENT_DEPRECATE_EOF == 0) {
      readPacket()
      if (packetHeader != 0x00 && packetHeader != 0xFE) {
         throw java.lang.IllegalStateException("expected EOF after column definitions")
      }
   }

   while (true) {
      readPacket()
      // EOF?
      if (packetHeader == 0x00 || packetHeader == 0xFE) {
         break
      }
      checkError()
      resultSet.push(packet)
   }

   return resultSet
}

//data class ColumnDefinition(
//   /**
//    * The catalog used. Currently always "def"
//    */
//   val catalog: String,
//
//   /**
//    * schema name
//    */
//   val schema: String,
//
//   /**
//    * virtual table name
//    */
//   val table: String,
//
//   /**
//    * physical table name
//    */
//   val orgTable: String,
//
//   /**
//    * virtual column name
//    */
//   val name: String,
//
//   /**
//    * physical column name
//    */
//   val orgName: String,
//
//   val lengthOfFixedLengthFields: Int,
//
//   /**
//    * the column character set as defined in Character Set
//    */
//   val characterSet: Int,
//
//   /**
//    * maximum length of the field
//    */
//   val columnLength: Int,
//
//   /**
//    * type of the column as defined in enum_field_types
//    */
//   val type: Int,
//
//   /**
//    * Flags as defined in Column Definition Flags
//    */
//   val flags: Int,
//
//   /**
//    * max shown decimal digits:
//    * 0x00 for integers and static strings
//    * 0x1f for dynamic strings, double, float
//    * 0x00 to 0x51 for decimals
//    */
//   val decimals: Int,
//
//   val defaultValue: String? = null
//)

//internal fun MySQLConnection.readColumnDef(): ColumnDefinition {
//
//   return ColumnDefinition(
//      catalog = packet.readLengthEncodedString(),
//      schema = packet.readLengthEncodedString(),
//      table = packet.readLengthEncodedString(),
//      orgTable = packet.readLengthEncodedString(),
//      name = packet.readLengthEncodedString(),
//      orgName = packet.readLengthEncodedString(),
//      lengthOfFixedLengthFields = packet.readPackedInteger(),
//      characterSet =  packet.readCharLE().code,
//      columnLength = packet.readIntLE(),
//      type = packet.readUByte(),
//      flags = packet.readUByte(),
//      decimals = packet.readUByte(),
//
//      // skip reserved
//      defaultValue = if (packet.skip(2) != null && command == CommandType.FIELD_LIST.ordinal)
//         packet.readLengthEncodedString()
//      else null
//   )
//}

class ColumnDef(
   var buffer: MySQLBuffer = MySQLBuffer.allocate(128),
   var offset: Int = 0,
   var length: Int = 0
) {
   fun reset(
      buffer: MySQLBuffer,
      offset: Int,
      length: Int
   ) {
      this.buffer = buffer
      this.offset = offset
      this.length = length
      this.hydrated = false
   }

   var catalogOffset: Int = 0
      private set
   var catalogLength: Int = 0
      private set
   var schemaOffset: Int = 0
      private set
   var schemaLength: Int = 0
      private set
   var tableOffset: Int = 0
      private set
   var tableLength: Int = 0
      private set
   var orgTableOffset: Int = 0
      private set
   var orgTableLength: Int = 0
      private set
   var nameOffset: Int = 0
      private set
   var nameLength: Int = 0
      private set
   var orgNameOffset: Int = 0
      private set
   var orgNameLength: Int = 0
      private set
   var defaultValueOffset: Int = 0
      private set
   var defaultValueLength: Int = 0
      private set

   var lengthOfFixedLengthFields: Int = 0
      private set
   var characterSet: Int = 0
      private set
   var columnLength: Int = 0
      private set
   var type: Int = 0
      private set
   var flags: Int = 0
      private set
   var decimals: Int = 0
      private set

   val catalog: String
      get() {
         hydrate()
         if (isCatalogCached) return catalogCached
         isCatalogCached = true
         catalogCached = buffer.getString(catalogOffset, catalogLength)
         return catalogCached
      }

   val schema: String
      get() {
         hydrate()
         if (isSchemaCached) return schemaCached
         isSchemaCached = true
         schemaCached = buffer.getString(schemaOffset, schemaLength)
         return schemaCached
      }
   val table: String
      get() {
         hydrate()
         if (isTableCached) return tableCached
         isTableCached = true
         tableCached = buffer.getString(tableOffset, tableLength)
         return tableCached
      }

   val orgTable: String
      get() {
         hydrate()
         if (isOrgTableCached) return orgTableCached
         isOrgTableCached = true
         orgTableCached = buffer.getString(orgTableOffset, orgTableLength)
         return orgTableCached
      }
   val name: String
      get() {
         hydrate()
         if (isNameCached) return nameCached
         isNameCached = true
         nameCached = buffer.getString(nameOffset, nameLength)
         return nameCached
      }

   val orgName: String
      get() {
         hydrate()
         if (isOrgNameCached) return orgNameCached
         isOrgNameCached = true
         orgNameCached = buffer.getString(orgNameOffset, orgNameLength)
         return orgNameCached
      }
   val defaultValue: String
      get() {
         hydrate()
         if (isDefaultValueCached) return defaultValueCached
         isDefaultValueCached = true
         defaultValueCached = buffer.getString(defaultValueOffset, defaultValueLength)
         return defaultValueCached
      }

   private var hydrated: Boolean = false
   private var position: Int = 0
   private var catalogCached: String = ""
   private var isCatalogCached: Boolean = false
   private var schemaCached: String = ""
   private var isSchemaCached: Boolean = false
   private var tableCached: String = ""
   private var isTableCached: Boolean = false
   private var orgTableCached: String = ""
   private var isOrgTableCached: Boolean = false
   private var nameCached: String = ""
   private var isNameCached: Boolean = false
   private var orgNameCached: String = ""
   private var isOrgNameCached: Boolean = false
   private var defaultValueCached: String = ""
   private var isDefaultValueCached: Boolean = false

   fun hydrate() {
      if (hydrated) return
      hydrated = true

      position = offset

      isCatalogCached = false
      isSchemaCached = false
      isTableCached = false
      isOrgTableCached = false
      isNameCached = false
      isOrgNameCached = false
      isDefaultValueCached = false

      catalogCached = ""
      schemaCached = ""
      tableCached = ""
      orgTableCached = ""
      nameCached = ""
      orgNameCached = ""
      defaultValueCached = ""

      catalogLength = readLength()
      catalogOffset = advance(catalogLength)

      schemaLength = readLength()
      schemaOffset = advance(schemaLength)

      tableLength = readLength()
      tableOffset = advance(tableLength)

      orgTableLength = readLength()
      orgTableOffset = advance(orgTableLength)

      nameLength = readLength()
      nameOffset = advance(nameLength)

      orgNameLength = readLength()
      orgNameOffset = advance(orgNameLength)

      lengthOfFixedLengthFields = readPackedNumber().toInt()
      characterSet = buffer.getCharLE(advance(2)).code
      columnLength = buffer.getIntLE(advance(4))
      type = buffer.getUByte(advance(1))
      flags = buffer.getUByte(advance(1))
      decimals = buffer.getUByte(advance(1))

      defaultValueLength = readLength()
      defaultValueOffset = advance(defaultValueLength)
   }

   private fun advance(count: Int): Int {
      val before = position
      position += count
      return before
   }

   fun readLength(): Int {
      return readPackedNumber().toInt()
   }

   fun readPackedNumber(): Long {
      val b: Int = buffer.getUByte(position++)
      if (b < 251) {
         return b.toLong()
      } else if (b == 251) {
         return 0
      } else if (b == 252) {
         return buffer.getCharLE(advance(2)).code.toLong()
      } else if (b == 253) {
         return buffer.getLong24LE(advance(3))
      } else if (b == 254) {
         return buffer.getLongLE(advance(8))
      }
      throw IOException("Unexpected packed number byte $b")
   }
}


class ResultSet {
   var columnBuffer = MySQLBuffer.allocate(1024)
   var buffer = MySQLBuffer.allocate(65536)

   val columns = mutableListOf<ColumnDef>()
   val rows = mutableListOf<Row>()
   var size: Int = 0
      private set

   var columnCount: Int = 0
      set(value) {
         field = value
         if (columns.size <= value) {
            while (columns.size < value) {
               columns.add(ColumnDef(columnBuffer))
            }
         }
      }

   fun isEmpty() = size == 0
   fun isNotEmpty() = size > 0

   fun clear() {
      size = 0
      buffer.reset()
   }

   fun push(from: MySQLBuffer): Row {
      while (rows.size <= size) {
         rows.add(Row(rows.size, buffer, 0, 0))
      }
      val row: Row = rows[size++]
      row.set(from)
      return row
   }

   operator fun get(index: Int): Row {
      if (index < 0 || index >= size) throw ArrayIndexOutOfBoundsException(index)
      return rows[index]
   }

   class Row(
      val index: Int,
      val buffer: MySQLBuffer,
      var offset: Int,
      var length: Int
   ) {
      val values = mutableListOf<Value>()
      var size: Int = 0

      fun clear() {
         size = 0
      }

      operator fun get(index: Int): Value {
         if (index < 0 || index >= size) throw ArrayIndexOutOfBoundsException(index)
         return values[index]
      }

      fun set(from: MySQLBuffer) {
         size = 0
         this.offset = buffer.position
         while (from.available > 0) {
            val length = from.readPackedInteger()
            val offset = buffer.appendFrom(from, length)
            if (values.size <= size) {
               values.add(Value(size++, buffer, offset, length))
            } else {
               val value = values[size]
               value.offset = offset
               value.length = length
            }
         }
      }
   }

   class Value(
      val index: Int,
      val buffer: MySQLBuffer,
      var offset: Int,
      var length: Int
   ) {
      fun toBoolean(): Boolean {
         return buffer.getByte(offset) != 0.toByte()
      }

      fun toByte(): Byte {
         return buffer.getByte(offset)
      }

      fun toUByte(): UByte {
         return buffer.getUByte(offset).toUByte()
      }

      fun toShort(): Short {
         return buffer.getShortLE(offset)
      }

      fun toChar(): Char {
         return buffer.getCharLE(offset)
      }

      fun toInt(): Int {
         return buffer.getInt(offset)
      }

      fun toLong(): Long {
         return buffer.getLongLE(offset)
      }

      fun toFloat(): Float {
         return buffer.getFloatLE(offset)
      }

      fun toDouble(): Double {
         return buffer.getDoubleLE(offset)
      }

      override fun toString(): String {
         return buffer.getString(offset, length)
      }

      fun toByteArray(): ByteArray {
         if (length == 0) {
            return EMPTY_BYTES
         }
         val bytes = ByteArray(length)
         buffer.getBytes(offset, bytes, 0, length)
         return bytes
      }
   }
}