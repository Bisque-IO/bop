// use futures::future::poll_fn;
// 
// use bop_executor::mpsc_async::async_mpsc;
// use bop_executor::seg_spsc::SegSpsc;
// use std::task::Poll;
// 
// #[test]
// fn async_send_blocks_until_space_available() {
//     let (mut tx, mut rx) = async_mpsc::<u32, 1, 1>();
//     let capacity = SegSpsc::<u32, 1, 1>::capacity();
// 
//     futures_executor::block_on(async move {
//         for value in 0..capacity as u32 {
//             tx.try_send(value).unwrap();
//         }
// 
//         let mut send_fut = Box::pin(tx.send(999));
// 
//         let pending = poll_fn(|cx| match send_fut.as_mut().poll(cx) {
//             Poll::Pending => Poll::Ready(true),
//             Poll::Ready(_) => Poll::Ready(false),
//         })
//         .await;
//         assert!(pending, "send should block when all queues are full");
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
//     let (mut tx, mut rx) = async_mpsc::<u32, 1, 1>();
// 
//     futures_executor::block_on(async move {
//         let mut recv_fut = Box::pin(rx.recv());
// 
//         let pending = poll_fn(|cx| match recv_fut.as_mut().poll(cx) {
//             Poll::Pending => Poll::Ready(true),
//             Poll::Ready(_) => Poll::Ready(false),
//         })
//         .await;
//         assert!(pending, "recv should block when no items are present");
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
