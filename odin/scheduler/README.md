# Scheduler

Low latency multi-threaded scheduler. Simple primitives to layer more specific abstractions on top.

---
### Scheduling w/o Queues

Queues are not an ideal low level primitive for massively parallel task invocation.
Queues are still a very helpful primitive that can be layered on top for specific types
of tasks utilizing the Signals and Slots to notify that the queue has an item.

---
### Signals, Slots and Selectors

Simple low contention atomic primitives are used to signal work to be done.

---
### Task Fairness w/o Stealing

Since there is no head of line blocking, selectors can provide an even distribution of task selection.

---
### Bring your own threads

Task selection is thread safe and scalable allowin

---
### Selectors w/ SIMD



---
### Low latency

Signal latency is around 1-3ns. Slot selection using SIMD is around 1-7ns with <= 8192 tasks per hardware thread.
Single digit nanosecond latency from signal to execution achievable with proper configuration.

---
### Near linear scaling utilizing all cores


---
## What is a Slot?

A Slot is a simple atomic container that guarantees thread safety for a supplied worker function.
When a Slot is signaled, it becomes available for a thread to select. The selection is atomic
guaranteeing only a single thread can execute a Slot at any given time.

The worker function can be anything, a network socket, a queue of messages to process, etc.