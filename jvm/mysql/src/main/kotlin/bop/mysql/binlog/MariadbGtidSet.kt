package bop.mysql.binlog

import java.util.regex.Pattern


/**
 * Mariadb Global Transaction ID
 *
 * @see [GTID](https://mariadb.com/kb/en/gtid/) for the original doc
 */
class MariadbGtidSet : GtidSet {
   /*
        we keep two maps; one of them contains the current GTID position for
        each domain.  The other contains all the "seen" GTID positions for each
        domain and can be used to compare against another gtid postion.
     */
   var positionMap = mutableMapOf<Long, MariaGtid>()
   var seenMap = LinkedHashMap<Long, LinkedHashMap<Long, MariaGtid>>()

   constructor() : super("") //


   /**
    * Initialize a new MariaDB gtid set from a string, like:
    * 0-1-24,0-555555-9709
    * DOMAIN_ID-SERVER_ID-SEQUENCE[,DOMAIN_ID-SERVER_ID-SEQUENCE]
    *
    * note that for duplicate domain ids it's "last one wins" for the current position
    * @param gtidSet a string representing the gtid set.
    */
   constructor(gtidSet: String?) : super("") {
      if (gtidSet != null && gtidSet.length > 0) {
         val gtids =
            gtidSet.replace("\n".toRegex(), "").split(",".toRegex()).dropLastWhile { it.isEmpty() }.toTypedArray()
         for (gtid in gtids) {
            val mariaGtid = MariaGtid.Companion.parse(gtid)

            positionMap.put(mariaGtid.domainId, mariaGtid)
            addToSeenSet(mariaGtid)
         }
      }
   }

   private fun addToSeenSet(gtid: MariaGtid) {
      if (!this.seenMap.containsKey(gtid.domainId)) {
         this.seenMap.put(gtid.domainId, LinkedHashMap())
      }

      val domainMap = this.seenMap.get(gtid.domainId)
      domainMap?.put(gtid.serverId, gtid)
   }


   override fun toString(): String {
      val sb = StringBuilder()
      for (gtid in positionMap.values) {
         if (sb.length > 0) {
            sb.append(",")
         }
         sb.append(gtid.toString())
      }
      return sb.toString()
   }

   override fun toSeenString(): String {
      val sb = StringBuilder()
      for (domainID in seenMap.keys) {
         for (gtid in seenMap.get(domainID)!!.values) {
            if (sb.length > 0) {
               sb.append(",")
            }

            sb.append(gtid.toString())
         }
      }
      return sb.toString()
   }

   override val uuidSets: MutableCollection<UUIDSet>
      get() = throw UnsupportedOperationException("Mariadb gtid not support this method")

   override fun getUUIDSet(uuid: String): UUIDSet? {
      throw UnsupportedOperationException("Mariadb gtid not support this method")
   }

   override fun putUUIDSet(uuidSet: UUIDSet): UUIDSet? {
      throw UnsupportedOperationException("Mariadb gtid not support this method")
   }

   override fun add(gtid: String): Boolean {
      val mariaGtid = MariaGtid.Companion.parse(gtid)
      add(mariaGtid)
      return true
   }

   override fun addGtid(gtid: Any?) {
      when (gtid) {
         is MariaGtid -> {
            add(gtid)
         }

         is String -> {
            add(gtid)
         }

         else -> {
            throw IllegalArgumentException(gtid.toString() + " not supported")
         }
      }
   }

   fun add(gtid: MariaGtid) {
      positionMap.put(gtid.domainId, gtid)
      addToSeenSet(gtid)
   }

   /*
        we're trying to ask "is this position behind the other position?"
        - if we have a domain that the other doesn't, we're probably "ahead".
        - the inverse is true too
     */
   override fun isContainedWithin(other: GtidSet?): Boolean {
      if (other !is MariadbGtidSet) return false

      val o = other

      for (domainID in this.seenMap.keys) {
         if (!o.seenMap.containsKey(domainID)) {
            return false
         }

         val thisDomainMap = this.seenMap.get(domainID)!!
         val otherDomainMap = o.seenMap.get(domainID)!!

         for (serverID in thisDomainMap.keys) {
            if (!otherDomainMap.containsKey(serverID)) {
               return false
            }

            val thisGtid: MariaGtid = thisDomainMap.get(serverID)!!
            val otherGtid: MariaGtid = otherDomainMap.get(serverID)!!
            if (thisGtid.sequence > otherGtid.sequence) return false
         }
      }
      return true
   }

   override fun hashCode(): Int {
      return this.seenMap.keys.hashCode()
   }

   override fun equals(obj: Any?): Boolean {
      if (obj === this) {
         return true
      }
      if (obj is MariadbGtidSet) {
         val that = obj
         return this.positionMap == that.positionMap
      }
      return false
   }

   class MariaGtid {
      // {domainId}-{serverId}-{sequence}
      var domainId: Long
      var serverId: Long
      var sequence: Long

      constructor(
         domainId: Long,
         serverId: Long,
         sequence: Long
      ) {
         this.domainId = domainId
         this.serverId = serverId
         this.sequence = sequence
      }

      constructor(gtid: String) {
         val gtidArr = gtid.split("-".toRegex()).dropLastWhile { it.isEmpty() }.toTypedArray()
         this.domainId = gtidArr[0].toLong()
         this.serverId = gtidArr[1].toLong()
         this.sequence = gtidArr[2].toLong()
      }

      override fun equals(o: Any?): Boolean {
         if (this === o) {
            return true
         }
         if (o == null || javaClass != o.javaClass) {
            return false
         }
         val mariaGtid = o as MariaGtid
         return domainId == mariaGtid.domainId && serverId == mariaGtid.serverId && sequence == mariaGtid.sequence
      }

      override fun toString(): String {
         return String.format("%s-%s-%s", domainId, serverId, sequence)
      }

      companion object {
         fun parse(gtid: String): MariaGtid {
            return MariaGtid(gtid)
         }
      }
   }

   companion object {
      var threeDashes: String = "\\d{1,10}-\\d{1,10}-\\d{1,20}"

      var MARIA_GTID_PATTERN: Pattern = Pattern.compile(
         "^$threeDashes(\\s*,\\s*$threeDashes)*$"
      )

      @JvmStatic
      fun isMariaGtidSet(gtidSet: String): Boolean {
         return MARIA_GTID_PATTERN.matcher(gtidSet).find()
      }
   }
}

