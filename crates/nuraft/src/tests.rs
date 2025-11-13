//! Tests for the Raft wrapper functionality

#[cfg(test)]
mod tests {
    use crate::{
        Buffer, CallbackType, ClusterConfig, ClusterMembershipSnapshot, DataCenterId, LogIndex,
        MembershipEntry, PeerInfo, PrioritySetResult, RaftError, RaftParams, RaftServerBuilder,
        ServerConfig, ServerId, Term,
    };

    #[cfg(feature = "mdbx")]
    use crate::{LogStoreBuild, StateManagerBuild};

    #[test]
    fn test_strong_types() {
        let server_id = ServerId::new(1);
        let dc_id = DataCenterId::new(0);
        let term = Term::new(42);
        let log_idx = LogIndex::new(100);

        assert_eq!(server_id.inner(), 1);
        assert_eq!(dc_id.inner(), 0);
        assert_eq!(term.inner(), 42);
        assert_eq!(log_idx.inner(), 100);

        // Test ordering
        assert!(Term::new(1) < Term::new(2));
        assert!(LogIndex::new(1) < LogIndex::new(2));

        // Test next/prev
        assert_eq!(term.next().inner(), 43);
        assert_eq!(log_idx.next().inner(), 101);
        assert_eq!(log_idx.prev().unwrap().inner(), 99);
        assert_eq!(LogIndex::new(0).prev(), None);
    }

    #[test]
    fn test_buffer_creation() {
        let buffer = Buffer::new(1024);
        assert!(buffer.is_ok());

        if let Ok(buffer) = buffer {
            assert!(buffer.container_size() >= 1024);
            assert_eq!(buffer.pos(), 0);
        }
    }

    #[test]
    fn test_server_config_creation() {
        let config = ServerConfig::new(
            ServerId::new(1),
            DataCenterId::new(0),
            "127.0.0.1:8080",
            "test-node",
            false,
        );

        assert!(config.is_ok());

        if let Ok(config) = config {
            assert_eq!(config.id(), ServerId::new(1));
            assert_eq!(config.dc_id(), DataCenterId::new(0));
            assert!(!config.is_learner());
            assert_eq!(config.priority(), 0); // Default priority

            let endpoint = config.endpoint();
            assert!(endpoint.is_ok());
            if let Ok(endpoint) = endpoint {
                assert_eq!(endpoint, "127.0.0.1:8080");
            }

            let aux = config.aux();
            assert!(aux.is_ok());
            if let Ok(aux) = aux {
                assert_eq!(aux, "test-node");
            }
        }
    }

    #[test]
    fn test_cluster_config_creation() {
        let config = ClusterConfig::new();
        assert!(config.is_ok());

        if let Ok(config) = config {
            assert_eq!(config.servers_size(), 0); // Empty by default
        }
    }

    #[test]
    fn test_raft_params_creation() {
        let params = RaftParams::new();
        assert!(params.is_ok());

        // Test default
        let _ = RaftParams::default();
    }

    #[test]
    fn test_error_types() {
        // Test error creation and display
        let error = RaftError::ConfigError("Test error".to_string());
        assert!(error.to_string().contains("Test error"));

        let null_error = RaftError::NullPointer;
        assert_eq!(null_error.to_string(), "Null pointer returned from C API");
    }

    #[test]
    fn test_callback_type_enum() {
        assert_eq!(CallbackType::ProcessRequest.as_raw(), 1);
        assert_eq!(CallbackType::BecomeLeader.as_raw(), 6);
        assert_eq!(CallbackType::BecomeFresh.as_raw(), 12);
    }

    #[test]
    fn test_priority_set_result_enum() {
        assert_eq!(PrioritySetResult::Set as u32, 1);
        assert_eq!(PrioritySetResult::Broadcast as u32, 2);
        assert_eq!(PrioritySetResult::Ignored as u32, 3);
    }

    #[test]
    fn test_builder_pattern() {
        let builder = RaftServerBuilder::new();

        // Test builder methods don't panic
        let _builder = builder.params(RaftParams::default());

        // Builder should fail without all required components
        let result = RaftServerBuilder::new().build();
        assert!(result.is_err());

        if let Err(e) = result {
            // Should be a config error about missing components
            assert!(matches!(e, RaftError::ConfigError(_)));
        }
    }

    // Integration test that would require actual C library
    #[test]
    #[ignore] // Ignore by default since it requires the C library to be linked
    fn test_buffer_operations() {
        let mut buffer = Buffer::new(100).expect("Failed to create buffer");

        // Test basic operations
        assert_eq!(buffer.container_size(), 100);
        assert_eq!(buffer.size(), 0);
        assert_eq!(buffer.pos(), 0);

        buffer.set_pos(50);
        assert_eq!(buffer.pos(), 50);

        // Test data access (would need valid data to test fully)
        let _data_ptr = buffer.data();

        // Test conversion to Vec
        let _vec = buffer.to_vec();
    }

    #[test]
    fn test_peer_info_struct() {
        let peer_info = PeerInfo {
            server_id: ServerId::new(1),
            last_log_idx: LogIndex::new(100),
            last_succ_resp_us: 1000000,
        };

        assert_eq!(peer_info.server_id, ServerId::new(1));
        assert_eq!(peer_info.last_log_idx, LogIndex::new(100));
        assert_eq!(peer_info.last_succ_resp_us, 1000000u64);
    }

    #[cfg(not(feature = "mdbx"))]
    #[test]
    fn try_mdbx_storage_requires_feature() {
        let builder = RaftServerBuilder::new();
        let config = ServerConfig::new(
            ServerId::new(1),
            DataCenterId::new(0),
            "127.0.0.1:9000",
            "node",
            false,
        )
        .expect("config");

        let result = builder.try_mdbx_storage(config, "./tmp/mdbx-disabled");
        let err = match result {
            Ok(_) => panic!("expected feature disabled error"),
            Err(err) => err,
        };

        assert!(matches!(err, RaftError::FeatureDisabled(feature) if feature == "mdbx"));
    }

    #[cfg(feature = "mdbx")]
    #[test]
    fn mdbx_storage_into_components_uses_mdbx_backend() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let config = ServerConfig::new(
            ServerId::new(2),
            DataCenterId::new(0),
            "127.0.0.1:9001",
            "node-mdbx",
            false,
        )
        .expect("config");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_millis();
        let dir = std::env::temp_dir().join(format!("nuraft-mdbx-{suffix}"));

        let storage = crate::mdbx::MdbxStorage::new(config, dir.clone());
        let components = storage.into_components();
        let (state_manager, log_store, _runtime) = components.into_parts();

        matches!(state_manager, StateManagerBuild::Mdbx(_))
            .then_some(())
            .expect("state manager backend");
        matches!(log_store, LogStoreBuild::Mdbx(_))
            .then_some(())
            .expect("log store backend");

        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn raft_params_clone_and_copy() {
        let mut params = RaftParams::new().expect("params");
        params.set_election_timeout(150, 450);
        params.set_heart_beat_interval(120);
        params.set_rpc_failure_backoff(60);
        params.set_parallel_log_appending(true);

        assert_eq!(params.election_timeout(), (150, 450));
        assert_eq!(params.heart_beat_interval(), 120);
        assert!(params.parallel_log_appending());

        let mut clone = params.clone();
        assert_eq!(clone.election_timeout(), (150, 450));
        clone.set_heart_beat_interval(240);
        assert_eq!(clone.heart_beat_interval(), 240);
        assert_eq!(params.heart_beat_interval(), 120);

        let mut copy_target = RaftParams::new().expect("copy target");
        copy_target.copy_from(&clone);
        assert_eq!(copy_target.heart_beat_interval(), 240);
        assert_eq!(copy_target.election_timeout(), (150, 450));
        assert!(copy_target.parallel_log_appending());
    }

    #[test]
    fn membership_snapshot_diff_detects_changes() {
        let original = ClusterMembershipSnapshot::new(
            LogIndex::new(10),
            LogIndex::new(5),
            vec![
                MembershipEntry {
                    server_id: ServerId::new(1),
                    dc_id: DataCenterId::new(0),
                    endpoint: "127.0.0.1:9000".into(),
                    aux: "node-a".into(),
                    is_learner: false,
                    is_new_joiner: false,
                    priority: 1,
                },
                MembershipEntry {
                    server_id: ServerId::new(2),
                    dc_id: DataCenterId::new(0),
                    endpoint: "127.0.0.1:9001".into(),
                    aux: "node-b".into(),
                    is_learner: false,
                    is_new_joiner: false,
                    priority: 1,
                },
            ],
        );

        let updated = ClusterMembershipSnapshot::new(
            LogIndex::new(20),
            LogIndex::new(10),
            vec![
                MembershipEntry {
                    server_id: ServerId::new(1),
                    dc_id: DataCenterId::new(0),
                    endpoint: "127.0.0.1:9000".into(),
                    aux: "node-a".into(),
                    is_learner: false,
                    is_new_joiner: false,
                    priority: 5,
                },
                MembershipEntry {
                    server_id: ServerId::new(3),
                    dc_id: DataCenterId::new(1),
                    endpoint: "127.0.0.1:9002".into(),
                    aux: "node-c".into(),
                    is_learner: true,
                    is_new_joiner: true,
                    priority: 0,
                },
            ],
        );

        let diff = original.diff(&updated);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.updated.len(), 1);

        let added = &diff.added[0];
        assert_eq!(added.server_id, ServerId::new(3));
        assert!(added.is_learner);

        let removed = &diff.removed[0];
        assert_eq!(removed.server_id, ServerId::new(2));

        let delta = &diff.updated[0];
        assert_eq!(delta.before.server_id, ServerId::new(1));
        assert_eq!(delta.before.priority, 1);
        assert_eq!(delta.after.priority, 5);
    }
}
