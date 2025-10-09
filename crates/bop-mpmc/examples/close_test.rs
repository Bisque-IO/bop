use bop_mpmc::seg_spsc::SegSpsc;
use bop_mpmc::{PopError, PushError};

const P: usize = 4; // 16 items/segment
const NUM_SEGS_P2: usize = 4; // 16 segments
type Queue = SegSpsc<u64, P, NUM_SEGS_P2>;

fn main() {
    println!("Testing SegSpsc close functionality...");

    let q = Queue::new_unsafe();

    // Initially not closed
    assert!(!q.is_closed());
    println!("✓ Queue starts open");

    // Can push when open
    for i in 0..5 {
        q.try_push(i).unwrap();
    }
    println!("✓ Successfully pushed 5 items");

    // Close the queue
    q.close();
    assert!(q.is_closed());
    println!("✓ Queue successfully closed");

    // Cannot push after closing
    match q.try_push(42) {
        Err(PushError::Closed(_)) => println!("✓ Correctly rejected push after close"),
        _ => panic!("Expected PushError::Closed"),
    }

    // Can still pop existing items
    assert_eq!(q.try_pop(), Some(0));
    assert_eq!(q.try_pop(), Some(1));
    assert_eq!(q.try_pop(), Some(2));
    println!("✓ Successfully popped items after close");

    // But when empty, try_pop should return None
    assert_eq!(q.try_pop(), Some(3));
    assert_eq!(q.try_pop(), Some(4));
    assert_eq!(q.try_pop(), None);
    println!("✓ try_pop returns None when empty");

    // try_pop_n should return PopError::Closed when queue is closed and empty
    let mut buf = [0u64];
    match q.try_pop_n(&mut buf) {
        Err(PopError::Closed) => {
            println!("✓ try_pop_n correctly returns Closed when queue is closed and empty")
        }
        _ => panic!("Expected PopError::Closed"),
    }

    println!("\n🎉 All close functionality tests passed!");

    // Test capacity and close bit encoding
    let capacity = Queue::capacity();
    let close_mask = capacity.next_power_of_two() as u64;
    println!("\nConfiguration:");
    println!("  Segment size: {}", Queue::SEG_SIZE);
    println!("  Number of segments: {}", Queue::NUM_SEGS);
    println!("  Capacity: {}", capacity);
    println!(
        "  Close mask bit position: {}",
        64 - close_mask.leading_zeros()
    );
    println!("  Close mask value: {} (0x{:x})", close_mask, close_mask);
}
