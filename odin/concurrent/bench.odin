package concurrent

import "base:intrinsics"
import "core:fmt"
import "core:thread"
import "core:time"

import bop "../libbop"
import runtime "base:runtime"

main :: proc() {
//    bench_parallelism(8, 4096*4, 2000000000)
//    bop.alloc(1)
    runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()

    for i in 0 ..< 5 {
        stress_test_mpmc_push(8192 / 4, 20000000)
        stress_test_mpmc_bulk_push(8192 / 4, 20000000)
        stress_test_mpmc_blocking_bulk_push(8192 / 4, 20000000)
        stress_test_mpsc(1, 8192 / 4, 20000000)
        stress_test_libbop_spsc_push(8192 / 8, 20000000)
        stress_test_spsc(8192 * 4, 20000000)
        stress_test_spsc(8192 / 32, 20000000)
        fmt.println("")
    }

//    for i in 0 ..< 5 {
//        stress_test_mpsc_push(1, 8192 / 4, 20000000)
//    }
//
//    for i in 0 ..< 5 {
//        stress_test_libbop_spsc_push(8192 / 8, 20000000)
//    //        stress_test_mpsc_push(1, 8192 / 4, 20000000)
//    }
//
//    for i in 0 ..< 5 {
//        stress_test_spsc_push(8192 / 8, 20000000)
////        stress_test_mpsc_push(1, 8192 / 4, 20000000)
//    }
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

    queues: [THREADS + 1]^MPSC(u64, u64(QUEUE_SIZE))
    producers: [THREADS + 1]^thread.Thread
    consumers: [THREADS + 1]^thread.Thread
    producer_counters: [THREADS + 1]Counter
    consumer_counters: [THREADS + 1]Counter

    for i in 0 ..< THREADS {
        queues[i] = mpsc_make(u64, u64(QUEUE_SIZE))

        producers[i] = thread.create_and_start_with_poly_data3(
            rawptr(queues[i]),
            &producer_counters[i],
            i,
            proc(data: rawptr, counter: ^Counter, id: int) {

                id := id + 1
                q := (^MPSC(u64, u64(QUEUE_SIZE)))(data)
                for i in 0 ..< ITERS {
                    for push_sp(q, u64(i)) != .Success {
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
                q := (^MPSC(u64, u64(QUEUE_SIZE)))(data)
                loop: for i in 0 ..< ITERS {
                    value, err := mpsc_pop_wait_timeout(q, time.Millisecond * 50)
//                     value, err := pop(q)
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

stress_test_mpsc :: proc($THREADS: int, $QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//    "stress_test_mpsc_push: THREADS =",
//    THREADS,
//    " QUEUE_SIZE =",
//    QUEUE_SIZE,
//    " ITERS =",
//    ITERS,
//    )

    queue := mpsc_make(u64, u64(QUEUE_SIZE))
    defer mpsc_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [THREADS + 1]^thread.Thread

    for i in 0 ..< THREADS {
        producers[i] = thread.create_and_start_with_poly_data2(
            rawptr(queue),
            i,
            proc(data: rawptr, id: int) {
                id := id + 1
                q := (^MPSC(u64, u64(QUEUE_SIZE)))(data)
                for i in 0 ..< ITERS {
                    for mpsc_push(q, u64(i)) != .Success {
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
        value, err := mpsc_pop_wait_timeout(queue, time.Millisecond * 50)
        // value, err := pop(queue)
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

    fmt.printfln(
        "mpsc:                  %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

stress_test_spsc :: proc($QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//        "stress_test_spsc_push: ",
//        " QUEUE_SIZE =",
//        QUEUE_SIZE,
//        " ITERS =",
//        ITERS,
//    )

    queue := spsc_make(u64, u64(QUEUE_SIZE))
    defer spsc_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
            queue,
            i,
            proc(q: ^SPSC(u64, u64(QUEUE_SIZE)), id: int) {
                id := id + 1
                for i in 0 ..< ITERS {
                    for spsc_push(q, u64(i)) != .Success {
                        time.sleep(time.Nanosecond)
                    }
                }
            },
        )
    }

    count := 0
    loop: for {
        value, err := spsc_pop_wait_timeout(queue, time.Millisecond * 50)
        switch err {
        case .Success:
        case .Timeout:
            cpu_relax()
            continue
        case .Closed:
            time.stopwatch_stop(&stopwatch)
            break loop
        case .Empty:
            cpu_relax()
            continue
        }
        count += 1

        if count == ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.printfln(
        "spsc:                  %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

stress_test_libbop_spsc_push :: proc($QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//        "stress_test_libbop_spsc_push: ",
//        " QUEUE_SIZE =",
//        QUEUE_SIZE,
//        " ITERS =",
//        ITERS,
//    )

    queue := bop.spsc_create()
    defer bop.spsc_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
            queue,
            i,
            proc(q: ^bop.SPSC, id: int) {
                id := id + 1
                for i in 0 ..< ITERS {
                    for !bop.spsc_enqueue(q, rawptr(uintptr(u64(i)))) {
                        time.sleep(time.Nanosecond)
                    }
                }
            },
        )
    }

    count := 0
    loop: for {
        value : rawptr = nil
        for !bop.spsc_dequeue(queue, &value) {
            intrinsics.cpu_relax()
        }
        count += 1
        if count == ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.printfln(
        "libbop_spsc:           %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

stress_test_mpmc_push :: proc($QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//        "mpmc_bulk_push: ",
//        " QUEUE_SIZE =",
//        QUEUE_SIZE,
//        " ITERS =",
//        ITERS,
//    )

    BATCH_SIZE :: 128

    queue := mpmc_create()
    defer mpmc_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
            queue,
            i,
            proc(q: ^bop.MPMC, id: int) {
                id := id + 1
                token := mpmc_create_producer_token(q)
                defer mpmc_destroy_producer_token(token)
                items: [BATCH_SIZE]u64
                for i in 0 ..< ITERS {
                    for !mpmc_enqueue_token(token, u64(i+1)) {
                        time.sleep(time.Nanosecond)
                    }
                //                    fmt.println("EE")
                //                    time.sleep(time.Second)
                }
            },
        )
    }

    count := 0
    items_ := [BATCH_SIZE]u64{}
    items := items_[0:BATCH_SIZE]
    token := mpmc_create_consumer_token(queue)
    defer mpmc_destroy_consumer_token(token)
    loop: for {
        value : rawptr = nil
        size : i64 = 0

        #unroll for i in 0..<25 do intrinsics.cpu_relax()

        size = mpmc_dequeue_bulk_token(token, raw_data(items), BATCH_SIZE)

        if size == BATCH_SIZE {
            for size == BATCH_SIZE {
                count += int(size)
                if count >= ITERS {
                    time.stopwatch_stop(&stopwatch)
                    break
                }
                size = mpmc_dequeue_bulk_token(token, raw_data(items), BATCH_SIZE)
            }
            count += int(size)
        } else if size == 0 {
            for size == 0 {
                #unroll for i in 0..<25 do intrinsics.cpu_relax()
                size = mpmc_dequeue_bulk_token(token, raw_data(items), BATCH_SIZE)
                count += int(size)

                if count >= ITERS {
                    time.stopwatch_stop(&stopwatch)
                    break
                }
            }
        } else {
            count += int(size)
            if count >= ITERS {
                time.stopwatch_stop(&stopwatch)
                break
            }
            #unroll for i in 0..<10 do intrinsics.cpu_relax()
        }

        if count >= ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.printfln(
        "mpmc:                  %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

stress_test_mpmc_bulk_push :: proc($QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//        "mpmc_bulk_push: ",
//        " QUEUE_SIZE =",
//        QUEUE_SIZE,
//        " ITERS =",
//        ITERS,
//    )

    BATCH_SIZE :: 256
    ITERS_BULK :: ITERS / BATCH_SIZE

    queue := mpmc_create()
    defer mpmc_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
            queue,
            i,
            proc(q: ^bop.MPMC, id: int) {
                id := id + 1
                token := mpmc_create_producer_token(q)
                items: [BATCH_SIZE]u64
                for i in 0 ..< ITERS_BULK {
//                    for !mpmc_enqueue(q, u64(i+1)) {
                    for !mpmc_enqueue_bulk_token(token, raw_data(items[0:BATCH_SIZE]), BATCH_SIZE) {
                        time.sleep(time.Nanosecond)
                    }
//                    fmt.println("EE")
//                    time.sleep(time.Second)
                }
            },
        )
    }

    count := 0
    items_ := [BATCH_SIZE]u64{}
    items := items_[0:BATCH_SIZE]
    token := mpmc_create_consumer_token(queue)
    loop: for {
        value : rawptr = nil
        size : i64 = 0

        size = mpmc_dequeue_bulk_token(token, raw_data(items), BATCH_SIZE)
        for size == 0 {
            intrinsics.cpu_relax()
            size = mpmc_dequeue_bulk_token(token, raw_data(items), BATCH_SIZE)
        }
        count += int(size)
        if count == ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.printfln(
        "mpmc_bulk:             %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

stress_test_mpmc_blocking_bulk_push :: proc($QUEUE_SIZE: int, $ITERS: int) {
//    fmt.println(
//        "mpmc_blocking_bulk_push: ",
//        " QUEUE_SIZE =",
//        QUEUE_SIZE,
//        " ITERS =",
//        ITERS,
//    )

    BATCH_SIZE :: 256
    ITERS_BULK :: ITERS / BATCH_SIZE

    queue := mpmc_blocking_create()
    defer mpmc_blocking_destroy(queue)

    stopwatch: time.Stopwatch
    time.stopwatch_start(&stopwatch)

    producers: [2]^thread.Thread

    for i in 0 ..< 1 {
        producers[i] = thread.create_and_start_with_poly_data2(
            queue,
            i,
            proc(q: ^bop.MPMC_Blocking, id: int) {
                id := id + 1
                token := mpmc_blocking_create_producer_token(q)
                items: [BATCH_SIZE]u64
                for i in 0 ..< ITERS_BULK {
                    for !mpmc_blocking_enqueue_bulk_token(token, raw_data(items[0:BATCH_SIZE]), BATCH_SIZE) {
                        time.sleep(time.Nanosecond)
                    }
                }
            },
        )
    }

    count := 0
    items_ := [BATCH_SIZE]u64{}
    items := items_[0:BATCH_SIZE]
    token := mpmc_blocking_create_consumer_token(queue)
    loop: for {
        value : rawptr = nil
        size : i64 = 0

        size = mpmc_blocking_dequeue_bulk_wait_token(token, raw_data(items), BATCH_SIZE, -1)
        for size == 0 {
            intrinsics.cpu_relax()
            size = mpmc_blocking_dequeue_bulk_wait_token(token, raw_data(items), BATCH_SIZE, -1)
        }
        count += int(size)
        if count == ITERS {
            time.stopwatch_stop(&stopwatch)
            break
        }
    }

    fmt.printfln(
        "mpmc_blocking_bulk:    %.2f ns/op          %s ops/sec",
        f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS)),
        temp_humanize_number(u64(f64(time.Second) /
        (f64(time.stopwatch_duration(stopwatch)) / f64(time.Duration(ITERS))))),
    )
}

temp_humanize_number :: proc(n: u64, allocator := context.temp_allocator) -> string {
    buffer := make([dynamic]u8, 0, 32, allocator)
    num := n
    digit_count := 0

    if num == 0 {
        return "0"
    }

    for num > 0 {
        if digit_count > 0 && digit_count % 3 == 0 {
            append(&buffer, ',')
        }

        digit := u8('0' + num % 10)
        append(&buffer, digit)

        num /= 10
        digit_count += 1
    }

    // reverse the string
    for i := 0; i < len(buffer)/2; i += 1 {
        buffer[i], buffer[len(buffer)-1-i] = buffer[len(buffer)-1-i], buffer[i]
    }

    return string(buffer[0:len(buffer)])
}