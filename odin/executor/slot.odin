package executor

import "base:intrinsics"
import "core:sync"

// Slot represents a container for a unit of work—typically a user-supplied function—
// intended to be executed by a worker thread. Execution of the Slot is coordinated
// through a corresponding Signal, which ensures thread-safe scheduling.
//
// The Signal serves as a lightweight synchronization primitive that allows threads to
// atomically reserve a Slot for execution without requiring locks. This enables safe,
// concurrent scheduling across multiple threads.
//
// Additionally, the Slot maintains a flags field that uses atomic operations to manage
// scheduling and execution state. These flags are used to flip the scheduling signal
// atomically, ensuring thread-safe transitions between idle, scheduled, and executing states.
//
// Together, the Signal and flags enable precise, low-overhead coordination of work
// execution in multithreaded environments.
Slot :: struct {
	waker:             ^Waker,
	signal:            Signal,
	index:             u64,
	flags:             Slot_Flags,
	_:                 [CACHE_LINE_SIZE - size_of(
		u8,
	) - size_of(u64) - size_of(^int) - size_of(^int)]u8,
	cpu_time:          u64,
	cpu_time_overhead: u64,
	counter:           u64,
	contention:        u64,
	cpu_time_enabled:  bool,
	data:              rawptr,
	worker:            proc(data: rawptr) -> u8,
}

Slot_Flags :: enum u8 {
	Idle      = 0,
	Scheduled = 1,
	Executing = 2,
}

slot_schedule :: #force_inline proc "contextless" (self: ^Slot) -> bool {
	// Do an acquire load for fast exit if already scheduled.
	if (atomic_load(&self.flags, .Acquire) & Slot_Flags.Scheduled) != Slot_Flags.Idle {
		return false
	}

	// Try to flip schedule bit. Using relaxed semantics is ok since happens after an acquire load.
	previous_flags := atomic_or(&self.flags, Slot_Flags.Scheduled, .Relaxed)

	// Did we win the race to schedule?
	not_scheduled_nor_executing :=
		(previous_flags & (Slot_Flags.Scheduled | Slot_Flags.Executing)) == Slot_Flags.Idle
	if (not_scheduled_nor_executing) {
		// Set the signal to make available for processing.
		if was_empty, was_set := signal_set(self.signal, self.index); was_empty && was_set {
			//
			waker_incr(self.waker)
		}
		return true
	}
	return false
}

slot_execute :: #force_inline proc(self: ^Slot) {
	result := Slot_Flags.Idle

	atomic_store(&self.flags, Slot_Flags.Executing, .Release)
	self.counter += 1

	// intrinsics.atomic_add_explicit(&core.counter, 1, .Relaxed)
	if self.worker != nil {
		result = transmute(Slot_Flags)self.worker(self.data)
	}

	if result == Slot_Flags.Scheduled {
		atomic_store(&self.flags, Slot_Flags.Scheduled, .Release)
		if was_empty, was_set := signal_set(self.signal, self.index); was_empty && was_set {
			// Increment the waker
			waker_incr(self.waker)
		}
	} else {
		after_flags := intrinsics.atomic_sub_explicit(&self.flags, Slot_Flags.Executing, .Acq_Rel)
		if (after_flags & Slot_Flags.Scheduled) != Slot_Flags.Idle {
			signal_set(self.signal, self.index)
		}
	}
}
