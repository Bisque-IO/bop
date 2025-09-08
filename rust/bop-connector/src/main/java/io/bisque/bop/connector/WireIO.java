package io.bisque.bop.connector;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.*;

/**
 * Little-endian wire I/O helpers and primitive/collection serializers.
 */
final class WireIO {
    private WireIO() {}

    static final class WireException extends IOException {
        WireException(String message) { super(message); }
        WireException(String message, Throwable cause) { super(message, cause); }
    }

    static final class Writer {
        private final ByteArrayOutputStream out;

        Writer() { this.out = new ByteArrayOutputStream(); }

        void writeU8(int v) { out.write(v & 0xFF); }
        void writeBool(boolean b) { writeU8(b ? 1 : 0); }

        void writeU16(int v) {
            out.write(v & 0xFF);
            out.write((v >>> 8) & 0xFF);
        }

        void writeU32(long v) {
            out.write((int)(v & 0xFF));
            out.write((int)((v >>> 8) & 0xFF));
            out.write((int)((v >>> 16) & 0xFF));
            out.write((int)((v >>> 24) & 0xFF));
        }

        void writeU64(long v) {
            out.write((int)(v & 0xFF));
            out.write((int)((v >>> 8) & 0xFF));
            out.write((int)((v >>> 16) & 0xFF));
            out.write((int)((v >>> 24) & 0xFF));
            out.write((int)((v >>> 32) & 0xFF));
            out.write((int)((v >>> 40) & 0xFF));
            out.write((int)((v >>> 48) & 0xFF));
            out.write((int)((v >>> 56) & 0xFF));
        }

        void writeI64(long v) { writeU64(v); }

        void writeUSize(long v) { writeU64(v); }

        void writeBytes(byte[] data) {
            writeU32(data.length);
            out.write(data, 0, data.length);
        }

        void writeString(String s) {
            byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
            writeU32(bytes.length);
            out.write(bytes, 0, bytes.length);
        }

        byte[] toByteArray() { return out.toByteArray(); }
        void writeRaw(byte[] data) { out.write(data, 0, data.length); }
    }

    static final class Reader {
        private final ByteArrayInputStream in;

        Reader(byte[] data) { this.in = new ByteArrayInputStream(data); }

        int readU8() throws IOException {
            int b = in.read();
            if (b < 0) throw new EOFException();
            return b;
        }

        boolean readBool() throws IOException {
            int v = readU8();
            if (v == 0) return false;
            if (v == 1) return true;
            throw new WireException("Invalid bool value: " + v);
        }

        int readU16() throws IOException {
            int b0 = readU8();
            int b1 = readU8();
            return b0 | (b1 << 8);
        }

        long readU32() throws IOException {
            long b0 = readU8();
            long b1 = readU8();
            long b2 = readU8();
            long b3 = readU8();
            return (b0) | (b1 << 8) | (b2 << 16) | (b3 << 24);
        }

        long readU64() throws IOException {
            long b0 = readU8();
            long b1 = readU8();
            long b2 = readU8();
            long b3 = readU8();
            long b4 = readU8();
            long b5 = readU8();
            long b6 = readU8();
            long b7 = readU8();
            return (b0) | (b1 << 8) | (b2 << 16) | (b3 << 24)
                    | (b4 << 32) | (b5 << 40) | (b6 << 48) | (b7 << 56);
        }

        long readI64() throws IOException { return readU64(); }
        long readUSize() throws IOException { return readU64(); }

        byte[] readBytes() throws IOException {
            int len = (int) readU32();
            byte[] out = in.readNBytes(len);
            if (out.length != len) throw new EOFException();
            return out;
        }

        String readString() throws IOException {
            int len = (int) readU32();
            byte[] bytes = in.readNBytes(len);
            if (bytes.length != len) throw new EOFException();
            return new String(bytes, StandardCharsets.UTF_8);
        }

        int remaining() { return in.available(); }
    }

    static void writeVecBytes(Writer w, List<byte[]> vec) {
        w.writeU32(vec.size());
        for (byte[] b : vec) w.writeBytes(b);
    }

    static List<byte[]> readVecBytes(Reader r) throws IOException {
        int len = (int) r.readU32();
        List<byte[]> out = new ArrayList<>(len);
        for (int i = 0; i < len; i++) out.add(r.readBytes());
        return out;
    }

    static void writeVecString(Writer w, List<String> vec) {
        w.writeU32(vec.size());
        for (String s : vec) w.writeString(s);
    }

    static List<String> readVecString(Reader r) throws IOException {
        int len = (int) r.readU32();
        List<String> out = new ArrayList<>(len);
        for (int i = 0; i < len; i++) out.add(r.readString());
        return out;
    }

    static void writePairBytes(Writer w, byte[] a, byte[] b) {
        w.writeBytes(a);
        w.writeBytes(b);
    }

    static Map<byte[], Set<byte[]>> readMapBytesToSet(Reader r) throws IOException {
        int len = (int) r.readU32();
        Map<byte[], Set<byte[]>> map = new LinkedHashMap<>(len);
        for (int i = 0; i < len; i++) {
            byte[] key = r.readBytes();
            int setLen = (int) r.readU32();
            Set<byte[]> set = new LinkedHashSet<>(setLen);
            for (int j = 0; j < setLen; j++) set.add(r.readBytes());
            map.put(key, set);
        }
        return map;
    }

    static void writeMapBytesToSet(Writer w, Map<byte[], Set<byte[]>> map) {
        w.writeU32(map.size());
        for (Map.Entry<byte[], Set<byte[]>> e : map.entrySet()) {
            w.writeBytes(e.getKey());
            Set<byte[]> set = e.getValue();
            w.writeU32(set.size());
            for (byte[] v : set) w.writeBytes(v);
        }
    }
}

