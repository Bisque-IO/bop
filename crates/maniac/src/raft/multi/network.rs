use crate::io::{AsyncReadRent, AsyncWriteRent};
use crate::raft::multi::config::MultiRaftConfig;
use crate::sync::mpsc::bounded as mpsc_bounded;
use crate::sync::mutex::Mutex;
use crate::sync::oneshot;
use dashmap::DashMap;
use maniac_raft::OptionalSend;
use maniac_raft::OptionalSync;
use maniac_raft::RaftTypeConfig;
use maniac_raft::StorageError;
use maniac_raft::async_runtime::AsyncRuntime;
use maniac_raft::async_runtime::{Mpsc, MpscReceiver, MpscSender};
use maniac_raft::error::InstallSnapshotError;
use maniac_raft::error::RPCError;
use maniac_raft::error::RaftError;
use maniac_raft::error::StreamingError;
use maniac_raft::error::decompose::DecomposeResult;
use maniac_raft::network::RPCOption;
use maniac_raft::network::RaftNetworkFactory;
use maniac_raft::network::v2::RaftNetworkV2;
use maniac_raft::raft::AppendEntriesRequest;
use maniac_raft::raft::AppendEntriesResponse;
use maniac_raft::raft::InstallSnapshotRequest;
use maniac_raft::raft::InstallSnapshotResponse;
use maniac_raft::raft::SnapshotResponse;
use maniac_raft::raft::VoteRequest;
use maniac_raft::raft::VoteResponse;
use maniac_raft::storage::Snapshot;
use maniac_raft::{ErrorSubject, ErrorVerb};
use std::future::Future;
use std::marker::Unpin;
use std::sync::Arc;
use std::time::Duration;

pub trait MultiplexedTransport<C: RaftTypeConfig>: OptionalSend + OptionalSync + 'static {
    fn send_append_entries(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: AppendEntriesRequest<C>,
    ) -> impl Future<Output = Result<AppendEntriesResponse<C>, RPCError<C, RaftError<C>>>> + OptionalSend;

    fn send_vote(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: VoteRequest<C>,
    ) -> impl Future<Output = Result<VoteResponse<C>, RPCError<C, RaftError<C>>>> + OptionalSend;

    fn send_install_snapshot(
        &self,
        target: C::NodeId,
        group_id: u64,
        rpc: InstallSnapshotRequest<C>,
    ) -> impl Future<
        Output = Result<
            InstallSnapshotResponse<C>,
            RPCError<C, RaftError<C, InstallSnapshotError>>,
        >,
    > + OptionalSend;

    fn send_heartbeat_batch(
        &self,
        target: C::NodeId,
        batch: &[(u64, AppendEntriesRequest<C>)],
    ) -> impl Future<
        Output = Result<Vec<(u64, AppendEntriesResponse<C>)>, RPCError<C, RaftError<C>>>,
    > + OptionalSend;
}

type HeartbeatTx<C> = oneshot::Sender<Result<AppendEntriesResponse<C>, RPCError<C, RaftError<C>>>>;

type HeartbeatMsg<C> = (u64, AppendEntriesRequest<C>, HeartbeatTx<C>);

struct HeartbeatBuffer<C: RaftTypeConfig> {
    tx: mpsc_bounded::MpscSender<HeartbeatMsg<C>>,
}

impl<C: RaftTypeConfig> Clone for HeartbeatBuffer<C> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

pub struct MultiRaftNetworkFactory<C: RaftTypeConfig, T: MultiplexedTransport<C>> {
    transport: Arc<T>,
    config: MultiRaftConfig,
    buffers: Arc<DashMap<C::NodeId, HeartbeatBuffer<C>>>,
}

#[derive(Debug)]
struct BatchError(String);

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BatchError {}

impl<C, T> MultiRaftNetworkFactory<C, T>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
    C::Entry: Clone,
{
    pub fn new(transport: T, config: MultiRaftConfig) -> Self {
        Self {
            transport: Arc::new(transport),
            config,
            buffers: Arc::new(DashMap::new()),
        }
    }

    fn get_buffer(&self, target: C::NodeId) -> HeartbeatBuffer<C> {
        if let Some(buffer) = self.buffers.get(&target) {
            return buffer.clone();
        }

        let (tx, mut rx) = mpsc_bounded::channel();
        let buffer = HeartbeatBuffer { tx };
        self.buffers.insert(target.clone(), buffer.clone());

        // Spawn flush task
        let flush_transport = self.transport.clone();
        let flush_target = target.clone();

        // Use C::AsyncRuntime to spawn
        let _ = C::AsyncRuntime::spawn(async move {
            let mut batch_req: Vec<(u64, AppendEntriesRequest<C>)> = Vec::new();
            let mut pending_txs: Vec<(u64, HeartbeatTx<C>)> = Vec::new();
            let mut resp_map: std::collections::HashMap<u64, AppendEntriesResponse<C>> =
                std::collections::HashMap::new();

            loop {
                // Wait for the first message
                let first = match rx.recv().await {
                    Ok(msg) => msg,
                    Err(_) => return, // Channel closed
                };

                batch_req.clear();
                pending_txs.clear();

                // Add first message
                batch_req.push((first.0, first.1));
                pending_txs.push((first.0, first.2));

                // Drain any other available messages
                while let Ok(msg) = rx.try_recv() {
                    batch_req.push((msg.0, msg.1));
                    pending_txs.push((msg.0, msg.2));
                }

                let result = flush_transport
                    .send_heartbeat_batch(flush_target.clone(), &batch_req)
                    .await;

                match result {
                    Ok(responses) => {
                        resp_map.clear();
                        for (gid, resp) in responses {
                            resp_map.insert(gid, resp);
                        }

                        for (gid, sender) in pending_txs.drain(..) {
                            if let Some(resp) = resp_map.remove(&gid) {
                                let _ = sender.send(Ok(resp));
                            } else {
                                let err = BatchError("Missing response in batch".to_string());
                                let _ = sender.send(Err(RPCError::Network(
                                    maniac_raft::error::NetworkError::new(&err),
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        for (_, sender) in pending_txs.drain(..) {
                            let _ = sender.send(Err(e.clone()));
                        }
                    }
                }
            }
        });

        buffer
    }
}

impl<C, T> RaftNetworkFactory<C> for MultiRaftNetworkFactory<C, T>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
    C::Entry: Clone,
{
    type Network = MultiRaftNetwork<C, T>;

    async fn new_client(&mut self, target: C::NodeId, node: &C::Node) -> Self::Network {
        panic!("MultiRaftNetworkFactory cannot be used directly. Use GroupNetworkFactory.");
    }
}

pub struct GroupNetworkFactory<C: RaftTypeConfig, T: MultiplexedTransport<C>> {
    parent: Arc<MultiRaftNetworkFactory<C, T>>,
    group_id: u64,
}

impl<C, T> GroupNetworkFactory<C, T>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
{
    pub fn new(parent: Arc<MultiRaftNetworkFactory<C, T>>, group_id: u64) -> Self {
        Self { parent, group_id }
    }
}

impl<C, T> RaftNetworkFactory<C> for GroupNetworkFactory<C, T>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
    C::Entry: Clone,
{
    type Network = MultiRaftNetwork<C, T>;

    async fn new_client(&mut self, target: C::NodeId, _node: &C::Node) -> Self::Network {
        let buffer = self.parent.get_buffer(target.clone());
        MultiRaftNetwork {
            transport: self.parent.transport.clone(),
            buffer,
            target,
            group_id: self.group_id,
        }
    }
}

pub struct MultiRaftNetwork<C: RaftTypeConfig, T: MultiplexedTransport<C>> {
    transport: Arc<T>,
    buffer: HeartbeatBuffer<C>,
    target: C::NodeId,
    group_id: u64,
}

impl<C, T> RaftNetworkV2<C> for MultiRaftNetwork<C, T>
where
    C: RaftTypeConfig,
    T: MultiplexedTransport<C>,
    C::SnapshotData: AsyncReadRent + AsyncWriteRent + Unpin,
    C::Entry: Clone,
{
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<C>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<C>, RPCError<C>> {
        // Check if heartbeat (empty entries)
        if rpc.entries.is_empty() {
            let (tx, rx) = oneshot::channel();

            if let Err(_) = self.buffer.tx.send((self.group_id, rpc, tx)).await {
                let err = BatchError("Heartbeat buffer closed".to_string());
                return Err(RPCError::Network(maniac_raft::error::NetworkError::new(
                    &err,
                )));
            }

            // Wait for response
            match rx.await {
                Ok(res) => res.decompose_infallible(),
                Err(_) => {
                    let err = BatchError("Heartbeat channel closed".to_string());
                    Err(RPCError::Network(maniac_raft::error::NetworkError::new(
                        &err,
                    )))
                }
            }
        } else {
            self.transport
                .send_append_entries(self.target.clone(), self.group_id, rpc)
                .await
                .decompose_infallible()
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<C>,
        _option: RPCOption,
    ) -> Result<VoteResponse<C>, RPCError<C>> {
        self.transport
            .send_vote(self.target.clone(), self.group_id, rpc)
            .await
            .decompose_infallible()
    }

    async fn full_snapshot(
        &mut self,
        vote: maniac_raft::type_config::alias::VoteOf<C>,
        snapshot: Snapshot<C>,
        _cancel: impl Future<Output = maniac_raft::error::ReplicationClosed> + OptionalSend + 'static,
        option: RPCOption,
    ) -> Result<SnapshotResponse<C>, StreamingError<C>> {
        let mut offset = 0u64;
        let mut snapshot_data = snapshot.snapshot;
        let snapshot_meta = snapshot.meta;

        loop {
            let chunk_size = option.snapshot_chunk_size().unwrap_or(1024 * 1024);
            let buf = vec![0u8; chunk_size];

            let (res, mut buf) = snapshot_data.read(buf).await;
            let n = res.map_err(|e| {
                StreamingError::from(StorageError::from_io_error(
                    ErrorSubject::Snapshot(Some(snapshot_meta.signature())),
                    ErrorVerb::Read,
                    e,
                ))
            })?;

            buf.truncate(n);

            if n == 0 {
                // EOF - send final empty chunk with done=true
                let req = InstallSnapshotRequest {
                    vote: vote.clone(),
                    meta: snapshot_meta.clone(),
                    offset,
                    data: Vec::new(),
                    done: true,
                };
                let res = match self
                    .transport
                    .send_install_snapshot(self.target.clone(), self.group_id, req)
                    .await
                    .decompose()
                {
                    Ok(Ok(r)) => r,
                    Ok(Err(_snapshot_err)) => {
                        // InstallSnapshotError is received - convert to network error
                        return Err(StreamingError::Network(
                            maniac_raft::error::NetworkError::new(&BatchError(
                                "Snapshot rejected by remote".to_string(),
                            )),
                        ));
                    }
                    Err(rpc_err) => return Err(StreamingError::from(rpc_err)),
                };
                return Ok(SnapshotResponse::new(res.vote));
            }

            let req = InstallSnapshotRequest {
                vote: vote.clone(),
                meta: snapshot_meta.clone(),
                offset,
                data: buf,
                done: false,
            };

            match self
                .transport
                .send_install_snapshot(self.target.clone(), self.group_id, req)
                .await
                .decompose()
            {
                Ok(Ok(_)) => {}
                Ok(Err(_snapshot_err)) => {
                    return Err(StreamingError::Network(
                        maniac_raft::error::NetworkError::new(&BatchError(
                            "Snapshot rejected by remote".to_string(),
                        )),
                    ));
                }
                Err(rpc_err) => return Err(StreamingError::from(rpc_err)),
            }

            offset += n as u64;
        }
    }

    fn backoff(&self) -> maniac_raft::network::Backoff {
        maniac_raft::network::Backoff::new(std::iter::repeat(Duration::from_millis(500)))
    }
}
