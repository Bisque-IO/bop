package io.bisque.bop.connector;

import java.io.IOException;
import java.util.zip.CRC32;

/**
 * Framed message with checksum, size, version, flags, kind, requestId, and payload.
 * Generic version for simple payloads and a specialized path for Message (kind-aware).
 */
public final class FramedMessage<T> {
    public final int protocolVersion; // u8
    public final MessageFlags flags;  // u8
    public final int messageKind;     // u16
    public final long requestId;      // u32
    public final T payload;

    public FramedMessage(int protocolVersion, MessageFlags flags, int messageKind, long requestId, T payload) {
        this.protocolVersion = protocolVersion & 0xFF;
        this.flags = flags;
        this.messageKind = messageKind & 0xFFFF;
        this.requestId = requestId & 0xFFFFFFFFL;
        this.payload = payload;
    }

    public byte[] serialize(GenericPayloadSerializer<T> serializer) {
        WireIO.Writer w = new WireIO.Writer();
        // Build header and payload into temp buffer (without checksum + size)
        WireIO.Writer temp = new WireIO.Writer();
        temp.writeU8(protocolVersion);
        temp.writeU8(flags.asU8());
        temp.writeU16(messageKind);
        temp.writeU32(requestId);
        serializer.write(payload, temp);

        byte[] tempBytes = temp.toByteArray();
        long totalSize = 4 + 4 + tempBytes.length; // checksum + size + rest

        // Compute CRC32 over size (LE) + rest
        CRC32 crc = new CRC32();
        WireIO.Writer sizeOnly = new WireIO.Writer();
        sizeOnly.writeU32(totalSize);
        crc.update(sizeOnly.toByteArray());
        crc.update(tempBytes);
        long checksum = crc.getValue() & 0xFFFFFFFFL;

        // Emit full frame
        w.writeU32(checksum);
        w.writeU32(totalSize);
        w.writeRaw(tempBytes);
        return w.toByteArray();
    }

    public static <T> FramedMessage<T> deserialize(byte[] data, GenericPayloadDeserializer<T> deserializer) throws IOException {
        WireIO.Reader r = new WireIO.Reader(data);
        long storedCrc = r.readU32();
        long size = r.readU32();
        int remain = (int) (size - 8);
        byte[] rest = new byte[remain];
        for (int i = 0; i < remain; i++) rest[i] = (byte) r.readU8();

        CRC32 crc = new CRC32();
        WireIO.Writer sizeOnly = new WireIO.Writer();
        sizeOnly.writeU32(size);
        crc.update(sizeOnly.toByteArray());
        crc.update(rest);
        long calc = crc.getValue() & 0xFFFFFFFFL;
        if (calc != storedCrc) throw new WireIO.WireException("Checksum mismatch: expected " + storedCrc + ", got " + calc);

        WireIO.Reader rr = new WireIO.Reader(rest);
        int version = rr.readU8();
        MessageFlags flags = MessageFlags.ofU8(rr.readU8());
        int kind = rr.readU16();
        long reqId = rr.readU32();
        T payload = deserializer.read(kind, rr);
        return new FramedMessage<>(version, flags, kind, reqId, payload);
    }

    // Specialization for Message (kind-aware)
    public byte[] serializeMessage() {
        return serialize(new GenericPayloadSerializer<>() {
            @Override public void write(Message payload, WireIO.Writer w) {
                Message.writePayload(payload, w);
            }
        });
    }

    public static FramedMessage<Message> deserializeMessage(byte[] data) throws IOException {
        return deserialize(data, new GenericPayloadDeserializer<>() {
            @Override public Message read(int kind, WireIO.Reader r) throws IOException {
                return Message.readPayload(kind, r);
            }
        });
    }

    public interface GenericPayloadSerializer<T> { void write(T payload, WireIO.Writer w); }
    public interface GenericPayloadDeserializer<T> { T read(int kind, WireIO.Reader r) throws IOException; }
}

