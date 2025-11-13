#![cfg_attr(not(feature = "mdbx"), allow(dead_code))]

#[cfg(feature = "mdbx")]
fn main() -> bop_raft::RaftResult<()> {
    use std::env;

    use bop_raft::{
        ClusterMembershipSnapshot, DataCenterId, LogIndex, MdbxStorage, MembershipEntry,
        RaftParams, RaftServerBuilder, ServerConfig, ServerId,
    };

    let data_dir = env::temp_dir().join("bop-raft-mdbx-example");
    println!("Using MDBX data directory: {}", data_dir.display());

    let server_config = ServerConfig::new(
        ServerId::new(1),
        DataCenterId::new(0),
        "127.0.0.1:5000",
        "example-node",
        false,
    )?;

    let mut params = RaftParams::new()?;
    params.set_election_timeout(150, 450);
    params.set_heart_beat_interval(120);
    params.set_parallel_log_appending(true);
    params.with_raw_mut(|raw| {
        raw.snapshot_distance = 4_096;
    });

    println!(
        "Configured RaftParams: election {:?}, heartbeat {}ms, parallel appending {}",
        params.election_timeout(),
        params.heart_beat_interval(),
        params.parallel_log_appending()
    );

    let storage = MdbxStorage::new(server_config, &data_dir);
    let builder = RaftServerBuilder::new()
        .with_mdbx_storage(storage)
        .params(params.clone());

    println!(
        "RaftServerBuilder ready; attach your state machine, state manager, and logger as needed."
    );
    println!("Example membership diff after configuration:");

    let baseline = ClusterMembershipSnapshot::new(
        LogIndex::new(5),
        LogIndex::new(0),
        vec![MembershipEntry {
            server_id: ServerId::new(1),
            dc_id: DataCenterId::new(0),
            endpoint: "127.0.0.1:5000".into(),
            aux: "example-node".into(),
            is_learner: false,
            is_new_joiner: false,
            priority: 1,
        }],
    );

    let updated = ClusterMembershipSnapshot::new(
        LogIndex::new(6),
        LogIndex::new(5),
        vec![
            MembershipEntry {
                server_id: ServerId::new(1),
                dc_id: DataCenterId::new(0),
                endpoint: "127.0.0.1:5000".into(),
                aux: "example-node".into(),
                is_learner: false,
                is_new_joiner: false,
                priority: 5,
            },
            MembershipEntry {
                server_id: ServerId::new(2),
                dc_id: DataCenterId::new(0),
                endpoint: "127.0.0.1:5001".into(),
                aux: "follower".into(),
                is_learner: true,
                is_new_joiner: true,
                priority: 0,
            },
        ],
    );

    let diff = baseline.diff(&updated);
    println!(
        "  added: {:?}",
        diff.added
            .iter()
            .map(|e| e.server_id.inner())
            .collect::<Vec<_>>()
    );
    println!(
        "  removed: {:?}",
        diff.removed
            .iter()
            .map(|e| e.server_id.inner())
            .collect::<Vec<_>>()
    );
    println!(
        "  updated priorities: {:?}",
        diff.updated
            .iter()
            .map(|delta| (
                delta.before.server_id.inner(),
                delta.before.priority,
                delta.after.priority
            ))
            .collect::<Vec<_>>()
    );

    drop(builder);

    Ok(())
}

#[cfg(not(feature = "mdbx"))]
fn main() {
    eprintln!(
        "Enable the `mdbx` feature to run this example: cargo run --example mdbx_cluster --features \"mdbx\""
    );
}
