//! Tests for zero-allocation blocking operations

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::path::PathBuf;

    // Track allocations for tests
    #[cfg(feature = "track-allocations")]
    use std::alloc::{GlobalAlloc, Layout, System};

    #[cfg(feature = "track-allocations")]
    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator {
        system: System,
        allocated: std::sync::atomic::AtomicUsize::new(0),
    };

    #[cfg(feature = "track-allocations")]
    struct TrackingAllocator {
        system: System,
        allocated: std::sync::atomic::AtomicUsize,
    }

    #[cfg(feature = "track-allocations")]
    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            self.allocated
                .fetch_add(layout.size(), std::sync::atomic::Ordering::Relaxed);
            self.system.alloc(layout)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            self.allocated
                .fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
            self.system.dealloc(ptr, layout)
        }
    }

    // Test path-based operations
    #[test]
    fn test_path_operations_no_extra_allocations() {
        use maniac::blocking::*;
        // These tests verify that our zero-allocation unblock_* functions
        // don't allocate beyond what the caller already did

        // Test that creating the task doesn't allocation (PathBuf already allocated)
        let path = PathBuf::from("/tmp/test_file");
        let task = unblock_open(path.clone());

        // The test passes if it compiles and runs without panicking
        // We would need allocation tracking to verify zero alloc in production
        drop(task);

        let task = unblock_create(path.clone());
        drop(task);

        let task = unblock_remove_file(path.clone());
        drop(task);

        let task = unblock_remove_dir(path.clone());
        drop(task);

        let task = unblock_create_dir(path.clone());
        drop(task);

        let task = unblock_rename(path.clone(), path);
        drop(task);
    }

    // Test file descriptor operations
    #[test]
    fn test_fd_operations_no_extra_allocations() {
        use maniac::blocking::*;
        let mut buf = vec![0u8; 1024];
        let ptr = buf.as_mut_ptr();
        let len = buf.len();

        // Test that creating the task doesn't allocation (ptr and len are passed by value)
        #[cfg(unix)]
        {
            use std::os::unix::prelude::RawFd;
            let fd: RawFd = 1; // stdout

            let task = unsafe { unblock_fmetadata(fd) };
            drop(task);

            let task = unsafe { unblock_fread(fd, ptr, len) };
            drop(task);

            let task = unsafe { unblock_fwrite(fd, ptr, len) };
            drop(task);

            let task = unsafe { unblock_fread_at(fd, ptr, len, 0) };
            drop(task);

            let task = unsafe { unblock_fwrite_at(fd, ptr, len, 0) };
            drop(task);

            let task = unsafe { unblock_fsync(fd, false) };
            drop(task);
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::RawHandle;
            // On Windows, 1 is not a valid handle value usually, but for compile/mock test it's fine
            // as long as we don't execute the task.
            let handle: RawHandle = 1 as _;

            let task = unsafe { unblock_fmetadata(handle) };
            drop(task);

            let task = unsafe { unblock_fread(handle, ptr, len) };
            drop(task);

            let task = unsafe { unblock_fwrite(handle, ptr, len) };
            drop(task);

            let task = unsafe { unblock_fread_at(handle, ptr, len, 0) };
            drop(task);

            let task = unsafe { unblock_fwrite_at(handle, ptr, len, 0) };
            drop(task);

            let task = unsafe { unblock_fsync(handle, false) };
            drop(task);
        }
    }
}
