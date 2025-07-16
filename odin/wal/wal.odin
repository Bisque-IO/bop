package wal

import "../fs"

WAL :: struct {
    segment_size: u32le,
    tail: ^Segment,
    path: string,
    lsn:  u64,
    gsn:  u64,
}

Flag :: enum {
    NONE,
    XXH3_64,
    CRC64_NVME,
    CRC32,
}

Hash_Frequency :: enum u8 {
    None            = 0,
    Per_Record      = 1,
    Per_Segment     = 2,
    Both            = 3,
}

Hash_Kind :: enum u8 {
    NONE            = 0,
    XXH3_64         = 1,
    CRC32C          = 2,
    CRC64_NVME      = 3,
}

Hashed_Record_Header :: struct {
    checksum:  u64le,
    size:      u32le,
    hash_freq: Hash_Frequency,
    hash_kind: Hash_Kind,
}

Index_Page :: struct {}

Tail :: struct {
    lo:           u64le,
    hi:           u64le,
    records:      u32le,
    idx_offset:   u32le,
    idx_size:     u32le,
    idx_rec_size: u32le,
    rec_hash:     Hash_Kind,
    seg_hash:     Hash_Kind,
    checksum:     u64le,
    magic:        u64le,
}

Header  :: struct {
    magic: u64le,
}


create  :: proc() {}

aof_append  :: proc() {}

aof_recover :: proc() {}
