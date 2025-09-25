use crate::page_cache::{GlobalPageCache, PageCacheKey};
use generator::{Gn, Scope};
use std::sync::{Arc, Mutex};

/// Steps yielded by the VFS coroutine so drivers can perform blocking IO.
#[derive(Debug, Clone)]
pub enum VfsOp {
    FetchPage {
        key: PageCacheKey,
    },
    FlushPage {
        key: PageCacheKey,
        data: Arc<Vec<u8>>,
    },
    Sync,
}

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("storage fetch failed: {0}")]
    Fetch(String),
    #[error("flush failed: {0}")]
    Flush(String),
    #[error("sync failed: {0}")]
    Sync(String),
}

type ReadGen = generator::Generator<'static, Result<Arc<Vec<u8>>, VfsError>, VfsOp>;
type WriteGen = generator::Generator<'static, Result<(), VfsError>, VfsOp>;

#[derive(Debug)]
pub struct LibsqlVfs {
    cache: Arc<GlobalPageCache>,
}

impl LibsqlVfs {
    pub fn new(cache: Arc<GlobalPageCache>) -> Self {
        Self { cache }
    }

    pub fn read(&self, store_id: u64, generation: u64, page_id: u32) -> VfsReadCoroutine {
        VfsReadCoroutine::new(
            Arc::clone(&self.cache),
            store_id,
            generation,
            page_id as u64,
        )
    }

    pub fn write(
        &self,
        store_id: u64,
        generation: u64,
        page_id: u32,
        data: Arc<Vec<u8>>,
        require_sync: bool,
    ) -> VfsWriteCoroutine {
        VfsWriteCoroutine::new(
            Arc::clone(&self.cache),
            store_id,
            generation,
            page_id as u64,
            data,
            require_sync,
        )
    }
}

#[derive(Debug)]
pub struct VfsReadCoroutine {
    inner: ReadGen,
    result: Arc<Mutex<Option<Result<Arc<Vec<u8>>, VfsError>>>>,
    started: bool,
    finished: bool,
}

impl VfsReadCoroutine {
    fn new(cache: Arc<GlobalPageCache>, store: u64, generation: u64, page_id: u64) -> Self {
        let key = PageCacheKey {
            store_id: store,
            generation,
            page_id,
        };
        let result_slot: Arc<Mutex<Option<Result<Arc<Vec<u8>>, VfsError>>>> =
            Arc::new(Mutex::new(None));
        let slot = Arc::clone(&result_slot);
        let inner: ReadGen =
            Gn::<Result<Arc<Vec<u8>>, VfsError>>::new_scoped(move |mut scope: Scope<_, _>| {
                if let Some(hit) = cache.get(&key) {
                    *slot.lock().expect("result slot poisoned") = Some(Ok(hit));
                    generator::done!();
                }
                match scope.yield_(VfsOp::FetchPage { key: key.clone() }) {
                    Some(Ok(bytes)) => {
                        cache.insert(key.clone(), Arc::clone(&bytes));
                        *slot.lock().expect("result slot poisoned") = Some(Ok(bytes));
                    }
                    Some(Err(err)) => {
                        *slot.lock().expect("result slot poisoned") = Some(Err(err));
                    }
                    None => {
                        *slot.lock().expect("result slot poisoned") =
                            Some(Err(VfsError::Fetch("generator dropped".into())));
                    }
                }
                generator::done!();
            });
        Self {
            inner,
            result: result_slot,
            started: false,
            finished: false,
        }
    }

    pub fn next(
        &mut self,
        input: Option<Result<Arc<Vec<u8>>, VfsError>>,
    ) -> VfsStatus<Arc<Vec<u8>>> {
        if self.finished {
            panic!("VFS read coroutine already completed");
        }
        let step = if !self.started {
            self.started = true;
            self.inner.raw_send(None)
        } else {
            let value = input.expect("read coroutine result required");
            self.inner.raw_send(Some(value))
        };
        match step {
            Some(op) => VfsStatus::InProgress(op),
            None => {
                self.finished = true;
                let res = self
                    .result
                    .lock()
                    .expect("result slot poisoned")
                    .take()
                    .unwrap_or_else(|| Err(VfsError::Fetch("missing result".into())));
                VfsStatus::Finished(res)
            }
        }
    }
}

#[derive(Debug)]
pub struct VfsWriteCoroutine {
    inner: WriteGen,
    result: Arc<Mutex<Option<Result<(), VfsError>>>>,
    started: bool,
    finished: bool,
}

impl VfsWriteCoroutine {
    fn new(
        cache: Arc<GlobalPageCache>,
        store: u64,
        generation: u64,
        page_id: u64,
        data: Arc<Vec<u8>>,
        require_sync: bool,
    ) -> Self {
        let key = PageCacheKey {
            store_id: store,
            generation,
            page_id,
        };
        let result_slot: Arc<Mutex<Option<Result<(), VfsError>>>> = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&result_slot);
        let inner: WriteGen =
            Gn::<Result<(), VfsError>>::new_scoped(move |mut scope: Scope<_, _>| {
                cache.insert(key.clone(), Arc::clone(&data));
                if let Some(res) = scope.yield_(VfsOp::FlushPage {
                    key: key.clone(),
                    data: Arc::clone(&data),
                }) {
                    if let Err(err) = res {
                        *slot.lock().expect("result slot poisoned") = Some(Err(err));
                        generator::done!();
                    }
                } else {
                    *slot.lock().expect("result slot poisoned") =
                        Some(Err(VfsError::Flush("generator dropped".into())));
                    generator::done!();
                }
                if require_sync {
                    match scope.yield_(VfsOp::Sync) {
                        Some(Ok(_)) => {}
                        Some(Err(err)) => {
                            *slot.lock().expect("result slot poisoned") = Some(Err(err));
                            generator::done!();
                        }
                        None => {
                            *slot.lock().expect("result slot poisoned") =
                                Some(Err(VfsError::Sync("generator dropped".into())));
                            generator::done!();
                        }
                    }
                }
                *slot.lock().expect("result slot poisoned") = Some(Ok(()));
                generator::done!();
            });
        Self {
            inner,
            result: result_slot,
            started: false,
            finished: false,
        }
    }

    pub fn next(&mut self, input: Option<Result<(), VfsError>>) -> VfsStatus<()> {
        if self.finished {
            panic!("VFS write coroutine already completed");
        }
        let step = if !self.started {
            self.started = true;
            self.inner.raw_send(None)
        } else {
            let value = input.expect("write coroutine result required");
            self.inner.raw_send(Some(value))
        };
        match step {
            Some(op) => VfsStatus::InProgress(op),
            None => {
                self.finished = true;
                let res = self
                    .result
                    .lock()
                    .expect("result slot poisoned")
                    .take()
                    .unwrap_or_else(|| Err(VfsError::Flush("missing result".into())));
                VfsStatus::Finished(res)
            }
        }
    }
}

#[derive(Debug)]
pub enum VfsStatus<T> {
    InProgress(VfsOp),
    Finished(Result<T, VfsError>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn read_hits_cache_without_fetch() {
        let cache = Arc::new(GlobalPageCache::new());
        let key = PageCacheKey {
            store_id: 1,
            generation: 2,
            page_id: 10,
        };
        cache.insert(key.clone(), Arc::new(vec![9u8; 4]));
        let vfs = LibsqlVfs::new(Arc::clone(&cache));
        let mut co = vfs.read(key.store_id, key.generation, key.page_id as u32);
        match co.next(None) {
            VfsStatus::Finished(Ok(page)) => assert_eq!(&*page, &[9u8; 4]),
            other => panic!("unexpected status {other:?}"),
        }
    }

    #[test]
    fn read_miss_fetches_from_driver() {
        let cache = Arc::new(GlobalPageCache::new());
        let vfs = LibsqlVfs::new(Arc::clone(&cache));
        let mut co = vfs.read(1, 1, 1);
        match co.next(None) {
            VfsStatus::InProgress(VfsOp::FetchPage { .. }) => {}
            other => panic!("unexpected status {other:?}"),
        }
        let page = Arc::new(vec![7u8; 8]);
        match co.next(Some(Ok(Arc::clone(&page)))) {
            VfsStatus::Finished(Ok(result)) => assert_eq!(&*result, &*page),
            other => panic!("unexpected status {other:?}"),
        }
        assert!(
            cache
                .get(&PageCacheKey {
                    store_id: 1,
                    generation: 1,
                    page_id: 1,
                })
                .is_some()
        );
    }

    #[test]
    fn write_flushes_then_syncs() {
        let cache = Arc::new(GlobalPageCache::new());
        let vfs = LibsqlVfs::new(Arc::clone(&cache));
        let data = Arc::new(vec![1u8; 16]);
        let mut co = vfs.write(3, 4, 5, Arc::clone(&data), true);
        match co.next(None) {
            VfsStatus::InProgress(VfsOp::FlushPage { key, data: flush }) => {
                assert_eq!(key.store_id, 3);
                assert_eq!(flush.len(), data.len());
            }
            other => panic!("unexpected status {other:?}"),
        }
        match co.next(Some(Ok(()))) {
            VfsStatus::InProgress(VfsOp::Sync) => {}
            other => panic!("expected sync op, got {other:?}"),
        }
        match co.next(Some(Ok(()))) {
            VfsStatus::Finished(Ok(())) => {}
            other => panic!("unexpected status {other:?}"),
        }
    }
}
