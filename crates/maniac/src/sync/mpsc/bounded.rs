use crate::sync::mpsc::{
    self, AsyncMpscReceiver, AsyncMpscSender, WeakAsyncMpscSender, async_mpsc,
};
pub use crate::{PopError, PushError};

pub type MpscSender<T> = AsyncMpscSender<T, 6, 8>;
pub type MpscReceiver<T> = AsyncMpscReceiver<T, 6, 8>;
pub type WeakMpscSender<T> = WeakAsyncMpscSender<T, 6, 8>;

pub use crate::PushError as SendError;

#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

pub fn channel<T>() -> (MpscSender<T>, MpscReceiver<T>) {
    async_mpsc()
}
