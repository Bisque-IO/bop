pub mod bits;
pub mod padded;
pub mod parking;
pub mod striped_atomic;

pub use bits::*;
pub use padded::*;
pub use parking::*;

pub fn num_cpus() -> usize {
    core_affinity::get_core_ids().unwrap().len()
}