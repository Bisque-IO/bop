//! File descriptor table management for WASI Preview 1.
//!
//! This module uses maniac's async I/O primitives with sync_await for non-blocking operations.

use crate::wasi::types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

/// File state for async file operations.
#[derive(Debug)]
pub struct FileState {
    /// The file path (used for reopening if needed)
    pub path: PathBuf,
    /// Current file position
    pub position: u64,
    /// File is open for reading
    pub read: bool,
    /// File is open for writing
    pub write: bool,
    /// File should append
    pub append: bool,
}

impl FileState {
    pub fn new(path: PathBuf, read: bool, write: bool, append: bool) -> Self {
        Self {
            path,
            position: 0,
            read,
            write,
            append,
        }
    }
}

/// Directory state for path operations.
#[derive(Debug, Clone)]
pub struct DirState {
    /// The directory path on the host filesystem
    pub host_path: PathBuf,
}

impl DirState {
    pub fn new(path: PathBuf) -> Self {
        Self { host_path: path }
    }

    /// Resolve a guest path relative to this directory.
    pub fn resolve(&self, guest_path: &str) -> PathBuf {
        // Simple path resolution - join and canonicalize
        // In a real implementation, this would need proper sandboxing
        let mut resolved = self.host_path.clone();
        for component in guest_path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    resolved.pop();
                }
                c => resolved.push(c),
            }
        }
        resolved
    }
}

/// A file descriptor entry in the table.
#[derive(Debug)]
pub enum FdEntry {
    /// Standard input
    Stdin,
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// A regular file
    File {
        state: Arc<Mutex<FileState>>,
        rights_base: Rights,
        rights_inheriting: Rights,
        flags: FdFlags,
    },
    /// A directory (preopen or opened)
    Dir {
        state: Arc<DirState>,
        rights_base: Rights,
        rights_inheriting: Rights,
        flags: FdFlags,
        /// If this is a preopen, the guest path
        preopen_path: Option<String>,
    },
    /// A TCP socket
    #[cfg(feature = "wasi-net")]
    TcpListener {
        listener: Arc<Mutex<crate::net::TcpListener>>,
        rights_base: Rights,
    },
    #[cfg(feature = "wasi-net")]
    TcpStream {
        stream: Arc<Mutex<crate::net::TcpStream>>,
        rights_base: Rights,
    },
    #[cfg(feature = "wasi-net")]
    UdpSocket {
        socket: Arc<Mutex<crate::net::UdpSocket>>,
        rights_base: Rights,
    },
}

impl FdEntry {
    pub fn filetype(&self) -> Filetype {
        match self {
            FdEntry::Stdin | FdEntry::Stdout | FdEntry::Stderr => Filetype::CharacterDevice,
            FdEntry::File { .. } => Filetype::RegularFile,
            FdEntry::Dir { .. } => Filetype::Directory,
            #[cfg(feature = "wasi-net")]
            FdEntry::TcpListener { .. } | FdEntry::TcpStream { .. } => Filetype::SocketStream,
            #[cfg(feature = "wasi-net")]
            FdEntry::UdpSocket { .. } => Filetype::SocketDgram,
        }
    }

    pub fn rights_base(&self) -> Rights {
        match self {
            FdEntry::Stdin => Rights::FD_READ | Rights::POLL_FD_READWRITE,
            FdEntry::Stdout | FdEntry::Stderr => Rights::FD_WRITE | Rights::POLL_FD_READWRITE,
            FdEntry::File { rights_base, .. } => *rights_base,
            FdEntry::Dir { rights_base, .. } => *rights_base,
            #[cfg(feature = "wasi-net")]
            FdEntry::TcpListener { rights_base, .. } => *rights_base,
            #[cfg(feature = "wasi-net")]
            FdEntry::TcpStream { rights_base, .. } => *rights_base,
            #[cfg(feature = "wasi-net")]
            FdEntry::UdpSocket { rights_base, .. } => *rights_base,
        }
    }

    pub fn rights_inheriting(&self) -> Rights {
        match self {
            FdEntry::Stdin | FdEntry::Stdout | FdEntry::Stderr => Rights(0),
            FdEntry::File {
                rights_inheriting, ..
            } => *rights_inheriting,
            FdEntry::Dir {
                rights_inheriting, ..
            } => *rights_inheriting,
            #[cfg(feature = "wasi-net")]
            _ => Rights(0),
        }
    }

    pub fn flags(&self) -> FdFlags {
        match self {
            FdEntry::Stdin | FdEntry::Stdout | FdEntry::Stderr => FdFlags(0),
            FdEntry::File { flags, .. } => *flags,
            FdEntry::Dir { flags, .. } => *flags,
            #[cfg(feature = "wasi-net")]
            _ => FdFlags(0),
        }
    }

    pub fn set_flags(&mut self, new_flags: FdFlags) {
        match self {
            FdEntry::File { flags, .. } => *flags = new_flags,
            FdEntry::Dir { flags, .. } => *flags = new_flags,
            _ => {}
        }
    }

    pub fn is_preopen(&self) -> bool {
        matches!(
            self,
            FdEntry::Dir {
                preopen_path: Some(_),
                ..
            }
        )
    }

    pub fn preopen_path(&self) -> Option<&str> {
        match self {
            FdEntry::Dir { preopen_path, .. } => preopen_path.as_deref(),
            _ => None,
        }
    }

    /// Check if this entry has the required rights.
    pub fn check_rights(&self, required: Rights) -> Result<(), Errno> {
        if self.rights_base().contains(required) {
            Ok(())
        } else {
            Err(Errno::NotCapable)
        }
    }

    /// Get the file state if this is a file entry.
    pub fn file_state(&self) -> Option<Arc<Mutex<FileState>>> {
        match self {
            FdEntry::File { state, .. } => Some(Arc::clone(state)),
            _ => None,
        }
    }

    /// Get the directory state if this is a directory entry.
    pub fn dir_state(&self) -> Option<Arc<DirState>> {
        match self {
            FdEntry::Dir { state, .. } => Some(Arc::clone(state)),
            _ => None,
        }
    }
}

/// The file descriptor table.
#[derive(Debug, Default)]
pub struct FdTable {
    entries: HashMap<u32, FdEntry>,
    next_fd: u32,
}

impl FdTable {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_fd: 0,
        }
    }

    /// Insert stdin(0), stdout(1), stderr(2).
    pub fn insert_stdio(&mut self) {
        self.entries.insert(0, FdEntry::Stdin);
        self.entries.insert(1, FdEntry::Stdout);
        self.entries.insert(2, FdEntry::Stderr);
        self.next_fd = 3;
    }

    /// Push a preopen directory.
    pub fn push_preopen(&mut self, host_path: PathBuf, guest_path: String) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::Dir {
                state: Arc::new(DirState::new(host_path)),
                rights_base: Rights::DIR_BASE,
                rights_inheriting: Rights::DIR_INHERITING,
                flags: FdFlags(0),
                preopen_path: Some(guest_path),
            },
        );
        fd
    }

    /// Insert a file entry, returning the new fd.
    pub fn push_file(
        &mut self,
        path: PathBuf,
        read: bool,
        write: bool,
        append: bool,
        rights_base: Rights,
        rights_inheriting: Rights,
        flags: FdFlags,
    ) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::File {
                state: Arc::new(Mutex::new(FileState::new(path, read, write, append))),
                rights_base,
                rights_inheriting,
                flags,
            },
        );
        fd
    }

    /// Insert a directory entry, returning the new fd.
    pub fn push_dir(
        &mut self,
        host_path: PathBuf,
        rights_base: Rights,
        rights_inheriting: Rights,
        flags: FdFlags,
    ) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::Dir {
                state: Arc::new(DirState::new(host_path)),
                rights_base,
                rights_inheriting,
                flags,
                preopen_path: None,
            },
        );
        fd
    }

    #[cfg(feature = "wasi-net")]
    pub fn push_tcp_listener(&mut self, listener: crate::net::TcpListener) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::TcpListener {
                listener: Arc::new(Mutex::new(listener)),
                rights_base: Rights::SOCK_LISTEN | Rights::SOCK_ACCEPT, // Basic rights
            },
        );
        fd
    }

    #[cfg(feature = "wasi-net")]
    pub fn push_tcp_stream(&mut self, stream: crate::net::TcpStream) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::TcpStream {
                stream: Arc::new(Mutex::new(stream)),
                rights_base: Rights::SOCK_RECV | Rights::SOCK_SEND | Rights::SOCK_SHUTDOWN,
            },
        );
        fd
    }

    #[cfg(feature = "wasi-net")]
    pub fn push_udp_socket(&mut self, socket: crate::net::UdpSocket) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.entries.insert(
            fd,
            FdEntry::UdpSocket {
                socket: Arc::new(Mutex::new(socket)),
                rights_base: Rights::SOCK_RECV | Rights::SOCK_SEND, // And connect/bind?
            },
        );
        fd
    }

    /// Get an entry by fd.
    pub fn get(&self, fd: u32) -> Result<&FdEntry, Errno> {
        self.entries.get(&fd).ok_or(Errno::BadF)
    }

    /// Get a mutable entry by fd.
    pub fn get_mut(&mut self, fd: u32) -> Result<&mut FdEntry, Errno> {
        self.entries.get_mut(&fd).ok_or(Errno::BadF)
    }

    /// Remove an entry.
    pub fn remove(&mut self, fd: u32) -> Result<FdEntry, Errno> {
        self.entries.remove(&fd).ok_or(Errno::BadF)
    }

    /// Renumber fd from `from` to `to`.
    pub fn renumber(&mut self, from: u32, to: u32) -> Result<(), Errno> {
        let entry = self.entries.remove(&from).ok_or(Errno::BadF)?;
        // Close `to` if it exists
        self.entries.remove(&to);
        self.entries.insert(to, entry);
        Ok(())
    }

    /// Iterate over all preopens (fd >= 3 with preopen_path set).
    pub fn preopens(&self) -> impl Iterator<Item = (u32, &str)> {
        self.entries.iter().filter_map(|(fd, entry)| {
            if let FdEntry::Dir {
                preopen_path: Some(path),
                ..
            } = entry
            {
                Some((*fd, path.as_str()))
            } else {
                None
            }
        })
    }

    /// Count of preopens.
    pub fn preopen_count(&self) -> usize {
        self.preopens().count()
    }
}
