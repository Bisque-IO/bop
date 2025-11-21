#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    fn run_serialized<F: FnOnce()>(f: F) {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let mutex = LOCK.get_or_init(|| Mutex::new(()));
        let _guard = match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        f();
    }

    // #[test]
    // fn test_init_worker_preemption() {
    //     run_serialized(|| {
    //         let flag = AtomicBool::new(false);
    //         crate::runtime::preemption::init_worker_thread_preemption(&flag);
    //     });
    // }

    // #[test]
    // fn test_check_and_clear_preemption() {
    //     run_serialized(|| {
    //         let flag = AtomicBool::new(true);
    //         assert!(crate::runtime::preemption::check_and_clear_preemption(
    //             &flag
    //         ));
    //         assert!(!flag.load(Ordering::Relaxed));
    //         assert!(!crate::runtime::preemption::check_and_clear_preemption(
    //             &flag
    //         ));
    //     });
    // }

    #[test]
    // #[cfg(not(miri))]
    fn test_preemption_interruption() {
        run_serialized(|| {
            // This test verifies that a thread running a generator loop can be preempted.
            // 1. Spawn a worker thread that initializes preemption and runs a generator.
            // 2. The generator sends its WorkerThreadHandle back to main thread.
            // 3. The generator enters a tight loop.
            // 4. Main thread interrupts the worker.
            // 5. Generator yields, worker thread resumes, marks shutdown, and cleanly finishes.

            let flag = Arc::new(AtomicBool::new(false));
            let should_exit = Arc::new(AtomicBool::new(false));

            let (tx, rx) = std::sync::mpsc::channel();

            let worker_thread = {
                let flag_clone = Arc::clone(&flag);
                let exit_signal = Arc::clone(&should_exit);
                thread::spawn(move || {
                    let _preemption_guard = crate::runtime::preemption::init_worker_preemption()
                        .expect("Failed to install preemption handler");

                    let mut generator = {
                        let exit_signal = Arc::clone(&exit_signal);
                        let flag_for_handle = Arc::clone(&flag_clone);
                        crate::generator::Gn::<()>::new_scoped(move |mut scope| {
                            let scope_ptr = &mut scope as *mut _ as *mut ();
                            crate::runtime::preemption::set_generator_scope(scope_ptr);

                            // Send our handle to the interrupter
                            #[cfg(unix)]
                            let handle =
                                crate::runtime::preemption::WorkerThreadHandle::current().unwrap();
                            #[cfg(windows)]
                            let handle = crate::runtime::preemption::WorkerThreadHandle::current(
                                &flag_for_handle,
                            )
                            .unwrap();

                            tx.send(handle).unwrap();

                            // Busy loop waiting for preemption
                            let mut i = 0u64;
                            loop {
                                std::hint::black_box(i);
                                i = i.wrapping_add(1);

                                if exit_signal.load(Ordering::Acquire) {
                                    crate::runtime::preemption::clear_generator_scope();
                                    return crate::generator::done();
                                }
                            }
                        })
                    };

                    // First resume: blocks until preemption triggers scope.yield_(Preempted).
                    use crate::runtime::preemption::GeneratorYieldReason;

                    let result = generator.resume();
                    let reason = result
                        .and_then(GeneratorYieldReason::from_usize)
                        .expect("generator should yield a structured reason");
                    assert_eq!(
                        reason,
                        GeneratorYieldReason::Preempted,
                        "preemption should yield exactly once"
                    );

                    // Tell the generator to exit and resume it to completion.
                    exit_signal.store(true, Ordering::Release);
                    let final_result = generator.resume();
                    assert!(
                        final_result.is_none(),
                        "generator should finish after exit flag"
                    );
                })
            };

            // Receive handle
            let handle = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Worker timed out");

            // Wait a bit for worker to be in loop
            thread::sleep(Duration::from_millis(50));

            // Interrupt
            handle.interrupt().expect("Failed to interrupt worker");

            // Wait for worker to finish (which means it yielded)
            // If preemption fails, this join will hang/timeout.
            worker_thread.join().expect("Worker thread panicked");
        });
    }

    #[test]
    fn test_multiple_generator_preemptions_on_same_thread() {
        run_serialized(|| {
            #[derive(Debug)]
            enum WorkerEvent {
                GeneratorReady(usize),
                GeneratorFinished(usize),
            }

            let preempt_flag = Arc::new(AtomicBool::new(false));
            let (handle_tx, handle_rx) = std::sync::mpsc::channel();
            let (event_tx, event_rx) = std::sync::mpsc::channel();

            let worker = {
                let flag_clone = Arc::clone(&preempt_flag);
                thread::spawn(move || {
                    let _guard = crate::runtime::preemption::init_worker_preemption()
                        .expect("install handler");

                    #[cfg(unix)]
                    let handle = crate::runtime::preemption::WorkerThreadHandle::current()
                        .expect("obtain unix handle");
                    #[cfg(windows)]
                    let handle =
                        crate::runtime::preemption::WorkerThreadHandle::current(&flag_clone)
                            .expect("obtain windows handle");
                    handle_tx.send(handle).unwrap();

                    use crate::runtime::preemption::GeneratorYieldReason;

                    for gen_id in 0..2 {
                        // println!("[Test] Starting generator {}", gen_id);
                        let exit_signal = Arc::new(AtomicBool::new(false));
                        let exit_clone = Arc::clone(&exit_signal);
                        let events = event_tx.clone();

                        let mut generator =
                            crate::generator::Gn::<()>::new_scoped(move |mut scope| {
                                // let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                //     crate::runtime::preemption::set_generator_scope(
                                //         &mut scope as *mut _ as *mut (),
                                //     );
                                //     // println!("[Gen {}] Scope set, notifying ready", gen_id);
                                //     events
                                //         .send(WorkerEvent::GeneratorReady(gen_id))
                                //         .expect("notify ready");

                                //     let mut spin = 0u64;
                                //     loop {
                                //         std::hint::black_box(spin);
                                //         spin = spin.wrapping_add(1);

                                //         if exit_clone.load(Ordering::Acquire) {
                                //             // println!("[Gen {}] Exit signal received", gen_id);
                                //             crate::runtime::preemption::clear_generator_scope();
                                //             return crate::generator::done();
                                //             // return 0;
                                //         }
                                //     }
                                // }));

                                // match result {
                                //     Ok(val) => val,
                                //     Err(e) => {
                                //         eprintln!("[Gen {}] Panic caught: {:?}", gen_id, e);
                                //         crate::runtime::preemption::clear_generator_scope();
                                //         gen_id
                                //     }
                                // }

                                crate::runtime::preemption::set_generator_scope(
                                    &mut scope as *mut _ as *mut (),
                                );
                                // println!("[Gen {}] Scope set, notifying ready", gen_id);
                                events
                                    .send(WorkerEvent::GeneratorReady(gen_id))
                                    .expect("notify ready");

                                let mut spin = 0u64;
                                loop {
                                    std::hint::black_box(spin);
                                    spin = spin.wrapping_add(1);

                                    if exit_clone.load(Ordering::Acquire) {
                                        // println!("[Gen {}] Exit signal received", gen_id);
                                        crate::runtime::preemption::clear_generator_scope();
                                        return crate::generator::done();
                                        // return 0;
                                    }
                                }
                            });

                        // println!("[Test] Resuming generator {}", gen_id);
                        let reason = generator
                            .resume()
                            .and_then(GeneratorYieldReason::from_usize)
                            .expect("generator should yield");
                        assert_eq!(reason, GeneratorYieldReason::Preempted);
                        // println!("[Test] Generator {} preempted", gen_id);

                        exit_signal.store(true, Ordering::Release);
                        assert!(
                            generator.resume().is_none(),
                            "generator should exit after exit flag"
                        );
                        // println!("[Test] Generator {} finished", gen_id);
                        event_tx
                            .send(WorkerEvent::GeneratorFinished(gen_id))
                            .expect("notify finished");
                    }
                })
            };

            let handle = handle_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("handle unavailable");

            for gen_id in 0..2 {
                match event_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("missing ready event")
                {
                    WorkerEvent::GeneratorReady(id) => assert_eq!(id, gen_id),
                    WorkerEvent::GeneratorFinished(id) => {
                        panic!("generator {id} finished before being preempted")
                    }
                }

                handle.interrupt().expect("preemption interrupt failed");

                match event_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("missing finished event")
                {
                    WorkerEvent::GeneratorFinished(id) => assert_eq!(id, gen_id),
                    WorkerEvent::GeneratorReady(id) => {
                        panic!("unexpected ready event for generator {id}")
                    }
                }
            }

            worker.join().expect("worker panicked");
        });
    }
}
