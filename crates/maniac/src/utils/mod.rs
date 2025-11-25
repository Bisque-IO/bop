pub mod bits;
pub mod padded;
pub mod parking;
pub mod random;
pub mod striped_atomic;

pub(crate) mod box_into_inner;
pub(crate) mod linked_list;
#[allow(dead_code)]
pub(crate) mod slab;
#[allow(dead_code)]
pub(crate) mod thread_id;
pub(crate) mod uring_detect;

mod rand;
pub use rand::thread_rng_n;
pub use uring_detect::detect_uring;

pub use crate::driver::op::is_legacy;

#[cfg(feature = "signal")]
mod ctrlc;
#[cfg(feature = "signal")]
pub use self::ctrlc::{CtrlC, Error as CtrlCError};

#[cfg(feature = "utils")]
mod bind_to_cpu_set;
#[cfg(feature = "utils")]
pub use bind_to_cpu_set::{BindError, bind_to_cpu_set};

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
    static CPU_CORES: once_cell::sync::Lazy<Box<Option<Vec<CoreId>>>> =
        once_cell::sync::Lazy::new(|| Box::new(core_affinity::get_core_ids()));
    match (*CPU_CORES).as_ref() {
        Some(cores) => cores.as_slice(),
        None => &[],
    }
}
