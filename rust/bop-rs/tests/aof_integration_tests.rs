// Integration tests for AOF module
// This file includes all the test modules from Phases 1, 2, and 3

// Include test modules
mod aof {
    mod config_tests;
    mod filesystem_tests;
    mod flush_tests;
    mod index_tests;
    mod layout_tests;
    mod reader_tests;
    mod segment_store_tests;
}
