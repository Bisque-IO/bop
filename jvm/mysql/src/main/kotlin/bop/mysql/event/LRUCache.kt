package bop.mysql.event

class LRUCache<K, V> (initialCapacity: Int, loadFactor: Float, val maxSize: Int) :
   LinkedHashMap<K, V>(initialCapacity, loadFactor, true) {
   override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>): Boolean {
      return size > maxSize
   }
}
