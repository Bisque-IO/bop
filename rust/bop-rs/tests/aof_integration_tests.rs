// Integration tests for AOF module
// This file includes all the test modules from Phases 1, 2, and 3

// Include test modules
mod aof {
    // Phase 1 tests (Critical Priority)
    mod aof_core_test;
    mod aof_manager_test;
    mod appender_test;
    mod reader_test;
    mod segment_test;

    // Phase 2 tests (Integration & Error Handling)
    mod error_handling_test;
    mod index_test;
    mod record_test;
    mod tail_appender_test;

    // Phase 3 tests (Performance & Stress)
    mod concurrency_test;
    mod memory_test;
    mod performance_test;
    mod stress_test;

    // Phase 4 tests (Advanced Features)
    mod advanced_features_test;
    mod edge_cases_test;
    mod integration_scenarios_test;
    mod production_simulation_test;

    // Existing tests
    mod segment_index_test;
}
