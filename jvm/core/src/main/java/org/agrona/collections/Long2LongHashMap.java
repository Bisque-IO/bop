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
import static org.agrona.BitUtil.findNextPositivePowerOfTwo;
import static org.agrona.collections.CollectionUtil.validateLoadFactor;

import java.util.AbstractCollection;
import java.util.AbstractSet;
import java.util.Arrays;
import java.util.Iterator;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Objects;
import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.LongBinaryOperator;
import java.util.function.LongPredicate;
import java.util.function.LongUnaryOperator;
import org.agrona.generation.DoNotSub;

/**
 * An open-addressing with linear probing hash map specialised for primitive key and value pairs.
 */
public class Long2LongHashMap implements Map<Long, Long> {
  @DoNotSub
  static final int MIN_CAPACITY = 8;

  private final float loadFactor;
  private final long missingValue;

  @DoNotSub
  private int resizeThreshold;

  @DoNotSub
  private int size = 0;

  private final boolean shouldAvoidAllocation;

  private long[] entries;
  private KeySet keySet;
  private ValueCollection values;
  private EntrySet entrySet;

  /**
   * Create a map instance that does not allocate iterators with a specified {@code missingValue}.
   *
   * @param missingValue for the map that represents null.
   */
  public Long2LongHashMap(final long missingValue) {
    this(MIN_CAPACITY, Hashing.DEFAULT_LOAD_FACTOR, missingValue);
  }

  /**
   * Create a map instance that does not allocate iterators with specified parameters.
   *
   * @param initialCapacity for the map to override {@link #MIN_CAPACITY}
   * @param loadFactor for the map to override {@link Hashing#DEFAULT_LOAD_FACTOR}.
   * @param missingValue for the map that represents null.
   */
  public Long2LongHashMap(
      @DoNotSub final int initialCapacity,
      @DoNotSub final float loadFactor,
      final long missingValue) {
    this(initialCapacity, loadFactor, missingValue, true);
  }

  /**
   * Create a map instance with specified parameters.
   *
   * @param initialCapacity for the map to override {@link #MIN_CAPACITY}
   * @param loadFactor for the map to override {@link Hashing#DEFAULT_LOAD_FACTOR}.
   * @param missingValue for the map that represents null.
   * @param shouldAvoidAllocation should allocation be avoided by caching iterators and map entries.
   */
  public Long2LongHashMap(
      @DoNotSub final int initialCapacity,
      @DoNotSub final float loadFactor,
      final long missingValue,
      final boolean shouldAvoidAllocation) {
    validateLoadFactor(loadFactor);

    this.loadFactor = loadFactor;
    this.missingValue = missingValue;
    this.shouldAvoidAllocation = shouldAvoidAllocation;

    capacity(findNextPositivePowerOfTwo(Math.max(MIN_CAPACITY, initialCapacity)));
  }

  /**
   * Copy construct a new map from an existing one.
   *
   * @param mapToCopy for construction.
   */
  public Long2LongHashMap(final Long2LongHashMap mapToCopy) {
    this.loadFactor = mapToCopy.loadFactor;
    this.resizeThreshold = mapToCopy.resizeThreshold;
    this.size = mapToCopy.size;
    this.shouldAvoidAllocation = mapToCopy.shouldAvoidAllocation;
    this.missingValue = mapToCopy.missingValue;

    entries = mapToCopy.entries.clone();
  }

  /**
   * The value to be used as a null marker in the map.
   *
   * @return value to be used as a null marker in the map.
   */
  public long missingValue() {
    return missingValue;
  }

  /**
   * Get the load factor applied for resize operations.
   *
   * @return the load factor applied for resize operations.
   */
  public float loadFactor() {
    return loadFactor;
  }

  /**
   * Get the total capacity for the map to which the load factor will be a fraction of.
   *
   * @return the total capacity for the map.
   */
  @DoNotSub
  public int capacity() {
    return entries.length >> 1;
  }

  /**
   * Get the actual threshold which when reached the map will resize. This is a function of the
   * current capacity and load factor.
   *
   * @return the threshold when the map will resize.
   */
  @DoNotSub
  public int resizeThreshold() {
    return resizeThreshold;
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

  /**
   * Returns the value to which the specified key is mapped, or {@code defaultValue} if this map
   * contains no mapping for the key.
   *
   * @param key whose associated value is to be returned.
   * @param defaultValue to be returned if there is no value in the map for a given {@code key}.
   * @return the value to which the specified key is mapped, or {@code defaultValue} if this map
   *     contains no mapping for the key.
   */
  public long getOrDefault(final long key, final long defaultValue) {
    final long value = get(key);
    return missingValue != value ? value : defaultValue;
  }

  /**
   * Get a value using provided key avoiding boxing.
   *
   * @param key lookup key.
   * @return value associated with the key or {@link #missingValue()} if key is not found in the
   *     map.
   */
  public long get(final long key) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long value;
    while (missingValue != (value = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    return value;
  }

  /**
   * Put a key value pair in the map.
   *
   * @param key lookup key
   * @param value new value, must not be {@link #missingValue()}
   * @return previous value associated with the key, or {@link #missingValue()} if none found
   * @throws IllegalArgumentException if value is {@link #missingValue()}
   */
  public long put(final long key, final long value) {
    final long missingValue = this.missingValue;
    if (missingValue == value) {
      throw new IllegalArgumentException("cannot accept missingValue");
    }

    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    if (missingValue == oldValue) {
      ++size;
      entries[index] = key;
    }

    entries[index + 1] = value;

    increaseCapacity();

    return oldValue;
  }

  /**
   * Primitive specialised version of {@link Map#putIfAbsent(Object, Object)} method.
   *
   * @param key key with which the specified value is to be associated
   * @param value value to be associated with the specified key
   * @return the previous value associated with the specified key, or {@link #missingValue()} if
   *     there was no mapping for the key.
   * @throws IllegalArgumentException if value is {@link #missingValue()}
   */
  public long putIfAbsent(final long key, final long value) {
    final long missingValue = this.missingValue;
    if (missingValue == value) {
      throw new IllegalArgumentException("cannot accept missingValue");
    }

    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[index + 1])) {
      if (key == entries[index]) {
        return oldValue;
      }

      index = next(index, mask);
    }

    ++size;
    entries[index] = key;
    entries[index + 1] = value;

    increaseCapacity();

    return oldValue;
  }

  private void increaseCapacity() {
    if (size > resizeThreshold) {
      // entries.length = 2 * capacity
      @DoNotSub final int newCapacity = entries.length;
      rehash(newCapacity);
    }
  }

  private void rehash(@DoNotSub final int newCapacity) {
    final long missingValue = this.missingValue;
    final long[] oldEntries = entries;
    @DoNotSub final int length = oldEntries.length;

    capacity(newCapacity);

    final long[] newEntries = entries;
    @DoNotSub final int mask = newEntries.length - 1;

    for (@DoNotSub int valueIndex = 1; valueIndex < length; valueIndex += 2) {
      final long value = oldEntries[valueIndex];
      if (missingValue != value) {
        final long key = oldEntries[valueIndex - 1];
        @DoNotSub int newKeyIndex = Hashing.evenHash(key, mask);

        while (missingValue != newEntries[newKeyIndex + 1]) {
          newKeyIndex = next(newKeyIndex, mask);
        }

        newEntries[newKeyIndex] = key;
        newEntries[newKeyIndex + 1] = value;
      }
    }
  }

  /**
   * Use {@link #forEachLong(LongLongConsumer)} instead.
   *
   * @param consumer a callback called for each key/value pair in the map.
   * @see #forEachLong(LongLongConsumer)
   * @deprecated Use {@link #forEachLong(LongLongConsumer)} instead.
   */
  @Deprecated
  public void longForEach(final LongLongConsumer consumer) {
    forEachLong(consumer);
  }

  /**
   * Primitive specialised forEach implementation.
   *
   * <p>NB: Renamed from forEach to avoid overloading on parameter types of lambda expression, which
   * doesn't play well with type inference in lambda expressions.
   *
   * @param consumer a callback called for each key/value pair in the map.
   */
  public void forEachLong(final LongLongConsumer consumer) {
    requireNonNull(consumer);
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int length = entries.length;

    for (@DoNotSub int valueIndex = 1, remaining = size;
        remaining > 0 && valueIndex < length;
        valueIndex += 2) {
      if (missingValue != entries[valueIndex]) {
        consumer.accept(entries[valueIndex - 1], entries[valueIndex]);
        --remaining;
      }
    }
  }

  /**
   * Long primitive specialised containsKey.
   *
   * @param key the key to check.
   * @return true if the map contains key as a key, false otherwise.
   */
  public boolean containsKey(final long key) {
    return missingValue != get(key);
  }

  /**
   * Does the map contain the value.
   *
   * @param value to be tested against contained values.
   * @return true if contained otherwise value.
   */
  public boolean containsValue(final long value) {
    boolean found = false;
    final long missingValue = this.missingValue;
    if (missingValue != value) {
      final long[] entries = this.entries;
      @DoNotSub final int length = entries.length;
      @DoNotSub int remaining = size;

      for (@DoNotSub int valueIndex = 1; remaining > 0 && valueIndex < length; valueIndex += 2) {
        final long existingValue = entries[valueIndex];
        if (missingValue != existingValue) {
          if (existingValue == value) {
            found = true;
            break;
          }
          --remaining;
        }
      }
    }

    return found;
  }

  /** {@inheritDoc} */
  public void clear() {
    if (size > 0) {
      Arrays.fill(entries, missingValue);
      size = 0;
    }
  }

  /**
   * Compact the backing arrays by rehashing with a capacity just larger than current size and
   * giving consideration to the load factor.
   */
  public void compact() {
    @DoNotSub final int idealCapacity = (int) Math.round(size() * (1.0d / loadFactor));
    rehash(findNextPositivePowerOfTwo(Math.max(MIN_CAPACITY, idealCapacity)));
  }

  /**
   * Primitive specialised version of {@link Map#computeIfAbsent(Object, Function)}.
   *
   * @param key to search on.
   * @param mappingFunction to provide a value if the get returns null.
   * @return the value if found otherwise the missing value.
   */
  public long computeIfAbsent(final long key, final LongUnaryOperator mappingFunction) {
    requireNonNull(mappingFunction);
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long value;
    while (missingValue != (value = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    if (missingValue == value && missingValue != (value = mappingFunction.applyAsLong(key))) {
      entries[index] = key;
      entries[index + 1] = value;
      ++size;
      increaseCapacity();
    }

    return value;
  }

  /**
   * Primitive specialised version of {@link Map#computeIfPresent(Object, BiFunction)}.
   *
   * @param key to search on.
   * @param remappingFunction to compute a value if a mapping is found.
   * @return the updated value if a mapping was found, otherwise the missing value.
   */
  public long computeIfPresent(final long key, final LongBinaryOperator remappingFunction) {
    requireNonNull(remappingFunction);
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long value;
    while (missingValue != (value = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    if (missingValue != value) {
      value = remappingFunction.applyAsLong(key, value);
      entries[index + 1] = value;
      if (missingValue == value) {
        size--;
        compactChain(index);
      }
    }

    return value;
  }

  /**
   * Primitive specialised version of {@link Map#compute(Object, BiFunction)}.
   *
   * @param key to search on.
   * @param remappingFunction to compute a value.
   * @return the updated value.
   */
  public long compute(final long key, final LongBinaryOperator remappingFunction) {
    requireNonNull(remappingFunction);
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    final long newValue = remappingFunction.applyAsLong(key, oldValue);
    if (missingValue != newValue) {
      entries[index + 1] = newValue;
      if (oldValue == missingValue) {
        entries[index] = key;
        ++size;
        increaseCapacity();
      }
    } else if (missingValue != oldValue) {
      entries[index + 1] = missingValue;
      size--;
      compactChain(index);
    }

    return newValue;
  }

  // ---------------- Boxed Versions Below ----------------

  /** {@inheritDoc} */
  public Long get(final Object key) {
    return valOrNull(get((long) key));
  }

  /** {@inheritDoc} */
  public Long put(final Long key, final Long value) {
    return valOrNull(put((long) key, (long) value));
  }

  /** {@inheritDoc} */
  public void forEach(final BiConsumer<? super Long, ? super Long> action) {
    forEachLong(action::accept);
  }

  /** {@inheritDoc} */
  public boolean containsKey(final Object key) {
    return containsKey((long) key);
  }

  /** {@inheritDoc} */
  public boolean containsValue(final Object value) {
    return containsValue((long) value);
  }

  /** {@inheritDoc} */
  public void putAll(final Map<? extends Long, ? extends Long> map) {
    for (final Entry<? extends Long, ? extends Long> entry : map.entrySet()) {
      put(entry.getKey(), entry.getValue());
    }
  }

  /**
   * Put all values from the given map longo this map without allocation.
   *
   * @param map whose value are to be added.
   */
  public void putAll(final Long2LongHashMap map) {
    final EntryIterator it = map.entrySet().iterator();
    while (it.hasNext()) {
      it.findNext();
      put(it.getLongKey(), it.getLongValue());
    }
  }

  /** {@inheritDoc} */
  public Long putIfAbsent(final Long key, final Long value) {
    return valOrNull(putIfAbsent((long) key, (long) value));
  }

  /** {@inheritDoc} */
  public Long replace(final Long key, final Long value) {
    return valOrNull(replace((long) key, (long) value));
  }

  /** {@inheritDoc} */
  public boolean replace(final Long key, final Long oldValue, final Long newValue) {
    return replace((long) key, (long) oldValue, (long) newValue);
  }

  /** {@inheritDoc} */
  public void replaceAll(final BiFunction<? super Long, ? super Long, ? extends Long> function) {
    replaceAllLong(function::apply);
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
    if (null == values) {
      values = new ValueCollection();
    }

    return values;
  }

  /** {@inheritDoc} */
  public EntrySet entrySet() {
    if (null == entrySet) {
      entrySet = new EntrySet();
    }

    return entrySet;
  }

  /** {@inheritDoc} */
  public Long remove(final Object key) {
    return valOrNull(remove((long) key));
  }

  /** {@inheritDoc} */
  public boolean remove(final Object key, final Object value) {
    return remove((long) key, (long) value);
  }

  /**
   * Remove value from the map using given key avoiding boxing.
   *
   * @param key whose mapping is to be removed from the map.
   * @return removed value or {@link #missingValue()} if key was not found in the map.
   */
  public long remove(final long key) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int keyIndex = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[keyIndex + 1])) {
      if (key == entries[keyIndex]) {
        entries[keyIndex + 1] = missingValue;
        size--;

        compactChain(keyIndex);

        break;
      }

      keyIndex = next(keyIndex, mask);
    }

    return oldValue;
  }

  /**
   * Primitive specialised version of {@link Map#remove(Object, Object)}.
   *
   * @param key with which the specified value is associated.
   * @param value expected to be associated with the specified key.
   * @return {@code true} if the value was removed.
   */
  public boolean remove(final long key, final long value) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int keyIndex = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[keyIndex + 1])) {
      if (key == entries[keyIndex]) {
        if (value == oldValue) {
          entries[keyIndex + 1] = missingValue;
          size--;

          compactChain(keyIndex);
          return true;
        }
        break;
      }

      keyIndex = next(keyIndex, mask);
    }

    return false;
  }

  /**
   * Primitive specialised version of {@link Map#merge(Object, Object, BiFunction)}.
   *
   * @param key with which the resulting value is to be associated.
   * @param value to be merged with the existing value associated with the key or, if no existing
   *     value or a null value is associated with the key, to be associated with the key.
   * @param remappingFunction the function to recompute a value if present.
   * @return the new value associated with the specified key, or {@link #missingValue()} if no value
   *     is associated with the key as the result of this operation.
   */
  public long merge(final long key, final long value, final LongLongFunction remappingFunction) {
    requireNonNull(remappingFunction);
    final long missingValue = this.missingValue;
    if (missingValue == value) {
      throw new IllegalArgumentException("cannot accept missingValue");
    }
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int index = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[index + 1])) {
      if (key == entries[index]) {
        break;
      }

      index = next(index, mask);
    }

    final long newValue =
        missingValue == oldValue ? value : remappingFunction.apply(oldValue, value);
    if (missingValue != newValue) {
      entries[index + 1] = newValue;
      if (missingValue == oldValue) {
        entries[index] = key;
        ++size;
        increaseCapacity();
      }
    } else {
      entries[index + 1] = missingValue;
      size--;
      compactChain(index);
    }

    return newValue;
  }

  @SuppressWarnings("FinalParameters")
  private void compactChain(@DoNotSub int deleteKeyIndex) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int keyIndex = deleteKeyIndex;

    while (true) {
      keyIndex = next(keyIndex, mask);
      final long value = entries[keyIndex + 1];
      if (missingValue == value) {
        break;
      }

      final long key = entries[keyIndex];
      @DoNotSub final int hash = Hashing.evenHash(key, mask);

      if ((keyIndex < hash && (hash <= deleteKeyIndex || deleteKeyIndex <= keyIndex))
          || (hash <= deleteKeyIndex && deleteKeyIndex <= keyIndex)) {
        entries[deleteKeyIndex] = key;
        entries[deleteKeyIndex + 1] = value;

        entries[keyIndex + 1] = missingValue;
        deleteKeyIndex = keyIndex;
      }
    }
  }

  /**
   * Get the minimum value stored in the map. If the map is empty then it will return
   * {@link #missingValue()}.
   *
   * @return the minimum value stored in the map.
   */
  public long minValue() {
    final long missingValue = this.missingValue;
    long min = 0 == size ? missingValue : Long.MAX_VALUE;
    final long[] entries = this.entries;
    @DoNotSub final int length = entries.length;

    for (@DoNotSub int valueIndex = 1; valueIndex < length; valueIndex += 2) {
      final long value = entries[valueIndex];
      if (missingValue != value) {
        min = Math.min(min, value);
      }
    }

    return min;
  }

  /**
   * Get the maximum value stored in the map. If the map is empty then it will return
   * {@link #missingValue()}.
   *
   * @return the maximum value stored in the map.
   */
  public long maxValue() {
    final long missingValue = this.missingValue;
    long max = 0 == size ? missingValue : Long.MIN_VALUE;
    final long[] entries = this.entries;
    @DoNotSub final int length = entries.length;

    for (@DoNotSub int valueIndex = 1; valueIndex < length; valueIndex += 2) {
      final long value = entries[valueIndex];
      if (missingValue != value) {
        max = Math.max(max, value);
      }
    }

    return max;
  }

  /** {@inheritDoc} */
  public String toString() {
    if (isEmpty()) {
      return "{}";
    }

    final EntryIterator entryIterator = new EntryIterator();
    entryIterator.reset();

    final StringBuilder sb = new StringBuilder().append('{');
    while (true) {
      entryIterator.next();
      sb.append(entryIterator.getLongKey()).append('=').append(entryIterator.getLongValue());
      if (!entryIterator.hasNext()) {
        return sb.append('}').toString();
      }
      sb.append(',').append(' ');
    }
  }

  /**
   * Primitive specialised version of {@link Map#replace(Object, Object)}.
   *
   * @param key key with which the specified value is associated.
   * @param value value to be associated with the specified key.
   * @return the previous value associated with the specified key, or {@link #missingValue()} if
   *     there was no mapping for the key.
   */
  public long replace(final long key, final long value) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int keyIndex = Hashing.evenHash(key, mask);

    long oldValue;
    while (missingValue != (oldValue = entries[keyIndex + 1])) {
      if (key == entries[keyIndex]) {
        entries[keyIndex + 1] = value;
        break;
      }

      keyIndex = next(keyIndex, mask);
    }

    return oldValue;
  }

  /**
   * Primitive specialised version of {@link Map#replace(Object, Object, Object)}.
   *
   * @param key key with which the specified value is associated.
   * @param oldValue value expected to be associated with the specified key.
   * @param newValue value to be associated with the specified key.
   * @return {@code true} if the value was replaced.
   */
  public boolean replace(final long key, final long oldValue, final long newValue) {
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int mask = entries.length - 1;
    @DoNotSub int keyIndex = Hashing.evenHash(key, mask);

    long value;
    while (missingValue != (value = entries[keyIndex + 1])) {
      if (key == entries[keyIndex]) {
        if (oldValue == value) {
          entries[keyIndex + 1] = newValue;
          return true;
        }
        break;
      }

      keyIndex = next(keyIndex, mask);
    }

    return false;
  }

  /**
   * Primitive specialised version of {@link Map#replaceAll(BiFunction)}.
   *
   * <p>NB: Renamed from replaceAll to avoid overloading on parameter types of lambda expression,
   * which doesn't play well with type inference in lambda expressions.
   *
   * @param function to apply to each entry.
   */
  public void replaceAllLong(final LongLongFunction function) {
    requireNonNull(function);
    final long missingValue = this.missingValue;
    final long[] entries = this.entries;
    @DoNotSub final int length = entries.length;

    for (@DoNotSub int valueIndex = 1, remaining = size;
        remaining > 0 && valueIndex < length;
        valueIndex += 2) {
      final long existingValue = entries[valueIndex];
      if (missingValue != existingValue) {
        final long newValue = function.apply(entries[valueIndex - 1], existingValue);
        if (missingValue == newValue) {
          throw new IllegalArgumentException("cannot replace with a missingValue");
        }
        entries[valueIndex] = newValue;
        --remaining;
      }
    }
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

    return size == that.size() && entrySet().equals(that.entrySet());
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int hashCode() {
    return entrySet().hashCode();
  }

  @DoNotSub
  private static int next(final int index, final int mask) {
    return (index + 2) & mask;
  }

  private void capacity(@DoNotSub final int newCapacity) {
    @DoNotSub final int entriesLength = newCapacity * 2;
    if (entriesLength < 0) {
      throw new IllegalStateException("max capacity reached at size=" + size);
    }

    /*@DoNotSub*/ resizeThreshold = (int) (newCapacity * loadFactor);
    entries = new long[entriesLength];
    Arrays.fill(entries, missingValue);
  }

  private Long valOrNull(final long value) {
    return missingValue == value ? null : value;
  }

  // ---------------- Utility Classes ----------------

  /** Base iterator implementation. */
  abstract class AbstractIterator {
    /** Is current position valid. */
    protected boolean isPositionValid = false;

    @DoNotSub
    private int remaining;

    @DoNotSub
    private int positionCounter;

    @DoNotSub
    private int stopCounter;

    final void reset() {
      isPositionValid = false;
      remaining = Long2LongHashMap.this.size;
      final long missingValue = Long2LongHashMap.this.missingValue;
      final long[] entries = Long2LongHashMap.this.entries;
      @DoNotSub final int capacity = entries.length;

      @DoNotSub int keyIndex = capacity;
      if (missingValue != entries[capacity - 1]) {
        for (@DoNotSub int i = 1; i < capacity; i += 2) {
          if (missingValue == entries[i]) {
            keyIndex = i - 1;
            break;
          }
        }
      }

      stopCounter = keyIndex;
      positionCounter = keyIndex + capacity;
    }

    /**
     * Returns position of the key of the current entry.
     *
     * @return key position.
     */
    @DoNotSub
    protected final int keyPosition() {
      return positionCounter & entries.length - 1;
    }

    /**
     * Number of remaining elements.
     *
     * @return number of remaining elements.
     */
    @DoNotSub
    public int remaining() {
      return remaining;
    }

    /**
     * Check if there are more elements remaining.
     *
     * @return {@code true} if {@code remaining > 0}.
     */
    public boolean hasNext() {
      return remaining > 0;
    }

    /**
     * Advance to the next entry.
     *
     * @throws NoSuchElementException if no more entries available.
     */
    protected final void findNext() {
      if (!hasNext()) {
        throw new NoSuchElementException();
      }

      final long[] entries = Long2LongHashMap.this.entries;
      final long missingValue = Long2LongHashMap.this.missingValue;
      @DoNotSub final int mask = entries.length - 1;

      for (@DoNotSub int keyIndex = positionCounter - 2, stop = stopCounter;
          keyIndex >= stop;
          keyIndex -= 2) {
        @DoNotSub final int index = keyIndex & mask;
        if (missingValue != entries[index + 1]) {
          isPositionValid = true;
          positionCounter = keyIndex;
          --remaining;
          return;
        }
      }

      isPositionValid = false;
      throw new IllegalStateException();
    }

    /** Removes from the underlying collection the last element returned by this iterator. */
    public void remove() {
      if (isPositionValid) {
        @DoNotSub final int position = keyPosition();
        entries[position + 1] = missingValue;
        --size;

        compactChain(position);

        isPositionValid = false;
      } else {
        throw new IllegalStateException();
      }
    }
  }

  /** Iterator over keys which supports access to unboxed keys via {@link #nextValue()}. */
  public final class KeyIterator extends AbstractIterator implements Iterator<Long> {
    /** Create a new instance. */
    public KeyIterator() {}

    /** {@inheritDoc} */
    public Long next() {
      return nextValue();
    }

    /**
     * Return next key.
     *
     * @return next key.
     */
    public long nextValue() {
      findNext();
      return entries[keyPosition()];
    }
  }

  /** Iterator over values which supports access to unboxed values. */
  public final class ValueIterator extends AbstractIterator implements Iterator<Long> {
    /** Create a new instance. */
    public ValueIterator() {}

    /** {@inheritDoc} */
    public Long next() {
      return nextValue();
    }

    /**
     * Return next value.
     *
     * @return next value.
     */
    public long nextValue() {
      findNext();
      return entries[keyPosition() + 1];
    }
  }

  /** Iterator over entries which supports access to unboxed keys and values. */
  public final class EntryIterator extends AbstractIterator
      implements Iterator<Entry<Long, Long>>, Entry<Long, Long> {
    /** Create a new instance. */
    public EntryIterator() {}

    /** {@inheritDoc} */
    public Long getKey() {
      return getLongKey();
    }

    /**
     * Returns the key of the current entry.
     *
     * @return the key.
     */
    public long getLongKey() {
      return entries[keyPosition()];
    }

    /** {@inheritDoc} */
    public Long getValue() {
      return getLongValue();
    }

    /**
     * Returns the value of the current entry.
     *
     * @return the value.
     */
    public long getLongValue() {
      return entries[keyPosition() + 1];
    }

    /** {@inheritDoc} */
    public Long setValue(final Long value) {
      return setValue(value.longValue());
    }

    /**
     * Sets the value of the current entry.
     *
     * @param value to be set.
     * @return previous value of the entry.
     */
    public long setValue(final long value) {
      if (!isPositionValid) {
        throw new IllegalStateException();
      }

      if (missingValue == value) {
        throw new IllegalArgumentException("cannot accept missingValue");
      }

      @DoNotSub final int keyPosition = keyPosition();
      final long[] entries = Long2LongHashMap.this.entries;
      final long prevValue = entries[keyPosition + 1];
      entries[keyPosition + 1] = value;
      return prevValue;
    }

    /** {@inheritDoc} */
    public Entry<Long, Long> next() {
      findNext();

      if (shouldAvoidAllocation) {
        return this;
      }

      return allocateDuplicateEntry();
    }

    private Entry<Long, Long> allocateDuplicateEntry() {
      return new MapEntry(getLongKey(), getLongValue());
    }

    /** {@inheritDoc} */
    @DoNotSub
    public int hashCode() {
      return Long.hashCode(getLongKey()) ^ Long.hashCode(getLongValue());
    }

    /** {@inheritDoc} */
    public boolean equals(final Object o) {
      if (this == o) {
        return true;
      }

      if (!(o instanceof Entry)) {
        return false;
      }

      final Entry<?, ?> that = (Entry<?, ?>) o;

      return Objects.equals(getKey(), that.getKey()) && Objects.equals(getValue(), that.getValue());
    }

    /** An {@link Entry} implementation. */
    public final class MapEntry implements Entry<Long, Long> {
      private final long k;
      private final long v;

      /**
       * Constructs entry with given key and value.
       *
       * @param k key.
       * @param v value.
       */
      public MapEntry(final long k, final long v) {
        this.k = k;
        this.v = v;
      }

      /** {@inheritDoc} */
      public Long getKey() {
        return k;
      }

      /** {@inheritDoc} */
      public Long getValue() {
        return v;
      }

      /** {@inheritDoc} */
      public Long setValue(final Long value) {
        return Long2LongHashMap.this.put(k, value.longValue());
      }

      /** {@inheritDoc} */
      @DoNotSub
      public int hashCode() {
        return Long.hashCode(getLongKey()) ^ Long.hashCode(getLongValue());
      }

      /** {@inheritDoc} */
      @DoNotSub
      public boolean equals(final Object o) {
        if (!(o instanceof Map.Entry)) {
          return false;
        }

        final Entry<?, ?> e = (Entry<?, ?>) o;

        return (e.getKey() != null && e.getValue() != null)
            && (e.getKey().equals(k) && e.getValue().equals(v));
      }

      /** {@inheritDoc} */
      public String toString() {
        return k + "=" + v;
      }
    }
  }

  /** Set of keys which supports optional cached iterators to avoid allocation. */
  public final class KeySet extends AbstractSet<Long> {
    private final KeyIterator keyIterator = shouldAvoidAllocation ? new KeyIterator() : null;

    /** Create a new instance. */
    public KeySet() {}

    /** {@inheritDoc} */
    public KeyIterator iterator() {
      KeyIterator keyIterator = this.keyIterator;
      if (null == keyIterator) {
        keyIterator = new KeyIterator();
      }

      keyIterator.reset();

      return keyIterator;
    }

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2LongHashMap.this.size();
    }

    /** {@inheritDoc} */
    public boolean isEmpty() {
      return Long2LongHashMap.this.isEmpty();
    }

    /** {@inheritDoc} */
    public void clear() {
      Long2LongHashMap.this.clear();
    }

    /** {@inheritDoc} */
    public boolean contains(final Object o) {
      return contains((long) o);
    }

    /**
     * Checks if key is contained in the map without boxing.
     *
     * @param key to check.
     * @return {@code true} if key is contained in this map.
     */
    public boolean contains(final long key) {
      return containsKey(key);
    }

    /**
     * Removes all the elements of this collection that satisfy the given predicate.
     *
     * <p>NB: Renamed from removeIf to avoid overloading on parameter types of lambda expression,
     * which doesn't play well with type inference in lambda expressions.
     *
     * @param filter a predicate to apply.
     * @return {@code true} if at least one key was removed.
     */
    public boolean removeIfLong(final LongPredicate filter) {
      boolean removed = false;
      final KeyIterator iterator = iterator();
      while (iterator.hasNext()) {
        if (filter.test(iterator.nextValue())) {
          iterator.remove();
          removed = true;
        }
      }
      return removed;
    }
  }

  /** Collection of values which supports optionally cached iterators to avoid allocation. */
  public final class ValueCollection extends AbstractCollection<Long> {
    private final ValueIterator valueIterator = shouldAvoidAllocation ? new ValueIterator() : null;

    /** Create a new instance. */
    public ValueCollection() {}

    /** {@inheritDoc} */
    public ValueIterator iterator() {
      ValueIterator valueIterator = this.valueIterator;
      if (null == valueIterator) {
        valueIterator = new ValueIterator();
      }

      valueIterator.reset();

      return valueIterator;
    }

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2LongHashMap.this.size();
    }

    /** {@inheritDoc} */
    public boolean contains(final Object o) {
      return contains((long) o);
    }

    /**
     * Checks if the value is contained in the map.
     *
     * @param value to be checked.
     * @return {@code true} if value is contained in this map.
     */
    public boolean contains(final long value) {
      return containsValue(value);
    }

    /**
     * Removes all the elements of this collection that satisfy the given predicate.
     *
     * <p>NB: Renamed from removeIf to avoid overloading on parameter types of lambda expression,
     * which doesn't play well with type inference in lambda expressions.
     *
     * @param filter a predicate to apply.
     * @return {@code true} if at least one value was removed.
     */
    public boolean removeIfLong(final LongPredicate filter) {
      boolean removed = false;
      final ValueIterator iterator = iterator();
      while (iterator.hasNext()) {
        if (filter.test(iterator.nextValue())) {
          iterator.remove();
          removed = true;
        }
      }
      return removed;
    }
  }

  /** Set of entries which supports optionally cached iterators to avoid allocation. */
  public final class EntrySet extends AbstractSet<Entry<Long, Long>> {
    private final EntryIterator entryIterator = shouldAvoidAllocation ? new EntryIterator() : null;

    /** Create a new instance. */
    public EntrySet() {}

    /** {@inheritDoc} */
    public EntryIterator iterator() {
      EntryIterator entryIterator = this.entryIterator;
      if (null == entryIterator) {
        entryIterator = new EntryIterator();
      }

      entryIterator.reset();

      return entryIterator;
    }

    /** {@inheritDoc} */
    @DoNotSub
    public int size() {
      return Long2LongHashMap.this.size();
    }

    /** {@inheritDoc} */
    public boolean isEmpty() {
      return Long2LongHashMap.this.isEmpty();
    }

    /** {@inheritDoc} */
    public void clear() {
      Long2LongHashMap.this.clear();
    }

    /** {@inheritDoc} */
    public boolean contains(final Object o) {
      if (!(o instanceof Entry)) {
        return false;
      }
      final Entry<?, ?> entry = (Entry<?, ?>) o;
      final Long value = get(entry.getKey());

      return value != null && value.equals(entry.getValue());
    }

    /**
     * Removes all the elements of this collection that satisfy the given predicate.
     *
     * <p>NB: Renamed from removeIf to avoid overloading on parameter types of lambda expression,
     * which doesn't play well with type inference in lambda expressions.
     *
     * @param filter a predicate to apply.
     * @return {@code true} if at least one entry was removed.
     */
    public boolean removeIfLong(final LongLongPredicate filter) {
      boolean removed = false;
      final EntryIterator iterator = iterator();
      while (iterator.hasNext()) {
        iterator.findNext();
        if (filter.test(iterator.getLongKey(), iterator.getLongValue())) {
          iterator.remove();
          removed = true;
        }
      }
      return removed;
    }

    /** {@inheritDoc} */
    public Object[] toArray() {
      return toArray(new Object[size()]);
    }

    /** {@inheritDoc} */
    @SuppressWarnings("unchecked")
    public <T> T[] toArray(final T[] a) {
      final T[] array = a.length >= size
          ? a
          : (T[]) java.lang.reflect.Array.newInstance(a.getClass().getComponentType(), size);
      final EntryIterator it = iterator();

      for (@DoNotSub int i = 0; i < array.length; i++) {
        if (it.hasNext()) {
          it.next();
          array[i] = (T) it.allocateDuplicateEntry();
        } else {
          array[i] = null;
          break;
        }
      }

      return array;
    }
  }
}
