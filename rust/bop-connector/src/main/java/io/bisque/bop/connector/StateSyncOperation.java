package io.bisque.bop.connector;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

/**
 * Represents a single state synchronization operation.
 * Mirrors the variants in rust `StateSyncOperation` with u8 tag.
 */
public sealed interface StateSyncOperation permits StateSyncOperation.MapCreated, StateSyncOperation.MapUpdated,
        StateSyncOperation.MapDeleted, StateSyncOperation.SetCreated, StateSyncOperation.SetUpdated,
        StateSyncOperation.SetDeleted, StateSyncOperation.CounterCreated, StateSyncOperation.CounterUpdated,
        StateSyncOperation.CounterDeleted, StateSyncOperation.MultimapCreated, StateSyncOperation.MultimapUpdated,
        StateSyncOperation.MultimapKeyCleared, StateSyncOperation.MultimapDeleted, StateSyncOperation.LockCreated,
        StateSyncOperation.LockAcquired, StateSyncOperation.LockReleased, StateSyncOperation.LockDeleted,
        StateSyncOperation.NodeJoined, StateSyncOperation.NodeLeft, StateSyncOperation.NodeConnectionAdded,
        StateSyncOperation.NodeConnectionRemoved {

    // Tags (u8) following the Rust order
    int TAG_MAP_CREATED = 0;
    int TAG_MAP_UPDATED = 1;
    int TAG_MAP_DELETED = 2;
    int TAG_SET_CREATED = 3;
    int TAG_SET_UPDATED = 4;
    int TAG_SET_DELETED = 5;
    int TAG_COUNTER_CREATED = 6;
    int TAG_COUNTER_UPDATED = 7;
    int TAG_COUNTER_DELETED = 8;
    int TAG_MULTIMAP_CREATED = 9;
    int TAG_MULTIMAP_UPDATED = 10;
    int TAG_MULTIMAP_KEY_CLEARED = 11;
    int TAG_MULTIMAP_DELETED = 12;
    int TAG_LOCK_CREATED = 13;
    int TAG_LOCK_ACQUIRED = 14;
    int TAG_LOCK_RELEASED = 15;
    int TAG_LOCK_DELETED = 16;
    int TAG_NODE_JOINED = 17;
    int TAG_NODE_LEFT = 18;
    int TAG_NODE_CONN_ADDED = 19;
    int TAG_NODE_CONN_REMOVED = 20;

    static void serialize(StateSyncOperation op, WireIO.Writer w) {
        if (op instanceof MapCreated v) {
            w.writeU8(TAG_MAP_CREATED);
            w.writeString(v.mapName);
        } else if (op instanceof MapUpdated v) {
            w.writeU8(TAG_MAP_UPDATED);
            w.writeString(v.mapName);
            w.writeBytes(v.key);
            if (v.value == null) {
                w.writeU8(0);
            } else {
                w.writeU8(1);
                w.writeBytes(v.value);
            }
        } else if (op instanceof MapDeleted v) {
            w.writeU8(TAG_MAP_DELETED);
            w.writeString(v.mapName);
        } else if (op instanceof SetCreated v) {
            w.writeU8(TAG_SET_CREATED);
            w.writeString(v.setName);
        } else if (op instanceof SetUpdated v) {
            w.writeU8(TAG_SET_UPDATED);
            w.writeString(v.setName);
            w.writeBytes(v.value);
            w.writeBool(v.added);
        } else if (op instanceof SetDeleted v) {
            w.writeU8(TAG_SET_DELETED);
            w.writeString(v.setName);
        } else if (op instanceof CounterCreated v) {
            w.writeU8(TAG_COUNTER_CREATED);
            w.writeString(v.counterName);
        } else if (op instanceof CounterUpdated v) {
            w.writeU8(TAG_COUNTER_UPDATED);
            w.writeString(v.counterName);
            w.writeI64(v.value);
        } else if (op instanceof CounterDeleted v) {
            w.writeU8(TAG_COUNTER_DELETED);
            w.writeString(v.counterName);
        } else if (op instanceof MultimapCreated v) {
            w.writeU8(TAG_MULTIMAP_CREATED);
            w.writeString(v.multimapName);
        } else if (op instanceof MultimapUpdated v) {
            w.writeU8(TAG_MULTIMAP_UPDATED);
            w.writeString(v.multimapName);
            w.writeBytes(v.key);
            w.writeBytes(v.value);
            w.writeBool(v.added);
        } else if (op instanceof MultimapKeyCleared v) {
            w.writeU8(TAG_MULTIMAP_KEY_CLEARED);
            w.writeString(v.multimapName);
            w.writeBytes(v.key);
        } else if (op instanceof MultimapDeleted v) {
            w.writeU8(TAG_MULTIMAP_DELETED);
            w.writeString(v.multimapName);
        } else if (op instanceof LockCreated v) {
            w.writeU8(TAG_LOCK_CREATED);
            w.writeString(v.lockName);
        } else if (op instanceof LockAcquired v) {
            w.writeU8(TAG_LOCK_ACQUIRED);
            w.writeString(v.lockName);
            w.writeString(v.nodeId);
        } else if (op instanceof LockReleased v) {
            w.writeU8(TAG_LOCK_RELEASED);
            w.writeString(v.lockName);
            w.writeString(v.nodeId);
        } else if (op instanceof LockDeleted v) {
            w.writeU8(TAG_LOCK_DELETED);
            w.writeString(v.lockName);
        } else if (op instanceof NodeJoined v) {
            w.writeU8(TAG_NODE_JOINED);
            w.writeString(v.nodeId);
        } else if (op instanceof NodeLeft v) {
            w.writeU8(TAG_NODE_LEFT);
            w.writeString(v.nodeId);
        } else if (op instanceof NodeConnectionAdded v) {
            w.writeU8(TAG_NODE_CONN_ADDED);
            w.writeString(v.nodeId);
            w.writeU64(v.connectionId);
        } else if (op instanceof NodeConnectionRemoved v) {
            w.writeU8(TAG_NODE_CONN_REMOVED);
            w.writeString(v.nodeId);
            w.writeU64(v.connectionId);
        } else {
            throw new IllegalArgumentException("Unknown op: " + op);
        }
    }

    static StateSyncOperation deserialize(WireIO.Reader r) throws IOException {
        int tag = r.readU8();
        return switch (tag) {
            case TAG_MAP_CREATED -> new MapCreated(r.readString());
            case TAG_MAP_UPDATED -> new MapUpdated(r.readString(), r.readBytes(), (r.readU8() == 0) ? null : r.readBytes());
            case TAG_MAP_DELETED -> new MapDeleted(r.readString());
            case TAG_SET_CREATED -> new SetCreated(r.readString());
            case TAG_SET_UPDATED -> new SetUpdated(r.readString(), r.readBytes(), r.readBool());
            case TAG_SET_DELETED -> new SetDeleted(r.readString());
            case TAG_COUNTER_CREATED -> new CounterCreated(r.readString());
            case TAG_COUNTER_UPDATED -> new CounterUpdated(r.readString(), r.readI64());
            case TAG_COUNTER_DELETED -> new CounterDeleted(r.readString());
            case TAG_MULTIMAP_CREATED -> new MultimapCreated(r.readString());
            case TAG_MULTIMAP_UPDATED -> new MultimapUpdated(r.readString(), r.readBytes(), r.readBytes(), r.readBool());
            case TAG_MULTIMAP_KEY_CLEARED -> new MultimapKeyCleared(r.readString(), r.readBytes());
            case TAG_MULTIMAP_DELETED -> new MultimapDeleted(r.readString());
            case TAG_LOCK_CREATED -> new LockCreated(r.readString());
            case TAG_LOCK_ACQUIRED -> new LockAcquired(r.readString(), r.readString());
            case TAG_LOCK_RELEASED -> new LockReleased(r.readString(), r.readString());
            case TAG_LOCK_DELETED -> new LockDeleted(r.readString());
            case TAG_NODE_JOINED -> new NodeJoined(r.readString());
            case TAG_NODE_LEFT -> new NodeLeft(r.readString());
            case TAG_NODE_CONN_ADDED -> new NodeConnectionAdded(r.readString(), r.readU64());
            case TAG_NODE_CONN_REMOVED -> new NodeConnectionRemoved(r.readString(), r.readU64());
            default -> throw new WireIO.WireException("Unknown StateSyncOperation tag: " + tag);
        };
    }

    static void writeVec(WireIO.Writer w, List<StateSyncOperation> ops) {
        w.writeU32(ops.size());
        for (StateSyncOperation op : ops) serialize(op, w);
    }

    static List<StateSyncOperation> readVec(WireIO.Reader r) throws IOException {
        int len = (int) r.readU32();
        List<StateSyncOperation> list = new ArrayList<>(len);
        for (int i = 0; i < len; i++) list.add(deserialize(r));
        return list;
    }

    record MapCreated(String mapName) implements StateSyncOperation {}
    record MapUpdated(String mapName, byte[] key, byte[] value) implements StateSyncOperation {}
    record MapDeleted(String mapName) implements StateSyncOperation {}
    record SetCreated(String setName) implements StateSyncOperation {}
    record SetUpdated(String setName, byte[] value, boolean added) implements StateSyncOperation {}
    record SetDeleted(String setName) implements StateSyncOperation {}
    record CounterCreated(String counterName) implements StateSyncOperation {}
    record CounterUpdated(String counterName, long value) implements StateSyncOperation {}
    record CounterDeleted(String counterName) implements StateSyncOperation {}
    record MultimapCreated(String multimapName) implements StateSyncOperation {}
    record MultimapUpdated(String multimapName, byte[] key, byte[] value, boolean added) implements StateSyncOperation {}
    record MultimapKeyCleared(String multimapName, byte[] key) implements StateSyncOperation {}
    record MultimapDeleted(String multimapName) implements StateSyncOperation {}
    record LockCreated(String lockName) implements StateSyncOperation {}
    record LockAcquired(String lockName, String nodeId) implements StateSyncOperation {}
    record LockReleased(String lockName, String nodeId) implements StateSyncOperation {}
    record LockDeleted(String lockName) implements StateSyncOperation {}
    record NodeJoined(String nodeId) implements StateSyncOperation {}
    record NodeLeft(String nodeId) implements StateSyncOperation {}
    record NodeConnectionAdded(String nodeId, long connectionId) implements StateSyncOperation {}
    record NodeConnectionRemoved(String nodeId, long connectionId) implements StateSyncOperation {}
}

