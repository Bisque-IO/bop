use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::config::SegmentId;
use crate::error::AofResult;
use crate::store::{InstanceId, Tier2Config, Tier2Manager};
use tokio::runtime::Handle;

/// Context passed to metadata persistence failure injection hooks.
#[derive(Debug, Clone, Copy)]
pub struct MetadataPersistContext {
    pub instance_id: InstanceId,
    pub segment_id: SegmentId,
    pub attempt: u32,
    pub requested_bytes: u32,
    pub durable_bytes: u32,
}

/// Hook signature for metadata persistence overrides.
pub type MetadataPersistHook =
    dyn Fn(&MetadataPersistContext) -> Option<AofResult<()>> + Send + Sync + 'static;

#[cfg(debug_assertions)]
fn metadata_hook_slot() -> &'static RwLock<Option<Arc<MetadataPersistHook>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<MetadataPersistHook>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Query the currently installed metadata persistence hook, if any, to determine whether
/// the default implementation should be overridden.
pub fn metadata_persist_override(ctx: &MetadataPersistContext) -> Option<AofResult<()>> {
    #[cfg(debug_assertions)]
    {
        metadata_hook_slot()
            .read()
            .as_ref()
            .and_then(|hook| hook(ctx))
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = ctx;
        None
    }
}

/// Guard that restores the previous metadata persistence hook when dropped.
pub struct MetadataPersistHookGuard {
    #[cfg(debug_assertions)]
    previous: Option<Arc<MetadataPersistHook>>,
}

impl MetadataPersistHookGuard {
    /// Construct a no-op guard when failure injection is disabled at compile time.
    #[cfg(not(debug_assertions))]
    pub fn new() -> Self {
        Self {}
    }
}

impl Drop for MetadataPersistHookGuard {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let mut slot = metadata_hook_slot().write();
            *slot = self.previous.take();
        }
    }
}

/// Install a metadata persistence hook, returning a guard that will revert to the previous
/// hook (if any) when dropped.
#[cfg(debug_assertions)]
pub fn install_metadata_persist_hook<F>(hook: F) -> MetadataPersistHookGuard
where
    F: Fn(&MetadataPersistContext) -> Option<AofResult<()>> + Send + Sync + 'static,
{
    let mut slot = metadata_hook_slot().write();
    let previous = std::mem::replace(&mut *slot, Some(Arc::new(hook)));
    MetadataPersistHookGuard { previous }
}

/// Install a metadata persistence hook. This is a no-op when failure injection is compiled out.
#[cfg(not(debug_assertions))]
pub fn install_metadata_persist_hook<F>(_hook: F) -> MetadataPersistHookGuard
where
    F: Fn(&MetadataPersistContext) -> Option<AofResult<()>> + Send + Sync + 'static,
{
    MetadataPersistHookGuard::new()
}

/// Clear any registered metadata persistence hook.
#[cfg(debug_assertions)]
pub fn clear_metadata_persist_hook() {
    let mut slot = metadata_hook_slot().write();
    *slot = None;
}

/// Clear metadata persistence hook (no-op when compiled without failure injection support).
#[cfg(not(debug_assertions))]
pub fn clear_metadata_persist_hook() {}

/// Factory signature for overriding Tier2 manager construction in tests.
pub type Tier2ManagerFactory =
    dyn Fn(Handle, Tier2Config) -> AofResult<Tier2Manager> + Send + Sync + 'static;

#[cfg(debug_assertions)]
fn tier2_factory_slot() -> &'static RwLock<Option<Arc<Tier2ManagerFactory>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<Tier2ManagerFactory>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Attempt to build a Tier2 manager using the registered factory, if present.
pub fn tier2_manager_override(
    handle: Handle,
    config: &Tier2Config,
) -> Option<AofResult<Tier2Manager>> {
    #[cfg(debug_assertions)]
    {
        tier2_factory_slot()
            .read()
            .as_ref()
            .map(|factory| factory(handle, config.clone()))
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = (handle, config);
        None
    }
}

/// Guard that restores the previous Tier2 manager factory when dropped.
pub struct Tier2ManagerFactoryGuard {
    #[cfg(debug_assertions)]
    previous: Option<Arc<Tier2ManagerFactory>>,
}

impl Tier2ManagerFactoryGuard {
    #[cfg(not(debug_assertions))]
    pub fn new() -> Self {
        Self {}
    }
}

impl Drop for Tier2ManagerFactoryGuard {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let mut slot = tier2_factory_slot().write();
            *slot = self.previous.take();
        }
    }
}

/// Install a Tier2 manager factory override for tests.
#[cfg(debug_assertions)]
pub fn install_tier2_manager_factory<F>(factory: F) -> Tier2ManagerFactoryGuard
where
    F: Fn(Handle, Tier2Config) -> AofResult<Tier2Manager> + Send + Sync + 'static,
{
    let mut slot = tier2_factory_slot().write();
    let previous = std::mem::replace(&mut *slot, Some(Arc::new(factory)));
    Tier2ManagerFactoryGuard { previous }
}

/// Install a Tier2 manager factory override (no-op when compiled without failure injection support).
#[cfg(not(debug_assertions))]
pub fn install_tier2_manager_factory<F>(_factory: F) -> Tier2ManagerFactoryGuard
where
    F: Fn(Handle, Tier2Config) -> AofResult<Tier2Manager> + Send + Sync + 'static,
{
    Tier2ManagerFactoryGuard::new()
}

/// Clear any registered Tier2 manager factory override.
#[cfg(debug_assertions)]
pub fn clear_tier2_manager_factory() {
    let mut slot = tier2_factory_slot().write();
    *slot = None;
}

/// Clear Tier2 manager factory override (no-op when compiled without failure injection support).
#[cfg(not(debug_assertions))]
pub fn clear_tier2_manager_factory() {}

/// Create an `InstanceId` for tests.
pub fn make_instance_id(value: u64) -> InstanceId {
    InstanceId::new(value)
}

