use bop_rs::aof::error::AofError;
use bop_rs::aof::index::SerializableIndex;
use std::collections::BTreeMap;

#[test]
fn serializable_index_roundtrip_preserves_entries() {
    let mut entries = BTreeMap::new();
    entries.insert(1, 0);
    entries.insert(7, 128);
    entries.insert(42, 1_024);

    let index = SerializableIndex::new(&entries);
    assert_eq!(index.version, 1);
    assert_eq!(index.record_count, entries.len() as u64);

    let bytes = index.serialize().expect("serialize index");
    let decoded = SerializableIndex::deserialize(&bytes).expect("deserialize index");
    let restored = decoded.to_btree_map().expect("restore index map");

    assert_eq!(restored, entries);
}

#[test]
fn serializable_index_detects_checksum_mismatch() {
    let mut entries = BTreeMap::new();
    entries.insert(100, 4_096);
    entries.insert(200, 8_192);

    let mut index = SerializableIndex::new(&entries);
    index.checksum ^= 0xFFFF;

    let result = index.to_btree_map();
    assert!(matches!(result, Err(AofError::CorruptedRecord(_))));
}

#[test]
fn serializable_index_deserialize_rejects_truncated_payload() {
    let mut entries = BTreeMap::new();
    entries.insert(5, 64);
    entries.insert(6, 96);

    let index = SerializableIndex::new(&entries);
    let mut bytes = index.serialize().expect("serialize index");
    bytes.pop();

    let err = SerializableIndex::deserialize(&bytes).expect_err("expected deserialize failure");
    assert!(matches!(err, AofError::Serialization(_)));
}
