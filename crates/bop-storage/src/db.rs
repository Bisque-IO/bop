use crate::manager::Manager;

/// Top-level handle to a storage database instance.
pub struct DB {
    manager: Manager,
}

impl DB {
    /// Construct a new database backed by the provided manager.
    pub fn new(manager: Manager) -> Self {
        Self { manager }
    }

    /// Access the underlying manager.
    pub fn manager(&self) -> &Manager {
        &self.manager
    }

    /// Consume the database and return the owned manager.
    pub fn into_manager(self) -> Manager {
        self.manager
    }
}
