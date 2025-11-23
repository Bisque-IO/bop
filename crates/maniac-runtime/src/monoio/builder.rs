use std::{io, marker::PhantomData};

#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::monoio::driver::IoUringDriver;
#[cfg(feature = "poll")]
use crate::monoio::driver::PollerDriver;
#[cfg(any(feature = "poll", feature = "iouring"))]
use crate::monoio::utils::thread_id::gen_id;
use crate::monoio::{
    driver::Driver,
    Runtime,
};

#[cfg(any(feature = "poll", feature = "iouring"))]
thread_local! {
    pub(crate) static BUILD_THREAD_ID: usize = gen_id();
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
pub type FusionDriver = IoUringDriver;
#[cfg(all(not(all(target_os = "linux", feature = "iouring")), feature = "poll"))]
pub type FusionDriver = PollerDriver;

pub trait Buildable {
    type Output;
    fn build(self) -> io::Result<Self::Output>;
}

pub struct RuntimeBuilder<D> {
    _mark: PhantomData<D>,
}

impl<D> RuntimeBuilder<D> {
    pub fn new() -> Self {
        Self {
            _mark: PhantomData,
        }
    }
}

pub trait DriverNew: Sized {
    fn new() -> io::Result<Self>;
}

#[cfg(feature = "poll")]
impl DriverNew for PollerDriver {
    fn new() -> io::Result<Self> {
        PollerDriver::new()
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
impl DriverNew for IoUringDriver {
    fn new() -> io::Result<Self> {
        let builder = io_uring::IoUring::builder();
        IoUringDriver::new(&builder)
    }
}

impl<D> Buildable for RuntimeBuilder<D> 
where D: Driver + DriverNew {
    type Output = Runtime<D>;

    fn build(self) -> io::Result<Self::Output> {
        let driver = D::new()?;
        let context = crate::monoio::runtime::Context::new();
        
        Ok(Runtime::new(context, driver))
    }
}
