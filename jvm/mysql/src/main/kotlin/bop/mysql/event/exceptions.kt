package bop.mysql.binlog.event.deserialization

import bop.mysql.event.EventHeader
import java.io.IOException

class EventDataDeserializationException(
   eventHeader: EventHeader?,
   cause: Throwable?
) : IOException("Failed to deserialize data of $eventHeader", cause)

class MissingTableMapEventException(message: String?) : IOException(message)