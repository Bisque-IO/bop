package mpsc

import mpsc "./"
import "base:intrinsics"
import "core:fmt"
import "core:thread"
import "core:time"

//import bop "../../libbop"

main :: proc() {
//    bench_parallelism(8, 4096*4, 2000000000)
//    bop.alloc(1)

    for i in 0 ..< 5 {
//            stress_test_mpsc_push(1, 8192 / 4, 20000000)
    }

    for i in 0 ..< 5 {
        stress_test_spsc_push(8192 / 64, 20000000)
//        stress_test_mpsc_push(1, 8192 / 4, 20000000)
    }
    for i in 0 ..< 5 {
//        stress_test_mpsc_push(2, 2048, 20000000)
    }
    // stress_test_mpsc_push(3, 4096, 1000000)
//    stress_test_mpsc_push(3, 2048, 10000000)
}

bench_parallelism :: proc($THREADS: int, $QUEUE_SIZE: int, $ITERS: int) {
    fmt.println(
        "stress_test_mpsc_push: THREADS =",
        THREADS,
        " QUEUE_SIZE =",
        QUEUE_SIZE,
        " ITERS =",
        ITERS,
    )

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    Counter :: struct {
        value:   u64,
        padding: [56]u8, // 64 bytes total
    }

    queues: [THREADS + 1]^mpsc.Queue(u64, u64(QUEUE_SIZE))
    producers: [THREADS + 1]^thread.Thread
    consumers: [THREADS + 1]^thread.Thread
    producer_counters: [THREADS + 1]Counter
    consumer_counters: [THREADS + 1]Counter

    for i in 0 ..< THREADS {
        queues[i] = mpsc.queue_make(u64, u64(QUEUE_SIZE))

        producers[i] = thread.create_and_start_with_poly_data3(
            rawptr(queues[i]),
            &producer_counters[i],
            i,
            proc(data: rawptr, counter: ^Counter, id: int) {

                id := id + 1
                q := (^mpsc.Queue(u64, u64(QUEUE_SIZE)))(data)
                for i in 0 ..< ITERS {
                    for mpsc.push_sp(q, u64(i)) != .Success {
                    // intrinsics.cpu_relax()
                    // #unroll for i in 0 ..< 2 do intrinsics.cpu_relax()
                    }
                    intrinsics.atomic_add(&counter.value, 1)
                //				intrinsics.cpu_relax()
                }
            },
        )

        consumers[i] = thread.create_and_start_with_poly_data3(
            rawptr(queues[i]),
            &consumer_counters[i],
            i,
            proc(data: rawptr, counter: ^Counter, id: int) {
                id := id + 1
                q := (^mpsc.Queue(u64, u64(QUEUE_SIZE)))(data)
                loop: for i in 0 ..< ITERS {
                    value, err := mpsc.pop_wait_timeout(q, time.Millisecond * 50)
//                     value, err := mpsc.pop(q)
                    switch err {
                    case .Success:
                    case .Timeout:
                    //			time.sleep(time.Microsecond*10)
                        intrinsics.cpu_relax()
                        continue
                    case .Closed:
                        break loop
                    case .Empty:
                    //			time.sleep(time.Microsecond*10)
                        intrinsics.cpu_relax()
                        continue
                    }
                    intrinsics.atomic_add(&counter.value, 1)
                //				intrinsics.cpu_relax()
                }
            },
        )
    }

    for {
        time.sleep(time.Millisecond * 1000)

        total_consumed := u64(0)
        total_produced := u64(0)
        for i in 0 ..< THREADS {
            total_consumed += intrinsics.atomic_exchange(&consumer_counters[i].value, 0)
            total_produced += intrinsics.atomic_exchange(&producer_counters[i].value, 0)
        }
        fmt.println("Produced:", total_produced, "Consumed:", total_consumed)
    // if total_produced == u64(ITERS * THREADS) {
    // 	break
    // }
    }

    fmt.println(
        "done in",
        time.stopwatch_duration(stopwatch),
        "   ",
        time.stopwatch_duration(stopwatch) / time.Duration(ITERS * THREADS),
        "/op",
        "   ",
        u64(time.Second) /
        u64(time.stopwatch_duration(stopwatch) / time.Duration(ITERS * THREADS)),
        "/sec",
    )
}

stress_test_mpsc_push :: proc($THREADS: int, $QUEUE_SIZE: int, $ITERS: int) {
    fmt.println(
    "stress_test_mpsc_push: THREADS =",
    THREADS,
    " QUEUE_SIZE =",
    QUEUE_SIZE,
    " ITERS =",
    ITERS,
    )

    queue := mpsc.queue_make(u64, u64(QUEUE_SIZE))
    defer mpsc.destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [THREADS + 1]^thread.Thread

    for i in 0 ..< THREADS {
        producers[i] = thread.create_and_start_with_poly_data2(
            rawptr(queue),
            i,
            proc(data: rawptr, id: int) {
                id := id + 1
                q := (^mpsc.Queue(u64, u64(QUEUE_SIZE)))(data)
                for i in 0 ..< ITERS {
                    for mpsc.push(q, u64(i)) != .Success {
//                        cnt := intrinsics.read_cycle_counter()%3
//                        if cnt != 0 {
//
//                            return
//                        }
//                        for i in 0..<cnt {
////                            intrinsics.cpu_relax()
//                        }

                        time.sleep(time.Nanosecond)
//                        return
//                     intrinsics.cpu_relax()
//                     #unroll for i in 0 ..< 2 do intrinsics.cpu_relax()
                    }
                //				intrinsics.cpu_relax()
                }
            },
        )
    }

    count := 0
    loop: for {
        value, err := mpsc.pop_wait_timeout(queue, time.Millisecond * 50)
        // value, err := mpsc.pop(queue)
        switch err {
        case .Success:
        case .Timeout:
        //			time.sleep(time.Microsecond*10)
//            intrinsics.cpu_relax()
            continue
        case .Closed:
            time.stopwatch_stop(&stopwatch)
            break loop
        case .Empty:
        //			time.sleep(time.Microsecond*10)
//            intrinsics.cpu_relax()
            continue
        }
        count += 1

        if count == ITERS * THREADS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.println(
        "done in",
        time.stopwatch_duration(stopwatch),
        "   ",
        time.stopwatch_duration(stopwatch) / time.Duration(ITERS * THREADS),
        "/op",
        "   ",
        u64(time.Second) /
        u64(time.stopwatch_duration(stopwatch) / time.Duration(ITERS * THREADS)),
        "/sec",
    )
}

stress_test_spsc_push :: proc($QUEUE_SIZE: int, $ITERS: int) {
    fmt.println(
        "stress_test_spsc_push: ",
        " QUEUE_SIZE =",
        QUEUE_SIZE,
        " ITERS =",
        ITERS,
    )

    queue := mpsc.queue_make(u64, u64(QUEUE_SIZE))
    defer mpsc.destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
        rawptr(queue),
        i,
        proc(data: rawptr, id: int) {
            id := id + 1
            q := (^mpsc.Queue(u64, u64(QUEUE_SIZE)))(data)
            for i in 0 ..< ITERS {
                for mpsc.push_sp(q, u64(i)) != .Success {
//                //                        cnt := intrinsics.read_cycle_counter()%3
//                //                        if cnt != 0 {
//                //
//                //                            return
//                //                        }
//                //                        for i in 0..<cnt {
//                ////                            intrinsics.cpu_relax()
//                //                        }
//
                    time.sleep(time.Nanosecond)
//                //                        return
//                //                     intrinsics.cpu_relax()
//                //                     #unroll for i in 0 ..< 2 do intrinsics.cpu_relax()
                }
            //				intrinsics.cpu_relax()
            }
        },
        )
    }

    count := 0
    loop: for {
        value, err := mpsc.pop_wait_timeout(queue, time.Millisecond * 50)
        // value, err := mpsc.pop(queue)
        switch err {
        case .Success:
        case .Timeout:
        //			time.sleep(time.Microsecond*10)
        //            intrinsics.cpu_relax()
            continue
        case .Closed:
            time.stopwatch_stop(&stopwatch)
            break loop
        case .Empty:
        //			time.sleep(time.Microsecond*10)
        //            intrinsics.cpu_relax()
            continue
        }
        count += 1

        if count == ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.println(
    "done in",
    time.stopwatch_duration(stopwatch),
    "   ",
    time.stopwatch_duration(stopwatch) / time.Duration(ITERS),
    "/op",
    "   ",
    u64(time.Second) /
    u64(time.stopwatch_duration(stopwatch) / time.Duration(ITERS)),
    "/sec",
    )
}