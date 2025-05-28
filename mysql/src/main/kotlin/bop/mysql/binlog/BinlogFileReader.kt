package bop.mysql.binlog

import bop.mysql.event.Event
import bop.mysql.binlog.event.deserialization.EventDeserializer
import bop.mysql.io.ByteArrayInputStream
import java.io.Closeable
import java.io.File
import java.io.IOException
import java.io.InputStream

/**
 * MySQL binary log file reader.
 */
class BinlogFileReader @JvmOverloads constructor(
   inputStream: InputStream, eventDeserializer: EventDeserializer = EventDeserializer()
) : Closeable {
   private val inputStream: ByteArrayInputStream
   private val eventDeserializer: EventDeserializer

   @JvmOverloads
   constructor(
      file: File?, eventDeserializer: EventDeserializer = EventDeserializer()
   ) : this(
      (if (file != null) java.io.BufferedInputStream(java.io.FileInputStream(file)) else null)!!, eventDeserializer
   )

   init {
      requireNotNull(inputStream) { "Input stream cannot be NULL" }
      requireNotNull(eventDeserializer) { "Event deserializer cannot be NULL" }
      this.inputStream = ByteArrayInputStream(inputStream)
      try {
         val magicHeader: ByteArray? = this.inputStream.read(MAGIC_HEADER.size)
         if (!magicHeader.contentEquals(MAGIC_HEADER)) {
            throw IOException("Not a valid binary log")
         }
      } catch (e: IOException) {
         try {
            this.inputStream.close()
         } catch (ex: IOException) {
            // ignore
         }
         throw e
      }
      this.eventDeserializer = eventDeserializer
   }

   /**
    * @return deserialized event or null in case of end-of-stream
    * @throws IOException if reading the event fails
    */
   @Throws(IOException::class)
   fun readEvent(): Event? {
      return eventDeserializer.nextEvent(inputStream)
   }

   @Throws(IOException::class)
   override fun close() {
      inputStream.close()
   }

   companion object {
      val MAGIC_HEADER: ByteArray = byteArrayOf(0xfe.toByte(), 0x62.toByte(), 0x69.toByte(), 0x6e.toByte())
   }
}
