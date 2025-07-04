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
package org.agrona.concurrent.errors;

import static org.agrona.BitUtil.SIZE_OF_INT;
import static org.agrona.BitUtil.align;
import static org.agrona.concurrent.errors.DistinctErrorLog.*;

import org.agrona.concurrent.AtomicBuffer;

/**
 * Reader for the log created by a {@link org.agrona.concurrent.errors.DistinctErrorLog} encoded as
 * UTF-8 errors.
 *
 * <p>The read methods are thread safe.
 */
public final class ErrorLogReader {
  private ErrorLogReader() {}

  /**
   * Has the error buffer any recorded errors?
   *
   * @param buffer containing the {@link org.agrona.concurrent.errors.DistinctErrorLog}.
   * @return true if there is at least one error.
   */
  public static boolean hasErrors(final AtomicBuffer buffer) {
    return buffer.capacity() >= SIZE_OF_INT && buffer.getIntVolatile(LENGTH_OFFSET) > 0;
  }

  /**
   * Read all the errors in a log since the creation of the log.
   *
   * @param buffer containing the {@link org.agrona.concurrent.errors.DistinctErrorLog}.
   * @param consumer to be called for each exception encountered.
   * @return the number of entries that has been read.
   */
  public static int read(
      final AtomicBuffer buffer, final org.agrona.concurrent.errors.ErrorConsumer consumer) {
    return read(buffer, consumer, 0);
  }

  /**
   * Read all the errors in a log since a given timestamp.
   *
   * @param buffer containing the {@link DistinctErrorLog}.
   * @param consumer to be called for each exception encountered.
   * @param sinceTimestamp for filtering errors that have been recorded since this time.
   * @return the number of entries that has been read.
   */
  public static int read(
      final AtomicBuffer buffer, final ErrorConsumer consumer, final long sinceTimestamp) {
    int entries = 0;
    int offset = 0;
    final int capacity = buffer.capacity();

    while (offset <= capacity - ENCODED_ERROR_OFFSET) {
      final int length = Math.min(buffer.getIntVolatile(offset + LENGTH_OFFSET), capacity - offset);
      if (length <= 0) {
        break;
      }

      final long lastObservationTimestamp =
          buffer.getLongVolatile(offset + LAST_OBSERVATION_TIMESTAMP_OFFSET);
      if (lastObservationTimestamp >= sinceTimestamp) {
        ++entries;

        consumer.accept(
            buffer.getInt(offset + OBSERVATION_COUNT_OFFSET),
            buffer.getLong(offset + FIRST_OBSERVATION_TIMESTAMP_OFFSET),
            lastObservationTimestamp,
            buffer.getStringWithoutLengthUtf8(
                offset + ENCODED_ERROR_OFFSET, length - ENCODED_ERROR_OFFSET));
      }

      offset += align(length, RECORD_ALIGNMENT);
    }

    return entries;
  }
}
