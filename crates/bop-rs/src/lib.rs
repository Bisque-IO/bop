pub mod allocator {
    pub use bop_allocator::*;
}

pub use bop_aof as aof2;

pub mod mdbx {
    pub use bop_mdbx::*;
}

pub mod mpmc {
    pub use bop_mpmc::*;
}

pub mod raft {
    pub use bop_raft::*;
}

pub mod usockets {
    pub use bop_usockets::usockets::*;
}

#[cfg(feature = "usockets-udp")]
pub mod socket {
    pub use bop_usockets::socket::*;
}

#[cfg(feature = "usockets-udp")]
pub mod uws {
    pub use bop_usockets::uws::*;
}

pub fn greeting() -> &'static str {
    "Hello from bop-rs!"
}
