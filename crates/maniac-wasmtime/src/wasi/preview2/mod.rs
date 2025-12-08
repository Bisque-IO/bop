//! WASI Preview 2 (Component Model) implementation for Maniac.
//!
//! This provides the host-side implementation of WASI Preview 2 interfaces
//! using Maniac's async runtime. Preview 2 uses the component model with
//! typed resources and streams.

pub mod host;
pub mod resources;

pub use host::*;
pub use resources::ResourceTable;

use anyhow::Result;
use resources::{InputStreamReader, OutputStreamWriter};
use std::path::Path;

/// WASI Preview 2 context.
pub struct WasiCtx {
    pub table: ResourceTable,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub preopens: Vec<(cap_std::fs::Dir, String)>,
    pub stdin: Option<Box<dyn InputStreamReader + Send + Sync>>,
    pub stdout: Option<Box<dyn OutputStreamWriter + Send + Sync>>,
    pub stderr: Option<Box<dyn OutputStreamWriter + Send + Sync>>,
    pub exit_code: Option<i32>,
}

impl Default for WasiCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl WasiCtx {
    pub fn new() -> Self {
        Self {
            table: ResourceTable::new(),
            argv: Vec::new(),
            env: Vec::new(),
            preopens: Vec::new(),
            stdin: None,
            stdout: None,
            stderr: None,
            exit_code: None,
        }
    }

    pub fn args(&self) -> &[String] {
        &self.argv
    }

    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }
}

/// Trait for store types containing WasiCtx.
pub trait WasiView {
    fn wasi_ctx(&self) -> &WasiCtx;
    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx;
    fn table(&self) -> &ResourceTable;
    fn table_mut(&mut self) -> &mut ResourceTable;
}

impl WasiView for WasiCtx {
    fn wasi_ctx(&self) -> &WasiCtx {
        self
    }
    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx {
        self
    }
    fn table(&self) -> &ResourceTable {
        &self.table
    }
    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

/// Builder for WasiCtx.
#[derive(Default)]
pub struct WasiCtxBuilder {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    preopens: Vec<(cap_std::fs::Dir, String)>,
    inherit_stdio: bool,
}

impl WasiCtxBuilder {
    pub fn new() -> Self {
        Self {
            inherit_stdio: true,
            ..Default::default()
        }
    }

    pub fn inherit_stdio(mut self) -> Self {
        self.inherit_stdio = true;
        self
    }

    pub fn inherit_args(mut self) -> Self {
        self.argv = std::env::args().collect();
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.argv = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        self
    }

    pub fn arg(mut self, arg: impl AsRef<str>) -> Self {
        self.argv.push(arg.as_ref().to_string());
        self
    }

    pub fn inherit_env(mut self) -> Self {
        self.env = std::env::vars().collect();
        self
    }

    pub fn env(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.env
            .push((key.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    pub fn preopen_dir(
        mut self,
        host_path: impl AsRef<Path>,
        guest_path: impl Into<String>,
    ) -> Result<Self> {
        let dir =
            cap_std::fs::Dir::open_ambient_dir(host_path.as_ref(), cap_std::ambient_authority())?;
        self.preopens.push((dir, guest_path.into()));
        Ok(self)
    }

    pub fn build(self) -> WasiCtx {
        WasiCtx {
            table: ResourceTable::new(),
            argv: self.argv,
            env: self.env,
            preopens: self.preopens,
            stdin: if self.inherit_stdio {
                Some(Box::new(std::io::stdin()))
            } else {
                None
            },
            stdout: if self.inherit_stdio {
                Some(Box::new(std::io::stdout()))
            } else {
                None
            },
            stderr: if self.inherit_stdio {
                Some(Box::new(std::io::stderr()))
            } else {
                None
            },
            exit_code: None,
        }
    }
}

/// Add WASI Preview 2 interfaces to a component linker.
///
/// This registers all the standard WASI interfaces:
/// - wasi:cli/environment
/// - wasi:cli/exit
/// - wasi:cli/stdin, stdout, stderr
/// - wasi:clocks/wall-clock, monotonic-clock
/// - wasi:filesystem/types, preopens
/// - wasi:io/streams, poll
/// - wasi:random/random, insecure, insecure-seed
/// - wasi:sockets/* (if wasi-net feature enabled)
pub fn add_to_linker<T: WasiView + 'static>(
    linker: &mut wasmtime::component::Linker<T>,
) -> Result<()> {
    host::add_wasi_to_linker(linker)
}
