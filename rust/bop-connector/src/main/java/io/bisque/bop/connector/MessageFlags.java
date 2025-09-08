package io.bisque.bop.connector;

public final class MessageFlags {
    public static final MessageFlags REQUEST = new MessageFlags(0x01);
    public static final MessageFlags RESPONSE = new MessageFlags(0x02);
    public static final MessageFlags PING = new MessageFlags(0x04);
    public static final MessageFlags PONG = new MessageFlags(0x08);

    private final int value; // u8

    public MessageFlags(int value) { this.value = value & 0xFF; }
    public int asU8() { return value; }
    public static MessageFlags ofU8(int v) { return new MessageFlags(v); }

    public MessageFlags or(MessageFlags other) { return new MessageFlags(this.value | other.value); }
    public boolean contains(MessageFlags flag) { return (this.value & flag.value) != 0; }

    @Override public String toString() { return "MessageFlags(0x" + Integer.toHexString(value) + ")"; }
}

