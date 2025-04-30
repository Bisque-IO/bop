package bop.cluster.message;

import bop.cluster.Topic;
import org.agrona.DirectBuffer;

public sealed interface Command extends Event permits Command.Builder, CommandImpl {

  enum Type {
    ACTION((byte) 1),
    QUEUE((byte) 2),
    ACTOR((byte) 3),
    ;
    public final byte code;

    Type(byte code) {
      this.code = code;
    }
  }

  enum NAKReason {
    BUSY,
    TIMEOUT,
    REJECTED,
  }

  static WithTopic allocate(int capacity, Type type) {
    return allocate(capacity, type.code);
  }

  static WithTopic allocate(int capacity, byte type) {
    return CommandImpl.allocate(capacity, type);
  }

  long deadline();

  long dedupeHash();

  long actorIDHash();

  int attempt();

  int maxAttempts();

  int dedupeSize();

  int actorIDSize();

  int dedupeOffset();

  int actorIDOffset();

  String dedupe();

  byte[] dedupeBytes();

  int dedupeCopyTo(byte[] buf);

  int dedupeCopyTo(byte[] buf, int offset, int length);

  int dedupeCopyTo(long address, int length);

  long dedupeAddress();

  String actorID();

  byte[] actorIDBytes();

  int actorIDCopyTo(byte[] buf);

  int actorIDCopyTo(byte[] buf, int offset, int length);

  int actorIDCopyTo(long address, int length);

  long actorIDAddress();

  sealed interface Builder extends Command, Event.Builder permits CommandImpl {
    Command.Builder deadline(long deadline);

    Command.Builder dedupeHash(long hash);

    Command.Builder actorIDHash(long hash);

    Command.Builder attempt(int attempt);

    Command.Builder maxAttempts(int maxAttempts);
  }

  sealed interface WithTopic extends Event.WithTopic, AutoCloseable permits CommandImpl {
    WithDedupe topic(Topic topic);

    WithDedupe topic(String topicName);

    WithDedupe topic(byte[] topicName);

    WithDedupe topic(byte[] topicName, int offset, int length);

    WithDedupe topic(long address, int length);

    WithDedupe topic(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithDedupe extends Event.WithHeader, WithActorID, WithHeader, WithBody
      permits CommandImpl {
    WithActorID dedupe(String topicName);

    WithActorID dedupe(byte[] topicName);

    WithActorID dedupe(byte[] topicName, int offset, int length);

    WithActorID dedupe(long address, int length);

    WithActorID dedupe(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithActorID extends Event.WithHeader, WithHeader, WithBody
      permits WithDedupe, CommandImpl {
    WithHeader actorID(String topicName);

    WithHeader actorID(byte[] topicName);

    WithHeader actorID(byte[] topicName, int offset, int length);

    WithHeader actorID(long address, int length);

    WithHeader actorID(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithBody extends Event.WithBody
      permits WithActorID, WithDedupe, WithHeader, CommandImpl {
    Builder body(String value);

    Builder body(byte[] value);

    Builder body(byte[] value, int offset, int length);

    Builder body(long address, int length);

    Builder body(DirectBuffer buffer, int offset, int length);
  }

  sealed interface WithHeader extends WithBody permits WithActorID, WithDedupe, CommandImpl {
    WithBody header(String value);

    WithBody header(byte[] value);

    WithBody header(byte[] value, int offset, int length);

    WithBody header(long address, int length);

    WithBody header(DirectBuffer buffer, int offset, int length);
  }
}
