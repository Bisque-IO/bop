use std::{io, marker::PhantomData};

#[cfg(all(target_os = "linux", feature = "iouring"))]
use crate::monoio::driver::IoUringDriver;
#[cfg(feature = "legacy")]
use crate::monoio::driver::LegacyDriver;
#[cfg(any(feature = "legacy", feature = "iouring"))]
use crate::monoio::utils::thread_id::gen_id;
use crate::monoio::{
    driver::Driver,
    time::{driver::TimeDriver, Clock},
    Runtime,
};

#[cfg(any(feature = "legacy", feature = "iouring"))]
thread_local! {
    pub(crate) static BUILD_THREAD_ID: usize = gen_id();
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
pub type FusionDriver = IoUringDriver;
#[cfg(all(not(all(target_os = "linux", feature = "iouring")), feature = "legacy"))]
pub type FusionDriver = LegacyDriver;

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

    pub fn enable_timer(self) -> RuntimeBuilder<TimeDriver<D>> {
        RuntimeBuilder {
            _mark: PhantomData,
        }
    }

    pub fn enable_all(self) -> RuntimeBuilder<TimeDriver<D>> {
        self.enable_timer()
    }
}

pub trait DriverNew: Sized {
    fn new() -> io::Result<Self>;
}

#[cfg(feature = "legacy")]
impl DriverNew for LegacyDriver {
    fn new() -> io::Result<Self> {
        LegacyDriver::new()
    }
}

#[cfg(all(target_os = "linux", feature = "iouring"))]
impl DriverNew for IoUringDriver {
    fn new() -> io::Result<Self> {
        IoUringDriver::new()
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

impl<D> Buildable for RuntimeBuilder<TimeDriver<D>> 
where D: Driver + DriverNew {
    type Output = Runtime<TimeDriver<D>>;

    fn build(self) -> io::Result<Self::Output> {
        let driver = D::new()?;
        let driver = TimeDriver::new(driver, Clock::new());
        let context = {
            let mut ctx = crate::monoio::runtime::Context::new();
            ctx.time_handle = Some(driver.handle());
            ctx
        };

        Ok(Runtime::new(context, driver))
    }
}
