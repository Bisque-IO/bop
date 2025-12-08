//! Resource table for WASI Preview 2.
//!
//! The component model uses typed resources that need to be tracked in a table.
//! This provides a simple slab-based resource table.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// A handle to a resource in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceHandle(pub u32);

impl ResourceHandle {
    pub fn rep(&self) -> u32 {
        self.0
    }
}

/// A typed resource entry.
pub struct Resource<T> {
    pub handle: ResourceHandle,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Resource<T> {
    pub fn new(handle: ResourceHandle) -> Self {
        Self {
            handle,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn rep(&self) -> u32 {
        self.handle.0
    }
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Copy for Resource<T> {}

/// Entry in the resource table.
struct TableEntry {
    data: Box<dyn Any + Send>,
}

/// Resource table for managing WASI resources.
#[derive(Default)]
pub struct ResourceTable {
    entries: HashMap<u32, TableEntry>,
    next_id: AtomicU32,
}

impl std::fmt::Debug for ResourceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceTable")
            .field("count", &self.entries.len())
            .finish()
    }
}

impl ResourceTable {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_id: AtomicU32::new(1), // Start at 1, 0 is often invalid
        }
    }

    /// Push a new resource into the table, returning its handle.
    pub fn push<T: Send + 'static>(&mut self, value: T) -> ResourceHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(
            id,
            TableEntry {
                data: Box::new(value),
            },
        );
        ResourceHandle(id)
    }

    /// Get a reference to a resource.
    pub fn get<T: 'static>(&self, handle: ResourceHandle) -> Option<&T> {
        self.entries
            .get(&handle.0)
            .and_then(|entry| entry.data.downcast_ref::<T>())
    }

    /// Get a mutable reference to a resource.
    pub fn get_mut<T: 'static>(&mut self, handle: ResourceHandle) -> Option<&mut T> {
        self.entries
            .get_mut(&handle.0)
            .and_then(|entry| entry.data.downcast_mut::<T>())
    }

    /// Remove a resource from the table.
    pub fn delete(&mut self, handle: ResourceHandle) -> bool {
        self.entries.remove(&handle.0).is_some()
    }

    /// Check if a handle is valid.
    pub fn contains(&self, handle: ResourceHandle) -> bool {
        self.entries.contains_key(&handle.0)
    }

    /// Get the number of resources in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// Common resource types for WASI Preview 2

/// An input stream resource.
pub struct InputStream {
    pub inner: Box<dyn InputStreamReader + Send + Sync>,
}

pub trait InputStreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
    // Future expansion: strict async read trait
}

impl<T: std::io::Read + Send + Sync> InputStreamReader for T {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf)
    }
}

impl InputStream {
    pub fn new(reader: impl InputStreamReader + Send + Sync + 'static) -> Self {
        Self {
            inner: Box::new(reader),
        }
    }
}

/// An output stream resource.
pub struct OutputStream {
    pub inner: Box<dyn OutputStreamWriter + Send + Sync>,
}

pub trait OutputStreamWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
    fn flush(&mut self) -> std::io::Result<()>;
}

impl<T: std::io::Write + Send + Sync> OutputStreamWriter for T {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.flush()
    }
}

impl OutputStream {
    pub fn new(writer: impl OutputStreamWriter + Send + Sync + 'static) -> Self {
        Self {
            inner: Box::new(writer),
        }
    }
}

/// A pollable resource for async operations.
pub struct Pollable {
    pub ready: bool,
    // In a full implementation, this would hold a Waker or Future
}

impl Pollable {
    pub fn new(ready: bool) -> Self {
        Self { ready }
    }

    pub fn ready() -> Self {
        Self { ready: true }
    }

    pub fn pending() -> Self {
        Self { ready: false }
    }
}

/// A directory resource.
pub struct DirectoryEntry {
    pub dir: cap_std::fs::Dir,
    pub path: String,
    pub perms: DirPerms,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DirPerms {
    pub readable: bool,
    pub writable: bool,
    // Add more perms as needed (e.g. mutate directory)
}

/// A file descriptor resource.
pub struct Descriptor {
    pub file: cap_std::fs::File,
    pub perms: FilePerms,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FilePerms {
    pub readable: bool,
    pub writable: bool,
}

pub enum DescriptorTypes {
    Descriptor(Descriptor),
    DirectoryEntry(DirectoryEntry),
    Stdin,
    Stdout,
    Stderr,
}

#[cfg(feature = "wasi-net")]
pub mod net {
    use crate::net::{TcpListener, TcpStream, UdpSocket};

    /// A TCP socket resource.
    pub struct TcpSocketResource {
        pub state: TcpSocketState,
    }

    pub enum TcpSocketState {
        Unbound,
        Bound(std::net::SocketAddr),
        Listening(TcpListener),
        Connected(TcpStream),
    }

    /// A UDP socket resource.
    pub struct UdpSocketResource {
        pub socket: UdpSocket,
    }

    /// Network resource.
    pub struct NetworkResource;
}
