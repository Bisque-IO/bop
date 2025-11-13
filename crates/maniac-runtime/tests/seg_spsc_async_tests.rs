// use futures::future::poll_fn;
// use futures::stream::StreamExt;
//
// use bop_executor::seg_spsc::SegSpsc;
// use bop_executor::seg_spsc_async::{async_seg_spsc, async_seg_spsc_with_config};
// use std::task::Poll;
//
// #[test]
// fn async_send_recv_basic() {
//     let (mut tx, mut rx) = async_seg_spsc::<u64, 4, 2>();
//
//     futures_executor::block_on(async move {
//         tx.send(42).await.expect("send succeeds");
//         assert_eq!(rx.recv().await.expect("recv succeeds"), 42);
//
//         tx.send_batch([1, 2, 3]).await.expect("batch send ok");
//         assert_eq!(rx.recv().await.unwrap(), 1);
//         assert_eq!(rx.recv().await.unwrap(), 2);
//         assert_eq!(rx.recv().await.unwrap(), 3);
//     });
// }
//
// #[test]
// fn async_send_slice_and_recv_batch() {
//     let (mut tx, mut rx) = async_seg_spsc_with_config::<u32, 3, 2>(4);
//
//     futures_executor::block_on(async move {
//         tx.send_slice(&[10, 20, 30, 40, 50])
//             .await
//             .expect("slice send");
//
//         let mut buf = [0u32; 5];
//         let received = rx.recv_batch(&mut buf).await.expect("batch recv");
//         assert_eq!(received, 5);
//         assert_eq!(buf, [10, 20, 30, 40, 50]);
//
//         tx.send_slice(&[99]).await.unwrap();
//         assert_eq!(rx.recv().await.unwrap(), 99);
//     });
// }
//
// #[test]
// fn async_send_blocks_until_space_available() {
//     let (mut tx, mut rx) = async_seg_spsc::<u32, 1, 1>();
//     let capacity = SegSpsc::<u32, 1, 1>::capacity();
//
//     futures_executor::block_on(async move {
//         for value in 0..capacity as u32 {
//             tx.try_send(value).unwrap();
//         }
//
//         let mut send_fut = Box::pin(tx.send(999));
//
//         let first_poll_pending = poll_fn(|cx| match send_fut.as_mut().poll(cx) {
//             Poll::Pending => Poll::Ready(true),
//             Poll::Ready(_) => Poll::Ready(false),
//         })
//         .await;
//         assert!(first_poll_pending, "send should pend while queue is full");
//
//         assert_eq!(rx.recv().await.unwrap(), 0);
//
//         poll_fn(|cx| send_fut.as_mut().poll(cx))
//             .await
//             .expect("send should complete once space is available");
//
//         assert_eq!(rx.recv().await.unwrap(), 1);
//         assert_eq!(rx.recv().await.unwrap(), 2);
//         assert_eq!(rx.recv().await.unwrap(), 999);
//     });
// }
//
// #[test]
// fn async_recv_blocks_until_item_available() {
//     let (mut tx, mut rx) = async_seg_spsc::<u32, 1, 1>();
//
//     futures_executor::block_on(async move {
//         let mut recv_fut = Box::pin(rx.recv());
//
//         let first_poll_pending = poll_fn(|cx| match recv_fut.as_mut().poll(cx) {
//             Poll::Pending => Poll::Ready(true),
//             Poll::Ready(_) => Poll::Ready(false),
//         })
//         .await;
//         assert!(first_poll_pending, "recv should pend when queue is empty");
//
//         tx.send(123).await.unwrap();
//
//         let value = poll_fn(|cx| match recv_fut.as_mut().poll(cx) {
//             Poll::Pending => Poll::Pending,
//             Poll::Ready(result) => Poll::Ready(result.unwrap()),
//         })
//         .await;
//         assert_eq!(value, 123);
//     });
// }
//
// #[test]
// fn async_stream_and_sink_traits() {
//     let (mut tx, mut rx) = async_seg_spsc::<usize, 2, 2>();
//
//     futures_executor::block_on(async move {
//         tx.send(1).await.unwrap();
//         tx.send(2).await.unwrap();
//         tx.send(3).await.unwrap();
//
//         // Use Sink interface
//         tx.send(4).await.unwrap();
//         tx.close();
//
//         // Stream interface drains everything then reports None after close.
//         let mut collected = Vec::new();
//         while let Some(value) = rx.next().await {
//             collected.push(value);
//             if collected.len() == 4 {
//                 break;
//             }
//         }
//         assert_eq!(collected, vec![1, 2, 3, 4]);
//         assert!(rx.next().await.is_none(), "stream terminates after close");
//     });
// }
