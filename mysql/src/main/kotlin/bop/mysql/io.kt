package bop.mysql

import bop.io.DirectBytes
import bop.unsafe.Danger
import java.io.IOException

fun DirectBytes.readUShortLE(): UShort {
   return readShortLE().toUShort()
}

fun DirectBytes.readUIntLE(): UInt {
   return readIntLE().toUInt()
}

class MySQLBuffer private constructor(
   base: ByteArray,
   address: Long,
   size: Int,
   capacity: Int,
   owned: Boolean
) : DirectBytes(
   base, address, size, capacity, owned
) {
   fun readPackedNumber(): Long {
      val b: Int = this.readUByte()
      if (b < 251) {
         return b.toLong()
      } else if (b == 251) {
         return Long.MIN_VALUE
      } else if (b == 252) {
         return readCharLE().code.toLong()
      } else if (b == 253) {
         return readLong24LE()
      } else if (b == 254) {
         return readLongLE()
      }
      throw IOException("Unexpected packed number byte $b")
   }

   fun readPackedInteger(): Int {
      val num = readPackedNumber()
      if (num == Long.MIN_VALUE) {
         throw IOException("unexpected NULL where int should have been")
      }
      if (num > Int.MAX_VALUE) {
         throw IOException("stumbled upon long even though int expected")
      }
      return num.toInt()
   }

   fun readLengthEncodedString(): String {
      return readString(readPackedInteger())
   }

   fun appendFromLengthEncoded(from: MySQLBuffer): Int {
      return appendFrom(from, from.readPackedInteger())
   }

   fun appendFrom(from: MySQLBuffer, length: Int): Int {
      val fromPosition: Int = from.position
      from.skip(length)
      val offset = this.size
      writeBytes(from.base as ByteArray, fromPosition, length)
      return offset
   }

   companion object {
      fun allocate(size: Int): MySQLBuffer {
         return MySQLBuffer(
            ByteArray(size), Danger.BYTE_BASE.toLong(), 0, size, false
         )
      }

      fun wrap(b: ByteArray): MySQLBuffer {
         return MySQLBuffer(
            b, Danger.BYTE_BASE.toLong(), b.size, b.size, false
         )
      }
   }
}