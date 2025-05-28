package bop.mysql


class PreparedStmt(
   var connection: MySQLConnection,
   var sql: String,
) {
   var id: Int = 0
   var numColumns: Int = 0
   var numParams: Int = 0
      set(value) {
         nulls = ByteArray(value * 7 / 8)
      }
   var warningCount: Int = 0
   var nulls: ByteArray = EMPTY_BYTES
   var hasMetaData: Boolean = false
}

internal fun PreparedStmt.onPrepareOK(packet: MySQLBuffer) {
   packet.readUByte()
   id = packet.readIntLE()
   numColumns = packet.readCharLE().code
   numParams = packet.readCharLE().code
   packet.skip(1)

   if (packet.size >= 12) {
      warningCount = packet.readCharLE().code

      if (connection.clientCapabilities and ClientCapabilities.CLIENT_OPTIONAL_RESULTSET_METADATA != 0) {
         // ResultSetMetadata.RESULTSET_METADATA_FULL
         hasMetaData = packet.readByte() != 0.toByte()
      }
   } else {
      warningCount = 0
   }
}

internal fun MySQLConnection.onOK(packet: MySQLBuffer) {

}

internal fun MySQLConnection.onQueryResponse(packet: MySQLBuffer) {

}
