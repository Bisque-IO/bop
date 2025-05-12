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

import java.util.AbstractList;
import java.util.Arrays;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Objects;
import java.util.RandomAccess;
import java.util.function.Consumer;
import java.util.function.LongConsumer;
import java.util.function.LongPredicate;
import java.util.stream.LongStream;
import org.agrona.generation.DoNotSub;

/**
 * A {@link List} implementation that stores long values with the ability to not have them boxed.
 */
public class LongArrayList extends AbstractList<Long> implements List<Long>, RandomAccess {
  /** The default value that will be used in place of null for an element. */
  public static final long DEFAULT_NULL_VALUE = Long.MIN_VALUE;

  /** Initial capacity to which the array will be sized. */
  @DoNotSub
  public static final int INITIAL_CAPACITY = 10;

  private final long nullValue;

  @DoNotSub
  private int size = 0;

  private long[] elements;

  /** Constructs a new list with the {@link #INITIAL_CAPACITY} using {@link #DEFAULT_NULL_VALUE}. */
  public LongArrayList() {
    this(INITIAL_CAPACITY, DEFAULT_NULL_VALUE);
  }

  /**
   * Construct a new list.
   *
   * @param initialCapacity for the backing array.
   * @param nullValue to be used to represent a null element.
   */
  public LongArrayList(@DoNotSub final int initialCapacity, final long nullValue) {
    this.nullValue = nullValue;
    elements = new long[Math.max(initialCapacity, INITIAL_CAPACITY)];
  }

  /**
   * Create a new list that wraps an existing arrays without copying it.
   *
   * @param initialElements to be wrapped.
   * @param initialSize of the array to wrap.
   * @param nullValue to be used to represent a null element.
   */
  @SuppressWarnings("this-escape")
  public LongArrayList(
      final long[] initialElements, @DoNotSub final int initialSize, final long nullValue) {
    wrap(initialElements, initialSize);
    this.nullValue = nullValue;
  }

  /**
   * Wrap an existing array without copying it.
   *
   * @param initialElements to be wrapped.
   * @param initialSize of the array to wrap.
   * @throws IllegalArgumentException if the initialSize is less than 0 or greater than the length
   *     of the initial array.
   */
  public void wrap(final long[] initialElements, final @DoNotSub int initialSize) {
    if (initialSize < 0 || initialSize > initialElements.length) {
      throw new IllegalArgumentException(
          "illegal initial size " + initialSize + " for array length of " + initialElements.length);
    }

    elements = initialElements;
    size = initialSize;
  }

  /**
   * The value representing a null element.
   *
   * @return value representing a null element.
   */
  public long nullValue() {
    return nullValue;
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int size() {
    return size;
  }

  /**
   * The current capacity for the collection.
   *
   * @return the current capacity for the collection.
   */
  @DoNotSub
  public int capacity() {
    return elements.length;
  }

  /** {@inheritDoc} */
  public void clear() {
    size = 0;
  }

  /**
   * Trim the underlying array to be the current size, or {@link #INITIAL_CAPACITY} if size is less.
   */
  public void trimToSize() {
    if (elements.length != size && elements.length > INITIAL_CAPACITY) {
      elements = Arrays.copyOf(elements, Math.max(INITIAL_CAPACITY, size));
    }
  }

  /** {@inheritDoc} */
  public Long get(@DoNotSub final int index) {
    final long value = getLong(index);

    return nullValue == value ? null : value;
  }

  /**
   * Get the element at a given index without boxing.
   *
   * @param index to get.
   * @return the unboxed element.
   */
  public long getLong(@DoNotSub final int index) {
    checkIndex(index);

    return elements[index];
  }

  /** {@inheritDoc} */
  public boolean add(final Long element) {
    return addLong(null == element ? nullValue : element);
  }

  /**
   * Add an element without boxing.
   *
   * @param element to be added.
   * @return true
   */
  public boolean addLong(final long element) {
    ensureCapacityPrivate(size + 1);

    elements[size] = element;
    size++;

    return true;
  }

  /** {@inheritDoc} */
  public void add(@DoNotSub final int index, final Long element) {
    addLong(index, null == element ? nullValue : element);
  }

  /**
   * Add an element without boxing at a given index.
   *
   * @param index at which the element should be added.
   * @param element to be added.
   */
  public void addLong(@DoNotSub final int index, final long element) {
    checkIndexForAdd(index);

    @DoNotSub final int requiredSize = size + 1;
    ensureCapacityPrivate(requiredSize);

    if (index < size) {
      System.arraycopy(elements, index, elements, index + 1, size - index);
    }

    elements[index] = element;
    size++;
  }

  /** {@inheritDoc} */
  public Long set(@DoNotSub final int index, final Long element) {
    final long previous = setLong(index, null == element ? nullValue : element);

    return nullValue == previous ? null : previous;
  }

  /**
   * Set an element at a given index without boxing.
   *
   * @param index at which to set the element.
   * @param element to be added.
   * @return the previous element at the index.
   */
  public long setLong(@DoNotSub final int index, final long element) {
    checkIndex(index);

    final long previous = elements[index];
    elements[index] = element;

    return previous;
  }

  /** {@inheritDoc} */
  public boolean contains(final Object o) {
    return containsLong(null == o ? nullValue : (long) o);
  }

  /**
   * Does the list contain this element value.
   *
   * @param value of the element.
   * @return true if present otherwise false.
   */
  public boolean containsLong(final long value) {
    return -1 != indexOf(value);
  }

  /**
   * Index of the first element with this value.
   *
   * @param value for the element.
   * @return the index if found otherwise -1.
   */
  public @DoNotSub int indexOf(final long value) {
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      if (value == elements[i]) {
        return i;
      }
    }

    return -1;
  }

  /**
   * Index of the last element with this value.
   *
   * @param value for the element.
   * @return the index if found otherwise -1.
   */
  public @DoNotSub int lastIndexOf(final long value) {
    final long[] elements = this.elements;
    for (@DoNotSub int i = size - 1; i >= 0; i--) {
      if (value == elements[i]) {
        return i;
      }
    }

    return -1;
  }

  /**
   * Appends all the elements in the specified list to the end of this list, in the order that they
   * are stored in the specified list.
   *
   * @param list containing elements to be added to this list.
   * @return {@code true} if this list changed as a result of the call.
   */
  public boolean addAll(final LongArrayList list) {
    @DoNotSub final int numElements = list.size;
    if (numElements > 0) {
      ensureCapacityPrivate(size + numElements);
      System.arraycopy(list.elements, 0, elements, size, numElements);
      size += numElements;
      return true;
    }
    return false;
  }

  /**
   * Inserts all the elements from the specified list to this list at the specified position. Shifts
   * the element currently at that position (if any) and any subsequent elements to the right
   * (increases their indices). The new elements will appear in this list in the order that they are
   * stored in the specified list.
   *
   * @param index at which to insert the first element from the specified collection.
   * @param list containing elements to be added to this list.
   * @return {@code true} if this list changed as a result of the call.
   */
  public boolean addAll(@DoNotSub final int index, final LongArrayList list) {
    checkIndexForAdd(index);

    @DoNotSub final int numElements = list.size;
    if (numElements > 0) {
      @DoNotSub final int size = this.size;
      ensureCapacityPrivate(size + numElements);
      final long[] elements = this.elements;
      for (@DoNotSub int i = size - 1; i >= index; i--) {
        elements[i + numElements] = elements[i];
      }
      System.arraycopy(list.elements, 0, elements, index, numElements);
      this.size += numElements;
      return true;
    }
    return false;
  }

  /**
   * Returns {@code true} if this list contains all the elements of the specified list.
   *
   * @param list to be checked for containment in this list.
   * @return {@code true} if this list contains all the elements of the specified list.
   */
  public boolean containsAll(final LongArrayList list) {
    final long[] listElements = list.elements;
    final long listNullValue = list.nullValue;
    final boolean hasNulls = contains(null);
    for (@DoNotSub int i = 0, size = list.size; i < size; i++) {
      final long value = listElements[i];
      if (!(containsLong(value) || hasNulls && listNullValue == value)) {
        return false;
      }
    }

    return true;
  }

  /**
   * Retains only the elements in this list that are contained in the specified list. In other
   * words, removes from this list all of its elements that are not contained in the specified list.
   *
   * @param list containing elements to be removed from this list.
   * @return {@code true} if this list changed as a result of the call.
   */
  public boolean retainAll(final LongArrayList list) {
    final long[] elements = this.elements;
    @DoNotSub final int size = this.size;
    if (size > 0) {
      if (list.isEmpty()) {
        this.size = 0;
        return true;
      }

      final long nullValue = this.nullValue;
      final boolean listHasNulls = list.contains(null);
      long[] filteredElements = null;
      @DoNotSub int j = -1;
      for (@DoNotSub int i = 0; i < size; i++) {
        final long value = elements[i];
        if (!(list.containsLong(value) || (listHasNulls && nullValue == value))) {
          if (null == filteredElements) {
            filteredElements = Arrays.copyOf(elements, size);
            j = i - 1;
          }
        } else if (null != filteredElements) {
          filteredElements[++j] = value;
        }
      }

      if (null != filteredElements) {
        this.elements = filteredElements;
        this.size = j + 1;
        return true;
      }
    }
    return false;
  }

  /**
   * Removes all of this collection's elements that are also contained in the specified list. After
   * this call returns, this list will contain no elements in common with the specified list.
   *
   * @param list whose elements are to be removed from this list.
   * @return {@code true} if this list changed as a result of the call.
   */
  public boolean removeAll(final LongArrayList list) {
    final long[] elements = this.elements;
    @DoNotSub final int size = this.size;
    if (size > 0 && !list.isEmpty()) {
      final long nullValue = this.nullValue;
      final boolean listHasNulls = list.contains(null);
      long[] filteredElements = null;
      @DoNotSub int j = -1;
      for (@DoNotSub int i = 0; i < size; i++) {
        final long value = elements[i];
        if (list.containsLong(value) || (listHasNulls && nullValue == value)) {
          if (null == filteredElements) {
            filteredElements = Arrays.copyOf(elements, size);
            j = i - 1;
          }
        } else if (null != filteredElements) {
          filteredElements[++j] = value;
        }
      }

      if (null != filteredElements) {
        this.elements = filteredElements;
        this.size = j + 1;
        return true;
      }
    }
    return false;
  }

  /**
   * Removes all the elements of this collection that satisfy the given predicate.
   *
   * @param filter a predicate which returns true for elements to be removed.
   * @return {@code true} if any elements were removed.
   */
  public boolean removeIfLong(final LongPredicate filter) {
    requireNonNull(filter);
    final long[] elements = this.elements;
    @DoNotSub final int size = this.size;
    if (size > 0) {
      long[] filteredElements = null;
      @DoNotSub int j = -1;
      for (@DoNotSub int i = 0; i < size; i++) {
        final long value = elements[i];
        if (filter.test(value)) {
          if (null == filteredElements) {
            filteredElements = Arrays.copyOf(elements, size);
            j = i - 1;
          }
        } else if (null != filteredElements) {
          filteredElements[++j] = value;
        }
      }

      if (null != filteredElements) {
        this.elements = filteredElements;
        this.size = j + 1;
        return true;
      }
    }
    return false;
  }

  /** {@inheritDoc} */
  public boolean remove(final Object o) {
    return removeLong(null == o ? nullValue : (long) o);
  }

  /**
   * Remove at a given index.
   *
   * @param index of the element to be removed.
   * @return the existing value at this index.
   */
  public Long remove(@DoNotSub final int index) {
    final long value = removeAt(index);
    return nullValue == value ? null : value;
  }

  /**
   * Remove at a given index.
   *
   * @param index of the element to be removed.
   * @return the existing value at this index.
   */
  public long removeAt(@DoNotSub final int index) {
    checkIndex(index);

    final long value = elements[index];

    @DoNotSub final int moveCount = size - index - 1;
    if (moveCount > 0) {
      System.arraycopy(elements, index + 1, elements, index, moveCount);
    }

    size--;

    return value;
  }

  /**
   * Removes element at index, but instead of copying all elements to the left, it replaces the item
   * in the slot with the last item in the list. This avoids the copy costs at the expense of
   * preserving list order. If index is the last element it is just removed.
   *
   * @param index of the element to be removed.
   * @return the existing value at this index.
   * @throws IndexOutOfBoundsException if index is out of bounds.
   */
  public long fastUnorderedRemove(@DoNotSub final int index) {
    checkIndex(index);

    final long value = elements[index];
    elements[index] = elements[--size];

    return value;
  }

  /**
   * Remove the first instance of a value if found in the list.
   *
   * <p>Primitive specialization of the {@link List#remove(Object)} method.
   *
   * @param value to be removed.
   * @return true if successful otherwise false.
   */
  public boolean removeLong(final long value) {
    @DoNotSub final int index = indexOf(value);
    if (-1 != index) {
      removeAt(index);

      return true;
    }

    return false;
  }

  /**
   * Remove the first instance of a value if found in the list and replaces it with the last item in
   * the list. This saves a copy down of all items at the expense of not preserving list order.
   *
   * @param value to be removed.
   * @return true if successful otherwise false.
   */
  public boolean fastUnorderedRemoveLong(final long value) {
    @DoNotSub final int index = indexOf(value);
    if (-1 != index) {
      elements[index] = elements[--size];

      return true;
    }

    return false;
  }

  /**
   * Push an element onto the end of the array like a stack.
   *
   * @param element to be pushed onto the end of the array.
   */
  public void pushLong(final long element) {
    ensureCapacityPrivate(size + 1);

    elements[size] = element;
    size++;
  }

  /**
   * Pop a value off the end of the array as a stack operation.
   *
   * @return the value at the end of the array.
   * @throws NoSuchElementException if the array is empty.
   */
  public long popLong() {
    if (isEmpty()) {
      throw new NoSuchElementException();
    }

    return elements[--size];
  }

  /**
   * For each element in order provide the long value to a {@link LongConsumer}.
   *
   * @param action to be taken for each element.
   */
  public void forEachOrderedLong(final LongConsumer action) {
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      action.accept(elements[i]);
    }
  }

  /**
   * Create a {@link LongStream} over the elements of underlying array.
   *
   * @return a {@link LongStream} over the elements of underlying array.
   */
  public LongStream longStream() {
    return Arrays.stream(elements, 0, size);
  }

  /**
   * Create a new array that is a copy of the elements.
   *
   * @return a copy of the elements.
   */
  public long[] toLongArray() {
    return Arrays.copyOf(elements, size);
  }

  /**
   * Create a new array that is a copy of the elements.
   *
   * @param dst destination array for the copy if it is the correct size.
   * @return a copy of the elements.
   */
  public long[] toLongArray(final long[] dst) {
    if (dst.length == size) {
      System.arraycopy(elements, 0, dst, 0, dst.length);
      return dst;
    } else {
      return Arrays.copyOf(elements, size);
    }
  }

  /**
   * Ensure the backing array has a required capacity.
   *
   * @param requiredCapacity for the backing array.
   */
  public void ensureCapacity(@DoNotSub final int requiredCapacity) {
    ensureCapacityPrivate(Math.max(requiredCapacity, INITIAL_CAPACITY));
  }

  /**
   * Type-safe overload of the {@link #equals(Object)} method.
   *
   * @param that other list.
   * @return {@code true} if lists are equal.
   */
  public boolean equals(final LongArrayList that) {
    if (that == this) {
      return true;
    }

    boolean isEqual = false;

    @DoNotSub final int size = this.size;
    if (size == that.size) {
      isEqual = true;

      final long[] elements = this.elements;
      final long[] thatElements = that.elements;
      for (@DoNotSub int i = 0; i < size; i++) {
        final long thisValue = elements[i];
        final long thatValue = thatElements[i];

        if (thisValue != thatValue) {
          if (thisValue != this.nullValue || thatValue != that.nullValue) {
            isEqual = false;
            break;
          }
        }
      }
    }

    return isEqual;
  }

  /** {@inheritDoc} */
  public boolean equals(final Object other) {
    if (other == this) {
      return true;
    }

    boolean isEqual = false;

    if (other instanceof LongArrayList) {
      return equals((LongArrayList) other);
    } else if (other instanceof List) {
      final List<?> that = (List<?>) other;

      if (size == that.size()) {
        isEqual = true;
        @DoNotSub int i = 0;

        for (final Object o : that) {
          if (null == o || o instanceof Long) {
            final Long thisValue = get(i++);
            final Long thatValue = (Long) o;

            if (Objects.equals(thisValue, thatValue)) {
              continue;
            }
          }

          isEqual = false;
          break;
        }
      }
    }

    return isEqual;
  }

  /** {@inheritDoc} */
  @DoNotSub
  public int hashCode() {
    @DoNotSub int hashCode = 1;
    final long nullValue = this.nullValue;
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      final long value = elements[i];
      hashCode = 31 * hashCode + (nullValue == value ? 0 : Long.hashCode(value));
    }

    return hashCode;
  }

  /** {@inheritDoc} */
  public void forEach(final Consumer<? super Long> action) {
    requireNonNull(action);
    final long nullValue = this.nullValue;
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      final long value = elements[i];
      action.accept(nullValue != value ? value : null);
    }
  }

  /**
   * Iterate over the collection without boxing.
   *
   * @param action to be taken for each element.
   */
  public void forEachLong(final LongConsumer action) {
    requireNonNull(action);
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      action.accept(elements[i]);
    }
  }

  /** {@inheritDoc} */
  public String toString() {
    final StringBuilder sb = new StringBuilder();
    sb.append('[');

    final long nullValue = this.nullValue;
    final long[] elements = this.elements;
    for (@DoNotSub int i = 0, size = this.size; i < size; i++) {
      final long value = elements[i];
      sb.append(value != nullValue ? value : null).append(", ");
    }

    if (sb.length() > 1) {
      sb.setLength(sb.length() - 2);
    }

    sb.append(']');

    return sb.toString();
  }

  private void ensureCapacityPrivate(@DoNotSub final int requiredCapacity) {
    @DoNotSub final int currentCapacity = elements.length;
    if (requiredCapacity > currentCapacity) {
      if (requiredCapacity > ArrayUtil.MAX_CAPACITY) {
        throw new IllegalStateException("max capacity: " + ArrayUtil.MAX_CAPACITY);
      }

      @DoNotSub
      int newCapacity = currentCapacity > INITIAL_CAPACITY ? currentCapacity : INITIAL_CAPACITY;

      while (newCapacity < requiredCapacity) {
        newCapacity = newCapacity + (newCapacity >> 1);

        if (newCapacity < 0 || newCapacity >= ArrayUtil.MAX_CAPACITY) {
          newCapacity = ArrayUtil.MAX_CAPACITY;
        }
      }

      final long[] newElements = new long[newCapacity];
      System.arraycopy(elements, 0, newElements, 0, currentCapacity);
      elements = newElements;
    }
  }

  private void checkIndex(@DoNotSub final int index) {
    if (index >= size || index < 0) {
      throw new IndexOutOfBoundsException("index=" + index + " size=" + size);
    }
  }

  private void checkIndexForAdd(@DoNotSub final int index) {
    if (index > size || index < 0) {
      throw new IndexOutOfBoundsException("index=" + index + " size=" + size);
    }
  }
}
