//! WASI Preview 1 complete implementation.
//!
//! All snapshot_preview1 host functions are implemented here using Maniac's
//! async runtime with `sync_await` for non-blocking I/O from within
//! synchronous host functions.

pub mod fd_table;
pub mod host;

pub use fd_table::{DirState, FdEntry, FdTable, FileState};
pub use host::add_wasi_snapshot_preview1_to_linker;

use crate::wasi::types::*;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// WASI Preview 1 context holding all state for a guest.
pub struct WasiCtx {
    pub fd_table: FdTable,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub exit_code: Option<i32>,
}

impl Default for WasiCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl WasiCtx {
    pub fn new() -> Self {
        let mut fd_table = FdTable::new();
        // Pre-populate stdin(0), stdout(1), stderr(2)
        fd_table.insert_stdio();
        Self {
            fd_table,
            argv: Vec::new(),
            env: Vec::new(),
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

/// Trait for store types that contain a WasiCtx.
pub trait WasiView {
    fn wasi_ctx(&self) -> &WasiCtx;
    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx;
}

impl WasiView for WasiCtx {
    fn wasi_ctx(&self) -> &WasiCtx {
        self
    }
    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx {
        self
    }
}

/// Builder for WasiCtx.
#[derive(Default)]
pub struct WasiCtxBuilder {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    preopens: Vec<(PathBuf, String)>,
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

    pub fn envs<I, K, V>(mut self, envs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (k, v) in envs {
            self.env
                .push((k.as_ref().to_string(), v.as_ref().to_string()));
        }
        self
    }

    pub fn preopen_dir(
        mut self,
        host_path: impl AsRef<Path>,
        guest_path: impl Into<String>,
    ) -> Result<Self> {
        let path = host_path.as_ref().canonicalize()?;
        self.preopens.push((path, guest_path.into()));
        Ok(self)
    }

    pub fn build(self) -> WasiCtx {
        let mut ctx = WasiCtx::new();
        ctx.argv = self.argv;
        ctx.env = self.env;

        // Add preopened directories starting at fd 3
        for (host_path, guest_path) in self.preopens {
            ctx.fd_table.push_preopen(host_path, guest_path);
        }

        ctx
    }
}

/// Register all WASI preview1 functions with a Wasmtime Linker.
pub fn add_to_linker<T: WasiView + 'static>(linker: &mut wasmtime::Linker<T>) -> Result<()> {
    host::add_wasi_snapshot_preview1_to_linker(linker)
}
