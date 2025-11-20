use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

pub type TimerId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer {
    pub id: TimerId,
}

struct TimerState {
    callback: Option<Box<dyn FnMut()>>,
    repeat: Duration,
    next_fire: Instant,
}

struct TimerEntry {
    fire_time: Instant,
    id: TimerId,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.fire_time == other.fire_time
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.fire_time.cmp(&self.fire_time)
    }
}

pub struct TimerManager {
    timers: Vec<Option<TimerState>>,
    free_slots: Vec<usize>,
    heap: BinaryHeap<TimerEntry>,
}

impl TimerManager {
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            free_slots: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }

    pub fn add_timer(&mut self, callback: Box<dyn FnMut()>) -> Timer {
        let id = if let Some(id) = self.free_slots.pop() {
            id
        } else {
            let id = self.timers.len();
            self.timers.push(None);
            id
        };

        self.timers[id] = Some(TimerState {
            callback: Some(callback),
            repeat: Duration::from_secs(0),
            next_fire: Instant::now(),
        });

        Timer { id }
    }

    pub fn set_timer(&mut self, timer: Timer, delay: Duration, repeat: Duration) {
        if let Some(Some(state)) = self.timers.get_mut(timer.id) {
            state.repeat = repeat;
            
            if delay.as_millis() == 0 && repeat.as_millis() == 0 {
                return;
            }

            state.next_fire = Instant::now() + delay;
            
            self.heap.push(TimerEntry {
                fire_time: state.next_fire,
                id: timer.id,
            });
        }
    }

    pub fn remove_timer(&mut self, timer: Timer) {
        if timer.id < self.timers.len() {
            self.timers[timer.id] = None;
            self.free_slots.push(timer.id);
        }
    }

    pub fn next_timeout(&self) -> Option<Duration> {
        while let Some(entry) = self.heap.peek() {
            if let Some(Some(state)) = self.timers.get(entry.id) {
                if state.next_fire == entry.fire_time {
                    let now = Instant::now();
                    if entry.fire_time > now {
                        return Some(entry.fire_time - now);
                    } else {
                        return Some(Duration::from_secs(0));
                    }
                }
            }
            // Peeked invalid entry, we can't pop in immutable method.
            // Return 0 to trigger process_timers which cleans up.
            return Some(Duration::from_secs(0));
        }
        None
    }

    pub fn process_timers(&mut self) {
        let now = Instant::now();
        let mut fired_callbacks = Vec::new();

        // 1. Pop expired timers
        while let Some(entry) = self.heap.peek() {
            if entry.fire_time > now {
                break;
            }
            let entry = self.heap.pop().unwrap();
            
            // Check validity
            if let Some(Some(state)) = self.timers.get_mut(entry.id) {
                if state.next_fire == entry.fire_time {
                    // Valid fire
                    if let Some(cb) = state.callback.take() {
                        fired_callbacks.push((entry.id, cb));
                    }
                }
            }
        }

        // 2. Execute callbacks and reschedule
        for (id, mut mut_cb) in fired_callbacks {
            // Execute
            mut_cb();

            // Reschedule if exists (wasn't removed during callback) and repeats
            if let Some(Some(state)) = self.timers.get_mut(id) {
                state.callback = Some(mut_cb); // Put back

                if state.repeat.as_millis() > 0 {
                    state.next_fire = now + state.repeat;
                    self.heap.push(TimerEntry {
                        fire_time: state.next_fire,
                        id,
                    });
                }
            }
        }
    }
}
