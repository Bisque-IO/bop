package bop.mysql.binlog

import bop.mysql.binlog.MariadbGtidSet.Companion.isMariaGtidSet
import bop.mysql.event.MySqlGtid
import java.util.*
import kotlin.math.max
import kotlin.math.min

/**
 * GTID set as described in [GTID
 * Concepts](https://dev.mysql.com/doc/refman/5.6/en/replication-gtids-concepts.html) of MySQL 5.6 Reference Manual.
 *
 * <pre>
 * gtid_set: uuid_set[,uuid_set]...
 * uuid_set: uuid:interval[:interval]...
 * uuid: hhhhhhhh-hhhh-hhhh-hhhh-hhhhhhhhhhhh, h: [0-9|A-F]
 * interval: n[-n], (n &gt;= 1)
 * </pre>
 */
open class GtidSet(gtidSet: String) {
   private val map = LinkedHashMap<UUID, UUIDSet>()

   /**
    * @param gtidSet gtid set comprised of closed intervals (like MySQL's executed_gtid_set).
    */
   init {
      val uuidSets: Array<String> =
         if (gtidSet.isEmpty()) emptyArray() else gtidSet.replace("\n", "").split(",".toRegex())
            .dropLastWhile { it.isEmpty() }.toTypedArray()
      for (uuidSet in uuidSets) {
         val uuidSeparatorIndex = uuidSet.indexOf(":")
         val sourceId = UUID.fromString(uuidSet.substring(0, uuidSeparatorIndex))
         val intervals: MutableList<Interval> = ArrayList<Interval>()
         val rawIntervals =
            uuidSet.substring(uuidSeparatorIndex + 1).split(":".toRegex()).dropLastWhile { it.isEmpty() }.toTypedArray()
         for (interval in rawIntervals) {
            val `is` = interval.split("-".toRegex()).dropLastWhile { it.isEmpty() }.toTypedArray()
            var split = LongArray(`is`.size)
            var i = 0
            val e = `is`.size
            while (i < e) {
               split[i] = `is`[i].toLong()
               i++
            }
            if (split.size == 1) {
               split = longArrayOf(split[0], split[0])
            }
            intervals.add(Interval(split[0], split[1]))
         }
         map.put(sourceId, UUIDSet(sourceId, intervals))
      }
   }

   /**
    * Get an immutable collection of the [range of GTIDs for a single server][UUIDSet].
    * @return the [GTID ranges for each server][UUIDSet]; never null
    */
   open val uuidSets: MutableCollection<UUIDSet>
      get() = Collections.unmodifiableCollection<UUIDSet>(map.values)

   /**
    * Find the [UUIDSet] for the server with the specified UUID.
    * @param uuid the UUID of the server
    * @return the [UUIDSet] for the identified server, or `null` if there are no GTIDs from that server.
    */
   open fun getUUIDSet(uuid: String): UUIDSet? {
      return map.get(UUID.fromString(uuid))
   }

   /**
    * Add or replace the UUIDSet
    * @param uuidSet UUIDSet to be added
    * @return the old [UUIDSet] for the server given in uuidSet param,
    * or `null` if there are no UUIDSet for the given server.
    */
   open fun putUUIDSet(uuidSet: UUIDSet): UUIDSet? {
      return map.put(uuidSet.serverId, uuidSet)
   }

   /**
    * @param gtid GTID ("source_id:transaction_id")
    * @return whether or not gtid was added to the set (false if it was already there)
    */
   open fun add(gtid: String): Boolean {
      return add(MySqlGtid.fromString(gtid))
   }

   open fun addGtid(gtid: Any?) {
      if (gtid is MySqlGtid) {
         add(gtid)
      } else if (gtid is String) {
         add(gtid)
      } else {
         throw IllegalArgumentException(gtid.toString() + " not supported")
      }
   }

   private fun add(mySqlGtid: MySqlGtid): Boolean {
      var uuidSet = map.get(mySqlGtid.serverId)
      if (uuidSet == null) {
         uuidSet = UUIDSet(mySqlGtid.serverId, ArrayList<Interval>())
         map.put(
            mySqlGtid.serverId, uuidSet
         )
      }
      return uuidSet.add(mySqlGtid.transactionId)
   }

   /**
    * Determine if the GTIDs represented by this object are contained completely within the supplied set of GTIDs.
    * Note that if two [GtidSet]s are equal, then they both are subsets of the other.
    * @param other the other set of GTIDs; may be null
    * @return `true` if all of the GTIDs in this set are equal to or completely contained within the supplied
    * set of GTIDs, or `false` otherwise
    */
   open fun isContainedWithin(other: GtidSet?): Boolean {
      if (other == null) {
         return false
      }
      if (this === other) {
         return true
      }
      if (this == other) {
         return true
      }
      for (uuidSet in map.values) {
         val thatSet = other.map.get(uuidSet.serverId)
         if (!uuidSet.isContainedWithin(thatSet)) {
            return false
         }
      }
      return true
   }

   override fun hashCode(): Int {
      return map.keys.hashCode()
   }

   override fun equals(other: Any?): Boolean {
      if (other === this) {
         return true
      }
      if (other is GtidSet) {
         val that = other
         return this.map == that.map
      }
      return false
   }

   override fun toString(): String {
      val gtids: MutableList<String> = ArrayList<String>()
      for (uuidSet in map.values) {
         gtids.add(uuidSet.serverId.toString() + ":" + join(uuidSet.intervals, ":"))
      }
      return join(gtids, ",")!!
   }

   open fun toSeenString(): String? {
      return this.toString()
   }

   /**
    * A range of GTIDs for a single server with a specific UUID.
    * @see GtidSet
    */
   class UUIDSet(val serverId: UUID, val intervals: MutableList<Interval>) {
      constructor(uuid: String, intervals: MutableList<Interval>) : this(UUID.fromString(uuid), intervals)

      init {
         if (intervals.size > 1) {
            joinAdjacentIntervals(0)
         }
      }

      fun add(transactionId: Long): Boolean {
         val index = findInterval(transactionId)
         var addedToExisting = false
         if (index < intervals.size) {
            val interval = intervals.get(index)
            if (interval.start == transactionId + 1) {
               interval.start = transactionId
               addedToExisting = true
            } else if (interval.end + 1 == transactionId) {
               interval.end = transactionId
               addedToExisting = true
            } else if (interval.start <= transactionId && transactionId <= interval.end) {
               return false
            }
         }
         if (!addedToExisting) {
            intervals.add(index, Interval(transactionId, transactionId))
         }
         if (intervals.size > 1) {
            joinAdjacentIntervals(index)
         }
         return true
      }

      /**
       * Collapses intervals like a-(b-1):b-c into a-c (only in index+-1 range).
       */
      fun joinAdjacentIntervals(index: Int) {
         var i = min(index + 1, intervals.size - 1)
         val e = max(index - 1, 0)
         while (i > e) {
            val a = intervals.get(i - 1)
            val b = intervals.get(i)
            if (a.end + 1 == b.start) {
               a.end = b.end
               intervals.removeAt(i)
            }
            i--
         }
      }

      /**
       * @return index which is either a pointer to the interval containing v or a position at which v can be added
       */
      fun findInterval(v: Long): Int {
         var l = 0
         var p = 0
         var r = intervals.size
         while (l < r) {
            p = (l + r) / 2
            val i = intervals.get(p)
            if (i.end < v) {
               l = p + 1
            } else if (v < i.start) {
               r = p
            } else {
               return p
            }
         }
         if (!intervals.isEmpty() && intervals.get(p).end < v) {
            p++
         }
         return p
      }

      @get:Deprecated("")
      val uUID: String?
         /**
          * Get the UUID for the server that generated the GTIDs.
          * @return the server's UUID; never null
          */
         get() = serverId.toString()


      /**
       * Get the intervals of transaction numbers.
       * @return the immutable transaction intervals; never null
       */
//      fun getIntervals(): MutableList<Interval?> {
//         return Collections.unmodifiableList<Interval?>(intervals)
//      }

      /**
       * Determine if the set of transaction numbers from this server is completely within the set of transaction
       * numbers from the set of transaction numbers in the supplied set.
       * @param other the set to compare with this set
       * @return `true` if this server's transaction numbers are equal to or a subset of the transaction
       * numbers of the supplied set, or false otherwise
       */
      fun isContainedWithin(other: UUIDSet?): Boolean {
         if (other == null) {
            return false
         }
         if (this.serverId != other.serverId) {
            // not even the same server ...
            return false
         }
         if (this.intervals.isEmpty()) {
            return true
         }
         if (other.intervals.isEmpty()) {
            return false
         }
         // every interval in this must be within an interval of the other ...
         for (thisInterval in this.intervals) {
            var found = false
            for (otherInterval in other.intervals) {
               if (thisInterval.isContainedWithin(otherInterval)) {
                  found = true
                  break
               }
            }
            if (!found) {
               return false // didn't find a match
            }
         }
         return true
      }

      override fun hashCode(): Int {
         return serverId.hashCode()
      }

      override fun equals(obj: Any?): Boolean {
         if (obj === this) {
            return true
         }
         if (obj is UUIDSet) {
            val that = obj
            return this.serverId == that.serverId && this.intervals == that.intervals
         }
         return super.equals(obj)
      }

      override fun toString(): String {
         val sb = StringBuilder()
         if (sb.length != 0) {
            sb.append(',')
         }
         sb.append(this.serverId).append(':')
         val iter = intervals.iterator()
         if (iter.hasNext()) {
            sb.append(iter.next())
         }
         while (iter.hasNext()) {
            sb.append(':')
            sb.append(iter.next())
         }
         return sb.toString()
      }
   }

   /**
    * An interval of contiguous transaction identifiers.
    * @see GtidSet
    */
   class Interval(
      /**
       * Get the starting transaction number in this interval.
       * @return this interval's first transaction number
       */
      @JvmField
      var start: Long,
      /**
       * Get the ending transaction number in this interval.
       * @return this interval's last transaction number
       */
      @JvmField
      var end: Long
   ) : Comparable<Interval?> {
      /**
       * Determine if this interval is completely within the supplied interval.
       * @param other the interval to compare with
       * @return `true` if the [start][.getStart] is greater than or equal to the supplied interval's
       * [start][.getStart] and the [end][.getEnd] is less than or equal to the supplied
       * interval's [end][.getEnd], or `false` otherwise
       */
      fun isContainedWithin(other: Interval?): Boolean {
         if (other === this) {
            return true
         }
         if (other == null) {
            return false
         }
         return this.start >= other.start && this.end <= other.end
      }

      override fun hashCode(): Int {
         return this.start.toInt()
      }

      override fun equals(obj: Any?): Boolean {
         if (this === obj) {
            return true
         }
         if (obj is Interval) {
            val that = obj
            return this.start == that.start && this.end == that.end
         }
         return false
      }

      override fun toString(): String {
         return start.toString() + "-" + end
      }

      override fun compareTo(other: Interval?): Int {
         return saturatedCast(this.start - (other?.start ?: 0L))
      }

      fun saturatedCast(value: Long): Int {
         if (value > Int.Companion.MAX_VALUE) {
            return Int.Companion.MAX_VALUE
         }
         if (value < Int.Companion.MIN_VALUE) {
            return Int.Companion.MIN_VALUE
         }
         return value.toInt()
      }
   }

   companion object {
      val NULL = GtidSet("")

      fun parse(gtidStr: String): GtidSet {
         if (isMariaGtidSet(gtidStr)) {
            return MariadbGtidSet(gtidStr)
         } else {
            return GtidSet(gtidStr)
         }
      }

      private fun join(o: MutableCollection<*>, delimiter: String): String? {
         if (o.isEmpty()) {
            return ""
         }
         val sb = StringBuilder()
         for (o1 in o) {
            sb.append(o1).append(delimiter)
         }
         return sb.substring(0, sb.length - delimiter.length)
      }
   }
}
