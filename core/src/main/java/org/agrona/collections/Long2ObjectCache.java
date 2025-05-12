/*
 * Copyright 2014-2025 Real Logic Limited.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.agrona.collections;

import static java.util.Objects.requireNonNull;
import static org.agrona.collections.CollectionUtil.validatePositivePowerOfTwo;

import java.util.AbstractCollection;
import java.util.AbstractSet;
import java.util.Iterator;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Objects;
import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.LongFunction;
import java.util.function.Predicate;
import org.agrona.generation.DoNotSub;

/**
 * A cache implementation specialised for long keys using open addressing to probe a set of fixed
 * size.
 *
 * <p>The eviction strategy is to remove the oldest in a set if the key is not found, or if found
 * then that item. The newly inserted item becomes the youngest in the set. Sets are evicted on a
 * first in, first out, manner unless replacing a matching key.
 *
 * <p>A good set size would be in the range of 2 to 16 so that the references/keys can fit in a
 * cache-line (assuming references are 32-bit references and 64-byte cache lines, YMMV). A linear
 * search within a cache line is much less costly than a cache-miss to another line.
 *
 * <p>Null values are not supported by this cache.
 *
 * @param <V> type of values stored in the {@link Map}
 */
public class Long2ObjectCache<V> implements Map<Long, V> {
  /*
   * Example for numSets=2 and setSize=4:
   *
   *               newest               oldest
   * keys:        [  0  ][  1  ][  2  ][  3  ][  4  ][  5  ][  6  ][  7  ]
   * values:      [  0  ][  1  ][  2  ][  3  ][  4  ][  5  ][  6  ][  7  ]
   *              <-         set 0          -><-         set 1          ->
   * shuffleUp:       <---   <---   <---  X
   * shuffleDown:    X  --->   --->   --->
   */

  private long cachePuts = 0;
  private long cacheHits = 0;
  private long cacheMisses = 0;

  @DoNotSub
  private int size;

  @DoNotSub
  private final int capacity;

  @DoNotSub
  private final int setSize;

  @DoNotSub
  private final int setSizeShift;

  @DoNotSub
  private final int mask;

  private final long[] keys;
  private final Object[] values;
  private final Consumer<V> evictionConsumer;

  private ValueCollection valueCollection;
  private KeySet keySet;
  private EntrySet entrySet;

  /**
   * Constructs cache with provided configuration.
   *
   * @param numSets number of sets, must be power or two.
   * @param setSize size of a single set, must be power or two.
   * @param evictionConsumer consumer to be notified when entry is being evicted from the cache.
   */
  public Long2ObjectCache(
      @DoNotSub final int numSets,
      @DoNotSub final int setSize,
      final Consumer<V> evictionConsumer) {
    validatePositivePowerOfTwo(numSets);
    validatePositivePowerOfTwo(setSize);
    requireNonNull(evictionConsumer, "null values are not permitted");

    if (((long) numSets) * setSize > (Long.MAX_VALUE - 8)) {
      throw new IllegalArgumentException(
          "total capacity must be <= max array size: numSets=" + numSets + " setSize=" + setSize);
    }

    this.setSize = setSize;
    this.setSizeShift = Long.numberOfTrailingZeros(setSize);
    capacity = numSets << setSizeShift;
    mask = numSets - 1;

    keys = new long[capacity];
    values = new Object[capacity];
    this.evictionConsumer = evictionConsumer;
  }

  /**
   * The number of times a cache hit has occurred on the {@link #get(long)} method.
   *
   * @return the number of times a cache hit has occurred on the {@link #get(long)} method.
   */
  public long cacheHits() {
    return cacheHits;
  }

  /**
   * The number of times a cache miss has occurred on the {@link #get(long)} method.
   *
   * @return the number of times a cache miss has occurred on the {@link #get(long)} method.
   */
  public long cacheMisses() {
    return cacheMisses;
  }

  /**
   * The number of items that have been put in the cache.
   *
   * @return number of items that have been put in the cache.
   */
  public long cachePuts() {
    return cachePuts;
  }

  /** Reset the cache statistics counters to zero. */
  public void resetCounters() {
    cacheHits = 0;
    cacheMisses = 0;
    cachePuts = 0;
  }

  /**
   * Get the total capacity for the map to which the load factor will be a fraction of.
   *
   * @return the total capacity for the map.
   */
  @DoNotSub
  public int capacity() {
    return capacity;
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int size() {
    return size;
  }

  /** {@inheritDoc} */
  public boolean isEmpty() {
    return 0 == size;
  }

  /** {@inheritDoc} */
  public boolean containsKey(final Object key) {
    return containsKey((long) key);
  }

  /**
   * Overloaded version of {@link Map#containsKey(Object)} that takes a primitive long key.
   *
   * @param key for indexing the {@link Map}
   * @return true if the key is found otherwise false.
   */
  public boolean containsKey(final long key) {
    boolean found = false;
    @DoNotSub final int setNumber = Hashing.hash(key, mask);
    @DoNotSub final int setBeginIndex = setNumber << setSizeShift;

    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = setBeginIndex, setEndIndex = setBeginIndex + setSize;
        i < setEndIndex;
        i++) {
      if (null == values[i]) {
        break;
      }

      if (key == keys[i]) {
        found = true;
        break;
      }
    }

    return found;
  }

  /** {@inheritDoc} */
  public boolean containsValue(final Object value) {
    boolean found = false;
    if (null != value) {
      final Object[] values = this.values;
      for (final Object v : values) {
        if (Objects.equals(v, value)) {
          found = true;
          break;
        }
      }
    }

    return found;
  }

  /** {@inheritDoc} */
  public V get(final Object key) {
    return get((long) key);
  }

  /**
   * Overloaded version of {@link Map#get(Object)} that takes a primitive long key.
   *
   * @param key for indexing the {@link Map}
   * @return the value if found otherwise null
   */
  @SuppressWarnings("unchecked")
  public V get(final long key) {
    @DoNotSub final int setNumber = Hashing.hash(key, mask);
    @DoNotSub final int setBeginIndex = setNumber << setSizeShift;

    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = setBeginIndex, setEndIndex = setBeginIndex + setSize;
        i < setEndIndex;
        i++) {
      final Object value = values[i];
      if (null == value) {
        break;
      }

      if (key == keys[i]) {
        cacheHits++;
        return (V) value;
      }
    }

    cacheMisses++;
    return null;
  }

  /**
   * Returns the value to which the specified key is mapped, or defaultValue if this map contains no
   * mapping for the key.
   *
   * @param key whose associated value is to be returned.
   * @param defaultValue the default mapping of the key.
   * @return the value to which the specified key is mapped, or {@code defaultValue} if this map
   *     contains no mapping for the key.
   */
  public V getOrDefault(final long key, final V defaultValue) {
    final V value = get(key);
    return null != value ? value : defaultValue;
  }

  /** {@inheritDoc} */
  public void forEach(final BiConsumer<? super Long, ? super V> action) {
    forEachLong(action::accept);
  }

  /**
   * Implementation of the {@link #forEach(BiConsumer)} that avoids boxing of keys.
   *
   * @param consumer to be called for each key/value pair.
   */
  @SuppressWarnings("unchecked")
  public void forEachLong(final LongObjConsumer<? super V> consumer) {
    requireNonNull(consumer);
    final long[] keys = this.keys;
    final Object[] values = this.values;
    @DoNotSub final int length = values.length;
    @DoNotSub int remaining = size;

    for (@DoNotSub int index = 0; remaining > 0 && index < length; index++) {
      final Object value = values[index];
      if (null != value) {
        consumer.accept(keys[index], (V) value);
        --remaining;
      }
    }
  }

  /** {@inheritDoc} */
  public V computeIfAbsent(
      final Long key, final Function<? super Long, ? extends V> mappingFunction) {
    return computeIfAbsent((long) key, mappingFunction::apply);
  }

  /**
   * Get a value for a given key, or if it does ot exist then default the value via a
   * {@link LongFunction} and put it in the cache.
   *
   * <p>Primitive specialized version of {@link Map#computeIfAbsent}.
   *
   * @param key to search on.
   * @param mappingFunction to provide a value if the get returns null.
   * @return the value if found otherwise the default.
   */
  public V computeIfAbsent(final long key, final LongFunction<? extends V> mappingFunction) {
    requireNonNull(mappingFunction);
    V value = get(key);
    if (null == value) {
      value = mappingFunction.apply(key);
      if (null != value) {
        put(key, value);
      }
    }

    return value;
  }

  /** {@inheritDoc} */
  public V computeIfPresent(
      final Long key, final BiFunction<? super Long, ? super V, ? extends V> remappingFunction) {
    return computeIfPresent((long) key, remappingFunction::apply);
  }

  /**
   * If the value for the specified key is present attempts to compute a new mapping given the key
   * and its current mapped value.
   *
   * <p>Primitive specialized version of {@link Map#computeIfPresent(Object, BiFunction)}.
   *
   * @param key with which the specified value is to be associated.
   * @param remappingFunction the function to compute a value.
   * @return the new value associated with the specified key, or null if none.
   */
  public V computeIfPresent(
      final long key, final LongObjectToObjectFunction<? super V, ? extends V> remappingFunction) {
    requireNonNull(remappingFunction);
    final V oldValue = get(key);
    if (null != oldValue) {
      final V newValue = remappingFunction.apply(key, oldValue);
      if (null != newValue) {
        put(key, newValue);
        return newValue;
      } else {
        remove(key);
        return null;
      }
    }
    return null;
  }

  /** {@inheritDoc} */
  public V compute(
      final Long key, final BiFunction<? super Long, ? super V, ? extends V> remappingFunction) {
    return compute((long) key, remappingFunction::apply);
  }

  /**
   * Attempts to compute a mapping for the specified key and its current mapped value (or
   * {@code null} if there is no current mapping).
   *
   * <p>Primitive specialized version of {@link Map#compute(Object, BiFunction)}.
   *
   * @param key with which the specified value is to be associated.
   * @param remappingFunction the function to compute a value.
   * @return the new value associated with the specified key, or null if none.
   */
  public V compute(
      final long key, final LongObjectToObjectFunction<? super V, ? extends V> remappingFunction) {
    final V oldValue = get(key);
    final V newValue = remappingFunction.apply(key, oldValue);
    if (null != newValue) {
      put(key, newValue);
      return newValue;
    } else {
      if (null != oldValue) {
        remove(key);
      }
      return null;
    }
  }

  /** {@inheritDoc} */
  public V merge(
      final Long key,
      final V value,
      final BiFunction<? super V, ? super V, ? extends V> remappingFunction) {
    return merge((long) key, value, remappingFunction);
  }

  /**
   * If the specified key is not already associated with a value or is associated with null,
   * associates it with the given non-null value. Otherwise, replaces the associated value with the
   * results of the given remapping function, or removes if the result is {@code null}.
   *
   * <p>Primitive specialized version of {@link Map#merge(Object, Object, BiFunction)}.
   *
   * @param key with which the resulting value is to be associated.
   * @param value the non-null value to be merged with the existing value associated with the key
   *     or, if no existing value or a null value is associated with the key, to be associated with
   *     the key.
   * @param remappingFunction the function to recompute a value if present.
   * @return the new value associated with the specified key, or null if no value is associated with
   *     the key.
   */
  public V merge(
      final long key,
      final V value,
      final BiFunction<? super V, ? super V, ? extends V> remappingFunction) {
    requireNonNull(value);
    requireNonNull(remappingFunction);
    final V oldValue = get(key);
    final V newValue = null == oldValue ? value : remappingFunction.apply(oldValue, value);
    if (null != newValue) {
      put(key, newValue);
      return newValue;
    } else {
      remove(key);
      return null;
    }
  }

  /** {@inheritDoc} */
  public V putIfAbsent(final Long key, final V value) {
    return putIfAbsent((long) key, value);
  }

  /**
   * If the specified key is not already associated with a value (or is mapped to {@code null})
   * associates it with the given value and returns {@code null}, else returns the current value.
   *
   * <p>Primitive specialized version of {@link Map#putIfAbsent(Object, Object)}.
   *
   * @param key with which the specified value is to be associated.
   * @param value to be associated with the specified key.
   * @return the previous value associated with the specified key, or {@code null} if there was no
   *     mapping for the key.
   */
  public V putIfAbsent(final long key, final V value) {
    final V existingValue = get(key);
    if (null == existingValue) {
      put(key, value);
      return null;
    }
    return existingValue;
  }

  /** {@inheritDoc} */
  public V put(final Long key, final V value) {
    return put((long) key, value);
  }

  /**
   * Overloaded version of {@link Map#put(Object, Object)} that takes a primitive long key.
   *
   * @param key for indexing the {@link Map}
   * @param value to be inserted in the {@link Map}
   * @return always null (as per JCache API, as opposed to {@link Map})
   */
  @SuppressWarnings("unchecked")
  public V put(final long key, final V value) {
    requireNonNull(value, "null values are not supported");

    @DoNotSub final int setNumber = Hashing.hash(key, mask);
    @DoNotSub final int setBeginIndex = setNumber << setSizeShift;
    @DoNotSub int i = setBeginIndex;

    final Object[] values = this.values;
    final long[] keys = this.keys;
    Object evictedValue = null;
    for (@DoNotSub int nextSetIndex = setBeginIndex + setSize; i < nextSetIndex; i++) {
      evictedValue = values[i];
      if (null == evictedValue) {
        break;
      }

      if (key == keys[i]) {
        shuffleUp(i, nextSetIndex - 1);

        break;
      }
    }

    if (null == evictedValue) {
      evictedValue = values[setBeginIndex + (setSize - 1)];
    }

    shuffleDown(setBeginIndex);

    keys[setBeginIndex] = key;
    values[setBeginIndex] = value;

    cachePuts++;

    if (null != evictedValue) {
      evictionConsumer.accept((V) evictedValue);
    } else {
      ++size;
    }

    return null;
  }

  /** {@inheritDoc} */
  @SuppressWarnings("unchecked")
  public boolean remove(final Object key, final Object value) {
    return remove((long) key, (V) value);
  }

  /**
   * Removes the entry for the specified key only if it is currently mapped to the specified value.
   *
   * <p>Primitive specialized version of {@link Map#remove(Object, Object)}.
   *
   * @param key key with which the specified value is associated.
   * @param value expected to be associated with the specified key.
   * @return {@code true} if the value was removed.
   */
  public boolean remove(final long key, final V value) {
    final V existingValue = get(key);
    if (null != existingValue && Objects.equals(existingValue, value)) {
      remove(key);
      return true;
    }
    return false;
  }

  /** {@inheritDoc} */
  public V remove(final Object key) {
    return remove((long) key);
  }

  /**
   * Overloaded version of {@link Map#remove(Object)} that takes a primitive long key.
   *
   * @param key for indexing the {@link Map}
   * @return the value if found otherwise null
   */
  @SuppressWarnings("unchecked")
  public V remove(final long key) {
    @DoNotSub final int setNumber = Hashing.hash(key, mask);
    @DoNotSub final int setBeginIndex = setNumber << setSizeShift;

    final long[] keys = this.keys;
    final Object[] values = this.values;
    Object value = null;
    for (@DoNotSub int i = setBeginIndex, nextSetIndex = setBeginIndex + setSize;
        i < nextSetIndex;
        i++) {
      value = values[i];
      if (null == value) {
        break;
      }

      if (key == keys[i]) {
        shuffleUp(i, nextSetIndex - 1);
        --size;

        evictionConsumer.accept((V) value);
        break;
      }
    }

    return (V) value;
  }

  /** {@inheritDoc} */
  public boolean replace(final Long key, final V oldValue, final V newValue) {
    return replace((long) key, oldValue, newValue);
  }

  /**
   * Replaces the entry for the specified key only if currently mapped to the specified value.
   *
   * <p>Primitive specialized version of {@link Map#replace(Object, Object, Object)}.
   *
   * @param key with which the specified value is associated.
   * @param oldValue expected to be associated with the specified key.
   * @param newValue to be associated with the specified key.
   * @return {@code true} if the value was replaced.
   */
  public boolean replace(final long key, final V oldValue, final V newValue) {
    final V existingValue = get(key);
    if (null != existingValue && Objects.equals(existingValue, oldValue)) {
      put(key, newValue);
      return true;
    }
    return false;
  }

  /** {@inheritDoc} */
  public V replace(final Long key, final V value) {
    return replace((long) key, value);
  }

  /**
   * Replaces the entry for the specified key only if it is currently mapped to some value.
   *
   * <p>Primitive specialized version of {@link Map#replace(Object, Object)}.
   *
   * @param key with which the specified value is associated.
   * @param value to be associated with the specified key.
   * @return the previous value associated with the specified key.
   */
  public V replace(final long key, final V value) {
    final V oldValue = get(key);
    if (null != oldValue) {
      put(key, value);
    }
    return oldValue;
  }

  /** {@inheritDoc} */
  public void replaceAll(final BiFunction<? super Long, ? super V, ? extends V> function) {
    replaceAllLong(function::apply);
  }

  /**
   * Replaces each entry's value with the result of invoking the given function on that entry until
   * all entries have been processed or the function throws an exception.
   *
   * <p>Primitive specialized version of {@link Map#replaceAll(BiFunction)}.
   *
   * <p>NB: Renamed from forEach to avoid overloading on parameter types of lambda expression, which
   * doesn't play well with type inference in lambda expressions.
   *
   * @param function the function to apply to each entry.
   */
  @SuppressWarnings("unchecked")
  public void replaceAllLong(final LongObjectToObjectFunction<? super V, ? extends V> function) {
    requireNonNull(function);
    final long[] keys = this.keys;
    final Object[] values = this.values;
    @DoNotSub final int length = values.length;
    @DoNotSub int remaining = size;

    for (@DoNotSub int index = 0; remaining > 0 && index < length; index++) {
      final Object oldValue = values[index];
      if (null != oldValue) {
        final V newValue = function.apply(keys[index], (V) oldValue);
        requireNonNull(newValue, "null values are not supported");
        values[index] = newValue;
        --remaining;
      }
    }
  }

  @DoNotSub
  private void shuffleUp(final int fromIndex, final int toIndex) {
    final long[] keys = this.keys;
    final Object[] values = this.values;

    for (@DoNotSub int i = fromIndex; i < toIndex; i++) {
      values[i] = values[i + 1];
      keys[i] = keys[i + 1];
    }

    values[toIndex] = null;
  }

  @DoNotSub
  private void shuffleDown(final int setBeginIndex) {
    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = setBeginIndex + (setSize - 1); i > setBeginIndex; i--) {
      values[i] = values[i - 1];
      keys[i] = keys[i - 1];
    }

    values[setBeginIndex] = null;
  }

  /**
   * Clear down all items in the cache.
   *
   * <p>If an exception occurs during the eviction function callback then clear may need to be
   * called again to complete.
   *
   * <p>If an exception occurs the cache should only be used when {@link #size()} reports zero.
   */
  @SuppressWarnings("unchecked")
  public void clear() {
    final Object[] values = this.values;
    for (@DoNotSub int i = 0, length = values.length; i < length; i++) {
      final Object value = values[i];
      if (null != value) {
        values[i] = null;
        size--;

        evictionConsumer.accept((V) value);
      }
    }
  }

  /** {@inheritDoc} */
  public void putAll(final Map<? extends Long, ? extends V> map) {
    for (final Entry<? extends Long, ? extends V> entry : map.entrySet()) {
      put(entry.getKey(), entry.getValue());
    }
  }

  /**
   * Put all values from the given map longo this one without allocation.
   *
   * @param map whose value are to be added.
   */
  public void putAll(final Long2ObjectCache<? extends V> map) {
    final Long2ObjectCache<? extends V>.EntryIterator iterator = map.entrySet().iterator();
    while (iterator.hasNext()) {
      iterator.findNext();
      put(iterator.getLongKey(), iterator.getValue());
    }
  }

  /** {@inheritDoc} */
  public KeySet keySet() {
    if (null == keySet) {
      keySet = new KeySet();
    }

    return keySet;
  }

  /** {@inheritDoc} */
  public ValueCollection values() {
    if (null == valueCollection) {
      valueCollection = new ValueCollection();
    }

    return valueCollection;
  }

  /** {@inheritDoc} */
  public EntrySet entrySet() {
    if (null == entrySet) {
      entrySet = new EntrySet();
    }

    return entrySet;
  }

  /** {@inheritDoc} */
  public String toString() {
    final StringBuilder sb = new StringBuilder();
    sb.append('{');

    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = 0, length = values.length; i < length; i++) {
      final Object value = values[i];
      if (null != value) {
        sb.append(keys[i]).append('=').append(value).append(", ");
      }
    }

    if (sb.length() > 1) {
      sb.setLength(sb.length() - 2);
    }

    sb.append('}');

    return sb.toString();
  }

  /** {@inheritDoc} */
  public boolean equals(final Object o) {
    if (this == o) {
      return true;
    }

    if (!(o instanceof Map)) {
      return false;
    }

    final Map<?, ?> that = (Map<?, ?>) o;

    if (size != that.size()) {
      return false;
    }

    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = 0, length = values.length; i < length; i++) {
      final Object thisValue = values[i];
      if (null != thisValue) {
        final Object thatValue = that.get(keys[i]);
        if (!thisValue.equals(thatValue)) {
          return false;
        }
      }
    }

    return true;
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int hashCode() {
    @DoNotSub int result = 0;

    final long[] keys = this.keys;
    final Object[] values = this.values;
    for (@DoNotSub int i = 0, length = values.length; i < length; i++) {
      final Object value = values[i];
      if (null != value) {
        result += (Long.hashCode(keys[i]) ^ value.hashCode());
      }
    }

    return result;
  }

  /** A key set implementation which supports cached iterator to avoid allocation. */
  public final class KeySet extends AbstractSet<Long> {
    private final KeyIterator iterator = new KeyIterator();

    /** Create a new instance. */
    public KeySet() {}

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2ObjectCache.this.size();
    }

    /** {@inheritDoc} */
    public boolean contains(final Object o) {
      return Long2ObjectCache.this.containsKey(o);
    }

    /**
     * Check if the given key contained in the set without boxing.
     *
     * @param key to be checked.
     * @return {@code true} if key is contained in the cache.
     */
    public boolean contains(final long key) {
      return Long2ObjectCache.this.containsKey(key);
    }

    /** {@inheritDoc} */
    public KeyIterator iterator() {
      iterator.reset();

      return iterator;
    }

    /** {@inheritDoc} */
    public boolean remove(final Object o) {
      throw new UnsupportedOperationException("Cannot remove from KeySet");
    }

    /** {@inheritDoc} */
    public boolean removeIf(final Predicate<? super Long> filter) {
      throw new UnsupportedOperationException("Cannot remove from KeySet");
    }

    /** {@inheritDoc} */
    public void clear() {
      Long2ObjectCache.this.clear();
    }
  }

  /** Collection of values which supports cached iterator to avoid allocation. */
  public final class ValueCollection extends AbstractCollection<V> {
    private final ValueIterator iterator = new ValueIterator();

    /** Create a new instance. */
    public ValueCollection() {}

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2ObjectCache.this.size();
    }

    /** {@inheritDoc} */
    public boolean contains(final Object o) {
      return Long2ObjectCache.this.containsValue(o);
    }

    /** {@inheritDoc} */
    public ValueIterator iterator() {
      iterator.reset();

      return iterator;
    }

    /** {@inheritDoc} */
    public void clear() {
      Long2ObjectCache.this.clear();
    }

    /** {@inheritDoc} */
    public boolean remove(final Object o) {
      throw new UnsupportedOperationException("Cannot remove from ValueCollection");
    }

    /** {@inheritDoc} */
    public boolean removeIf(final Predicate<? super V> filter) {
      throw new UnsupportedOperationException("Cannot remove from ValueCollection");
    }
  }

  /** Set of entries which supports cached iterator to avoid allocation. */
  public final class EntrySet extends AbstractSet<Entry<Long, V>> {
    private final EntryIterator iterator = new EntryIterator();

    /** Create a new instance. */
    public EntrySet() {}

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2ObjectCache.this.size();
    }

    /** {@inheritDoc} */
    public EntryIterator iterator() {
      iterator.reset();

      return iterator;
    }

    /** {@inheritDoc} */
    public void clear() {
      Long2ObjectCache.this.clear();
    }

    /** {@inheritDoc} */
    public boolean remove(final Object o) {
      throw new UnsupportedOperationException("Cannot remove from EntrySet");
    }

    /** {@inheritDoc} */
    public boolean removeIf(final Predicate<? super Entry<Long, V>> filter) {
      throw new UnsupportedOperationException("Cannot remove from EntrySet");
    }
  }

  /**
   * Base iterator implementation that contains basic logic of traversing the element in the backing
   * array.
   *
   * @param <T> type of elements.
   */
  abstract class AbstractIterator<T> implements Iterator<T> {
    @DoNotSub
    private int remaining;

    @DoNotSub
    private int position = -1;

    /**
     * Position of the current element.
     *
     * @return position of the current element.
     */
    @DoNotSub
    protected final int position() {
      return position;
    }

    /** {@inheritDoc} */
    public boolean hasNext() {
      return remaining > 0;
    }

    /** Find next element. */
    protected final void findNext() {
      boolean found = false;
      final Object[] values = Long2ObjectCache.this.values;

      for (@DoNotSub int i = position + 1, size = capacity; i < size; i++) {
        if (null != values[i]) {
          found = true;
          position = i;
          --remaining;
          break;
        }
      }

      if (!found) {
        throw new NoSuchElementException();
      }
    }

    /** {@inheritDoc} */
    public abstract T next();

    /** {@inheritDoc} */
    public void remove() {
      throw new UnsupportedOperationException("Remove not supported on Iterator");
    }

    void reset() {
      remaining = size;
      position = -1;
    }
  }

  /** An iterator over values. */
  public final class ValueIterator extends AbstractIterator<V> {
    /** Create a new instance. */
    public ValueIterator() {}

    /** {@inheritDoc} */
    @SuppressWarnings("unchecked")
    public V next() {
      findNext();
      return (V) values[position()];
    }
  }

  /** Iterator over keys which supports access to unboxed keys via {@link #nextLong()}. */
  public final class KeyIterator extends AbstractIterator<Long> {
    /** Create a new instance. */
    public KeyIterator() {}

    /** {@inheritDoc} */
    public Long next() {
      return nextLong();
    }

    /**
     * Return next key without boxing.
     *
     * @return next key.
     */
    public long nextLong() {
      findNext();
      return keys[position()];
    }
  }

  /** Iterator over entries which supports access to unboxed keys via {@link #getLongKey()}. */
  public final class EntryIterator extends AbstractIterator<Entry<Long, V>>
      implements Entry<Long, V> {
    /** Create a new instance. */
    public EntryIterator() {}

    /** {@inheritDoc} */
    public Entry<Long, V> next() {
      findNext();

      return this;
    }

    /** {@inheritDoc} */
    public Long getKey() {
      return getLongKey();
    }

    /**
     * Get key of the current entry.
     *
     * @return key of the current entry.
     */
    public long getLongKey() {
      return keys[position()];
    }

    /** {@inheritDoc} */
    @SuppressWarnings("unchecked")
    public V getValue() {
      return (V) values[position()];
    }

    /** {@inheritDoc} */
    public V setValue(final V value) {
      throw new UnsupportedOperationException("no set on this iterator");
    }
  }
}
