use bop_rs::{raft, RaftBuffer};

#[test]
fn raft_buffer_alloc_and_drop() {
    let buf = RaftBuffer::new(64);
    assert!(buf.is_some());

    let buf = raft::Buffer::new(128);
    assert!(buf.is_some());
    // drop at end
}

