pub mod bits;
pub mod padded;
pub mod parking;
pub mod random;
pub mod striped_atomic;

pub use bits::*;
use core_affinity::CoreId;
pub use padded::*;
pub use parking::*;
pub use random::*;

pub fn num_cpus() -> usize {
    // static NUM_CPUS: once_cell::sync::Lazy<usize> = once_cell::sync::Lazy::new(|| {
    //     core_affinity::get_core_ids().unwrap().len()
    // });
    // *NUM_CPUS
    cpu_cores().len()
}

pub fn cpu_cores() -> &'static [CoreId] {
    static CPU_CORES: once_cell::sync::Lazy<Box<Option<Vec<CoreId>>>> = once_cell::sync::Lazy::new(|| {
        Box::new(core_affinity::get_core_ids())
    });
    match (*CPU_CORES).as_ref() {
        Some(cores) => cores.as_slice(),
        None => &[],
    }
}
