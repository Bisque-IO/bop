package bop.constant;

import bop.hash.Hash;
import bop.unsafe.UnsafeAccess;
import java.lang.foreign.MemorySegment;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;

public class Interned {
  static final Map CONSTANTS = new Map(2048);
  //  static final Cache<Long, Value> FLEX =
  //      Caffeine.newBuilder().maximumSize(100000).build();
  static final ConcurrentHashMap<Long, Value> FLEX = new ConcurrentHashMap<>();

  static Value internConst(String value) {
    if (value == null || value.isEmpty()) {
      return null;
    }
    final var result = Value.of(value);
    CONSTANTS.put(result);
    return result;
  }

  public static Value getConst(final long hash) {
    return CONSTANTS.get(hash);
  }

  public static Value get(final long hash) {
    var result = CONSTANTS.get(hash);
    if (result != null) {
      return result;
    }
    return FLEX.get(hash);
  }

  public static Value get(String value) {
    return get(Hash.xx3(value), value);
  }

  public static Value get(final long hash, final String value) {
    var result = CONSTANTS.get(hash);
    if (result != null) {
      return result;
    }
    result = FLEX.get(hash);
    if (result != null) {
      return result;
    }
    result = Value.of(value);
    FLEX.put(hash, result);
    return result;
  }

  public static int ceilPowerOfTwo(int x) {
    x = x - 1;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    return x + 1;
  }

  public static final class Value {
    private final int hash;
    private final long xx3;
    private final String value;
    private final byte[] bytes;
    private final MemorySegment segment;
    public Value next;

    public Value(long xx3, String value, byte[] bytes, MemorySegment segment) {
      this.hash = Map.hash(xx3);
      this.xx3 = xx3;
      this.value = value;
      this.bytes = bytes;
      this.segment = segment;
    }

    public static Value of(String value) {
      final var xx3 = Hash.xx3(value);
      final var bytes = value.getBytes(StandardCharsets.UTF_8);
      final var address = UnsafeAccess.UNSAFE.allocateMemory(bytes.length);
      UnsafeAccess.UNSAFE.copyMemory(bytes, UnsafeAccess.BYTE_BASE, null, address, bytes.length);
      final var directSegment = MemorySegment.ofAddress(address).reinterpret(bytes.length);
      return new Value(xx3, value, bytes, directSegment);
    }

    public int hash() {
      return hash;
    }

    public long key() {
      return xx3;
    }

    public long xx3() {
      return xx3;
    }

    public String value() {
      return value;
    }

    public byte[] bytes() {
      return bytes;
    }

    public long address() {
      return segment.address();
    }

    public int length() {
      return bytes.length;
    }

    public MemorySegment direct() {
      return segment;
    }

    @Override
    public boolean equals(Object obj) {
      if (obj == this) return true;
      if (obj == null || obj.getClass() != this.getClass()) return false;
      var that = (Value) obj;
      return this.xx3 == that.xx3
          && Objects.equals(this.value, that.value)
          && Objects.equals(this.bytes, that.bytes)
          && Objects.equals(this.segment, that.segment);
    }

    @Override
    public int hashCode() {
      return Objects.hash(xx3, value, bytes, segment);
    }

    @Override
    public String toString() {
      return "Value[" + "key="
          + xx3 + ", " + "value="
          + value + ", " + "bytes="
          + bytes + ", " + "directSegment="
          + segment + ']';
    }
  }

  /** Map is a stripped down version of java.util.HashMap */
  public static class Map {
    /** The default initial capacity - MUST be a power of two. */
    static final int DEFAULT_INITIAL_CAPACITY = 1 << 4; // aka 16

    /**
     * The maximum capacity, used if a higher value is implicitly specified by either of the
     * constructors with arguments. MUST be a power of two <= 1<<30.
     */
    static final int MAXIMUM_CAPACITY = 1 << 30;

    /** The load factor used when none specified in constructor. */
    static final float DEFAULT_LOAD_FACTOR = 0.75f;

    /**
     * The load factor for the hash table.
     *
     * @serial
     */
    final float loadFactor;

    /* ---------------- Static utilities -------------- */
    /**
     * The table, initialized on first use, and resized as necessary. When allocated, length is
     * always a power of two. (We also tolerate length zero in some operations to allow
     * bootstrapping mechanics that are currently not needed.)
     */
    Value[] table;
    /** The number of key-value mappings contained in this map. */
    int size;
    /**
     * The number of times this HashMap has been structurally modified Structural modifications are
     * those that change the number of mappings in the HashMap or otherwise modify its internal
     * structure (e.g., rehash). This field is used to make iterators on Collection-views of the
     * HashMap fail-fast. (See ConcurrentModificationException).
     */
    int modCount;
    /**
     * The next size value at which to resize (capacity * load factor).
     *
     * @serial
     */
    // (The javadoc description is true upon serialization.
    // Additionally, if the table array has not been allocated, this
    // field holds the initial array capacity, or zero signifying
    // DEFAULT_INITIAL_CAPACITY.)
    int threshold;

    int maxCollision = 0;

    /* ---------------- Fields -------------- */

    /**
     * Constructs an empty {@code HashMap} with the specified initial capacity and load factor.
     *
     * @param initialCapacity the initial capacity
     * @param loadFactor the load factor
     * @throws IllegalArgumentException if the initial capacity is negative or the load factor is
     *     nonpositive
     */
    public Map(int initialCapacity, float loadFactor) {
      if (initialCapacity < 0)
        throw new IllegalArgumentException("Illegal initial capacity: " + initialCapacity);
      if (initialCapacity > MAXIMUM_CAPACITY) initialCapacity = MAXIMUM_CAPACITY;
      if (loadFactor <= 0 || Float.isNaN(loadFactor))
        throw new IllegalArgumentException("Illegal load factor: " + loadFactor);
      this.loadFactor = loadFactor;
      this.threshold = tableSizeFor(initialCapacity);
    }

    /**
     * Constructs an empty {@code HashMap} with the specified initial capacity and the default load
     * factor (0.75).
     *
     * @param initialCapacity the initial capacity.
     * @throws IllegalArgumentException if the initial capacity is negative.
     */
    public Map(int initialCapacity) {
      this(initialCapacity, DEFAULT_LOAD_FACTOR);
    }

    /**
     * Constructs an empty {@code HashMap} with the default initial capacity (16) and the default
     * load factor (0.75).
     */
    public Map() {
      this.loadFactor = DEFAULT_LOAD_FACTOR; // all other fields defaulted
    }

    /**
     * Computes key.hashCode() and spreads (XORs) higher bits of hash to lower. Because the table
     * uses power-of-two masking, sets of hashes that vary only in bits above the current mask will
     * always collide. (Among known examples are sets of Float keys holding consecutive whole
     * numbers in small tables.) So we apply a transform that spreads the impact of higher bits
     * downward. There is a tradeoff between speed, utility, and quality of bit-spreading. Because
     * many common sets of hashes are already reasonably distributed (so don't benefit from
     * spreading), and because we use trees to handle large sets of collisions in bins, we just XOR
     * some shifted bits in the cheapest possible way to reduce systematic lossage, as well as to
     * incorporate impact of the highest bits that would otherwise never be used in index
     * calculations because of table bounds.
     */
    static final int hash(long key) {
      int h = Long.hashCode(key);
      //    h ^= (h >>> 20) ^ (h >>> 12);
      //    h = h ^ (h >>> 7) ^ (h >>> 4);
      return h ^ (h >>> 16);
    }

    /* ---------------- Public operations -------------- */

    /** Returns a power of two size for the given target capacity. */
    static final int tableSizeFor(int cap) {
      int n = -1 >>> Integer.numberOfLeadingZeros(cap - 1);
      return (n < 0) ? 1 : (n >= MAXIMUM_CAPACITY) ? MAXIMUM_CAPACITY : n + 1;
    }

    /**
     * Returns the number of key-value mappings in this map.
     *
     * @return the number of key-value mappings in this map
     */
    public int size() {
      return size;
    }

    /**
     * Returns {@code true} if this map contains no key-value mappings.
     *
     * @return {@code true} if this map contains no key-value mappings
     */
    public boolean isEmpty() {
      return size == 0;
    }

    /**
     * Returns the value to which the specified key is mapped, or {@code null} if this map contains
     * no mapping for the key.
     *
     * <p>More formally, if this map contains a mapping from a key {@code k} to a value {@code v}
     * such that {@code (key==null ? k==null : key.equals(k))}, then this method returns {@code v};
     * otherwise it returns {@code null}. (There can be at most one such mapping.)
     *
     * <p>A return value of {@code null} does not <i>necessarily</i> indicate that the map contains
     * no mapping for the key; it's also possible that the map explicitly maps the key to
     * {@code null}. The {@link #containsKey containsKey} operation may be used to distinguish these
     * two cases.
     */
    public Value get(long key) {
      return getNode(key);
    }

    /**
     * Implements Map.get and related methods.
     *
     * @param key the key
     * @return the node, or null if none
     */
    final Value getNode(long key) {
      Value[] tab;
      Value first, e;
      int n, hash;
      if ((tab = table) != null
          && (n = tab.length) > 0
          && (first = tab[(n - 1) & (hash = hash(key))]) != null) {
        if (first.hash == hash
            && // always check first node
            first.xx3 == key) return first;
        if ((e = first.next) != null) {
          do {
            if (e.hash == hash && e.xx3 == key) return e;
          } while ((e = e.next) != null);
        }
      }
      return null;
    }

    /**
     * Returns {@code true} if this map contains a mapping for the specified key.
     *
     * @param key The key whose presence in this map is to be tested
     * @return {@code true} if this map contains a mapping for the specified key.
     */
    public boolean containsKey(long key) {
      return getNode(key) != null;
    }

    /**
     * Associates the specified value with the specified key in this map. If the map previously
     * contained a mapping for the key, the old value is replaced.
     *
     * @param value value to be associated with the specified key
     * @return the previous value associated with {@code key}, or {@code null} if there was no
     *     mapping for {@code key}. (A {@code null} return can also indicate that the map previously
     *     associated {@code null} with {@code key}.)
     */
    public Value put(Value value) {
      return putVal(hash(value.key()), value);
    }

    /**
     * Implements Map.put and related methods.
     *
     * @param hash hash for key
     * @param value the value to put
     * @return previous value, or null if none
     */
    final Value putVal(int hash, Value value) {
      Value[] tab;
      Value p;
      int n, i;
      var key = value.key();
      if ((tab = table) == null || (n = tab.length) == 0) n = (tab = resize()).length;
      if ((p = tab[i = (n - 1) & hash]) == null) tab[i] = value;
      else {
        Value e;
        if (p.hash == hash && p.xx3 == key) e = p;
        else {
          int binCount = 0;
          for (; ; ++binCount) {
            if ((e = p.next) == null) {
              p.next = value;
              break;
            }
            if (e.hash == hash && e.xx3 == key) break;
            p = e;
          }
          maxCollision = Math.max(maxCollision, binCount);
        }
      }
      ++modCount;
      if (++size > threshold) resize();
      return null;
    }

    /**
     * Initializes or doubles table size. If null, allocates in accord with initial capacity target
     * held in field threshold. Otherwise, because we are using power-of-two expansion, the elements
     * from each bin must either stay at same index, or move with a power of two offset in the new
     * table.
     *
     * @return the table
     */
    final Value[] resize() {
      Value[] oldTab = table;
      int oldCap = (oldTab == null) ? 0 : oldTab.length;
      int oldThr = threshold;
      int newCap, newThr = 0;
      if (oldCap > 0) {
        if (oldCap >= MAXIMUM_CAPACITY) {
          threshold = Integer.MAX_VALUE;
          return oldTab;
        } else if ((newCap = oldCap << 1) < MAXIMUM_CAPACITY && oldCap >= DEFAULT_INITIAL_CAPACITY)
          newThr = oldThr << 1; // double threshold
      } else if (oldThr > 0) // initial capacity was placed in threshold
      newCap = oldThr;
      else { // zero initial threshold signifies using defaults
        newCap = DEFAULT_INITIAL_CAPACITY;
        newThr = (int) (DEFAULT_LOAD_FACTOR * DEFAULT_INITIAL_CAPACITY);
      }
      if (newThr == 0) {
        float ft = (float) newCap * loadFactor;
        newThr = (newCap < MAXIMUM_CAPACITY && ft < (float) MAXIMUM_CAPACITY
            ? (int) ft
            : Integer.MAX_VALUE);
      }
      threshold = newThr;
      @SuppressWarnings({"rawtypes", "unchecked"})
      Value[] newTab = new Value[newCap];
      table = newTab;
      if (oldTab != null) {
        for (int j = 0; j < oldCap; ++j) {
          Value e;
          if ((e = oldTab[j]) != null) {
            oldTab[j] = null;
            if (e.next == null) newTab[e.hash & (newCap - 1)] = e;
            else { // preserve order
              Value loHead = null, loTail = null;
              Value hiHead = null, hiTail = null;
              Value next;
              do {
                next = e.next;
                if ((e.hash & oldCap) == 0) {
                  if (loTail == null) loHead = e;
                  else loTail.next = e;
                  loTail = e;
                } else {
                  if (hiTail == null) hiHead = e;
                  else hiTail.next = e;
                  hiTail = e;
                }
              } while ((e = next) != null);
              if (loTail != null) {
                loTail.next = null;
                newTab[j] = loHead;
              }
              if (hiTail != null) {
                hiTail.next = null;
                newTab[j + oldCap] = hiHead;
              }
            }
          }
        }
      }
      return newTab;
    }

    /**
     * Removes the mapping for the specified key from this map if present.
     *
     * @param key key whose mapping is to be removed from the map
     * @return the previous value associated with {@code key}, or {@code null} if there was no
     *     mapping for {@code key}. (A {@code null} return can also indicate that the map previously
     *     associated {@code null} with {@code key}.)
     */
    public Value remove(long key) {
      return removeNode(hash(key), key, null, false);
    }

    /**
     * Implements Map.remove and related methods.
     *
     * @param hash hash for key
     * @param key the key
     * @param value the value to match if matchValue, else ignored
     * @param matchValue if true only remove if value is equal
     * @return the node, or null if none
     */
    final Value removeNode(int hash, long key, Value value, boolean matchValue) {
      Value[] tab;
      Value p;
      int n, index;
      if ((tab = table) != null
          && (n = tab.length) > 0
          && (p = tab[index = (n - 1) & hash]) != null) {
        Value node = null, e;
        if (p.hash == hash && (p.key() == key)) node = p;
        else if ((e = p.next) != null) {
          do {
            if (e.hash == hash && (e.xx3 == key)) {
              node = e;
              break;
            }
            p = e;
          } while ((e = e.next) != null);
        }
        if (node != null && (!matchValue || node == value)) {
          if (node == p) tab[index] = node.next;
          else p.next = node.next;
          ++modCount;
          --size;
          return node;
        }
      }
      return null;
    }

    /** Removes all of the mappings from this map. The map will be empty after this call returns. */
    public void clear() {
      Value[] tab;
      modCount++;
      if ((tab = table) != null && size > 0) {
        size = 0;
        Arrays.fill(tab, null);
      }
    }

    public boolean remove(long key, Value value) {
      return removeNode(hash(key), key, value, true) != null;
    }
  }
}
