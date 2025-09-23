// //! Example of using the BOP Raft wrapper
// //!
// //! This example demonstrates basic usage of the Raft consensus wrapper,
// //! including creating server configurations, buffers, and working with
// //! the type-safe API.

// use bop_raft::*;

// fn main() -> RaftResult<()> {
//     println!("BOP Raft Example");

//     // Create strong-typed IDs
//     let server_id = ServerId::new(1);
//     let dc_id = DataCenterId::new(0);
//     let term = Term::new(1);
//     let log_idx = LogIndex::new(0);

//     println!("Server ID: {:?}", server_id);
//     println!("Data Center ID: {:?}", dc_id);
//     println!("Term: {:?}", term);
//     println!("Log Index: {:?}", log_idx);

//     // Create a buffer
//     let buffer = Buffer::new(1024)?;
//     println!("Created buffer with capacity: {}", buffer.container_size());

//     // Create server configuration
//     let server_config = ServerConfig::new(
//         server_id,
//         dc_id,
//         "127.0.0.1:8080",
//         "example-node",
//         false, // Not a learner
//     )?;

//     println!("Server config ID: {:?}", server_config.id());
//     println!("Server config DC ID: {:?}", server_config.dc_id());
//     println!("Server config endpoint: {}", server_config.endpoint()?);
//     println!("Server config aux: {}", server_config.aux()?);
//     println!("Is learner: {}", server_config.is_learner());

//     // Create cluster configuration
//     let cluster_config = ClusterConfig::new()?;
//     println!("Cluster config servers: {}", cluster_config.servers_size());

//     // Create Raft parameters
//     let raft_params = RaftParams::new()?;
//     println!("Created Raft parameters");

//     // Demonstrate type safety
//     demonstrate_type_safety();

//     // Demonstrate builder pattern
//     demonstrate_builder_pattern()?;

//     println!("Example completed successfully");
//     Ok(())
// }

// fn demonstrate_type_safety() {
//     println!("\nDemonstrating type safety:");

//     let term1 = Term::new(1);
//     let term2 = Term::new(2);
//     let log_idx1 = LogIndex::new(10);
//     let log_idx2 = LogIndex::new(20);

//     // These comparisons are type-safe
//     println!("Term {} < Term {}: {}", term1.inner(), term2.inner(), term1 < term2);
//     println!("LogIndex {} < LogIndex {}: {}", log_idx1.inner(), log_idx2.inner(), log_idx1 < log_idx2);

//     // Demonstrate next/prev operations
//     println!("Term {} -> next: {}", term1.inner(), term1.next().inner());
//     println!("LogIndex {} -> next: {}", log_idx1.inner(), log_idx1.next().inner());
//     println!("LogIndex {} -> prev: {:?}", log_idx1.inner(), log_idx1.prev().map(|i| i.inner()));

//     // Demonstrate zero edge case
//     let zero_idx = LogIndex::new(0);
//     println!("LogIndex 0 -> prev: {:?}", zero_idx.prev());
// }

// fn demonstrate_builder_pattern() -> RaftResult<()> {
//     println!("\nDemonstrating builder pattern:");

//     let builder = RaftServerBuilder::new();
//     println!("Created RaftServerBuilder");

//     // This will fail because we don't have all required components
//     match builder.build() {
//         Ok(_) => println!("Unexpectedly succeeded in building server"),
//         Err(e) => println!("Expected error: {}", e),
//     }

//     // Show how you would configure the builder (though we can't actually build without C library)
//     let _configured_builder = RaftServerBuilder::new()
//         .params(RaftParams::default());

//     println!("Builder pattern demonstrated");
//     Ok(())
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_example_runs() {
//         // This test ensures the example compiles and the basic API works
//         assert!(main().is_ok());
//     }
// }
fn main() {}

