/*
 * Copyright 2013 Stanley Shyiko
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package bop.mysql.io

import java.io.ByteArrayInputStream
import java.io.EOFException
import java.io.IOException
import java.io.InputStream
import java.util.*
import kotlin.math.min

class ByteArrayInputStream(private val inputStream: InputStream) : InputStream() {
   private var peek = -1
   var position: Int = 0
      private set
   private var markPosition = 0
   private var blockLength = -1
   private var initialBlockLength = -1

   constructor(bytes: ByteArray) : this(ByteArrayInputStream(bytes))

   /**
    * Read int written in little-endian format.
    * @param length length of the integer to read
    * @throws IOException in case of EOF
    * @return the integer from the binlog
    */
   @Throws(IOException::class)
   fun readInteger(length: Int): Int {
      var result = 0
      for (i in 0..<length) {
         result = result or (this.read() shl (i shl 3))
      }
      return result
   }

   /**
    * Read long written in little-endian format.
    * @param length length of the long to read
    * @throws IOException in case of EOF
    * @return the long from the binlog
    */
   @Throws(IOException::class)
   fun readLong(length: Int): Long {
      var result: Long = 0
      for (i in 0..<length) {
         result = result or ((this.read().toLong()) shl (i shl 3))
      }
      return result
   }

   /**
    * Read fixed length string.
    * @param length length of string to read
    * @throws IOException in case of EOF
    * @return string
    */
   @Throws(IOException::class)
   fun readString(length: Int): String {
      return String(read(length))
   }

   /**
    * Read variable-length string. Preceding packed integer indicates the length of the string.
    * @throws IOException in case of EOF
    * @return string
    */
   @Throws(IOException::class)
   fun readLengthEncodedString(): String {
      return readString(readPackedInteger())
   }

   /**
    * Read variable-length string. End is indicated by 0x00 byte.
    * @throws IOException in case of EOF
    * @return string
    */
   @Throws(IOException::class)
   fun readZeroTerminatedString(): String {
      val s = ByteArrayOutputStream()
      var b: Int
      while ((this.read().also { b = it }) != 0) {
         s.writeInteger(b, 1)
      }
      return String(s.toByteArray())
   }

   @Throws(IOException::class)
   fun read(length: Int): ByteArray {
      val bytes = ByteArray(length)
      fill(bytes, 0, length)
      return bytes
   }

   @Throws(IOException::class)
   fun fill(
      bytes: ByteArray,
      offset: Int,
      length: Int
   ) {
      var remaining = length
      while (remaining != 0) {
         val read = read(bytes, offset + length - remaining, remaining)
         if (read == -1) {
            throw EOFException(
               String.format(
                  "Failed to read remaining %d of %d bytes from position %d. Block length: %d. Initial block length: %d.",
                  remaining, length, this.position, blockLength, initialBlockLength
               )
            )
         }
         remaining -= read
      }
   }

   @Throws(IOException::class)
   fun readBitSet(
      length: Int,
      bigEndian: Boolean
   ): BitSet {
      // according to MySQL internals the amount of storage required for N columns is INT((N+7)/8) bytes
      var bytes = read((length + 7) shr 3)
      bytes = if (bigEndian) bytes else reverse(bytes)
      val result = BitSet()
      for (i in 0..<length) {
         if ((bytes[i shr 3].toInt() and (1 shl (i % 8))) != 0) {
            result.set(i)
         }
      }
      return result
   }

   private fun reverse(bytes: ByteArray): ByteArray {
      var i = 0
      val length = bytes.size shr 1
      while (i < length) {
         val j = bytes.size - 1 - i
         val t = bytes[i]
         bytes[i] = bytes[j]
         bytes[j] = t
         i++
      }
      return bytes
   }

   /**
    * @see .readPackedNumber
    * @throws IOException in case of malformed number, eof, null, or long
    * @return integer
    */
   @Throws(IOException::class)
   fun readPackedInteger(): Int {
      val number = readPackedNumber()
      if (number == null) {
         throw IOException("Unexpected NULL where int should have been")
      }
      if (number.toLong() > Int.Companion.MAX_VALUE) {
         throw IOException("Stumbled upon long even though int expected")
      }
      return number.toInt()
   }

   /**
    * @see .readPackedNumber
    * @throws IOException in case of malformed number, eof, null
    * @return long
    */
   @Throws(IOException::class)
   fun readPackedLong(): Long {
      val number = readPackedNumber()
      if (number == null) {
         throw IOException("Unexpected NULL where long should have been")
      }
      return number.toLong()
   }

   /**
    * Format (first-byte-based):<br></br>
    * 0-250 - The first byte is the number (in the range 0-250). No additional bytes are used.<br></br>
    * 251 - SQL NULL value<br></br>
    * 252 - Two more bytes are used. The number is in the range 251-0xffff.<br></br>
    * 253 - Three more bytes are used. The number is in the range 0xffff-0xffffff.<br></br>
    * 254 - Eight more bytes are used. The number is in the range 0xffffff-0xffffffffffffffff.
    * @throws IOException in case of malformed number or EOF
    * @return long or null
    */
   @Throws(IOException::class)
   fun readPackedNumber(): Number? {
      val b = this.read()
      if (b < 251) {
         return b
      } else if (b == 251) {
         return null
      } else if (b == 252) {
         return readInteger(2).toLong()
      } else if (b == 253) {
         return readInteger(3).toLong()
      } else if (b == 254) {
         return readLong(8)
      }
      throw IOException("Unexpected packed number byte $b")
   }

   @Throws(IOException::class)
   override fun available(): Int {
      if (blockLength != -1) {
         return blockLength
      }
      return inputStream.available()
   }

   @Throws(IOException::class)
   fun peek(): Int {
      if (peek == -1) {
         peek = readWithinBlockBoundaries()
      }
      return peek
   }

   @Throws(IOException::class)
   override fun read(): Int {
      val result: Int
      if (peek == -1) {
         result = readWithinBlockBoundaries()
      } else {
         result = peek
         peek = -1
      }
      if (result == -1) {
         throw EOFException(String.format("Failed to read next byte from position %d", this.position))
      }
      this.position += 1
      return result
   }

   @Throws(IOException::class)
   private fun readWithinBlockBoundaries(): Int {
      if (blockLength != -1) {
         if (blockLength == 0) {
            return -1
         }
         blockLength--
      }
      return inputStream.read()
   }

   @Throws(IOException::class)
   override fun read(
      b: ByteArray,
      off: Int,
      len: Int
   ): Int {
      var off = off
      var len = len
      if (b == null) {
         throw NullPointerException()
      } else if (off < 0 || len < 0 || len > b.size - off) {
         throw IndexOutOfBoundsException()
      } else if (len == 0) {
         return 0
      }

      if (peek != -1) {
         b[off] = peek.toByte()
         off += 1
         len -= 1
      }

      var read = readWithinBlockBoundaries(b, off, len)

      if (read > 0) {
         this.position += read
      }

      if (peek != -1) {
         peek = -1
         read = if (read <= 0) 1 else read + 1
      }

      return read
   }

   @Throws(IOException::class)
   private fun readWithinBlockBoundaries(
      b: ByteArray,
      off: Int,
      len: Int
   ): Int {
      if (blockLength == -1) {
         return inputStream.read(b, off, len)
      } else if (blockLength == 0) {
         return -1
      }

      val read = inputStream.read(b, off, min(len, blockLength))
      if (read > 0) {
         blockLength -= read
      }
      return read
   }

   @Throws(IOException::class)
   override fun close() {
      inputStream.close()
   }

   fun enterBlock(length: Int) {
      this.blockLength = if (length < -1) -1 else length
      this.initialBlockLength = length
   }

   @Throws(IOException::class)
   fun skipToTheEndOfTheBlock() {
      if (blockLength != -1) {
         skip(blockLength.toLong())
         blockLength = -1
      }
   }

   @Synchronized
   override fun mark(readlimit: Int) {
      markPosition = this.position
      inputStream.mark(readlimit)
   }

   override fun markSupported(): Boolean {
      return inputStream.markSupported()
   }

   @Synchronized
   @Throws(IOException::class)
   override fun reset() {
      this.position = markPosition
      inputStream.reset()
   }

   /**
    * This method implements fast-forward skipping in the stream.
    * It can be used if and only if the underlying stream is fully available till its end.
    * In other cases the regular [.skip] method must be used.
    *
    * @param n - number of bytes to skip
    * @return number of bytes skipped
    * @throws IOException
    */
   @Synchronized
   @Throws(IOException::class)
   fun fastSkip(n: Long): Long {
      var skipOf = n
      if (blockLength != -1) {
         skipOf = min(blockLength.toLong(), skipOf)
         blockLength -= skipOf.toInt()
         if (blockLength == 0) {
            blockLength = -1
         }
      }
      this.position += skipOf.toInt()
      return inputStream.skip(skipOf)
   }
}
