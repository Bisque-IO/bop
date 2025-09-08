package io.bisque.bop.connector;

import java.io.IOException;
import java.util.*;

/**
 * Unified message type and its payload serialization.
 * Mirrors Rust enum Message variants + kind IDs.
 */
public interface Message {

    // Cluster management
    record JoinRequest(String nodeId) implements Message {}
    record JoinResponse(ResponseCode code, long connectionId) implements Message {}
    record LeaveRequest(String nodeId) implements Message {}
    record LeaveResponse(ResponseCode code) implements Message {}
    record GetNodesRequest() implements Message {}
    record GetNodesResponse(ResponseCode code, List<String> nodes) implements Message {}

    // AsyncMap
    record CreateMapRequest(String name) implements Message {}
    record CreateMapResponse(ResponseCode code, String mapName) implements Message {}
    record MapPutRequest(String mapName, byte[] key, byte[] value) implements Message {}
    record MapPutResponse(ResponseCode code) implements Message {}
    record MapGetRequest(String mapName, byte[] key) implements Message {}
    record MapGetResponse(ResponseCode code, byte[] valueOrNull) implements Message {}
    record MapRemoveRequest(String mapName, byte[] key) implements Message {}
    record MapRemoveResponse(ResponseCode code) implements Message {}
    record MapSizeRequest(String mapName) implements Message {}
    record MapSizeResponse(ResponseCode code, long size) implements Message {}

    // AsyncSet
    record CreateSetRequest(String name) implements Message {}
    record CreateSetResponse(ResponseCode code, String setName) implements Message {}
    record SetAddRequest(String setName, byte[] value) implements Message {}
    record SetAddResponse(ResponseCode code) implements Message {}
    record SetRemoveRequest(String setName, byte[] value) implements Message {}
    record SetRemoveResponse(ResponseCode code) implements Message {}
    record SetContainsRequest(String setName, byte[] value) implements Message {}
    record SetContainsResponse(ResponseCode code, boolean contains) implements Message {}
    record SetSizeRequest(String setName) implements Message {}
    record SetSizeResponse(ResponseCode code, long size) implements Message {}

    // AsyncCounter
    record CreateCounterRequest(String name) implements Message {}
    record CreateCounterResponse(ResponseCode code, String counterName) implements Message {}
    record CounterGetRequest(String counterName) implements Message {}
    record CounterGetResponse(ResponseCode code, long value) implements Message {}
    record CounterIncrementRequest(String counterName, long delta) implements Message {}
    record CounterIncrementResponse(ResponseCode code, long newValue) implements Message {}

    // AsyncLock
    record CreateLockRequest(String name) implements Message {}
    record CreateLockResponse(ResponseCode code, String lockName) implements Message {}
    record LockAcquireRequest(String lockName) implements Message {}
    record LockAcquireResponse(ResponseCode code, boolean acquired, Long queuePositionOrNull) implements Message {}
    record LockReleaseRequest(String lockName) implements Message {}
    record LockReleaseResponse(ResponseCode code) implements Message {}
    record LockSyncRequest(List<String> heldLocks) implements Message {}
    record LockSyncResponse(ResponseCode code, List<String> synchronizedLocks, List<String> failedLocks) implements Message {}

    // AsyncMultimap
    record CreateMultimapRequest(String name) implements Message {}
    record CreateMultimapResponse(ResponseCode code, String multimapName) implements Message {}
    record MultimapPutRequest(String multimapName, byte[] key, byte[] value) implements Message {}
    record MultimapPutResponse(ResponseCode code) implements Message {}
    record MultimapGetRequest(String multimapName, byte[] key) implements Message {}
    record MultimapGetResponse(ResponseCode code, Set<byte[]> values) implements Message {}
    record MultimapRemoveRequest(String multimapName, byte[] key) implements Message {}
    record MultimapRemoveResponse(ResponseCode code) implements Message {}
    record MultimapRemoveValueRequest(String multimapName, byte[] key, byte[] value) implements Message {}
    record MultimapRemoveValueResponse(ResponseCode code) implements Message {}
    record MultimapSizeRequest(String multimapName) implements Message {}
    record MultimapSizeResponse(ResponseCode code, long size) implements Message {}
    record MultimapKeySizeRequest(String multimapName, byte[] key) implements Message {}
    record MultimapKeySizeResponse(ResponseCode code, long size) implements Message {}
    record MultimapContainsKeyRequest(String multimapName, byte[] key) implements Message {}
    record MultimapContainsKeyResponse(ResponseCode code, boolean contains) implements Message {}
    record MultimapContainsValueRequest(String multimapName, byte[] value) implements Message {}
    record MultimapContainsValueResponse(ResponseCode code, boolean contains) implements Message {}
    record MultimapContainsEntryRequest(String multimapName, byte[] key, byte[] value) implements Message {}
    record MultimapContainsEntryResponse(ResponseCode code, boolean contains) implements Message {}
    record MultimapKeysRequest(String multimapName) implements Message {}
    record MultimapKeysResponse(ResponseCode code, Set<byte[]> keys) implements Message {}
    record MultimapValuesRequest(String multimapName) implements Message {}
    record MultimapValuesResponse(ResponseCode code, List<byte[]> values) implements Message {}
    record MultimapEntriesRequest(String multimapName) implements Message {}
    record MultimapEntriesResponse(ResponseCode code, Map<byte[], Set<byte[]>> entries) implements Message {}
    record MultimapClearRequest(String multimapName) implements Message {}
    record MultimapClearResponse(ResponseCode code) implements Message {}

    // Error and control
    record ErrorResponse(ResponseCode code, String error) implements Message {}
    record LeaderRedirectResponse(ResponseCode code, String leaderAddress) implements Message {}
    record Ping() implements Message {}
    record Pong() implements Message {}
    record StateSync(long syncId, List<StateSyncOperation> operations) implements Message {}
    record StateSyncAck(ResponseCode code, long syncId) implements Message {}

    // Helpers for mapping to/from kind and payload ser/de
    static int kindOf(Message msg) {
        return switch (msg) {
            case JoinRequest m -> MessageKind.MSG_JOIN_REQUEST;
            case JoinResponse m -> MessageKind.MSG_JOIN_RESPONSE;
            case LeaveRequest m -> MessageKind.MSG_LEAVE_REQUEST;
            case LeaveResponse m -> MessageKind.MSG_LEAVE_RESPONSE;
            case GetNodesRequest m -> MessageKind.MSG_GET_NODES_REQUEST;
            case GetNodesResponse m -> MessageKind.MSG_GET_NODES_RESPONSE;
            case CreateMapRequest m -> MessageKind.MSG_CREATE_MAP_REQUEST;
            case CreateMapResponse m -> MessageKind.MSG_CREATE_MAP_RESPONSE;
            case MapPutRequest m -> MessageKind.MSG_MAP_PUT_REQUEST;
            case MapPutResponse m -> MessageKind.MSG_MAP_PUT_RESPONSE;
            case MapGetRequest m -> MessageKind.MSG_MAP_GET_REQUEST;
            case MapGetResponse m -> MessageKind.MSG_MAP_GET_RESPONSE;
            case MapRemoveRequest m -> MessageKind.MSG_MAP_REMOVE_REQUEST;
            case MapRemoveResponse m -> MessageKind.MSG_MAP_REMOVE_RESPONSE;
            case MapSizeRequest m -> MessageKind.MSG_MAP_SIZE_REQUEST;
            case MapSizeResponse m -> MessageKind.MSG_MAP_SIZE_RESPONSE;
            case CreateSetRequest m -> MessageKind.MSG_CREATE_SET_REQUEST;
            case CreateSetResponse m -> MessageKind.MSG_CREATE_SET_RESPONSE;
            case SetAddRequest m -> MessageKind.MSG_SET_ADD_REQUEST;
            case SetAddResponse m -> MessageKind.MSG_SET_ADD_RESPONSE;
            case SetRemoveRequest m -> MessageKind.MSG_SET_REMOVE_REQUEST;
            case SetRemoveResponse m -> MessageKind.MSG_SET_REMOVE_RESPONSE;
            case SetContainsRequest m -> MessageKind.MSG_SET_CONTAINS_REQUEST;
            case SetContainsResponse m -> MessageKind.MSG_SET_CONTAINS_RESPONSE;
            case SetSizeRequest m -> MessageKind.MSG_SET_SIZE_REQUEST;
            case SetSizeResponse m -> MessageKind.MSG_SET_SIZE_RESPONSE;
            case CreateCounterRequest m -> MessageKind.MSG_CREATE_COUNTER_REQUEST;
            case CreateCounterResponse m -> MessageKind.MSG_CREATE_COUNTER_RESPONSE;
            case CounterGetRequest m -> MessageKind.MSG_COUNTER_GET_REQUEST;
            case CounterGetResponse m -> MessageKind.MSG_COUNTER_GET_RESPONSE;
            case CounterIncrementRequest m -> MessageKind.MSG_COUNTER_INCREMENT_REQUEST;
            case CounterIncrementResponse m -> MessageKind.MSG_COUNTER_INCREMENT_RESPONSE;
            case CreateLockRequest m -> MessageKind.MSG_CREATE_LOCK_REQUEST;
            case CreateLockResponse m -> MessageKind.MSG_CREATE_LOCK_RESPONSE;
            case LockAcquireRequest m -> MessageKind.MSG_LOCK_ACQUIRE_REQUEST;
            case LockAcquireResponse m -> MessageKind.MSG_LOCK_ACQUIRE_RESPONSE;
            case LockReleaseRequest m -> MessageKind.MSG_LOCK_RELEASE_REQUEST;
            case LockReleaseResponse m -> MessageKind.MSG_LOCK_RELEASE_RESPONSE;
            case LockSyncRequest m -> MessageKind.MSG_LOCK_SYNC_REQUEST;
            case LockSyncResponse m -> MessageKind.MSG_LOCK_SYNC_RESPONSE;
            case CreateMultimapRequest m -> MessageKind.MSG_CREATE_MULTIMAP_REQUEST;
            case CreateMultimapResponse m -> MessageKind.MSG_CREATE_MULTIMAP_RESPONSE;
            case MultimapPutRequest m -> MessageKind.MSG_MULTIMAP_PUT_REQUEST;
            case MultimapPutResponse m -> MessageKind.MSG_MULTIMAP_PUT_RESPONSE;
            case MultimapGetRequest m -> MessageKind.MSG_MULTIMAP_GET_REQUEST;
            case MultimapGetResponse m -> MessageKind.MSG_MULTIMAP_GET_RESPONSE;
            case MultimapRemoveRequest m -> MessageKind.MSG_MULTIMAP_REMOVE_REQUEST;
            case MultimapRemoveResponse m -> MessageKind.MSG_MULTIMAP_REMOVE_RESPONSE;
            case MultimapRemoveValueRequest m -> MessageKind.MSG_MULTIMAP_REMOVE_VALUE_REQUEST;
            case MultimapRemoveValueResponse m -> MessageKind.MSG_MULTIMAP_REMOVE_VALUE_RESPONSE;
            case MultimapSizeRequest m -> MessageKind.MSG_MULTIMAP_SIZE_REQUEST;
            case MultimapSizeResponse m -> MessageKind.MSG_MULTIMAP_SIZE_RESPONSE;
            case MultimapKeySizeRequest m -> MessageKind.MSG_MULTIMAP_KEY_SIZE_REQUEST;
            case MultimapKeySizeResponse m -> MessageKind.MSG_MULTIMAP_KEY_SIZE_RESPONSE;
            case MultimapContainsKeyRequest m -> MessageKind.MSG_MULTIMAP_CONTAINS_KEY_REQUEST;
            case MultimapContainsKeyResponse m -> MessageKind.MSG_MULTIMAP_CONTAINS_KEY_RESPONSE;
            case MultimapContainsValueRequest m -> MessageKind.MSG_MULTIMAP_CONTAINS_VALUE_REQUEST;
            case MultimapContainsValueResponse m -> MessageKind.MSG_MULTIMAP_CONTAINS_VALUE_RESPONSE;
            case MultimapContainsEntryRequest m -> MessageKind.MSG_MULTIMAP_CONTAINS_ENTRY_REQUEST;
            case MultimapContainsEntryResponse m -> MessageKind.MSG_MULTIMAP_CONTAINS_ENTRY_RESPONSE;
            case MultimapKeysRequest m -> MessageKind.MSG_MULTIMAP_KEYS_REQUEST;
            case MultimapKeysResponse m -> MessageKind.MSG_MULTIMAP_KEYS_RESPONSE;
            case MultimapValuesRequest m -> MessageKind.MSG_MULTIMAP_VALUES_REQUEST;
            case MultimapValuesResponse m -> MessageKind.MSG_MULTIMAP_VALUES_RESPONSE;
            case MultimapEntriesRequest m -> MessageKind.MSG_MULTIMAP_ENTRIES_REQUEST;
            case MultimapEntriesResponse m -> MessageKind.MSG_MULTIMAP_ENTRIES_RESPONSE;
            case MultimapClearRequest m -> MessageKind.MSG_MULTIMAP_CLEAR_REQUEST;
            case MultimapClearResponse m -> MessageKind.MSG_MULTIMAP_CLEAR_RESPONSE;
            case ErrorResponse m -> MessageKind.MSG_ERROR_RESPONSE;
            case LeaderRedirectResponse m -> MessageKind.MSG_LEADER_REDIRECT_RESPONSE;
            case Ping m -> MessageKind.MSG_PING;
            case Pong m -> MessageKind.MSG_PONG;
            case StateSync m -> MessageKind.MSG_STATE_SYNC;
            case StateSyncAck m -> MessageKind.MSG_STATE_SYNC_ACK;
        };
    }

    static void writePayload(Message msg, WireIO.Writer w) {
        switch (msg) {
            case JoinRequest m -> w.writeString(m.nodeId());
            case JoinResponse m -> { w.writeU8(m.code().asU8()); w.writeU64(m.connectionId()); }
            case LeaveRequest m -> w.writeString(m.nodeId());
            case LeaveResponse m -> w.writeU8(m.code().asU8());
            case GetNodesRequest m -> {}
            case GetNodesResponse m -> { w.writeU8(m.code().asU8()); WireIO.writeVecString(w, m.nodes()); }
            case CreateMapRequest m -> w.writeString(m.name());
            case CreateMapResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.mapName()); }
            case MapPutRequest m -> { w.writeString(m.mapName()); w.writeBytes(m.key()); w.writeBytes(m.value()); }
            case MapPutResponse m -> w.writeU8(m.code().asU8());
            case MapGetRequest m -> { w.writeString(m.mapName()); w.writeBytes(m.key()); }
            case MapGetResponse m -> { w.writeU8(m.code().asU8()); if (m.valueOrNull() == null) { w.writeU8(0); } else { w.writeU8(1); w.writeBytes(m.valueOrNull()); } }
            case MapRemoveRequest m -> { w.writeString(m.mapName()); w.writeBytes(m.key()); }
            case MapRemoveResponse m -> w.writeU8(m.code().asU8());
            case MapSizeRequest m -> w.writeString(m.mapName());
            case MapSizeResponse m -> { w.writeU8(m.code().asU8()); w.writeUSize(m.size()); }
            case CreateSetRequest m -> w.writeString(m.name());
            case CreateSetResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.setName()); }
            case SetAddRequest m -> { w.writeString(m.setName()); w.writeBytes(m.value()); }
            case SetAddResponse m -> w.writeU8(m.code().asU8());
            case SetRemoveRequest m -> { w.writeString(m.setName()); w.writeBytes(m.value()); }
            case SetRemoveResponse m -> w.writeU8(m.code().asU8());
            case SetContainsRequest m -> { w.writeString(m.setName()); w.writeBytes(m.value()); }
            case SetContainsResponse m -> { w.writeU8(m.code().asU8()); w.writeBool(m.contains()); }
            case SetSizeRequest m -> w.writeString(m.setName());
            case SetSizeResponse m -> { w.writeU8(m.code().asU8()); w.writeUSize(m.size()); }
            case CreateCounterRequest m -> w.writeString(m.name());
            case CreateCounterResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.counterName()); }
            case CounterGetRequest m -> w.writeString(m.counterName());
            case CounterGetResponse m -> { w.writeU8(m.code().asU8()); w.writeI64(m.value()); }
            case CounterIncrementRequest m -> { w.writeString(m.counterName()); w.writeI64(m.delta()); }
            case CounterIncrementResponse m -> { w.writeU8(m.code().asU8()); w.writeI64(m.newValue()); }
            case CreateLockRequest m -> w.writeString(m.name());
            case CreateLockResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.lockName()); }
            case LockAcquireRequest m -> w.writeString(m.lockName());
            case LockAcquireResponse m -> { w.writeU8(m.code().asU8()); w.writeBool(m.acquired()); if (m.queuePositionOrNull() == null) { w.writeU8(0); } else { w.writeU8(1); w.writeUSize(m.queuePositionOrNull()); } }
            case LockReleaseRequest m -> w.writeString(m.lockName());
            case LockReleaseResponse m -> w.writeU8(m.code().asU8());
            case LockSyncRequest m -> WireIO.writeVecString(w, m.heldLocks());
            case LockSyncResponse m -> { w.writeU8(m.code().asU8()); WireIO.writeVecString(w, m.synchronizedLocks()); WireIO.writeVecString(w, m.failedLocks()); }
            case CreateMultimapRequest m -> w.writeString(m.name());
            case CreateMultimapResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.multimapName()); }
            case MultimapPutRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); w.writeBytes(m.value()); }
            case MultimapPutResponse m -> w.writeU8(m.code().asU8());
            case MultimapGetRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); }
            case MultimapGetResponse m -> { w.writeU8(m.code().asU8()); w.writeU32(m.values().size()); for (byte[] v : m.values()) w.writeBytes(v); }
            case MultimapRemoveRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); }
            case MultimapRemoveResponse m -> w.writeU8(m.code().asU8());
            case MultimapRemoveValueRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); w.writeBytes(m.value()); }
            case MultimapRemoveValueResponse m -> w.writeU8(m.code().asU8());
            case MultimapSizeRequest m -> w.writeString(m.multimapName());
            case MultimapSizeResponse m -> { w.writeU8(m.code().asU8()); w.writeUSize(m.size()); }
            case MultimapKeySizeRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); }
            case MultimapKeySizeResponse m -> { w.writeU8(m.code().asU8()); w.writeUSize(m.size()); }
            case MultimapContainsKeyRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); }
            case MultimapContainsKeyResponse m -> { w.writeU8(m.code().asU8()); w.writeBool(m.contains()); }
            case MultimapContainsValueRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.value()); }
            case MultimapContainsValueResponse m -> { w.writeU8(m.code().asU8()); w.writeBool(m.contains()); }
            case MultimapContainsEntryRequest m -> { w.writeString(m.multimapName()); w.writeBytes(m.key()); w.writeBytes(m.value()); }
            case MultimapContainsEntryResponse m -> { w.writeU8(m.code().asU8()); w.writeBool(m.contains()); }
            case MultimapKeysRequest m -> w.writeString(m.multimapName());
            case MultimapKeysResponse m -> { w.writeU8(m.code().asU8()); w.writeU32(m.keys().size()); for (byte[] k : m.keys()) w.writeBytes(k); }
            case MultimapValuesRequest m -> w.writeString(m.multimapName());
            case MultimapValuesResponse m -> { w.writeU8(m.code().asU8()); WireIO.writeVecBytes(w, m.values()); }
            case MultimapEntriesRequest m -> w.writeString(m.multimapName());
            case MultimapEntriesResponse m -> { w.writeU8(m.code().asU8()); WireIO.writeMapBytesToSet(w, m.entries()); }
            case MultimapClearRequest m -> w.writeString(m.multimapName());
            case MultimapClearResponse m -> w.writeU8(m.code().asU8());
            case ErrorResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.error()); }
            case LeaderRedirectResponse m -> { w.writeU8(m.code().asU8()); w.writeString(m.leaderAddress()); }
            case Ping m -> {}
            case Pong m -> {}
            case StateSync m -> { w.writeU64(m.syncId()); StateSyncOperation.writeVec(w, m.operations()); }
            case StateSyncAck m -> { w.writeU8(m.code().asU8()); w.writeU64(m.syncId()); }
        }
    }

    static Message readPayload(int kind, WireIO.Reader r) throws IOException {
        return switch (kind) {
            case MessageKind.MSG_JOIN_REQUEST -> new JoinRequest(r.readString());
            case MessageKind.MSG_JOIN_RESPONSE -> new JoinResponse(ResponseCode.fromU8(r.readU8()), r.readU64());
            case MessageKind.MSG_LEAVE_REQUEST -> new LeaveRequest(r.readString());
            case MessageKind.MSG_LEAVE_RESPONSE -> new LeaveResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_GET_NODES_REQUEST -> new GetNodesRequest();
            case MessageKind.MSG_GET_NODES_RESPONSE -> new GetNodesResponse(ResponseCode.fromU8(r.readU8()), WireIO.readVecString(r));
            case MessageKind.MSG_CREATE_MAP_REQUEST -> new CreateMapRequest(r.readString());
            case MessageKind.MSG_CREATE_MAP_RESPONSE -> new CreateMapResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_MAP_PUT_REQUEST -> new MapPutRequest(r.readString(), r.readBytes(), r.readBytes());
            case MessageKind.MSG_MAP_PUT_RESPONSE -> new MapPutResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_MAP_GET_REQUEST -> new MapGetRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MAP_GET_RESPONSE -> new MapGetResponse(ResponseCode.fromU8(r.readU8()), (r.readU8() == 0) ? null : r.readBytes());
            case MessageKind.MSG_MAP_REMOVE_REQUEST -> new MapRemoveRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MAP_REMOVE_RESPONSE -> new MapRemoveResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_MAP_SIZE_REQUEST -> new MapSizeRequest(r.readString());
            case MessageKind.MSG_MAP_SIZE_RESPONSE -> new MapSizeResponse(ResponseCode.fromU8(r.readU8()), r.readUSize());
            case MessageKind.MSG_CREATE_SET_REQUEST -> new CreateSetRequest(r.readString());
            case MessageKind.MSG_CREATE_SET_RESPONSE -> new CreateSetResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_SET_ADD_REQUEST -> new SetAddRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_SET_ADD_RESPONSE -> new SetAddResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_SET_REMOVE_REQUEST -> new SetRemoveRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_SET_REMOVE_RESPONSE -> new SetRemoveResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_SET_CONTAINS_REQUEST -> new SetContainsRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_SET_CONTAINS_RESPONSE -> new SetContainsResponse(ResponseCode.fromU8(r.readU8()), r.readBool());
            case MessageKind.MSG_SET_SIZE_REQUEST -> new SetSizeRequest(r.readString());
            case MessageKind.MSG_SET_SIZE_RESPONSE -> new SetSizeResponse(ResponseCode.fromU8(r.readU8()), r.readUSize());
            case MessageKind.MSG_CREATE_COUNTER_REQUEST -> new CreateCounterRequest(r.readString());
            case MessageKind.MSG_CREATE_COUNTER_RESPONSE -> new CreateCounterResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_COUNTER_GET_REQUEST -> new CounterGetRequest(r.readString());
            case MessageKind.MSG_COUNTER_GET_RESPONSE -> new CounterGetResponse(ResponseCode.fromU8(r.readU8()), r.readI64());
            case MessageKind.MSG_COUNTER_INCREMENT_REQUEST -> new CounterIncrementRequest(r.readString(), r.readI64());
            case MessageKind.MSG_COUNTER_INCREMENT_RESPONSE -> new CounterIncrementResponse(ResponseCode.fromU8(r.readU8()), r.readI64());
            case MessageKind.MSG_CREATE_LOCK_REQUEST -> new CreateLockRequest(r.readString());
            case MessageKind.MSG_CREATE_LOCK_RESPONSE -> new CreateLockResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_LOCK_ACQUIRE_REQUEST -> new LockAcquireRequest(r.readString());
            case MessageKind.MSG_LOCK_ACQUIRE_RESPONSE -> new LockAcquireResponse(ResponseCode.fromU8(r.readU8()), r.readBool(), (r.readU8() == 0) ? null : r.readUSize());
            case MessageKind.MSG_LOCK_RELEASE_REQUEST -> new LockReleaseRequest(r.readString());
            case MessageKind.MSG_LOCK_RELEASE_RESPONSE -> new LockReleaseResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_LOCK_SYNC_REQUEST -> new LockSyncRequest(WireIO.readVecString(r));
            case MessageKind.MSG_LOCK_SYNC_RESPONSE -> new LockSyncResponse(ResponseCode.fromU8(r.readU8()), WireIO.readVecString(r), WireIO.readVecString(r));
            case MessageKind.MSG_CREATE_MULTIMAP_REQUEST -> new CreateMultimapRequest(r.readString());
            case MessageKind.MSG_CREATE_MULTIMAP_RESPONSE -> new CreateMultimapResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_MULTIMAP_PUT_REQUEST -> new MultimapPutRequest(r.readString(), r.readBytes(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_PUT_RESPONSE -> new MultimapPutResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_MULTIMAP_GET_REQUEST -> new MultimapGetRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_GET_RESPONSE -> {
                ResponseCode code = ResponseCode.fromU8(r.readU8());
                int count = (int) r.readU32();
                Set<byte[]> set = new LinkedHashSet<>(count);
                for (int i = 0; i < count; i++) set.add(r.readBytes());
                yield new MultimapGetResponse(code, set);
            }
            case MessageKind.MSG_MULTIMAP_REMOVE_REQUEST -> new MultimapRemoveRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_REMOVE_RESPONSE -> new MultimapRemoveResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_MULTIMAP_REMOVE_VALUE_REQUEST -> new MultimapRemoveValueRequest(r.readString(), r.readBytes(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_REMOVE_VALUE_RESPONSE -> new MultimapRemoveValueResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_MULTIMAP_SIZE_REQUEST -> new MultimapSizeRequest(r.readString());
            case MessageKind.MSG_MULTIMAP_SIZE_RESPONSE -> new MultimapSizeResponse(ResponseCode.fromU8(r.readU8()), r.readUSize());
            case MessageKind.MSG_MULTIMAP_KEY_SIZE_REQUEST -> new MultimapKeySizeRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_KEY_SIZE_RESPONSE -> new MultimapKeySizeResponse(ResponseCode.fromU8(r.readU8()), r.readUSize());
            case MessageKind.MSG_MULTIMAP_CONTAINS_KEY_REQUEST -> new MultimapContainsKeyRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_CONTAINS_KEY_RESPONSE -> new MultimapContainsKeyResponse(ResponseCode.fromU8(r.readU8()), r.readBool());
            case MessageKind.MSG_MULTIMAP_CONTAINS_VALUE_REQUEST -> new MultimapContainsValueRequest(r.readString(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_CONTAINS_VALUE_RESPONSE -> new MultimapContainsValueResponse(ResponseCode.fromU8(r.readU8()), r.readBool());
            case MessageKind.MSG_MULTIMAP_CONTAINS_ENTRY_REQUEST -> new MultimapContainsEntryRequest(r.readString(), r.readBytes(), r.readBytes());
            case MessageKind.MSG_MULTIMAP_CONTAINS_ENTRY_RESPONSE -> new MultimapContainsEntryResponse(ResponseCode.fromU8(r.readU8()), r.readBool());
            case MessageKind.MSG_MULTIMAP_KEYS_REQUEST -> new MultimapKeysRequest(r.readString());
            case MessageKind.MSG_MULTIMAP_KEYS_RESPONSE -> {
                ResponseCode code = ResponseCode.fromU8(r.readU8());
                int count = (int) r.readU32();
                Set<byte[]> keys = new LinkedHashSet<>(count);
                for (int i = 0; i < count; i++) keys.add(r.readBytes());
                yield new MultimapKeysResponse(code, keys);
            }
            case MessageKind.MSG_MULTIMAP_VALUES_REQUEST -> new MultimapValuesRequest(r.readString());
            case MessageKind.MSG_MULTIMAP_VALUES_RESPONSE -> new MultimapValuesResponse(ResponseCode.fromU8(r.readU8()), WireIO.readVecBytes(r));
            case MessageKind.MSG_MULTIMAP_ENTRIES_REQUEST -> new MultimapEntriesRequest(r.readString());
            case MessageKind.MSG_MULTIMAP_ENTRIES_RESPONSE -> new MultimapEntriesResponse(ResponseCode.fromU8(r.readU8()), WireIO.readMapBytesToSet(r));
            case MessageKind.MSG_MULTIMAP_CLEAR_REQUEST -> new MultimapClearRequest(r.readString());
            case MessageKind.MSG_MULTIMAP_CLEAR_RESPONSE -> new MultimapClearResponse(ResponseCode.fromU8(r.readU8()));
            case MessageKind.MSG_ERROR_RESPONSE -> new ErrorResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_LEADER_REDIRECT_RESPONSE -> new LeaderRedirectResponse(ResponseCode.fromU8(r.readU8()), r.readString());
            case MessageKind.MSG_PING -> new Ping();
            case MessageKind.MSG_PONG -> new Pong();
            case MessageKind.MSG_STATE_SYNC -> new StateSync(r.readU64(), StateSyncOperation.readVec(r));
            case MessageKind.MSG_STATE_SYNC_ACK -> new StateSyncAck(ResponseCode.fromU8(r.readU8()), r.readU64());
            default -> throw new WireIO.WireException("Unknown message kind: " + kind);
        };
    }
}

