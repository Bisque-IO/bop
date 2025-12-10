use crate::ManiacRuntime;
use openraft::AppData;
use openraft::AppDataResponse;
use openraft::RaftTypeConfig;
use openraft::impls::BasicNode;
use openraft::impls::Entry;
use openraft::impls::OneshotResponder;
use openraft::impls::Vote;
use openraft::impls::leader_id_adv::LeaderId;
use std::io::Cursor;

pub struct ManiacRaftTypeConfig<D, R> {
    _d: std::marker::PhantomData<D>,
    _r: std::marker::PhantomData<R>,
}

impl<D, R> Clone for ManiacRaftTypeConfig<D, R> {
    fn clone(&self) -> Self {
        Self {
            _d: std::marker::PhantomData,
            _r: std::marker::PhantomData,
        }
    }
}

impl<D, R> Copy for ManiacRaftTypeConfig<D, R> {}

impl<D, R> Default for ManiacRaftTypeConfig<D, R> {
    fn default() -> Self {
        Self {
            _d: std::marker::PhantomData,
            _r: std::marker::PhantomData,
        }
    }
}

impl<D, R> PartialEq for ManiacRaftTypeConfig<D, R> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<D, R> Eq for ManiacRaftTypeConfig<D, R> {}

impl<D, R> PartialOrd for ManiacRaftTypeConfig<D, R> {
    fn partial_cmp(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        Some(std::cmp::Ordering::Equal)
    }
}

impl<D, R> Ord for ManiacRaftTypeConfig<D, R> {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}

impl<D, R> std::fmt::Debug for ManiacRaftTypeConfig<D, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManiacRaftTypeConfig").finish()
    }
}

impl<D, R> RaftTypeConfig for ManiacRaftTypeConfig<D, R>
where
    D: AppData,
    R: AppDataResponse,
{
    type D = D;
    type R = R;
    type NodeId = u64;
    type Node = BasicNode;
    type Term = u64;
    type LeaderId = LeaderId<Self>;
    type Vote = Vote<Self>;
    type Entry = Entry<Self>;
    type SnapshotData = Cursor<Vec<u8>>;
    type Responder<T: openraft::OptionalSend + 'static> = OneshotResponder<Self, T>;
    type AsyncRuntime = ManiacRuntime;
}
