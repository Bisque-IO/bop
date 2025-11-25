use rand::RngCore;
use std::sync::Arc;

use crate::{loom_exports::cell::UnsafeCell, seg_spsc::Spsc, PushError};

const RND_MULTIPLIER: u64 = 0x5DEECE66D;
const RND_ADDEND: u64 = 0xB;
const RND_MASK: u64 = (1 << 48) - 1;

fn next_random(seed: &mut u64) -> u64 {
	let old_seed = *seed;
	let next_seed = (old_seed
			.wrapping_mul(RND_MULTIPLIER)
			.wrapping_add(RND_ADDEND))
			& RND_MASK;
	*seed = next_seed;
	next_seed >> 16
}

pub struct SegSpmc<T: Copy, const SHARDS: usize, const P: usize, const NUM_SEGS_P2: usize> {
	pub queues: Box<[*mut Spsc<T, P, NUM_SEGS_P2>]>,
	seed: UnsafeCell<u64>,
}

impl<T: Copy, const SHARDS: usize, const P: usize, const NUM_SEGS_P2: usize>
SegSpmc<T, SHARDS, P, NUM_SEGS_P2>
{
	// pub const NUM_SHARDS: usize = 1usize << SHARDS;
	pub const NUM_SHARDS_MASK: usize = SHARDS - 1;

	pub fn new() -> Self {
		let mut v = Vec::with_capacity(SHARDS);
		for _ in 0..SHARDS {
			v.push(Box::into_raw(Box::new(unsafe {
				Spsc::<T, P, NUM_SEGS_P2>::new_unsafe()
			})));
		}
		Self {
			queues: v.into_boxed_slice(),
			seed: UnsafeCell::new(rand::rng().next_u64()),
		}
	}

	pub fn with_queues(queues: Vec<*mut Spsc<T, P, NUM_SEGS_P2>>) -> Self {
		assert_eq!(queues.len(), SHARDS);
		Self {
			queues: queues.into_boxed_slice(),
			seed: UnsafeCell::new(rand::rng().next_u64()),
		}
	}

	fn next(&self) -> u64 {
		unsafe { next_random(&mut *(self.seed.get())) }
	}

	pub unsafe fn close(&self) {
		for i in 0..SHARDS {
			unsafe {
				(&*self.queues[i]).close();
			}
		}
	}

	pub fn try_push(&self, mut value: T) -> Result<(), PushError<T>> {
		for _ in 0..SHARDS {
			let next = self.next() as usize;
			let queue = self.queues[next & Self::NUM_SHARDS_MASK];
			match unsafe { &*queue }.try_push(value) {
				Ok(_) => return Ok(()),
				Err(PushError::Full(v)) => {
					value = v;
				}
				Err(err) => return Err(err),
			}
		}
		Err(PushError::Full(value))
	}

	pub fn try_push_n(&self, batch: &[T]) -> Result<usize, PushError<()>> {
		for _ in 0..SHARDS {
			let next = self.next() as usize;
			let queue = self.queues[next & Self::NUM_SHARDS_MASK];
			match unsafe { &*queue }.try_push_n(batch) {
				Ok(size) => return Ok(size),
				Err(PushError::Full(())) => continue,
				Err(err) => return Err(err),
			}
		}
		Err(PushError::Full(()))
	}

	pub fn try_push_n_with_selector(
		&self,
		mut next: usize,
		batch: &mut [T],
	) -> Result<usize, PushError<()>> {
		for _ in 0..SHARDS {
			let queue = self.queues[next & Self::NUM_SHARDS_MASK];
			match unsafe { &*queue }.try_push_n(batch) {
				Ok(size) => return Ok(size),
				Err(PushError::Full(())) => {}
				Err(err) => return Err(err),
			}
			next = self.next() as usize;
		}
		Err(PushError::Full(()))
	}
}
