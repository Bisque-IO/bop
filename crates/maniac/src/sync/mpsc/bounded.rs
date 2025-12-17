use crate::sync::mpsc::{
    self, AsyncMpscReceiver, AsyncMpscSender, WeakAsyncMpscSender, async_mpsc,
};
pub use crate::{PopError, PushError};

pub type MpscSender<T> = AsyncMpscSender<T>;
pub type MpscReceiver<T> = AsyncMpscReceiver<T>;
pub type WeakMpscSender<T> = WeakAsyncMpscSender<T>;

pub use crate::PushError as SendError;

#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

pub fn channel<T>() -> (MpscSender<T>, MpscReceiver<T>) {
    async_mpsc()
}
