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
package org.agrona.concurrent.ringbuffer;

import static java.lang.Math.max;
import static org.agrona.BitUtil.align;
import static org.agrona.concurrent.ControlledMessageHandler.Action.*;
import static org.agrona.concurrent.ringbuffer.RecordDescriptor.*;
import static org.agrona.concurrent.ringbuffer.RingBufferDescriptor.*;

import java.lang.invoke.VarHandle;
import org.agrona.DirectBuffer;
import org.agrona.concurrent.AtomicBuffer;
import org.agrona.concurrent.ControlledMessageHandler;
import org.agrona.concurrent.MessageHandler;

/**
 * A ring-buffer that supports the exchange of messages from a single producer to a single consumer.
 *
 * <p>This single producer ring-buffer can be used in combination a consumer using the
 * {@link ManyToOneRingBuffer} provided no other producers are accessing the same ring-buffer.
 */
public final class OneToOneRingBuffer implements RingBuffer {
  /**
   * Minimal required capacity of the ring buffer excluding
   * {@link org.agrona.concurrent.ringbuffer.RingBufferDescriptor#TRAILER_LENGTH}.
   */
  public static final int MIN_CAPACITY = HEADER_LENGTH * 2;

  private final int capacity;
  private final int maxMsgLength;
  private final int tailPositionIndex;
  private final int headCachePositionIndex;
  private final int headPositionIndex;
  private final int correlationIdCounterIndex;
  private final int consumerHeartbeatIndex;
  private final AtomicBuffer buffer;

  /**
   * Construct a new {@link RingBuffer} based on an underlying {@link AtomicBuffer}. The underlying
   * buffer must a power of 2 in size plus sufficient space for the
   * {@link org.agrona.concurrent.ringbuffer.RingBufferDescriptor#TRAILER_LENGTH}.
   *
   * @param buffer via which events will be exchanged.
   * @throws IllegalArgumentException if the buffer capacity is not a power of 2 plus
   *     {@link RingBufferDescriptor#TRAILER_LENGTH} or if capacity is less than
   *     {@link #MIN_CAPACITY}.
   */
  public OneToOneRingBuffer(final AtomicBuffer buffer) {
    capacity = checkCapacity(buffer.capacity(), MIN_CAPACITY);

    buffer.verifyAlignment();

    this.buffer = buffer;
    maxMsgLength = MIN_CAPACITY == capacity ? 0 : max(HEADER_LENGTH, capacity >> 3);
    tailPositionIndex = capacity + TAIL_POSITION_OFFSET;
    headCachePositionIndex = capacity + HEAD_CACHE_POSITION_OFFSET;
    headPositionIndex = capacity + HEAD_POSITION_OFFSET;
    correlationIdCounterIndex = capacity + CORRELATION_COUNTER_OFFSET;
    consumerHeartbeatIndex = capacity + CONSUMER_HEARTBEAT_OFFSET;
  }

  /** {@inheritDoc} */
  public int capacity() {
    return capacity;
  }

  /** {@inheritDoc} */
  public boolean write(
      final int msgTypeId, final DirectBuffer srcBuffer, final int offset, final int length) {
    checkTypeId(msgTypeId);
    checkMsgLength(length);

    final AtomicBuffer buffer = this.buffer;
    final int recordLength = length + HEADER_LENGTH;
    final int recordIndex = claimCapacity(buffer, recordLength);

    if (INSUFFICIENT_CAPACITY == recordIndex) {
      return false;
    }

    buffer.putIntRelease(lengthOffset(recordIndex), -recordLength);
    VarHandle.releaseFence();

    buffer.putBytes(encodedMsgOffset(recordIndex), srcBuffer, offset, length);
    buffer.putInt(typeOffset(recordIndex), msgTypeId);
    buffer.putIntRelease(lengthOffset(recordIndex), recordLength);

    return true;
  }

  /** {@inheritDoc} */
  public int tryClaim(final int msgTypeId, final int length) {
    checkTypeId(msgTypeId);
    checkMsgLength(length);

    final AtomicBuffer buffer = this.buffer;
    final int recordLength = length + HEADER_LENGTH;
    final int recordIndex = claimCapacity(buffer, recordLength);

    if (INSUFFICIENT_CAPACITY == recordIndex) {
      return recordIndex;
    }

    buffer.putIntRelease(lengthOffset(recordIndex), -recordLength);
    VarHandle.releaseFence();
    buffer.putInt(typeOffset(recordIndex), msgTypeId);

    return encodedMsgOffset(recordIndex);
  }

  /** {@inheritDoc} */
  public void commit(final int index) {
    final int recordIndex = computeRecordIndex(index);
    final AtomicBuffer buffer = this.buffer;
    final int recordLength = verifyClaimedSpaceNotReleased(buffer, recordIndex);

    buffer.putIntRelease(lengthOffset(recordIndex), -recordLength);
  }

  /** {@inheritDoc} */
  public void abort(final int index) {
    final int recordIndex = computeRecordIndex(index);
    final AtomicBuffer buffer = this.buffer;
    final int recordLength = verifyClaimedSpaceNotReleased(buffer, recordIndex);

    buffer.putInt(typeOffset(recordIndex), PADDING_MSG_TYPE_ID);
    buffer.putIntRelease(lengthOffset(recordIndex), -recordLength);
  }

  /** {@inheritDoc} */
  public int read(final MessageHandler handler) {
    return read(handler, Integer.MAX_VALUE);
  }

  /** {@inheritDoc} */
  public int read(final MessageHandler handler, final int messageCountLimit) {
    int messagesRead = 0;

    final AtomicBuffer buffer = this.buffer;
    final int headPositionIndex = this.headPositionIndex;
    final long head = buffer.getLong(headPositionIndex);

    int bytesRead = 0;

    final int capacity = this.capacity;
    final int headIndex = (int) head & (capacity - 1);
    final int contiguousBlockLength = capacity - headIndex;

    try {
      while ((bytesRead < contiguousBlockLength) && (messagesRead < messageCountLimit)) {
        final int recordIndex = headIndex + bytesRead;
        final int recordLength = buffer.getIntVolatile(lengthOffset(recordIndex));
        if (recordLength <= 0) {
          break;
        }

        bytesRead += align(recordLength, ALIGNMENT);

        final int messageTypeId = buffer.getInt(typeOffset(recordIndex));
        if (PADDING_MSG_TYPE_ID == messageTypeId) {
          continue;
        }

        handler.onMessage(
            messageTypeId, buffer, recordIndex + HEADER_LENGTH, recordLength - HEADER_LENGTH);
        ++messagesRead;
      }
    } finally {
      if (bytesRead > 0) {
        buffer.putLongRelease(headPositionIndex, head + bytesRead);
      }
    }

    return messagesRead;
  }

  /** {@inheritDoc} */
  public int controlledRead(final ControlledMessageHandler handler) {
    return controlledRead(handler, Integer.MAX_VALUE);
  }

  /** {@inheritDoc} */
  public int controlledRead(final ControlledMessageHandler handler, final int messageCountLimit) {
    int messagesRead = 0;

    final AtomicBuffer buffer = this.buffer;
    final int headPositionIndex = this.headPositionIndex;
    long head = buffer.getLong(headPositionIndex);

    int bytesRead = 0;

    final int capacity = this.capacity;
    int headIndex = (int) head & (capacity - 1);
    final int contiguousBlockLength = capacity - headIndex;

    try {
      while ((bytesRead < contiguousBlockLength) && (messagesRead < messageCountLimit)) {
        final int recordIndex = headIndex + bytesRead;
        final int recordLength = buffer.getIntVolatile(lengthOffset(recordIndex));
        if (recordLength <= 0) {
          break;
        }

        final int alignedLength = align(recordLength, ALIGNMENT);
        bytesRead += alignedLength;

        final int messageTypeId = buffer.getInt(typeOffset(recordIndex));
        if (PADDING_MSG_TYPE_ID == messageTypeId) {
          continue;
        }

        final ControlledMessageHandler.Action action = handler.onMessage(
            messageTypeId, buffer, recordIndex + HEADER_LENGTH, recordLength - HEADER_LENGTH);

        if (ABORT == action) {
          bytesRead -= alignedLength;
          break;
        }

        ++messagesRead;

        if (BREAK == action) {
          break;
        }
        if (COMMIT == action) {
          buffer.putLongRelease(headPositionIndex, head + bytesRead);
          headIndex += bytesRead;
          head += bytesRead;
          bytesRead = 0;
        }
      }
    } finally {
      if (bytesRead > 0) {
        buffer.putLongRelease(headPositionIndex, head + bytesRead);
      }
    }

    return messagesRead;
  }

  /** {@inheritDoc} */
  public int maxMsgLength() {
    return maxMsgLength;
  }

  /** {@inheritDoc} */
  public long nextCorrelationId() {
    return buffer.getAndAddLong(correlationIdCounterIndex, 1);
  }

  /** {@inheritDoc} */
  public AtomicBuffer buffer() {
    return buffer;
  }

  /** {@inheritDoc} */
  public void consumerHeartbeatTime(final long time) {
    buffer.putLongRelease(consumerHeartbeatIndex, time);
  }

  /** {@inheritDoc} */
  public long consumerHeartbeatTime() {
    return buffer.getLongVolatile(consumerHeartbeatIndex);
  }

  /** {@inheritDoc} */
  public long producerPosition() {
    return buffer.getLongVolatile(tailPositionIndex);
  }

  /** {@inheritDoc} */
  public long consumerPosition() {
    return buffer.getLongVolatile(headPositionIndex);
  }

  /** {@inheritDoc} */
  public int size() {
    final AtomicBuffer buffer = this.buffer;
    final int headPositionIndex = this.headPositionIndex;
    final int tailPositionIndex = this.tailPositionIndex;
    long headBefore;
    long tail;
    long headAfter = buffer.getLongVolatile(headPositionIndex);

    do {
      headBefore = headAfter;
      tail = buffer.getLongVolatile(tailPositionIndex);
      headAfter = buffer.getLongVolatile(headPositionIndex);
    } while (headAfter != headBefore);

    final long size = tail - headAfter;
    if (size < 0) {
      return 0;
    } else if (size > capacity) {
      return capacity;
    }

    return (int) size;
  }

  /** {@inheritDoc} */
  public boolean unblock() {
    return false;
  }

  private void checkMsgLength(final int length) {
    if (length < 0) {
      throw new IllegalArgumentException("invalid message length=" + length);
    } else if (length > maxMsgLength) {
      throw new IllegalArgumentException(
          "encoded message exceeds maxMsgLength=" + maxMsgLength + ", length=" + length);
    }
  }

  private int claimCapacity(final AtomicBuffer buffer, final int recordLength) {
    final int alignedRecordLength = align(recordLength, ALIGNMENT);
    final int requiredCapacity = alignedRecordLength + HEADER_LENGTH;
    final int capacity = this.capacity;
    final int tailPositionIndex = this.tailPositionIndex;
    final int headCachePositionIndex = this.headCachePositionIndex;
    final int mask = capacity - 1;

    long head = buffer.getLong(headCachePositionIndex);
    final long tail = buffer.getLong(tailPositionIndex);
    final int availableCapacity = capacity - (int) (tail - head);

    if (requiredCapacity > availableCapacity) {
      head = buffer.getLongVolatile(headPositionIndex);

      if (requiredCapacity > (capacity - (int) (tail - head))) {
        return INSUFFICIENT_CAPACITY;
      }

      buffer.putLong(headCachePositionIndex, head);
    }

    int padding = 0;
    final int recordIndex = (int) tail & mask;
    final int toBufferEndLength = capacity - recordIndex;
    int writeIndex = recordIndex;
    long nextTail = tail + alignedRecordLength;

    if (alignedRecordLength == toBufferEndLength) // message fits within the end of the buffer
    {
      buffer.putLongRelease(tailPositionIndex, nextTail);
      buffer.putLong(0, 0L); // pre-zero next message header
      return recordIndex;
    } else if (requiredCapacity > toBufferEndLength) {
      writeIndex = 0;
      int headIndex = (int) head & mask;

      if (requiredCapacity > headIndex) {
        head = buffer.getLongVolatile(headPositionIndex);
        headIndex = (int) head & mask;
        if (requiredCapacity > headIndex) {
          writeIndex = INSUFFICIENT_CAPACITY;
          nextTail = tail;
        }

        buffer.putLong(headCachePositionIndex, head);
      }

      padding = toBufferEndLength;
      nextTail += padding;
    }

    buffer.putLongRelease(tailPositionIndex, nextTail);

    if (0 != padding) {
      buffer.putLong(0, 0L);
      buffer.putIntRelease(lengthOffset(recordIndex), -padding);
      VarHandle.releaseFence();

      buffer.putInt(typeOffset(recordIndex), PADDING_MSG_TYPE_ID);
      buffer.putIntRelease(lengthOffset(recordIndex), padding);
    }

    if (INSUFFICIENT_CAPACITY != writeIndex) {
      buffer.putLong(writeIndex + alignedRecordLength, 0L); // pre-zero next message header
    }

    return writeIndex;
  }

  private int computeRecordIndex(final int index) {
    final int recordIndex = index - HEADER_LENGTH;
    if (recordIndex < 0 || recordIndex > (capacity - HEADER_LENGTH)) {
      throw new IllegalArgumentException("invalid message index " + index);
    }

    return recordIndex;
  }

  private int verifyClaimedSpaceNotReleased(final AtomicBuffer buffer, final int recordIndex) {
    final int recordLength = buffer.getInt(lengthOffset(recordIndex));
    if (recordLength < 0) {
      return recordLength;
    }

    throw new IllegalStateException("claimed space previously "
        + (PADDING_MSG_TYPE_ID == buffer.getInt(typeOffset(recordIndex))
            ? "aborted"
            : "committed"));
  }
}
