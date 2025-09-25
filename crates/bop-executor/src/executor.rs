use crate::selector::Selector;
use crate::signal::{SLOTS_PER_GROUP, SignalGroup};

pub const EXECUTE_ERR_EMPTY_SIGNAL: i32 = -1;
pub const EXECUTE_ERR_CORE_IS_NIL: i32 = -2;
pub const EXECUTE_ERR_CONTENDED: i32 = -3;
pub const EXECUTE_ERR_NO_WORK: i32 = -4;

pub struct Executor<const PARALLELISM: usize, const BLOCKING: bool> {
    #[allow(dead_code)]
    allocator: (),
    groups: Vec<SignalGroup>,
}

impl<const PARALLELISM: usize, const BLOCKING: bool> Executor<PARALLELISM, BLOCKING> {
    pub fn new() -> Self {
        assert!(PARALLELISM > 0, "PARALLELISM must be greater than zero");
        assert!(PARALLELISM < 1024, "PARALLELISM must be less than 1024");
        assert!(
            PARALLELISM.is_power_of_two(),
            "PARALLELISM must be a power of two"
        );

        let groups = (0..PARALLELISM).map(|_| SignalGroup::new()).collect();

        Self {
            allocator: (),
            groups,
        }
    }

    pub fn destroy(&mut self) {
        self.groups.clear();
    }

    pub fn capacity(&self) -> usize {
        PARALLELISM * SLOTS_PER_GROUP
    }

    pub fn execute(&self, _selector: &mut Selector) -> Result<(), ExecuteError> {
        // TODO: Implement the core execution loop once slot reservation is in place.
        Err(ExecuteError::NoWork)
    }

    pub fn groups(&self) -> &[SignalGroup] {
        &self.groups
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecuteError {
    EmptySignal,
    CoreIsNil,
    Contended,
    NoWork,
}
