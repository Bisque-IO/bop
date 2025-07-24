package concurrent

import "base:intrinsics"
import "core:mem"
import "core:sync"
import "core:time"

// A bounded MPSC queue, based on Dmitry Vyukov's MPMC queue and tachyonix Rust based MPSC
//
// The enqueue position, dequeue position and the slot stamps are all stored as
// `usize` and share the following layout:
//
// ```text
//
// | <- MSB                                LSB -> |
// | Sequence count | flag (1 bit) | Buffer index |
//
// ```
//
// The purpose of the flag differs depending on the field:
//
// - enqueue position: if set, the flag signals that the queue has been closed
//   by either the consumer or a producer,
// - dequeue position: the flag is not used (always 0),
// - slot stamp: the flag de-facto extends the mantissa of the buffer index,
//   which makes it in particular possible to support queues with a capacity of
//   1 without special-casing.
//
// The size of the buffer is constant and must be a power of 2 greater than 0.
MPSC :: struct($T: typeid, $SIZE: u64) where SIZE > 0 && (SIZE & (SIZE - 1)) == 0 {
	allocator:   mem.Allocator,
	// Buffer position of the slot to which the next value will be written.
	//
	// The position stores the buffer index in the least significant bits and a
	// sequence counter in the most significant bits.
	enqueue_pos: u64,
	_:           [CACHE_LINE_SIZE - size_of(u64) - size_of(mem.Allocator)]u8,

	// Buffer position of the slot from which the next value will be read.
	//
	// This is only ever mutated from a single thread but it must be stored in
	// an atomic or an `UnsafeCell` since it is shared between the consumers
	// and the producer. The reason it is shared is that the drop handler of
	// the last `Inner` owner (which may be a producer) needs access to the
	// dequeue position.
	dequeue_pos: u64,
	_:           [CACHE_LINE_SIZE - size_of(u64)]u8,
	waiter:      sync.Futex,
	_:           [CACHE_LINE_SIZE - size_of(sync.Futex)]u8,

	// Buffer holding the values and their stamps.
	buffer:      [SIZE]Slot(T),
}

mpsc_make :: proc($T: typeid, $SIZE: u64, allocator := context.allocator) -> ^MPSC(T, SIZE) {
	#assert(SIZE > 0 && (SIZE & (SIZE - 1)) == 0, "must be power of 2")
	#assert(SIZE < 1024 * 1024 * 128, "max queue size exceeded")

	q := new(MPSC(T, SIZE), allocator)
	q.allocator = allocator

	for i := 0; i < len(q.buffer); i += 1 {
		atomic_store(&q.buffer[i].stamp, u64(i), .Seq_Cst)
	}

	//	q.closed_channel_mask = u64(math.next_power_of_two(SIZE))
	//	q.right_mask, _ = overflow_sub(q.closed_channel_mask << 1, 1)

	return q
}

mpsc_destroy :: proc(q: ^MPSC($T, $SIZE)) {
	assert(q != nil)
	if !mpsc_is_closed(q) do mpsc_close(q)
	free(q, q.allocator)
}

mpsc_destroy_with_deleter :: proc(
	q: ^MPSC($T, $SIZE),
	user_data: rawptr,
	deleter: proc(user_data: rawptr, data: T, allocator: mem.Allocator),
) {
	assert(q != nil)

	if !mpsc_is_closed(q) do mpsc_close(q)

	if deleter != nil {
		for {
			value, err := mpsc_pop(q)
			if err != .Success do break
			deleter(user_data, value, q.allocator)
		}
	}

	free(q, q.allocator)
}

// next_queue_pos increments the queue position, incrementing the sequence
// count as well if the index wraps to 0.
//
// Precondition when used with enqueue positions: the closed-channel flag
// should be cleared.
@(private)
mpsc_next_queue_pos :: #force_inline proc "contextless" (
	q: ^MPSC($T, $SIZE),
	queue_pos: u64,
) -> u64 {
	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE

	// Bit mask for the 1-bit flag, used as closed-channel flag in the enqueue
	// position.
	RIGHT_MASK :: (SIZE << 1) - 1
	BUFFER_LEN :: SIZE

	// The queue position cannot wrap around: in the worst case it will
	// overflow the flag bit.
	new_queue_pos := queue_pos + 1
	new_index := new_queue_pos & RIGHT_MASK
	if new_index < BUFFER_LEN {
		return new_queue_pos
	}

	// The buffer index must wrap to 0 and the sequence count must be incremented.
	SEQUENCE_INCR :: RIGHT_MASK + 1
	sequence_count := queue_pos &~ RIGHT_MASK
	sequence_count, _ = overflow_add(sequence_count, SEQUENCE_INCR)
	return sequence_count
}

// push attempts to push an item into the queue if space is available.
mpsc_push :: proc(q: ^MPSC($T, $SIZE), value: T) -> Push_Error #no_bounds_check {
	assert(q != nil)

	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE
	// Bit mask for the 1-bit flag, used as closed-channel flag in the enqueue position.
	RIGHT_MASK :: (SIZE << 1) - 1

	enqueue_pos := atomic_load(&q.enqueue_pos, .Relaxed)

	for {
		if enqueue_pos & CLOSED_CHANNEL_MASK != 0 {
			return .Closed
		}

		slot := &q.buffer[enqueue_pos & RIGHT_MASK]
		stamp := atomic_load(&slot.stamp, .Acquire)

		stamp_delta, _ := overflow_sub(stamp, enqueue_pos)

		if stamp_delta == 0 {
			// The enqueue position matches the stamp: a push can be attempted.
			if pos, ok := cas_weak(
				&q.enqueue_pos,
				enqueue_pos,
				mpsc_next_queue_pos(q, enqueue_pos),
				.Relaxed,
				.Relaxed,
			); ok {
				slot.value = value
				stamp, _ = overflow_add(stamp, 1)
				atomic_store(&slot.stamp, stamp, .Release)

				if atomic_load(&q.waiter, .Relaxed) == 0 {
					if _, ok := cas_weak(&q.waiter, 0, 1, .Relaxed, .Relaxed); ok {
						sync.futex_signal(&q.waiter)
					}
				}

				return .Success
			} else {
				enqueue_pos = pos
				//                cpu_relax()
				// cpu_relax()
			}
		} else if stamp_delta > 0 {
			//            cpu_relax()
			// cpu_relax()
			// The stamp is greater than the enqueue position: this means we
			// raced with a concurrent producer which has already (i)
			// incremented the enqueue position and (ii) written a value to
			// this slot. A retry is required.
			enqueue_pos = atomic_load(&q.enqueue_pos, .Relaxed)
		} else {
			// The sequence count of the stamp is smaller than that of the
			// enqueue position: the value it contains has not been popped
			// yet, so report a full queue.
			return .Full
		}
	}
}

//push_sp :: proc(q: ^Queue($T, $SIZE), value: T) -> Push_Error #no_bounds_check {
//    assert(q != nil)
//
//    CLOSED_CHANNEL_MASK :: SIZE
//    RIGHT_MASK :: (SIZE << 1) - 1
//
//    // No atomic load — this is single-producer only
//    enqueue_pos := q.enqueue_pos
//
//    if enqueue_pos & CLOSED_CHANNEL_MASK != 0 {
//        return .Closed
//    }
//
//    slot := &q.buffer[enqueue_pos & RIGHT_MASK]
//    stamp := intrinsics.atomic_load_explicit(&slot.stamp, .Acquire)
//
//    stamp_delta, _ := overflow_sub(stamp, enqueue_pos)
//
//    if stamp_delta == 0 {
//    // slot is available; perform push
//        q.enqueue_pos = next_queue_pos(q, enqueue_pos)
//
//        slot.value = value
//        stamp, _ = overflow_add(stamp, 1)
//        intrinsics.atomic_store_explicit(&slot.stamp, stamp, .Release)
//
//        if intrinsics.atomic_load_explicit(&q.waiter, .Relaxed) == 0 {
//            if _, ok := intrinsics.atomic_compare_exchange_weak_explicit(
//                &q.waiter,
//                0,
//                1,
//                .Relaxed,
//                .Relaxed,
//            ); ok {
//                sync.futex_signal(&q.waiter)
//            }
//        }
//
//        return .Success
//
//    } else if stamp_delta > 0 {
//        // Producer moved ahead but slot not yet reclaimed by consumer
//
//        cpu_relax()
////        return .Full
//    } else {
//        return .Full
//    }
//    return .Full
//}

// Attempts to pop an item from the queue.
//
// # Safety
//
// This method may not be called concurrently from multiple threads.
mpsc_pop :: #force_inline proc(
	q: ^MPSC($T, $SIZE),
) -> (
	value: T,
	err: Pop_Error,
) #no_bounds_check {
	assert(q != nil)

	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE
	// Bit mask for the 1-bit flag, used as closed-channel flag in the enqueue position.
	RIGHT_MASK :: (SIZE << 1) - 1

	dequeue_pos := q.dequeue_pos
	slot := &q.buffer[dequeue_pos & RIGHT_MASK]
	stamp := atomic_load(&slot.stamp, .Acquire)

	if dequeue_pos != stamp {
		q.dequeue_pos = mpsc_next_queue_pos(q, dequeue_pos)

		// Read the value from the slot and set the stamp to the value of
		// the dequeue position increased by one sequence increment.
		value = slot.value

		incr, _ := overflow_add(stamp, RIGHT_MASK)
		atomic_store(&slot.stamp, incr, .Release)

		return value, .Success
	}

	if atomic_load(&q.enqueue_pos, .Relaxed) == (dequeue_pos | CLOSED_CHANNEL_MASK) {
		err = .Closed
	} else {
		err = .Empty
	}
	return
}

// Attempts to pop an item from the queue.
//
// # Safety
//
// This method may not be called concurrently from multiple threads.
mpsc_drain :: #force_inline proc(
	q: ^MPSC($T, $SIZE),
	target: []T,
) -> (
	count: int,
	err: Pop_Error,
) #no_bounds_check {
	assert(q != nil)

	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE
	// Bit mask for the 1-bit flag, used as closed-channel flag in the enqueue position.
	RIGHT_MASK :: (SIZE << 1) - 1

	for count < len(target) {
		dequeue_pos := q.dequeue_pos
		slot := &q.buffer[dequeue_pos & RIGHT_MASK]
		stamp := atomic_load(&slot.stamp, .Acquire)

		if dequeue_pos != stamp {
			q.dequeue_pos = mpsc_next_queue_pos(q, dequeue_pos)

			// Read the value from the slot and set the stamp to the value of
			// the dequeue position increased by one sequence increment.
			target[count] = slot.value

			incr, _ := overflow_add(stamp, RIGHT_MASK)
			atomic_store(&slot.stamp, incr, .Release)
			count += 1
			continue
		}

		if atomic_load(&q.enqueue_pos, .Relaxed) == (dequeue_pos | CLOSED_CHANNEL_MASK) {
			err = .Closed
		} else {
			err = .Empty
		}
		return
	}

	if atomic_load(&q.enqueue_pos, .Relaxed) == (dequeue_pos | CLOSED_CHANNEL_MASK) {
		err = .Closed
	} else {
		err = .Empty
	}
	return
}

// pop_wait waits for a new item (blocking)
mpsc_pop_wait :: proc(q: ^MPSC($T, $SIZE)) -> (value: T, err: Pop_Error) {
	assert(q != nil)

	value, err = mpsc_pop(q)
	if err >= .Closed {
		return
	}

	for i in 0 ..< SPINS {
		#unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()

		value, err = mpsc_pop(q)
		if err >= .Closed {
			return
		}
	}

	for {
		count := atomic_load(&q.waiter, .Relaxed)
		for {
			sync.futex_wait(&q.waiter, u32(count))
			count = atomic_load(&q.waiter, .Relaxed)
			if count != 0 {
				break
			}
			cpu_relax()
		}

		// reset waiter
		_ = atomic_swap(&q.waiter, 0, .Release)
		//        intrinsics.atomic_compare_exchange_strong_explicit(&q.waiter, count, 0, .Acquire, .Consume)

		#unroll for _ in 0 ..< CPU_RELAX_SPINS {
			value, err = mpsc_pop(q)
			if err >= .Closed {
				return
			}
			#unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()
		}
	}
}

// pop_wait_timeout waits for a new item up to a specified timeout duration (blocking)
//pop_wait_timeout :: proc(
//    q: ^Queue($T, $SIZE),
//    duration: time.Duration,
//) -> (
//    value: T,
//    err: Pop_Error,
//) {
//    assert(q != nil)
//    assert(duration > -1)
//
//    if duration == 0 {
//        return pop_wait(q)
//    }
//
//    value, err = pop(q)
//    if err == .Success {
//        return
//    }
//    if err == .Closed {
//        return
//    }
//    if duration <= 0 {
//        err = .Timeout
//        return
//    }
//
//    // Do some spins
//    for i in 0 ..< SPINS {
//        #unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()
//
//        value, err = pop(q)
//        if err == .Success {
//            return
//        }
//        if err == .Closed {
//            return
//        }
//    }
//
//    start := time.tick_now()
//    for {
//        count := intrinsics.atomic_load_explicit(&q.waiter, .Relaxed)
//        for count == 0 {
//            remaining := duration - time.tick_since(start)
//            if remaining <= 0 {
//                err = .Timeout
//                return
//            }
//            if !sync.futex_wait_with_timeout(&q.waiter, u32(count), remaining) {
//                err = .Timeout
//                return
//            }
//
//            count = intrinsics.atomic_load_explicit(&q.waiter, .Relaxed)
//            if count != 0 {
//                break
//            }
//
//            cpu_relax()
//        }
//
//        // reset waiter
//        intrinsics.atomic_compare_exchange_strong_explicit(&q.waiter, count, 0, .Acquire, .Consume)
//
//        #unroll for _ in 0 ..< CPU_RELAX_SPINS {
//            value, err = pop(q)
//            if err == .Success {
//                return
//            }
//            if err == .Closed {
//                return
//            }
//            #unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()
//        }
//    }
//}

mpsc_pop_wait_timeout :: proc(
	q: ^MPSC($T, $SIZE),
	duration: time.Duration,
) -> (
	value: T,
	err: Pop_Error,
) {
	assert(q != nil)
	assert(duration > -1)

	if duration == 0 {
		return mpsc_pop_wait(q)
	}

	value, err = mpsc_pop(q)
	if err >= .Closed {
		return
	}
	if duration <= 0 {
		err = .Timeout
		return
	}

	for i in 0 ..< SPINS {
		#unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()
		value, err = mpsc_pop(q)
		if err >= .Closed {
			return
		}
	}

	start := time.tick_now()
	for {
		count := atomic_load(&q.waiter, .Relaxed)
		for count == 0 {
			remaining := duration - time.tick_since(start)
			if remaining <= 0 {
				err = .Timeout
				return
			}
			if !sync.futex_wait_with_timeout(&q.waiter, u32(count), remaining) {
				value, err = mpsc_pop(q)
				if err >= .Closed {
					return
				}
				err = .Timeout
				return
			}
			break
		}

		_ = atomic_swap(&q.waiter, 0, .Release)

		#unroll for _ in 0 ..< CPU_RELAX_SPINS {
			value, err = mpsc_pop(q)
			if err >= .Closed {
				return
			}
			#unroll for _ in 0 ..< CPU_RELAX_SPINS do cpu_relax()
		}
	}
}

// Closes the queue
mpsc_close :: #force_inline proc(q: ^MPSC($T, $SIZE)) {
	assert(q != nil)

	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE

	// Set the closed-channel flag.
	//
	// Ordering: Relaxed ordering is enough here since neither the producers
	// nor the consumer rely on this flag for synchronizing reads and
	// writes.
	atomic_or(&q.enqueue_pos, CLOSED_CHANNEL_MASK, .Relaxed)
}

// Checks if the channel has been closed.
//
// Note that even if the channel is closed, some messages may still be
// present in the queue so further calls to `pop` may still succeed.
mpsc_is_closed :: #force_inline proc(q: ^MPSC($T, $SIZE)) -> bool {
	assert(q != nil)

	// Bit mask covering both the buffer index and the 1-bit flag.
	CLOSED_CHANNEL_MASK :: SIZE

	// Read the closed-channel flag.
	//
	// Ordering: Relaxed ordering is enough here since this is merely an
	// informational function and cannot lead to any unsafety. If the load
	// is stale, the worse that can happen is that the queue is seen as open
	// when it is in fact already closed, which is OK since the caller must
	// anyway be resilient to the case where the channel closes right after
	// `is_closed` returns `false`.
	return atomic_load(&q.enqueue_pos, .Relaxed) & CLOSED_CHANNEL_MASK != 0
}
