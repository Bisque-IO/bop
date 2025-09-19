use std::sync::Arc;

use tokio::runtime::Runtime;

pub struct FlushManager {
    rt: Arc<Runtime>,
}

#[cfg(test)]
mod tests {
    use memmap2::MmapMut;
    use std::fs::File;

    #[test]
    fn test_flush() {}
}
