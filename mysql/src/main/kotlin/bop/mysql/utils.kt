package bop.mysql

import bop.mysql.network.Packet
import bop.unsafe.Danger
import java.net.Socket

// https://dev.mysql.com/doc/internals/en/sending-more-than-16mbyte.html
const val MAX_PACKET_LENGTH = 16777215

val NULL_SOCKET = Socket()

const val NET_HEADER_SIZE: Int = 4
const val COMP_HEADER_SIZE: Int = 3

fun toCStringBytes(s: String): ByteArray {
   var src = Danger.getBytes(s)
   var dst = ByteArray(src.size + 1)
   System.arraycopy(src, 0, dst, 0, src.size)
   dst[src.size] = 0.toByte()
   return dst
}

fun readUInt24LE(bytes: ByteArray): Int {
   require(bytes.size >= 3) { "At least 3 bytes are required" }
   return (bytes[0].toUByte().toInt() and 0xFF) or ((bytes[1].toUByte()
      .toInt() and 0xFF) shl 8) or ((bytes[2].toUByte().toInt() and 0xFF) shl 16)
}

class MySQLError(
   val errorCode: Int,
   val sqlState: String,
   val errorMessage: String
) : Packet {
   companion object {
      fun parse(buffer: MySQLBuffer): MySQLError {
         return MySQLError(
            buffer.readIntLE(2), if (buffer.peek() == '#'.code) {
               // marker of the SQL State
               buffer.skip(1)
               buffer.readString(5)
            } else {
               ""
            }, buffer.readString(buffer.available)
         )
      }
   }
}