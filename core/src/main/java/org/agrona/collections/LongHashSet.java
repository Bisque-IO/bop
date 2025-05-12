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

import static org.agrona.BitUtil.findNextPositivePowerOfTwo;
import static org.agrona.collections.CollectionUtil.validateLoadFactor;

import java.lang.reflect.Array;
import java.util.AbstractSet;
import java.util.Arrays;
import java.util.Collection;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.Set;
import java.util.function.LongConsumer;
import java.util.function.LongPredicate;
import java.util.function.Predicate;
import org.agrona.generation.DoNotSub;

/**
 * Open-addressing with linear-probing expandable hash set. Allocation free in steady state use when
 * expanded.
 *
 * <p>By storing elements as long primitives this significantly reduces memory consumption compared
 * with Java's builtin <code>HashSet&lt;Long&gt;</code>. It implements <code>Set&lt;Long&gt;</code>
 * for convenience, but calling functionality via those methods can add boxing overhead to your
 * usage.
 *
 * <p>This class is not Threadsafe.
 *
 * <p>This HashSet caches its iterator object by default, so nested iteration is not supported. You
 * can override this behaviour at construction by indicating that the iterator should not be cached.
 *
 * @see LongIterator
 * @see Set
 */
public class LongHashSet extends AbstractSet<Long> {
  /** The initial capacity used when none is specified in the constructor. */
  @DoNotSub
  public static final int DEFAULT_INITIAL_CAPACITY = 8;

  static final long MISSING_VALUE = 0;

  private final boolean shouldAvoidAllocation;
  private boolean containsMissingValue;
  private final float loadFactor;

  @DoNotSub
  private int resizeThreshold;
  // NB: excludes missing value
  @DoNotSub
  private int sizeOfArrayValues;

  private long[] values;
  private LongIterator iterator;

  /**
   * Construct a hash set with {@link #DEFAULT_INITIAL_CAPACITY},
   * {@link Hashing#DEFAULT_LOAD_FACTOR}, iterator caching support and {@code 0} as a missing value.
   */
  public LongHashSet() {
    this(DEFAULT_INITIAL_CAPACITY);
  }

  /**
   * Construct a hash set with a proposed capacity, {@link Hashing#DEFAULT_LOAD_FACTOR}, iterator
   * caching support and {@code 0} as a missing value.
   *
   * @param proposedCapacity for the initial capacity of the set.
   */
  public LongHashSet(@DoNotSub final int proposedCapacity) {
    this(proposedCapacity, Hashing.DEFAULT_LOAD_FACTOR, true);
  }

  /**
   * Construct a hash set with a proposed initial capacity, load factor, iterator caching support
   * and {@code 0} as a missing value.
   *
   * @param proposedCapacity for the initial capacity of the set.
   * @param loadFactor to be used for resizing.
   */
  public LongHashSet(@DoNotSub final int proposedCapacity, final float loadFactor) {
    this(proposedCapacity, loadFactor, true);
  }

  /**
   * Construct a hash set with a proposed initial capacity, load factor, iterator caching support
   * and {@code -1} as a missing value.
   *
   * @param proposedCapacity for the initial capacity of the set.
   * @param loadFactor to be used for resizing.
   * @param shouldAvoidAllocation should the iterator be cached to avoid further allocation.
   */
  public LongHashSet(
      @DoNotSub final int proposedCapacity,
      final float loadFactor,
      final boolean shouldAvoidAllocation) {
    validateLoadFactor(loadFactor);

    this.shouldAvoidAllocation = shouldAvoidAllocation;
    this.loadFactor = loadFactor;
    sizeOfArrayValues = 0;
    @DoNotSub
    final int capacity =
        findNextPositivePowerOfTwo(Math.max(DEFAULT_INITIAL_CAPACITY, proposedCapacity));
    resizeThreshold = (int) (capacity * loadFactor); // @DoNotSub
    values = new long[capacity];
  }

  /**
   * Get the load factor beyond which the set will increase size.
   *
   * @return load factor for when the set should increase size.
   */
  public float loadFactor() {
    return loadFactor;
  }

  /**
   * Get the total capacity for the set to which the load factor with be a fraction of.
   *
   * @return the total capacity for the set.
   */
  @DoNotSub
  public int capacity() {
    return values.length;
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
  public boolean add(final Long value) {
    return add(value.longValue());
  }

  /**
   * Primitive specialised overload of {this#add(Long)}.
   *
   * @param value the value to add.
   * @return true if the collection has changed, false otherwise.
   */
  public boolean add(final long value) {
    if (MISSING_VALUE == value) {
      final boolean previousContainsMissingValue = this.containsMissingValue;
      containsMissingValue = true;
      return !previousContainsMissingValue;
    }

    final long[] values = this.values;
    @DoNotSub final int mask = values.length - 1;
    @DoNotSub int index = Hashing.hash(value, mask);

    long oldValue;
    while (MISSING_VALUE != (oldValue = values[index])) {
      if (oldValue == value) {
        return false;
      }

      index = next(index, mask);
    }

    values[index] = value;
    sizeOfArrayValues++;

    if (sizeOfArrayValues > resizeThreshold) {
      increaseCapacity();
    }

    return true;
  }

  private void increaseCapacity() {
    @DoNotSub final int newCapacity = values.length * 2;
    if (newCapacity < 0) {
      throw new IllegalStateException("max capacity reached at size=" + size());
    }

    rehash(newCapacity);
  }

  private void rehash(@DoNotSub final int newCapacity) {
    @DoNotSub final int capacity = newCapacity;
    @DoNotSub final int mask = newCapacity - 1;
    /* @DoNotSub */ resizeThreshold = (int) (newCapacity * loadFactor);

    final long[] tempValues = new long[capacity];
    final long[] values = this.values;
    for (final long value : values) {
      if (MISSING_VALUE != value) {
        @DoNotSub int newHash = Hashing.hash(value, mask);
        while (MISSING_VALUE != tempValues[newHash]) {
          newHash = ++newHash & mask;
        }

        tempValues[newHash] = value;
      }
    }

    this.values = tempValues;
  }

  /** {@inheritDoc} */
  public boolean remove(final Object value) {
    return remove((long) value);
  }

  /**
   * Specialised version of {this#remove(Object)} for long.
   *
   * @param value the value to remove.
   * @return true if the value was present, false otherwise.
   */
  public boolean remove(final long value) {
    if (MISSING_VALUE == value) {
      final boolean previousContainsMissingValue = this.containsMissingValue;
      containsMissingValue = false;
      return previousContainsMissingValue;
    }

    final long[] values = this.values;
    @DoNotSub final int mask = values.length - 1;
    @DoNotSub int index = Hashing.hash(value, mask);

    long oldValue;
    while (MISSING_VALUE != (oldValue = values[index])) {
      if (oldValue == value) {
        values[index] = MISSING_VALUE;
        compactChain(index);
        sizeOfArrayValues--;
        return true;
      }

      index = next(index, mask);
    }

    return false;
  }

  @DoNotSub
  private static int next(final int index, final int mask) {
    return (index + 1) & mask;
  }

  @SuppressWarnings("FinalParameters")
  @DoNotSub
  void compactChain(int deleteIndex) {
    final long[] values = this.values;
    @DoNotSub final int mask = values.length - 1;

    @DoNotSub int index = deleteIndex;
    while (true) {
      index = next(index, mask);
      final long value = values[index];
      if (MISSING_VALUE == value) {
        return;
      }

      @DoNotSub final int hash = Hashing.hash(value, mask);

      if ((index < hash && (hash <= deleteIndex || deleteIndex <= index))
          || (hash <= deleteIndex && deleteIndex <= index)) {
        values[deleteIndex] = value;

        values[index] = MISSING_VALUE;
        deleteIndex = index;
      }
    }
  }

  /**
   * Compact the backing arrays by rehashing with a capacity just larger than current size and
   * giving consideration to the load factor.
   */
  public void compact() {
    @DoNotSub final int idealCapacity = (int) Math.round(size() * (1.0 / loadFactor));
    rehash(findNextPositivePowerOfTwo(Math.max(DEFAULT_INITIAL_CAPACITY, idealCapacity)));
  }

  /** {@inheritDoc} */
  public boolean contains(final Object value) {
    return contains((long) value);
  }

  /**
   * Contains method that does not box values.
   *
   * @param value to be checked for if the set contains it.
   * @return true if the value is contained in the set otherwise false.
   * @see Collection#contains(Object)
   */
  public boolean contains(final long value) {
    if (MISSING_VALUE == value) {
      return containsMissingValue;
    }

    @DoNotSub final int mask = values.length - 1;
    @DoNotSub int index = Hashing.hash(value, mask);

    long existingValue;
    while (MISSING_VALUE != (existingValue = values[index])) {
      if (existingValue == value) {
        return true;
      }

      index = next(index, mask);
    }

    return false;
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int size() {
    return sizeOfArrayValues + (containsMissingValue ? 1 : 0);
  }

  /** {@inheritDoc} */
  public boolean isEmpty() {
    return size() == 0;
  }

  /** {@inheritDoc} */
  public void clear() {
    if (size() > 0) {
      Arrays.fill(values, MISSING_VALUE);
      sizeOfArrayValues = 0;
      containsMissingValue = false;
    }
  }

  /** {@inheritDoc} */
  public boolean addAll(final Collection<? extends Long> coll) {
    boolean added = false;

    for (final Long value : coll) {
      added |= add(value);
    }

    return added;
  }

  /**
   * Alias for {@link #addAll(Collection)} for the specialized case when adding another LongHashSet,
   * avoids boxing and allocations.
   *
   * @param coll containing the values to be added.
   * @return {@code true} if this set changed as a result of the call.
   */
  public boolean addAll(final LongHashSet coll) {
    boolean acc = false;

    for (final long value : coll.values) {
      if (MISSING_VALUE != value) {
        acc |= add(value);
      }
    }

    if (coll.containsMissingValue) {
      acc |= add(MISSING_VALUE);
    }

    return acc;
  }

  /**
   * LongHashSet specialised variant of {this#containsAll(Collection)}.
   *
   * @param coll long hash set to compare against.
   * @return true if every element in other is in this.
   */
  public boolean containsAll(final LongHashSet coll) {
    for (final long value : coll.values) {
      if (MISSING_VALUE != value && !contains(value)) {
        return false;
      }
    }

    return containsMissingValue || !coll.containsMissingValue;
  }

  /**
   * Fast Path set difference for comparison with another LongHashSet.
   *
   * <p><b>Note:</b> garbage free in the identical case, allocates otherwise.
   *
   * @param other the other set to subtract.
   * @return null if identical, otherwise the set of differences.
   */
  public LongHashSet difference(final LongHashSet other) {
    LongHashSet difference = null;

    final long[] values = this.values;
    for (final long value : values) {
      if (MISSING_VALUE != value && !other.contains(value)) {
        if (null == difference) {
          difference = new LongHashSet();
        }

        difference.add(value);
      }
    }

    if (containsMissingValue && !other.containsMissingValue) {
      if (null == difference) {
        difference = new LongHashSet();
      }

      difference.add(MISSING_VALUE);
    }

    return difference;
  }

  /** {@inheritDoc} */
  public boolean removeIf(final Predicate<? super Long> filter) {
    return super.removeIf(filter);
  }

  /**
   * Removes all the elements of this collection that satisfy the given predicate.
   *
   * <p>NB: Renamed from removeIf to avoid overloading on parameter types of lambda expression,
   * which doesn't play well with type inference in lambda expressions.
   *
   * @param filter which returns {@code true} for elements to be removed.
   * @return {@code true} if any elements were removed.
   */
  public boolean removeIfLong(final LongPredicate filter) {
    boolean removed = false;
    final LongIterator iterator = iterator();
    while (iterator.hasNext()) {
      if (filter.test(iterator.nextValue())) {
        iterator.remove();
        removed = true;
      }
    }

    return removed;
  }

  /** {@inheritDoc} */
  public boolean removeAll(final Collection<?> coll) {
    boolean removed = false;

    for (final Object value : coll) {
      removed |= remove(value);
    }

    return removed;
  }

  /**
   * Alias for {@link #removeAll(Collection)} for the specialized case when removing another
   * LongHashSet, avoids boxing and allocations.
   *
   * @param coll containing the values to be removed.
   * @return {@code true} if this set changed as a result of the call.
   */
  public boolean removeAll(final LongHashSet coll) {
    boolean removed = false;

    for (final long value : coll.values) {
      if (MISSING_VALUE != value) {
        removed |= remove(value);
      }
    }

    if (coll.containsMissingValue) {
      removed |= remove(MISSING_VALUE);
    }

    return removed;
  }

  /** {@inheritDoc} */
  public boolean retainAll(final Collection<?> coll) {
    boolean removed = false;
    final long[] values = this.values;
    @DoNotSub final int length = values.length;
    @DoNotSub int i = 0;
    for (; i < length; i++) {
      final long value = values[i];
      if (MISSING_VALUE != value && !coll.contains(value)) {
        values[i] = MISSING_VALUE;
        sizeOfArrayValues--;
        removed = true;
      }
    }

    if (removed && sizeOfArrayValues > 0) {
      @DoNotSub
      final int newCapacity =
          Math.max(DEFAULT_INITIAL_CAPACITY, findNextPositivePowerOfTwo(sizeOfArrayValues));
      rehash(newCapacity);
    }

    if (containsMissingValue && !coll.contains(MISSING_VALUE)) {
      containsMissingValue = false;
      removed = true;
    }
    return removed;
  }

  /**
   * Alias for {@link #retainAll(Collection)} for the specialized case when retaining on another
   * LongHashSet, avoids boxing and allocations.
   *
   * @param coll containing elements to be retained in this set.
   * @return {@code true} if this set changed as a result of the call.
   */
  public boolean retainAll(final LongHashSet coll) {
    boolean removed = false;
    @DoNotSub final int length = values.length;
    @DoNotSub int i = 0;
    for (; i < length; i++) {
      final long value = values[i];
      if (MISSING_VALUE != value && !coll.contains(value)) {
        values[i] = MISSING_VALUE;
        sizeOfArrayValues--;
        removed = true;
      }
    }

    if (removed && sizeOfArrayValues > 0) {
      @DoNotSub
      final int newCapacity =
          Math.max(DEFAULT_INITIAL_CAPACITY, findNextPositivePowerOfTwo(sizeOfArrayValues));
      rehash(newCapacity);
    }

    if (containsMissingValue && !coll.contains(MISSING_VALUE)) {
      containsMissingValue = false;
      removed = true;
    }
    return removed;
  }

  /** {@inheritDoc} */
  public LongIterator iterator() {
    LongIterator iterator = this.iterator;
    if (null == iterator) {
      iterator = new LongIterator();
      if (shouldAvoidAllocation) {
        this.iterator = iterator;
      }
    }

    return iterator.reset();
  }

  /**
   * Iterate over the collection without boxing.
   *
   * @param action to be taken for each element.
   */
  public void forEachLong(final LongConsumer action) {
    if (sizeOfArrayValues > 0) {
      final long[] values = this.values;
      for (final long v : values) {
        if (MISSING_VALUE != v) {
          action.accept(v);
        }
      }
    }
    if (containsMissingValue) {
      action.accept(MISSING_VALUE);
    }
  }

  /**
   * Copy values from another {@link LongHashSet} longo this one.
   *
   * @param that set to copy values from.
   */
  public void copy(final LongHashSet that) {
    if (values.length != that.values.length) {
      throw new IllegalArgumentException("cannot copy object: masks not equal");
    }

    System.arraycopy(that.values, 0, values, 0, values.length);
    this.sizeOfArrayValues = that.sizeOfArrayValues;
    this.containsMissingValue = that.containsMissingValue;
  }

  /** {@inheritDoc} */
  public String toString() {
    final StringBuilder sb = new StringBuilder();
    sb.append('{');

    final long[] values = this.values;
    for (final long value : values) {
      if (MISSING_VALUE != value) {
        sb.append(value).append(", ");
      }
    }

    if (containsMissingValue) {
      sb.append(MISSING_VALUE).append(", ");
    }

    if (sb.length() > 1) {
      sb.setLength(sb.length() - 2);
    }

    sb.append('}');

    return sb.toString();
  }

  /** {@inheritDoc} */
  @SuppressWarnings("unchecked")
  public <T> T[] toArray(final T[] a) {
    final Class<?> componentType = a.getClass().getComponentType();
    if (!componentType.isAssignableFrom(Long.class)) {
      throw new ArrayStoreException("cannot store Longs in array of type " + componentType);
    }

    @DoNotSub final int size = size();
    final T[] arrayCopy = a.length >= size ? a : (T[]) Array.newInstance(componentType, size);
    copyValues(arrayCopy);

    return arrayCopy;
  }

  /** {@inheritDoc} */
  public Object[] toArray() {
    final Object[] arrayCopy = new Object[size()];
    copyValues(arrayCopy);

    return arrayCopy;
  }

  private void copyValues(final Object[] arrayCopy) {
    @DoNotSub int i = 0;
    final long[] values = this.values;
    for (final long value : values) {
      if (MISSING_VALUE != value) {
        arrayCopy[i++] = value;
      }
    }

    if (containsMissingValue) {
      arrayCopy[sizeOfArrayValues] = MISSING_VALUE;
    }
  }

  /** {@inheritDoc} */
  public boolean equals(final Object other) {
    if (other == this) {
      return true;
    }

    if (other instanceof LongHashSet) {
      final LongHashSet otherSet = (LongHashSet) other;

      return otherSet.containsMissingValue == containsMissingValue
          && otherSet.sizeOfArrayValues == sizeOfArrayValues
          && containsAll(otherSet);
    }

    if (!(other instanceof Set)) {
      return false;
    }

    final Set<?> c = (Set<?>) other;
    if (c.size() != size()) {
      return false;
    }

    try {
      return containsAll(c);
    } catch (final @DoNotSub ClassCastException | NullPointerException ignore) {
      return false;
    }
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int hashCode() {
    @DoNotSub int hashCode = 0;
    for (final long value : values) {
      if (MISSING_VALUE != value) {
        hashCode += Long.hashCode(value);
      }
    }

    if (containsMissingValue) {
      hashCode += Long.hashCode(MISSING_VALUE);
    }

    return hashCode;
  }

  /** Iterator which supports unboxed access to the values via {@link #nextValue()}. */
  public final class LongIterator implements Iterator<Long> {
    @DoNotSub
    private int remaining;

    @DoNotSub
    private int positionCounter;

    @DoNotSub
    private int stopCounter;

    private boolean isPositionValid = false;

    /** Create a new instance. */
    public LongIterator() {}

    LongIterator reset() {
      remaining = size();

      final long[] values = LongHashSet.this.values;
      @DoNotSub final int length = values.length;
      @DoNotSub int i = length;

      if (MISSING_VALUE != values[length - 1]) {
        for (i = 0; i < length; i++) {
          if (MISSING_VALUE == values[i]) {
            break;
          }
        }
      }

      stopCounter = i;
      positionCounter = i + length;
      isPositionValid = false;

      return this;
    }

    /** {@inheritDoc} */
    public boolean hasNext() {
      return remaining > 0;
    }

    /**
     * Returns number of remaining (not yet visited) elements.
     *
     * @return number of remaining elements.
     */
    @DoNotSub
    public int remaining() {
      return remaining;
    }

    /** {@inheritDoc} */
    public Long next() {
      return nextValue();
    }

    /**
     * Strongly typed alternative of {@link Iterator#next()} to avoid boxing.
     *
     * @return the next long value.
     */
    public long nextValue() {
      if (remaining == 1 && containsMissingValue) {
        remaining = 0;
        isPositionValid = true;

        return MISSING_VALUE;
      }

      findNext();

      final long[] values = LongHashSet.this.values;

      return values[position(values)];
    }

    /** {@inheritDoc} */
    public void remove() {
      if (isPositionValid) {
        if (0 == remaining && containsMissingValue) {
          containsMissingValue = false;
        } else {
          final long[] values = LongHashSet.this.values;
          @DoNotSub final int position = position(values);
          values[position] = MISSING_VALUE;
          --sizeOfArrayValues;

          compactChain(position);
        }

        isPositionValid = false;
      } else {
        throw new IllegalStateException();
      }
    }

    private void findNext() {
      final long[] values = LongHashSet.this.values;
      @DoNotSub final int mask = values.length - 1;
      isPositionValid = true;

      for (@DoNotSub int i = positionCounter - 1, stop = stopCounter; i >= stop; i--) {
        @DoNotSub final int index = i & mask;
        if (MISSING_VALUE != values[index]) {
          positionCounter = i;
          --remaining;
          return;
        }
      }

      isPositionValid = false;
      throw new NoSuchElementException();
    }

    @DoNotSub
    private int position(final long[] values) {
      return positionCounter & (values.length - 1);
    }
  }
}
