mod unix;
mod windows;

use std::{os::unix::fs::MetadataExt, path::{Path, PathBuf}, time::SystemTime};

use super::{file_type::FileType, permissions::Permissions};
use crate::driver::op::Op;

/// Given a path, query the file system to get information about a file,
/// directory, etc.
///
/// This function will traverse symbolic links to query information about the
/// destination file.
///
/// # Platform-specific behavior
///
/// current implementation is only for Linux.
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * The user lacks permissions to perform `metadata` call on `path`.
///     * execute(search) permission is required on all of the directories in path that lead to the
///       file.
/// * `path` does not exist.
///
/// # Examples
///
/// ```rust,no_run
/// use fs;
///
/// #[main]
/// async fn main() -> std::io::Result<()> {
///     let attr = fs::metadata("/some/file/path.txt").await?;
///     // inspect attr ...
///     Ok(())
/// }
/// ```
pub async fn metadata<P: AsRef<Path>>(path: P) -> std::io::Result<Metadata> {
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        // For non-io_uring systems, use blocking thread pool to avoid blocking the async runtime
        let path = path.as_ref().to_path_buf();
        let std_metadata = crate::blocking::unblock(move || std::fs::metadata(&path)).await?;
        
        #[cfg(target_os = "linux")]
        {
            // Convert std::fs::Metadata to libc::stat64, then to FileAttr
            use std::os::unix::fs::MetadataExt;
            let mut stat: libc::stat64 = unsafe { std::mem::zeroed() };
            
            stat.st_dev = std_metadata.dev() as _;
            stat.st_ino = std_metadata.ino() as libc::ino64_t;
            stat.st_nlink = std_metadata.nlink() as libc::nlink_t;
            stat.st_mode = std_metadata.mode() as libc::mode_t;
            stat.st_uid = std_metadata.uid() as libc::uid_t;
            stat.st_gid = std_metadata.gid() as libc::gid_t;
            stat.st_rdev = std_metadata.rdev() as _;
            stat.st_size = std_metadata.size() as libc::off64_t;
            stat.st_blksize = std_metadata.blksize() as libc::blksize_t;
            stat.st_blocks = std_metadata.blocks() as libc::blkcnt64_t;
            
            let atime = std_metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
            let mtime = std_metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let ctime = std_metadata.created().or_else(|_| std_metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            if let Ok(duration) = atime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_atime = duration.as_secs() as libc::time_t;
                stat.st_atime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_mtime = duration.as_secs() as libc::time_t;
                stat.st_mtime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = ctime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_ctime = duration.as_secs() as libc::time_t;
                stat.st_ctime_nsec = duration.subsec_nanos() as _;
            }
            
            // Note: statx_extra_fields will be None since we don't have statx data
            let file_attr = FileAttr {
                stat,
                statx_extra_fields: None,
            };
            Ok(Metadata(file_attr))
        }
        
        #[cfg(all(unix, not(target_os = "linux")))]
        {
            // Convert std::fs::Metadata to libc::stat, then to FileAttr
            // This covers macOS, FreeBSD, and other Unix-like systems
            use std::os::unix::fs::MetadataExt;
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            
            stat.st_dev = std_metadata.dev() as _;
            stat.st_ino = std_metadata.ino() as libc::ino_t;
            stat.st_nlink = std_metadata.nlink() as libc::nlink_t;
            stat.st_mode = std_metadata.mode() as libc::mode_t;
            stat.st_uid = std_metadata.uid() as libc::uid_t;
            stat.st_gid = std_metadata.gid() as libc::gid_t;
            stat.st_rdev = std_metadata.rdev() as _;
            stat.st_size = std_metadata.size() as libc::off_t;
            stat.st_blksize = std_metadata.blksize() as libc::blksize_t;
            stat.st_blocks = std_metadata.blocks() as libc::blkcnt_t;
            
            let atime = std_metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
            let mtime = std_metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let ctime = std_metadata.created().or_else(|_| std_metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            if let Ok(duration) = atime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_atime = duration.as_secs() as libc::time_t;
                stat.st_atime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_mtime = duration.as_secs() as libc::time_t;
                stat.st_mtime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = ctime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_ctime = duration.as_secs() as libc::time_t;
                stat.st_ctime_nsec = duration.subsec_nanos() as _;
            }
            
            let file_attr = FileAttr::from(stat);
            Ok(Metadata(file_attr))
        }
    }
    
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        // For io_uring systems, use the existing Op machinery
        let flags = libc::AT_STATX_SYNC_AS_STAT;
        let op = Op::statx_using_path(path, flags)?;
        op.result().await.map(FileAttr::from).map(Metadata)
    }
}

/// Query the metadata about a file without following symlinks.
///
/// # Platform-specific behavior
///
/// This function currently corresponds to the `lstat` function on linux
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// * The user lacks permissions to perform `metadata` call on `path`.
///     * execute(search) permission is required on all of the directories in path that lead to the
///       file.
/// * `path` does not exist.
///
/// # Examples
/// ```rust,no_run
/// use fs;
///
/// #[main]
/// async fn main() -> std::io::Result<()> {
///     let attr = fs::symlink_metadata("/some/file/path.txt").await?;
///     // inspect attr ...
///     Ok(())
/// }
/// ```
pub async fn symlink_metadata<P: AsRef<Path>>(path: P) -> std::io::Result<Metadata> {
    #[cfg(not(all(target_os = "linux", feature = "iouring")))]
    {
        // For non-io_uring systems, use blocking thread pool to avoid blocking the async runtime
        let path = path.as_ref().to_path_buf();
        let std_metadata = crate::blocking::unblock(move || std::fs::symlink_metadata(&path)).await?;
        
        #[cfg(target_os = "linux")]
        {
            // Convert std::fs::Metadata to libc::stat64, then to FileAttr
            use std::os::unix::fs::MetadataExt;
            let mut stat: libc::stat64 = unsafe { std::mem::zeroed() };
            
            stat.st_dev = std_metadata.dev() as _;
            stat.st_ino = std_metadata.ino() as libc::ino64_t;
            stat.st_nlink = std_metadata.nlink() as libc::nlink_t;
            stat.st_mode = std_metadata.mode() as libc::mode_t;
            stat.st_uid = std_metadata.uid() as libc::uid_t;
            stat.st_gid = std_metadata.gid() as libc::gid_t;
            stat.st_rdev = std_metadata.rdev() as _;
            stat.st_size = std_metadata.size() as libc::off64_t;
            stat.st_blksize = std_metadata.blksize() as libc::blksize_t;
            stat.st_blocks = std_metadata.blocks() as libc::blkcnt64_t;
            
            let atime = std_metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
            let mtime = std_metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let ctime = std_metadata.created().or_else(|_| std_metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            if let Ok(duration) = atime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_atime = duration.as_secs() as libc::time_t;
                stat.st_atime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_mtime = duration.as_secs() as libc::time_t;
                stat.st_mtime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = ctime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_ctime = duration.as_secs() as libc::time_t;
                stat.st_ctime_nsec = duration.subsec_nanos() as _;
            }
            
            // Note: statx_extra_fields will be None since we don't have statx data
            let file_attr = FileAttr {
                stat,
                statx_extra_fields: None,
            };
            Ok(Metadata(file_attr))
        }
        
        #[cfg(all(unix, not(target_os = "linux")))]
        {
            // Convert std::fs::Metadata to libc::stat, then to FileAttr
            // This covers macOS, FreeBSD, and other Unix-like systems
            use std::os::unix::fs::MetadataExt;
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            
            stat.st_dev = std_metadata.dev() as _;
            stat.st_ino = std_metadata.ino() as libc::ino_t;
            stat.st_nlink = std_metadata.nlink() as libc::nlink_t;
            stat.st_mode = std_metadata.mode() as libc::mode_t;
            stat.st_uid = std_metadata.uid() as libc::uid_t;
            stat.st_gid = std_metadata.gid() as libc::gid_t;
            stat.st_rdev = std_metadata.rdev() as _;
            stat.st_size = std_metadata.size() as libc::off_t;
            stat.st_blksize = std_metadata.blksize() as libc::blksize_t;
            stat.st_blocks = std_metadata.blocks() as libc::blkcnt_t;
            
            let atime = std_metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
            let mtime = std_metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let ctime = std_metadata.created().or_else(|_| std_metadata.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            
            if let Ok(duration) = atime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_atime = duration.as_secs() as libc::time_t;
                stat.st_atime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_mtime = duration.as_secs() as libc::time_t;
                stat.st_mtime_nsec = duration.subsec_nanos() as _;
            }
            if let Ok(duration) = ctime.duration_since(SystemTime::UNIX_EPOCH) {
                stat.st_ctime = duration.as_secs() as libc::time_t;
                stat.st_ctime_nsec = duration.subsec_nanos() as _;
            }
            
            let file_attr = FileAttr::from(stat);
            Ok(Metadata(file_attr))
        }
    }
    
    #[cfg(all(target_os = "linux", feature = "iouring"))]
    {
        // For io_uring systems, use the existing Op machinery
        let flags = libc::AT_STATX_SYNC_AS_STAT | libc::AT_SYMLINK_NOFOLLOW;
        let op = Op::statx_using_path(path, flags)?;
        op.result().await.map(FileAttr::from).map(Metadata)
    }
}

#[cfg(unix)]
pub(crate) use unix::FileAttr;

/// Metadata information about a file.
///
/// This structure is returned from the [`metadata`] or
/// [`symlink_metadata`] function or method and represents known
/// metadata about a file such as its permissions, size, modification
/// times, etc.
#[cfg(unix)]
pub struct Metadata(pub(crate) FileAttr);

impl Metadata {
    /// Returns `true` if this metadata is for a directory.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("path/to/dir").await?;
    ///
    ///     println!("{:?}", metadata.is_dir());
    ///     Ok(())
    /// }
    /// ```
    pub fn is_dir(&self) -> bool {
        self.0.stat.st_mode & libc::S_IFMT == libc::S_IFDIR
    }

    /// Returns `true` if this metadata is for a regular file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.is_file());
    ///     Ok(())
    /// }
    /// ```
    pub fn is_file(&self) -> bool {
        self.0.stat.st_mode & libc::S_IFMT == libc::S_IFREG
    }

    /// Returns `true` if this metadata is for a symbolic link.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.is_symlink());
    ///     Ok(())
    /// }
    /// ```
    pub fn is_symlink(&self) -> bool {
        self.0.stat.st_mode & libc::S_IFMT == libc::S_IFLNK
    }

    /// Returns the size of the file, in bytes, this metadata is for.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.len());
    ///     Ok(())
    /// }
    /// ```
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u64 {
        self.0.size()
    }

    /// Returns the last modification time listed in this metadata.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///    let metadata = fs::metadata("foo.txt").await?;
    ///
    ///    println!("{:?}", metadata.modified());
    ///    Ok(())
    /// }
    pub fn modified(&self) -> std::io::Result<SystemTime> {
        let mtime = self.0.stat.st_mtime;
        let mtime_nsec = self.0.stat.st_mtime_nsec as u32;

        Ok(SystemTime::UNIX_EPOCH + std::time::Duration::new(mtime as u64, mtime_nsec))
    }

    /// Returns the last access time listed in this metadata.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.accessed());
    ///     Ok(())
    /// }
    /// ```
    pub fn accessed(&self) -> std::io::Result<SystemTime> {
        let atime = self.0.stat.st_atime;
        let atime_nsec = self.0.stat.st_atime_nsec as u32;

        Ok(SystemTime::UNIX_EPOCH + std::time::Duration::new(atime as u64, atime_nsec))
    }

    /// Returns the creation time listed in this metadata.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.created());
    ///     Ok(())
    /// }
    /// ```
    #[cfg(target_os = "linux")]
    pub fn created(&self) -> std::io::Result<SystemTime> {
        if let Some(extra) = self.0.statx_extra_fields.as_ref() {
            return if extra.stx_mask & libc::STATX_BTIME != 0 {
                let btime = extra.stx_btime.tv_sec;
                let btime_nsec = extra.stx_btime.tv_nsec;

                Ok(SystemTime::UNIX_EPOCH + std::time::Duration::new(btime as u64, btime_nsec))
            } else {
                Err(std::io::Error::other("Creation time is not available"))
            };
        }

        Err(std::io::Error::other("Creation time is not available"))
    }

    /// Returns the permissions of the file this metadata is for.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.permissions());
    ///     Ok(())
    /// }
    /// ```
    #[cfg(unix)]
    pub fn permissions(&self) -> Permissions {
        use super::permissions::Permissions;

        Permissions(self.0.perm())
    }

    /// Returns the file type for this metadata.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fs;
    ///
    /// #[main]
    /// async fn main() -> std::io::Result<()> {
    ///     let metadata = fs::metadata("foo.txt").await?;
    ///
    ///     println!("{:?}", metadata.file_type());
    ///     Ok(())
    /// }
    /// ```
    #[cfg(unix)]
    pub fn file_type(&self) -> FileType {
        self.0.file_type()
    }
}

impl std::fmt::Debug for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Metadata");
        // debug.field("file_type", &self.file_type());
        debug.field("permissions", &self.permissions());
        debug.field("len", &self.len());
        if let Ok(modified) = self.modified() {
            debug.field("modified", &modified);
        }
        if let Ok(accessed) = self.accessed() {
            debug.field("accessed", &accessed);
        }
        #[cfg(target_os = "linux")]
        if let Ok(created) = self.created() {
            debug.field("created", &created);
        }
        debug.finish_non_exhaustive()
    }
}

#[cfg(all(target_os = "linux", not(target_pointer_width = "32")))]
impl MetadataExt for Metadata {
    fn dev(&self) -> u64 {
        self.0.stat.st_dev
    }

    fn ino(&self) -> u64 {
        self.0.stat.st_ino
    }

    fn mode(&self) -> u32 {
        self.0.stat.st_mode
    }

    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    fn nlink(&self) -> u64 {
        self.0.stat.st_nlink.into()
    }

    /// longarch64 need the `into` convert.
    #[allow(clippy::useless_conversion)]
    #[cfg(not(any(target_arch = "aarch64", target_arch = "riscv64")))]
    fn nlink(&self) -> u64 {
        self.0.stat.st_nlink.into()
    }

    fn uid(&self) -> u32 {
        self.0.stat.st_uid
    }

    fn gid(&self) -> u32 {
        self.0.stat.st_gid
    }

    fn rdev(&self) -> u64 {
        self.0.stat.st_rdev
    }

    fn size(&self) -> u64 {
        self.0.stat.st_size as u64
    }

    fn atime(&self) -> i64 {
        self.0.stat.st_atime
    }

    fn atime_nsec(&self) -> i64 {
        self.0.stat.st_atime_nsec
    }

    fn mtime(&self) -> i64 {
        self.0.stat.st_mtime
    }

    fn mtime_nsec(&self) -> i64 {
        self.0.stat.st_mtime_nsec
    }

    fn ctime(&self) -> i64 {
        self.0.stat.st_ctime
    }

    fn ctime_nsec(&self) -> i64 {
        self.0.stat.st_ctime_nsec
    }

    fn blksize(&self) -> u64 {
        self.0.stat.st_blksize as u64
    }

    fn blocks(&self) -> u64 {
        self.0.stat.st_blocks as u64
    }
}

#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
impl MetadataExt for Metadata {
    fn dev(&self) -> u64 {
        self.0.stat.st_dev as u64
    }

    fn ino(&self) -> u64 {
        self.0.stat.st_ino
    }

    fn mode(&self) -> u32 {
        self.0.stat.st_mode as u32
    }

    fn nlink(&self) -> u64 {
        self.0.stat.st_nlink.into()
    }

    fn uid(&self) -> u32 {
        self.0.stat.st_uid
    }

    fn gid(&self) -> u32 {
        self.0.stat.st_gid
    }

    fn rdev(&self) -> u64 {
        self.0.stat.st_rdev as u64
    }

    fn size(&self) -> u64 {
        self.0.stat.st_size as u64
    }

    fn atime(&self) -> i64 {
        self.0.stat.st_atime
    }

    fn atime_nsec(&self) -> i64 {
        self.0.stat.st_atime_nsec
    }

    fn mtime(&self) -> i64 {
        self.0.stat.st_mtime
    }

    fn mtime_nsec(&self) -> i64 {
        self.0.stat.st_mtime_nsec
    }

    fn ctime(&self) -> i64 {
        self.0.stat.st_ctime
    }

    fn ctime_nsec(&self) -> i64 {
        self.0.stat.st_ctime_nsec
    }

    fn blksize(&self) -> u64 {
        self.0.stat.st_blksize as u64
    }

    fn blocks(&self) -> u64 {
        self.0.stat.st_blocks as u64
    }
}

#[cfg(all(unix, target_pointer_width = "32"))]
impl MetadataExt for Metadata {
    fn dev(&self) -> u64 {
        self.0.stat.st_dev.into()
    }

    fn ino(&self) -> u64 {
        self.0.stat.st_ino.into()
    }

    fn mode(&self) -> u32 {
        self.0.stat.st_mode
    }

    fn nlink(&self) -> u64 {
        self.0.stat.st_nlink.into()
    }

    fn uid(&self) -> u32 {
        self.0.stat.st_uid
    }

    fn gid(&self) -> u32 {
        self.0.stat.st_gid
    }

    fn rdev(&self) -> u64 {
        self.0.stat.st_rdev.into()
    }

    fn size(&self) -> u64 {
        self.0.stat.st_size as u64
    }

    fn atime(&self) -> i64 {
        self.0.stat.st_atime.into()
    }

    fn atime_nsec(&self) -> i64 {
        self.0.stat.st_atime_nsec.into()
    }

    fn mtime(&self) -> i64 {
        self.0.stat.st_mtime.into()
    }

    fn mtime_nsec(&self) -> i64 {
        self.0.stat.st_mtime_nsec.into()
    }

    fn ctime(&self) -> i64 {
        self.0.stat.st_ctime.into()
    }

    fn ctime_nsec(&self) -> i64 {
        self.0.stat.st_ctime_nsec.into()
    }

    fn blksize(&self) -> u64 {
        self.0.stat.st_blksize as u64
    }

    fn blocks(&self) -> u64 {
        self.0.stat.st_blocks as u64
    }
}
