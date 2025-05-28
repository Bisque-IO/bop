package bop.mysql.io

import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.OutputStream

class ByteArrayOutputStream(
   private val outputStream: OutputStream = ByteArrayOutputStream()
) : OutputStream() {
   /**
    * Write int in little-endian format.
    * @throws IOException on underlying stream error
    * @param value integer to write
    * @param length length in bytes of the integer
    */
   @Throws(IOException::class)
   fun writeInteger(
      value: Int,
      length: Int
   ) {
      for (i in 0..<length) {
         write(0x000000FF and (value ushr (i shl 3)))
      }
   }

   /**
    * Write long in little-endian format.
    * @throws IOException on underlying stream error
    * @param value long to write
    * @param length length in bytes of the long
    */
   @Throws(IOException::class)
   fun writeLong(
      value: Long,
      length: Int
   ) {
      for (i in 0..<length) {
         write((0x00000000000000FFL and (value ushr (i shl 3))).toInt())
      }
   }

   @Throws(IOException::class)
   fun writeString(value: String) {
      write(value.toByteArray())
   }

   /**
    * @see ByteArrayInputStream.readZeroTerminatedString
    * @param value string to write
    * @throws IOException on underlying stream error
    */
   @Throws(IOException::class)
   fun writeZeroTerminatedString(value: String?) {
      if (value != null) write(value.toByteArray())

      write(0)
   }

   @Throws(IOException::class)
   override fun write(b: Int) {
      outputStream.write(b)
   }

   @Throws(IOException::class)
   override fun write(bytes: ByteArray) {
      outputStream.write(bytes)
   }

   fun toByteArray(): ByteArray {
      // todo: whole approach feels wrong
      if (outputStream is ByteArrayOutputStream) {
         return outputStream.toByteArray()
      }
      return ByteArray(0)
   }

   @Throws(IOException::class)
   override fun flush() {
      outputStream.flush()
   }

   @Throws(IOException::class)
   override fun close() {
      outputStream.close()
   }
}

