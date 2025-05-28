package bop.mysql.io

import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream
import kotlin.math.min

class BufferedSocketInputStream @JvmOverloads constructor(
   `in`: InputStream?,
   bufferSize: Int = 512 * 1024
) : FilterInputStream(`in`) {
   private val buffer: ByteArray
   private var offset = 0
   private var limit = 0

   init {
      this.buffer = ByteArray(bufferSize)
   }

   @Throws(IOException::class)
   override fun available(): Int {
      return limit - offset + `in`.available()
   }

   @Throws(IOException::class)
   override fun read(): Int {
      if (offset < limit) {
         return buffer[offset++].toInt() and 0xff
      }
      offset = 0
      limit = `in`.read(buffer, 0, buffer.size)
      return if (limit != -1) buffer[offset++].toInt() and 0xff else -1
   }

   @Throws(IOException::class)
   override fun read(
      b: ByteArray,
      off: Int,
      len: Int
   ): Int {
      if (offset >= limit) {
         if (len >= buffer.size) {
            return `in`.read(b, off, len)
         }
         offset = 0
         limit = `in`.read(buffer, 0, buffer.size)

         if (limit == -1) {
            return -1
         }
      }
      val bytesRemainingInBuffer = min(len, limit - offset)
      System.arraycopy(buffer, offset, b, off, bytesRemainingInBuffer)
      offset += bytesRemainingInBuffer
      return bytesRemainingInBuffer
   }
}
