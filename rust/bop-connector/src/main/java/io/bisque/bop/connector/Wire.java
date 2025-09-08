package io.bisque.bop.connector;

import java.io.IOException;

/**
 * Convenience helpers equivalent to Rust's serialize_message and deserialize_message_specialized.
 */
public final class Wire {
    private Wire() {}

    public static byte[] serializeMessage(FramedMessage<Message> frame) {
        return frame.serializeMessage();
    }

    public static FramedMessage<Message> deserializeMessage(byte[] data) throws IOException {
        return FramedMessage.deserializeMessage(data);
    }
}

