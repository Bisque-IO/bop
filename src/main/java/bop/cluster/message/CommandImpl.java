package bop.cluster.message;

import bop.bit.Primitives;
import bop.cluster.Topic;
import jdk.internal.misc.Unsafe;
import org.agrona.DirectBuffer;

public final class CommandImpl extends EventImpl
    implements Event,
        Command,
        Command.Builder,
        Command.WithTopic,
        Command.WithDedupe,
        Command.WithActorID,
        Command.WithHeader,
        Command.WithBody {

  public static CommandImpl allocate(int capacity, Command.Type type) {
    return allocate(capacity, type.code);
  }

  public static CommandImpl allocate(int capacity, byte type) {
    if (capacity < 256) {
      capacity = 256;
    }
    long address = U.allocateMemory(capacity);
    if (address == 0L) {
      throw new OutOfMemoryError("Unsafe.allocateMemory failed");
    }
    final var result = new CommandImpl(address, capacity, true);
    result.reset(type);
    return result;
  }

  static final Unsafe U = Unsafe.getUnsafe();
  /*
    56  i64      deadline

    64  i64      dedupeHash

    72  i64      actorIDHash

    80  u8       attempt
    81  u8       maxAttempts
    82  u8       dedupeSize
    83  u8       actorIDSize
    84  u16      dedupeOffset
    86  u16      actorIDOffset
  */

  static final int DEADLINE_OFFSET = HEADER_SIZE;

  static final int DEDUPE_HASH_OFFSET = DEADLINE_OFFSET + 8;

  static final int ACTOR_ID_HASH_OFFSET = DEDUPE_HASH_OFFSET + 8;

  static final int ATTEMPT_OFFSET = ACTOR_ID_HASH_OFFSET + 8;
  static final int MAX_ATTEMPT_OFFSET = ATTEMPT_OFFSET + 1;
  static final int DEDUPE_SIZE_OFFSET = MAX_ATTEMPT_OFFSET + 1;
  static final int ACTOR_ID_SIZE_OFFSET = DEDUPE_SIZE_OFFSET + 1;
  static final int DEDUPE_OFFSET_OFFSET = ACTOR_ID_SIZE_OFFSET + 1;
  static final int ACTOR_ID_OFFSET_OFFSET = DEDUPE_OFFSET_OFFSET + 2;

  public static final int COMMAND_HEADER_SIZE = ACTOR_ID_OFFSET_OFFSET + 2;

  CommandImpl(long address, int capacity, boolean owned) {
    super(address, capacity, owned);
  }

  public Command.WithTopic reset(byte type) {
    U.putLong(address, COMMAND_HEADER_SIZE);
    U.putInt(address + 8, COMMAND_HEADER_SIZE);
    U.putInt(address + 12, 0);
    U.putLong(address + 16, 0);
    U.putLong(address + 24, 0);
    U.putLong(address + 32, 0);
    U.putLong(address + 40, 0);
    U.putLong(address + 48, 0);
    U.putLong(address + 56, 0);
    U.putLong(address + 64, 0);
    U.putLong(address + 72, 0);
    U.putLong(address + 80, 0);
    U.putLong(address + 88, 0);
    this.type(type);
    return this;
  }

  @Override
  public int attempt() {
    return U.getChar(address + ATTEMPT_OFFSET);
  }

  @Override
  public CommandImpl attempt(int value) {
    U.putChar(address + ATTEMPT_OFFSET, (char) value);
    return this;
  }

  @Override
  public int maxAttempts() {
    return U.getChar(address + MAX_ATTEMPT_OFFSET);
  }

  @Override
  public CommandImpl maxAttempts(int value) {
    U.putChar(address + MAX_ATTEMPT_OFFSET, (char) value);
    return this;
  }

  @Override
  public long deadline() {
    return U.getLong(address + DEADLINE_OFFSET);
  }

  @Override
  public CommandImpl deadline(long value) {
    U.putLong(address + DEADLINE_OFFSET, value);
    return this;
  }

  @Override
  public CommandImpl topic(Topic topic) {
    setTopic(topic);
    return this;
  }

  @Override
  public CommandImpl topic(String topicName) {
    setTopic(topicName);
    return this;
  }

  @Override
  public CommandImpl topic(byte[] topicName) {
    setTopic(topicName);
    return this;
  }

  @Override
  public CommandImpl topic(byte[] topicName, int offset, int length) {
    setTopic(topicName, offset, length);
    return this;
  }

  @Override
  public CommandImpl topic(long address, int length) {
    setTopic(address, length);
    return this;
  }

  @Override
  public CommandImpl topic(DirectBuffer buffer, int offset, int length) {
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
  public CommandImpl header(String value) {
    setHeader(value);
    return this;
  }

  @Override
  public CommandImpl header(byte[] value) {
    setHeader(value);
    return this;
  }

  @Override
  public CommandImpl header(byte[] value, int offset, int length) {
    setHeader(value, offset, length);
    return this;
  }

  @Override
  public CommandImpl header(long address, int length) {
    setHeader(address, length);
    return this;
  }

  @Override
  public CommandImpl header(DirectBuffer buffer, int offset, int length) {
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
  public CommandImpl body(String value) {
    setBody(value);
    return this;
  }

  @Override
  public CommandImpl body(byte[] value) {
    setBody(value);
    return this;
  }

  @Override
  public CommandImpl body(byte[] value, int offset, int length) {
    setBody(value, offset, length);
    return this;
  }

  @Override
  public CommandImpl body(long address, int length) {
    setBody(address, length);
    return this;
  }

  @Override
  public CommandImpl body(DirectBuffer buffer, int offset, int length) {
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
  // Dedupe
  ////////////////////////////////////////////////////////////////////////////////////

  @Override
  public String dedupe() {
    final int offset = dedupeOffset();
    final int size = dedupeSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return "";
    }
    return readString(offset, size);
  }

  @Override
  public byte[] dedupeBytes() {
    final int offset = dedupeOffset();
    final int size = dedupeSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return EMPTY_BYTES;
    }
    final byte[] bytes = new byte[size];
    read(offset, bytes, 0, bytes.length);
    return bytes;
  }

  @Override
  public int dedupeCopyTo(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0;
    }
    return dedupeCopyTo(buf, 0, buf.length);
  }

  @Override
  public int dedupeCopyTo(byte[] buf, int offset, int length) {
    final int dedupeOffset = dedupeOffset();
    final int size = dedupeSize();
    if (dedupeOffset <= 0 || size == 0 || dedupeOffset + size > capacity) {
      return 0;
    }
    read(dedupeOffset, buf, offset, length);
    return Math.min(size, length);
  }

  @Override
  public int dedupeCopyTo(long address, int length) {
    final int dedupeOffset = dedupeOffset();
    final int size = dedupeSize();
    if (dedupeOffset <= 0 || size == 0 || dedupeOffset + size > capacity) {
      return 0;
    }
    final var l = Math.min(size, length);
    U.copyMemory(this.address + dedupeOffset, address, l);
    return l;
  }

  @Override
  public long dedupeAddress() {
    final int offset = dedupeOffset();
    return (offset <= 0) ? 0L : address + offset;
  }

  private RuntimeException dedupeAlreadySetException() {
    return new RuntimeException("dedupe already set: " + dedupeOffset() + " : " + dedupeSize());
  }

  @Override
  public long dedupeHash() {
    return U.getLong(address + DEDUPE_HASH_OFFSET);
  }

  @Override
  public CommandImpl dedupeHash(long value) {
    U.putLong(address + DEDUPE_HASH_OFFSET, value);
    return this;
  }

  @Override
  public int dedupeOffset() {
    return U.getChar(address + DEDUPE_OFFSET_OFFSET);
  }

  private void dedupeOffset(char value) {
    U.putChar(address + DEDUPE_OFFSET_OFFSET, value);
  }

  @Override
  public int dedupeSize() {
    return Primitives.unsignedByte(U.getByte(address + DEDUPE_SIZE_OFFSET));
  }

  private void dedupeSize(int value) {
    U.putByte(address + DEDUPE_SIZE_OFFSET, (byte) value);
  }

  @Override
  public CommandImpl dedupe(String dedupeOrActorID) {
    if (dedupeOrActorID == null || dedupeOrActorID.isEmpty()) {
      return this;
    }
    final var b = UA.getBytes(dedupeOrActorID);
    return dedupe(b, 0, b.length);
  }

  @Override
  public CommandImpl dedupe(byte[] dedupeOrActorID) {
    if (dedupeOrActorID == null || dedupeOrActorID.length == 0) {
      return this;
    }
    return dedupe(dedupeOrActorID, 0, dedupeOrActorID.length);
  }

  @Override
  public CommandImpl dedupe(byte[] src, int offset, int length) {
    if (src == null || length <= 0) {}
    if (dedupeOffset() > 0) {
      throw dedupeAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    dedupeOffset((char) pos);
    dedupeSize((char) length);
    write(pos, src, offset, length);
    dedupeHash(computeHash(pos, length));
    return this;
  }

  @Override
  public CommandImpl dedupe(long address, int length) {
    if (length <= 0) {
      return this;
    }
    if (dedupeOffset() > 0) {
      throw dedupeAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    dedupeOffset((char) pos);
    dedupeSize((char) length);
    write(pos, address, length);
    dedupeHash(computeHash(pos, length));
    return this;
  }

  @Override
  public CommandImpl dedupe(DirectBuffer buffer, int offset, int length) {
    final var ba = buffer.byteArray();
    if (ba == null) {
      final var address = buffer.addressOffset();
      if (address == 0L) {
        return this;
      }
      return dedupe(address + offset, length);
    }
    return dedupe(ba, offset, length);
  }

  ////////////////////////////////////////////////////////////////////////////////////
  // ActorID
  ////////////////////////////////////////////////////////////////////////////////////

  @Override
  public String actorID() {
    final int offset = actorIDOffset();
    final int size = actorIDSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return "";
    }
    return readString(offset, size);
  }

  @Override
  public byte[] actorIDBytes() {
    final int offset = actorIDOffset();
    final int size = actorIDSize();
    if (offset <= 0 || size == 0 || offset + size > capacity) {
      return EMPTY_BYTES;
    }
    final byte[] bytes = new byte[size];
    read(offset, bytes, 0, bytes.length);
    return bytes;
  }

  @Override
  public int actorIDCopyTo(byte[] buf) {
    if (buf == null || buf.length == 0) {
      return 0;
    }
    return actorIDCopyTo(buf, 0, buf.length);
  }

  @Override
  public int actorIDCopyTo(byte[] buf, int offset, int length) {
    final int actorIDOffset = actorIDOffset();
    final int size = actorIDSize();
    if (actorIDOffset <= 0 || size == 0 || actorIDOffset + size > capacity) {
      return 0;
    }
    read(actorIDOffset, buf, offset, length);
    return Math.min(size, length);
  }

  @Override
  public int actorIDCopyTo(long address, int length) {
    final int actorIDOffset = actorIDOffset();
    final int size = actorIDSize();
    if (actorIDOffset <= 0 || size == 0 || actorIDOffset + size > capacity) {
      return 0;
    }
    final var l = Math.min(size, length);
    U.copyMemory(this.address + actorIDOffset, address, l);
    return l;
  }

  @Override
  public long actorIDAddress() {
    final int offset = actorIDOffset();
    return (offset <= 0) ? 0L : address + offset;
  }

  private RuntimeException actorIDAlreadySetException() {
    return new RuntimeException("actorID already set: " + actorIDOffset() + " : " + actorIDSize());
  }

  @Override
  public long actorIDHash() {
    return U.getLong(address + ACTOR_ID_HASH_OFFSET);
  }

  public CommandImpl actorIDHash(long value) {
    U.putLong(address + ACTOR_ID_HASH_OFFSET, value);
    return this;
  }

  @Override
  public int actorIDOffset() {
    return U.getChar(address + ACTOR_ID_OFFSET_OFFSET);
  }

  private void actorIDOffset(char value) {
    U.putChar(address + ACTOR_ID_OFFSET_OFFSET, value);
  }

  @Override
  public int actorIDSize() {
    return Primitives.unsignedByte(U.getByte(address + ACTOR_ID_SIZE_OFFSET));
  }

  private void actorIDSize(int value) {
    U.putByte(address + ACTOR_ID_SIZE_OFFSET, (byte) value);
  }

  @Override
  public CommandImpl actorID(String actorID) {
    if (actorID == null || actorID.isEmpty()) {
      return this;
    }
    final var b = UA.getBytes(actorID);
    return actorID(b, 0, b.length);
  }

  @Override
  public CommandImpl actorID(byte[] actorID) {
    if (actorID == null || actorID.length == 0) {
      return this;
    }
    return actorID(actorID, 0, actorID.length);
  }

  @Override
  public CommandImpl actorID(byte[] src, int offset, int length) {
    if (src == null || length <= 0) {}
    if (actorIDOffset() > 0) {
      throw actorIDAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    actorIDOffset((char) pos);
    actorIDSize(length);
    write(pos, src, offset, length);
    actorIDHash(computeHash(pos, length));
    return this;
  }

  @Override
  public CommandImpl actorID(long address, int length) {
    if (length <= 0) {
      return this;
    }
    if (actorIDOffset() > 0) {
      throw actorIDAlreadySetException();
    }
    final int pos = grow(size() + length);
    if (size() > MAX_HEADER_SIZE) {
      throw newHeaderOverflowException();
    }
    actorIDOffset((char) pos);
    actorIDSize((char) length);
    write(pos, address, length);
    actorIDHash(computeHash(pos, length));
    return this;
  }

  @Override
  public CommandImpl actorID(DirectBuffer buffer, int offset, int length) {
    final var ba = buffer.byteArray();
    if (ba == null) {
      final var address = buffer.addressOffset();
      if (address == 0L) {
        return this;
      }
      return actorID(address + offset, length);
    }
    return actorID(ba, offset, length);
  }
}
