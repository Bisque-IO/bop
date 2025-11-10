#[derive(Clone, Copy)]
struct SenderPtr<'a, T, const P: usize, const NUM_SEGS_P2: usize> {
    ptr: *mut Sender<T, P, NUM_SEGS_P2>,
    _marker: PhantomData<&'a mut Sender<T, P, NUM_SEGS_P2>>,
}

impl<'a, T, const P: usize, const NUM_SEGS_P2: usize> SenderPtr<'a, T, P, NUM_SEGS_P2> {
    #[inline]
    unsafe fn as_mut(self) -> &'a mut Sender<T, P, NUM_SEGS_P2> {
        &mut *self.ptr
    }

    #[inline]
    unsafe fn space_waker(self) -> &'a DiatomicWaker {
        &(*self.ptr).slot.space_waker
    }
}

unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Send for SenderPtr<'_, T, P, NUM_SEGS_P2> {}
unsafe impl<T: Send, const P: usize, const NUM_SEGS_P2: usize> Sync for SenderPtr<'_, T, P, NUM_SEGS_P2> {}

#[tokio::test]
async fn async_blocking_batch_operations() {
    let (mut sender, receiver) = async_blocking_mpsc::<u64, 6, 8>();
    let receiver = Arc::new(std::sync::Mutex::new(receiver));

    // Async sender sends batch
    tokio::spawn(async move {
        assert!(sender
            .send_slice(vec![1, 2, 3, 4, 5])
            .await
            .unwrap()
            .is_empty());
    });

    // Give async task time to send
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Blocking receiver receives batch
    let receiver_clone = Arc::clone(&receiver);
    let handle = thread::spawn(move || {
        let mut buf = [0u64; 5];
        let count = receiver_clone.lock().unwrap().recv_batch(&mut buf).unwrap();
        (count, buf)
    });

    let (count, buf) = handle.join().unwrap();
    assert_eq!(count, 5);
    assert_eq!(buf, [1, 2, 3, 4, 5]);
}
