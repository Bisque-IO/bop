package bop.cluster.message;

import bop.bit.Primitives;
import bop.cluster.Topic;
import bop.constant.Interned;
import bop.hash.Hash;
import bop.unsafe.Danger;
import java.lang.foreign.MemorySegment;
import java.nio.ByteBuffer;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;
import jdk.internal.misc.Unsafe;
import org.agrona.DirectBuffer;
import org.agrona.concurrent.UnsafeBuffer;

public sealed class EventImpl
    implements AutoCloseable,
        Event,
        Event.WithTopic,
        Event.WithBody,
        Event.WithHeader,
        Event.Builder
    permits CommandImpl {
  public static final int MAX_HEADER_SIZE = Short.MAX_VALUE;
  public static final int MAX_SIZE = 16 * 1024 * 1024;
  static final Unsafe U = Unsafe.getUnsafe();

  /* Header

   0   i32      size
   4   u8       type
   5   u8       flags
   6   u8       topicOffset
   7   u8       topicSize

   8   i64      topicHash

   16  i64      id

   24  i64      time

   32  u32      sourceIp
   36  u32      sourcePort

   40  i64      sourceId

   48  u32      sourceStarted
   52  u16      headerOffset
   54  u16      headerSize

   56  u8       format
   57  u8       reserved
   58  u16      bodyOffset
   60  i32      bodySize

  */

  /* Pipeline Body

   0   i32     count
   4   i32     flags

   Repeated "count" times
   Event | Command

  */

  /* Log Entry Body



  */

  /* Command

   64  i64      deadline

   72  i64      dedupeHash

   80  i64      actorIDHash

   88  u8       attempt
   89  u8       maxAttempts
   90  u8       dedupeSize
   91  u8       actorIDSize
   92  u16      dedupeOffset
   94  u16      actorIDOffset

  */

  //  static final int HASH_OFFSET = 0;

  static final int SIZE_OFFSET = 0;
  static final int TYPE_OFFSET = SIZE_OFFSET + 4;
  static final int FLAGS_OFFSET = TYPE_OFFSET + 1;
  static final int TOPIC_OFFSET_OFFSET = FLAGS_OFFSET + 1;
  static final int TOPIC_SIZE_OFFSET = TOPIC_OFFSET_OFFSET + 1;

  static final int TOPIC_HASH_OFFSET = TOPIC_SIZE_OFFSET + 1;

  public static final int ID_OFFSET = TOPIC_HASH_OFFSET + 8;

  static final int TIME_OFFSET = ID_OFFSET + 8;

  static final int SOURCE_OFFSET = TIME_OFFSET + 8;
  static final int SOURCE_PORT_OFFSET = SOURCE_OFFSET + 4;
  static final int SOURCE_ID_OFFSET = SOURCE_PORT_OFFSET + 4;
  static final int SOURCE_STARTED_OFFSET = SOURCE_ID_OFFSET + 8;
  static final int HEADER_OFFSET_OFFSET = SOURCE_STARTED_OFFSET + 4;
  static final int HEADER_SIZE_OFFSET = HEADER_OFFSET_OFFSET + 2;

  static final int FORMAT_OFFSET = HEADER_SIZE_OFFSET + 2;
  static final int BODY_FLAGS_OFFSET = FORMAT_OFFSET + 1;
  static final int BODY_OFFSET_OFFSET = BODY_FLAGS_OFFSET + 1;
  static final int BODY_SIZE_OFFSET = BODY_OFFSET_OFFSET + 2;

  static final int HEADER_SIZE = BODY_SIZE_OFFSET + 4;

  protected long address;
  protected int capacity;
  protected boolean owned;
  protected final UnsafeBuffer buffer = new UnsafeBuffer();
  protected volatile long ledgerId;
  protected AtomicLong sentNs = new AtomicLong(0L);
  protected volatile long ledgerAckNs;
  protected final ReentrantLock lock = new ReentrantLock();
  protected final Condition logIdCondition = lock.newCondition();

  public EventImpl(long address, int capacity, boolean owned) {
    this.address = address;
    this.capacity = capacity;
    this.owned = owned;
  }

  public static EventImpl allocate(int capacity, Type type) {
    return allocate(capacity, type.code);
  }

  public static EventImpl allocate(int capacity, byte type) {
    if (capacity < HEADER_SIZE) {
      capacity = HEADER_SIZE;
    }
    long address = U.allocateMemory(capacity);
    if (address == 0L) {
      throw new OutOfMemoryError("Unsafe.allocateMemory failed");
    }
    final var result = new EventImpl(address, capacity, true);
    result.reset(type);
    return result;
  }

  public long address() {
    return address;
  }

  public int capacity() {
    return capacity;
  }

  public boolean isOwned() {
    return owned;
  }

  @Override
  public UnsafeBuffer buffer() {
    buffer.wrap(address, size());
    return buffer;
  }

  @Override
  public long sentNs() {
    return this.sentNs.get();
  }

  @Override
  public Message sentNs(long nowNs) {
    this.sentNs.set(nowNs);
    return this;
  }

  @Override
  public long ledgerAckNs() {
    return this.ledgerAckNs;
  }

  static final TimeoutException TIMEOUT = new TimeoutException();

  public long waitForLedger() throws TimeoutException, InterruptedException {
    return this.waitForLedger(10L, TimeUnit.SECONDS);
  }

  public long waitForLedger(long time, TimeUnit unit)
      throws TimeoutException, InterruptedException {
    var ledgerId = this.ledgerId;
    if (ledgerId != 0L) {
      return ledgerId;
    }
    lock.lock();
    try {
      ledgerId = this.ledgerId;
      while (ledgerId == 0) {
        if (!logIdCondition.await(time, unit)) {
          throw TIMEOUT;
        }
        ledgerId = this.ledgerId;
      }
      return ledgerId;
    } finally {
      lock.unlock();
    }
  }

  public void pushLedgerId(long ledgerId, long nowNs) {
    while (true) {
      if (lock.tryLock()) {
        try {
          this.ledgerId = ledgerId;
          this.ledgerAckNs = nowNs - this.sentNs.get();
          logIdCondition.signal();
          return;
        } finally {
          lock.unlock();
        }
      }
      Thread.yield();
    }
  }

  //  public void pushLedgerId(long ledgerId, long nowNs) {
  //    this.ledgerId = ledgerId;
  //    this.ledgerAckNs = nowNs - this.sentNs.get();
  ////    logIdCondition.signal();
  //  }

  //  public long hash() {
  //    return U.getLong(address + HASH_OFFSET);
  //  }
  //
  //  public EventImpl hash(long value) {
  //    U.putLong(address + HASH_OFFSET, value);
  //    return this;
  //  }

  public int size() {
    return U.getInt(address + SIZE_OFFSET);
  }

  public EventImpl size(int size) {
    U.putInt(address + SIZE_OFFSET, size);
    return this;
  }

  public byte type() {
    return U.getByte(address + TYPE_OFFSET);
  }

  public EventImpl type(byte value) {
    U.putByte(address + TYPE_OFFSET, value);
    return this;
  }

  public byte flags() {
    return U.getByte(address + FLAGS_OFFSET);
  }

  public EventImpl flags(byte value) {
    U.putByte(address + FLAGS_OFFSET, value);
    return this;
  }

  public int topicOffset() {
    return Primitives.unsignedByte(U.getByte(address + TOPIC_OFFSET_OFFSET));
  }

  protected void setTopicOffset(int value) {
    U.putByte(address + TOPIC_OFFSET_OFFSET, (byte) value);
  }

  public int topicSize() {
    return Primitives.unsignedByte(U.getByte(address + TOPIC_SIZE_OFFSET));
  }

  protected void setTopicSize(char value) {
    U.putByte(address + TOPIC_SIZE_OFFSET, (byte) value);
  }

  public long topicHash() {
    return U.getLong(address + TOPIC_HASH_OFFSET);
  }

  protected void setTopicHash(long value) {
    U.putLong(address + TOPIC_HASH_OFFSET, value);
  }

  @Override
  public String topic() {
    final int offset = topicOffset();
    final int size = topicSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return "";
    }
    return readStringInterned(offset, size, topicHash());
  }

  @Override
  public byte[] topicBytes() {
    final int offset = topicOffset();
    final int size = topicSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return EMPTY_BYTES;
    }
    final byte[] bytes = new byte[size];
    read(offset, bytes, 0, bytes.length);
    return bytes;
  }

  @Override
  public long topicAddress() {
    final int offset = topicOffset();
    return (offset <= 0) ? 0L : address + offset;
  }

  public long id() {
    return U.getLong(address + ID_OFFSET);
  }

  public EventImpl id(long value) {
    U.putLong(address + ID_OFFSET, value);
    return this;
  }

  public long time() {
    return U.getLong(address + TIME_OFFSET);
  }

  public EventImpl time(long value) {
    U.putLong(address + TIME_OFFSET, value);
    return this;
  }

  public int source() {
    return U.getInt(address + SOURCE_OFFSET);
  }

  public EventImpl source(int value) {
    U.putInt(address + SOURCE_OFFSET, value);
    return this;
  }

  public int sourcePort() {
    return U.getInt(address + SOURCE_PORT_OFFSET);
  }

  public EventImpl sourcePort(int value) {
    U.putInt(address + SOURCE_PORT_OFFSET, value);
    return this;
  }

  public long sourceId() {
    return U.getLong(address + SOURCE_ID_OFFSET);
  }

  public EventImpl sourceId(long value) {
    U.putLong(address + SOURCE_ID_OFFSET, value);
    return this;
  }

  public int sourceStarted() {
    return U.getInt(address + SOURCE_STARTED_OFFSET);
  }

  public EventImpl sourceStarted(int value) {
    U.putInt(address + SOURCE_STARTED_OFFSET, value);
    return this;
  }

  public int headerOffset() {
    return U.getChar(address + HEADER_OFFSET_OFFSET);
  }

  protected void headerOffset(char value) {
    U.putChar(address + HEADER_OFFSET_OFFSET, value);
  }

  public int headerSize() {
    return U.getChar(address + HEADER_SIZE_OFFSET);
  }

  protected void headerSize(char value) {
    U.putChar(address + HEADER_SIZE_OFFSET, value);
  }

  public byte format() {
    return U.getByte(address + FORMAT_OFFSET);
  }

  public EventImpl format(byte value) {
    U.putByte(address + FORMAT_OFFSET, value);
    return this;
  }

  public int bodyOffset() {
    return U.getChar(address + BODY_OFFSET_OFFSET);
  }

  protected void bodyOffset(char value) {
    U.putChar(address + BODY_OFFSET_OFFSET, value);
  }

  public byte bodyFlags() {
    return U.getByte(address + BODY_FLAGS_OFFSET);
  }

  public EventImpl bodyFlags(byte value) {
    U.putByte(address + BODY_FLAGS_OFFSET, value);
    return this;
  }

  public int bodySize() {
    return U.getInt(address + BODY_SIZE_OFFSET);
  }

  protected void bodySize(int value) {
    U.putInt(address + BODY_SIZE_OFFSET, value);
  }

  @Override
  public String header() {
    final int offset = headerOffset();
    final int size = headerSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return "";
    }
    return readString(offset, size);
  }

  @Override
  public byte[] headerBytes() {
    final int offset = headerOffset();
    final int size = headerSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return EMPTY_BYTES;
    }
    final byte[] bytes = new byte[size];
    read(offset, bytes, 0, bytes.length);
    return bytes;
  }

  @Override
  public int headerCopyTo(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0;
    }
    return headerCopyTo(buf, 0, buf.length);
  }

  @Override
  public int headerCopyTo(byte[] buf, int offset, int length) {
    final int headerOffset = headerOffset();
    final int size = headerSize();
    if (headerOffset <= 0 || size == 0 || headerOffset + size > capacity) {
      return 0;
    }
    read(headerOffset, buf, offset, length);
    return Math.min(size, length);
  }

  @Override
  public int headerCopyTo(long address, int length) {
    final int headerOffset = headerOffset();
    final int size = headerSize();
    if (headerOffset <= 0 || size == 0 || headerOffset + size > capacity) {
      return 0;
    }
    final var l = Math.min(size, length);
    U.copyMemory(this.address + headerOffset, address, l);
    return l;
  }

  @Override
  public long headerAddress() {
    final int offset = headerOffset();
    return (offset <= 0) ? 0L : address + offset;
  }

  @Override
  public String body() {
    final int offset = bodyOffset();
    final int size = bodySize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return "";
    }
    return readString(offset, size);
  }

  @Override
  public byte[] bodyBytes() {
    final int offset = bodyOffset();
    final int size = bodySize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return EMPTY_BYTES;
    }
    final byte[] bytes = new byte[size];
    read(offset, bytes, 0, bytes.length);
    return bytes;
  }

  @Override
  public int bodyCopyTo(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0;
    }
    return bodyCopyTo(buf, 0, buf.length);
  }

  @Override
  public int bodyCopyTo(byte[] buf, int offset, int length) {
    final int bodyOffset = bodyOffset();
    final int size = bodySize();
    if (bodyOffset <= 0 || size == 0 || bodyOffset + size > capacity) {
      return 0;
    }
    read(bodyOffset, buf, offset, length);
    return Math.min(size, length);
  }

  @Override
  public int bodyCopyTo(long address, int length) {
    final int bodyOffset = bodyOffset();
    final int size = bodySize();
    if (bodyOffset <= 0 || size == 0 || bodyOffset + size > capacity) {
      return 0;
    }
    final var l = Math.min(size, length);
    U.copyMemory(this.address + bodyOffset, address, l);
    return l;
  }

  @Override
  public long bodyAddress() {
    final int offset = bodyOffset();
    return (offset <= 0) ? 0L : address + offset;
  }

  protected long computeHash(int offset, int length) {
    return Hash.xxh3(address + offset, length);
  }

  protected String readString(int offset, int length) {
    final var b = new byte[length];
    read(offset, b, 0, b.length);
    return new String(b);
  }

  protected String readStringInterned(int offset, int length, long hash) {
    if (hash == 0L) {
      hash = computeHash(offset, length);
    }
    final var interned = Interned.get(hash);
    if (interned != null) {
      return interned.value();
    }
    final var b = new byte[length];
    read(offset, b, 0, b.length);
    return new String(b);
  }

  protected int read(int offset, byte[] dst, int dstOffset, int length) {
    final int remaining = size() - offset;
    if (remaining <= 0) {
      return 0;
    }
    final int len = Math.min(length, remaining);
    U.copyMemory(null, address + offset, dst, Danger.BYTE_BASE + dstOffset, len);
    return len;
  }

  protected int read(int offset, long dst, int length) {
    final int remaining = size() - offset;
    if (remaining <= 0) {
      return 0;
    }
    final int len = Math.min(length, remaining);
    U.copyMemory(address + offset, dst, len);
    return len;
  }

  protected void write(int offset, ByteBuffer buffer) {
    Objects.requireNonNull(buffer);
    if (buffer.remaining() == 0) {
      return;
    }
    if (buffer.hasArray()) {
      U.copyMemory(
          buffer.array(),
          Danger.BYTE_BASE + buffer.position(),
          null,
          address + offset,
          buffer.remaining());
    } else {
      final var src = Danger.getAddress(buffer);
      if (src == 0L) {
        return;
      }
      U.copyMemory(src, address + offset, buffer.remaining());
    }
  }

  protected void write(int offset, String value) {
    final var b = Danger.getBytes(value);
    write(offset, b, 0, b.length);
  }

  protected void write(int offset, byte[] src) {
    write(offset, src, 0, src.length);
  }

  protected void write(int offset, byte[] src, int srcOffset, int length) {
    U.copyMemory(src, Danger.BYTE_BASE + srcOffset, null, address + offset, length);
  }

  protected void write(int offset, long src, int length) {
    U.copyMemory(src, address + offset, length);
  }

  protected int minCapacity() {
    return 256;
  }

  public void resize(int newCapacity) {
    if (capacity == newCapacity) {
      return;
    }
    if (newCapacity < minCapacity()) {
      newCapacity = minCapacity();
    }
    final var newAddress = U.allocateMemory(newCapacity);
    final var newSegment = MemorySegment.ofAddress(address).reinterpret(newCapacity);
    U.copyMemory(address, newAddress, Math.min(capacity, newCapacity));
    U.freeMemory(this.address);
    this.address = address;
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // Exceptions
  ////////////////////////////////////////////////////////////////////////////////////

  protected RuntimeException newHeaderOverflowException() {
    return new RuntimeException(
        "header overflow: header size must be less than " + Short.MAX_VALUE);
  }

  protected RuntimeException newMaxSizeExceededException() {
    return new RuntimeException("maximum message size exceeded: must be <=  " + MAX_SIZE);
  }

  protected RuntimeException topicAlreadySetException() {
    return new RuntimeException(
        "topic already set: body already set at " + topicOffset() + " : " + topicSize());
  }

  protected RuntimeException headerAlreadySetException() {
    return new RuntimeException(
        "header already set: body already set at " + headerOffset() + " : " + headerSize());
  }

  protected RuntimeException bodyAlreadySetException() {
    return new RuntimeException(
        "body already set: body already set at " + bodyOffset() + " : " + bodySize());
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // Append Topic
  ////////////////////////////////////////////////////////////////////////////////////

  protected void setTopic(Topic topic) {
    Objects.requireNonNull(topic);
    if (topicOffset() > 0) {
      throw topicAlreadySetException();
    }
    final var src = Danger.getBytes(topic.name());
    final int pos = grow(size() + src.length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    setTopicOffset((char) pos);
    setTopicSize((char) src.length);
    write(pos, src, 0, src.length);
    setTopicHash(topic.xx3());
  }

  protected void setTopic(String body) {
    if (body == null || body.isEmpty()) {
      return;
    }
    final var b = Danger.getBytes(body);
    setTopic(b, 0, b.length);
  }

  protected void setTopic(byte[] body) {
    if (body == null || body.length == 0) {
      return;
    }
    setTopic(body, 0, body.length);
  }

  protected void setTopic(byte[] src, int offset, int length) {
    if (src == null || length <= 0) {
      return;
    }
    if (topicOffset() > 0) {
      throw topicAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    setTopicOffset((char) pos);
    setTopicSize((char) length);
    write(pos, src, offset, length);
    setTopicHash(computeHash(topicOffset(), topicSize()));
  }

  protected void setTopic(long address, int length) {
    if (length <= 0) {
      return;
    }
    if (bodyOffset() > 0) {
      throw bodyAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    setTopicOffset((char) pos);
    setTopicSize((char) length);
    write(pos, address, length);
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // Append Header
  ////////////////////////////////////////////////////////////////////////////////////

  protected void setHeader(String body) {
    if (body == null || body.isEmpty()) {
      return;
    }
    final var b = Danger.getBytes(body);
    setHeader(b, 0, b.length);
  }

  protected void setHeader(byte[] body) {
    if (body == null || body.length == 0) {
      return;
    }
    setHeader(body, 0, body.length);
  }

  protected void setHeader(byte[] src, int offset, int length) {
    if (src == null || length <= 0) {}
    if (headerOffset() > 0) {
      throw headerAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    headerOffset((char) pos);
    headerSize((char) length);
    write(pos, src, offset, length);
  }

  protected void setHeader(long address, int length) {
    if (length <= 0) {
      return;
    }
    if (bodyOffset() > 0) {
      throw bodyAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    headerOffset((char) pos);
    headerSize((char) length);
    write(pos, address, length);
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // Append Body
  ////////////////////////////////////////////////////////////////////////////////////

  protected void setBody(String body) {
    if (body == null || body.isEmpty()) {
      return;
    }
    final var b = Danger.getBytes(body);
    setBody(b, 0, b.length);
  }

  protected void setBody(byte[] body) {
    if (body == null || body.length == 0) {
      return;
    }
    setBody(body, 0, body.length);
  }

  protected void setBody(byte[] src, int offset, int length) {
    if (src == null || length <= 0) {
      return;
    }
    if (bodyOffset() > 0) {
      throw bodyAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_SIZE) {
      throw newMaxSizeExceededException();
    }
    bodyOffset((char) pos);
    bodySize(length);
    write(pos, src, offset, length);
  }

  protected void setBody(ByteBuffer buffer) {
    Objects.requireNonNull(buffer);
    if (!buffer.hasRemaining()) {
      return;
    }
    final int pos = grow(size() + buffer.remaining());
    if (size() > MAX_SIZE) {
      throw newMaxSizeExceededException();
    }
    if (buffer.hasArray()) {
      U.copyMemory(
          buffer.array(),
          Danger.BYTE_BASE + buffer.position(),
          null,
          address + pos,
          buffer.remaining());
    } else {
      U.copyMemory(
          Danger.getAddress(buffer) + buffer.position(), address + pos, buffer.remaining());
    }
    bodyOffset((char) pos);
    bodySize(buffer.remaining());
  }

  protected void setBody(long address, int length) {
    if (length <= 0) {
      return;
    }
    if (bodyOffset() > 0) {
      throw bodyAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_SIZE) {
      throw newMaxSizeExceededException();
    }
    bodyOffset((char) pos);
    bodySize(length);
    write(pos, address, length);
  }

  @Override
  public EventImpl topic(Topic topic) {
    setTopic(topic);
    return this;
  }

  @Override
  public EventImpl topic(String topicName) {
    setTopic(topicName);
    return this;
  }

  @Override
  public EventImpl topic(byte[] topicName) {
    setTopic(topicName);
    return this;
  }

  @Override
  public EventImpl topic(byte[] topicName, int offset, int length) {
    setTopic(topicName, offset, length);
    return this;
  }

  @Override
  public EventImpl topic(long address, int length) {
    setTopic(address, length);
    return this;
  }

  @Override
  public EventImpl topic(DirectBuffer buffer, int offset, int length) {
    final var ba = buffer.byteArray();
    if (ba == null) {
      final var address = buffer.addressOffset();
      if (address == 0L) {
        return this;
      }
      return topic(address + offset, length);
    }
    return topic(ba, offset, length);
  }

  @Override
  public EventImpl header(String value) {
    setHeader(value);
    return this;
  }

  @Override
  public EventImpl header(byte[] value) {
    setHeader(value);
    return this;
  }

  @Override
  public EventImpl header(byte[] value, int offset, int length) {
    setHeader(value, offset, length);
    return this;
  }

  @Override
  public EventImpl header(long address, int length) {
    setHeader(address, length);
    return this;
  }

  @Override
  public EventImpl header(DirectBuffer buffer, int offset, int length) {
    final var ba = buffer.byteArray();
    if (ba == null) {
      final var address = buffer.addressOffset();
      if (address == 0L) {
        return this;
      }
      return header(address + offset, length);
    }
    return header(ba, offset, length);
  }

  @Override
  public EventImpl body(String value) {
    setBody(value);
    return this;
  }

  @Override
  public Event.Builder body(byte[] value) {
    setBody(value);
    return this;
  }

  @Override
  public Event.Builder body(byte[] value, int offset, int length) {
    setBody(value, offset, length);
    return this;
  }

  @Override
  public Event.Builder body(long address, int length) {
    setBody(address, length);
    return this;
  }

  @Override
  public Event.Builder body(DirectBuffer buffer, int offset, int length) {
    final var ba = buffer.byteArray();
    if (ba == null) {
      final var address = buffer.addressOffset();
      if (address == 0L) {
        return this;
      }
      return body(address + offset, length);
    }
    return body(ba, offset, length);
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // Grow
  ////////////////////////////////////////////////////////////////////////////////////

  int grow(int minNewSize) {
    final int size = size();
    if (minNewSize > capacity) {
      resize(Math.max(capacity * 2, minNewSize));
    }
    size(minNewSize);
    return size;
  }

  public Event.WithTopic reset(byte type) {
    U.putLong(address, HEADER_SIZE);
    U.putInt(address + 8, HEADER_SIZE);
    U.putInt(address + 12, 0);
    U.putLong(address + 16, 0);
    U.putLong(address + 24, 0);
    U.putLong(address + 32, 0);
    U.putLong(address + 40, 0);
    U.putLong(address + 48, 0);
    U.putLong(address + 56, 0);
    this.sentNs.set(0L);
    this.ledgerAckNs = 0L;
    this.ledgerId = 0L;
    this.type(type);
    this.size(HEADER_SIZE);
    return this;
  }

  @Override
  public void close() throws Exception {
    if (owned && address != 0L) {
      U.freeMemory(address);
      address = 0L;
    }
  }
}
