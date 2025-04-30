package bop.cluster.message;

import bop.cluster.Topic;
import org.agrona.DirectBuffer;

public sealed interface Event extends AutoCloseable, Message
    permits Event.Builder, Command, EventImpl, CommandImpl {
  byte[] EMPTY_BYTES = new byte[0];

  Topic JOIN = Topic.of("JOIN");

  Topic JOINED = Topic.of("JOINED");

  Topic REMOVE = Topic.of("REMOVE");

  Topic REMOVED = Topic.of("REMOVED");

  enum Type {
    EVENT((byte) 0),
    ACTION((byte) 1),
    QUEUE((byte) 2),
    ACTOR((byte) 3),
    TASK((byte) 4),
    ACCEPT((byte) 5),
    ACK((byte) 6),
    NACK((byte) 7),
    JOIN((byte) 20),
    JOINED((byte) 21),
    REMOVE((byte) 22),
    REMOVED((byte) 23),
    ROLL((byte) 30),
    ;
    public final byte code;

    Type(byte code) {
      this.code = code;
    }
  }

  enum Format {
    BYTES((byte) 0),
    UTF8((byte) 1),
    SBE((byte) 2),
    JSON((byte) 3),
    JSON_ARRAY((byte) 4),
    JSONB((byte) 5),
    JSONB_ARRAY((byte) 6),
    FURY((byte) 7),
    YAML((byte) 8),
    ;
    public final byte code;

    Format(byte code) {
      this.code = code;
    }
  }

  static WithTopic allocate(int capacity, Type type) {
    return allocate(capacity, type.code);
  }

  static WithTopic allocate(int capacity, byte type) {
    return EventImpl.allocate(capacity, type);
  }

  WithTopic reset(byte type);

  int size();

  byte type();

  byte flags();

  int topicOffset();

  int topicSize();

  long id();

  int source();

  int sourcePort();

  long sourceId();

  int sourceStarted();

  String topic();

  byte[] topicBytes();

  long topicAddress();

  long topicHash();

  long time();

  int headerSize();

  int headerOffset();

  String header();

  byte[] headerBytes();

  int headerCopyTo(byte[] buf);

  int headerCopyTo(byte[] buf, int offset, int length);

  int headerCopyTo(long address, int length);

  long headerAddress();

  byte format();

  int bodySize();

  int bodyOffset();

  String body();

  byte[] bodyBytes();

  int bodyCopyTo(byte[] buf);

  int bodyCopyTo(byte[] buf, int offset, int length);

  int bodyCopyTo(long address, int length);

  long bodyAddress();

  sealed interface Builder extends Event permits EventImpl, Command.Builder {

    Builder size(int size);

    Builder type(byte type);

    Builder flags(byte flags);

    Builder id(long id);

    Builder time(long time);

    Builder source(int source);

    Builder sourcePort(int port);

    Builder sourceId(long sourceId);

    Builder sourceStarted(int sourceStarted);

    Builder format(byte format);

    default Builder format(Format format) {
      format(format.code);
      return this;
    }

    Builder bodyFlags(byte bodyFlags);
  }

  sealed interface WithTopic extends AutoCloseable permits Command.WithTopic, EventImpl {
    WithHeader topic(Topic topic);

    WithHeader topic(String topicName);

    WithHeader topic(byte[] topicName);

    WithHeader topic(byte[] topicName, int offset, int length);

    WithHeader topic(long address, int length);

    WithHeader topic(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithBody permits EventImpl, Command.WithBody, WithHeader {
    Builder body(String value);

    Builder body(byte[] value);

    Builder body(byte[] value, int offset, int length);

    Builder body(long address, int length);

    Builder body(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithHeader extends WithBody
      permits Command.WithActorID, Command.WithDedupe, EventImpl {
    WithBody header(String value);

    WithBody header(byte[] value);

    WithBody header(byte[] value, int offset, int length);

    WithBody header(long address, int length);

    WithBody header(DirectBuffer buffer, int offset, int length);
  }
}
