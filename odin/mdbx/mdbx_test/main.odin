package mdbx_test

import "base:runtime"
import "core:fmt"
import "core:strings"
import "core:time"

import c "core:c/libc"

import bop "../"

maybe_panic :: proc(err: bop.MDBX_Error, location := #caller_location) {
    if err != nil && err != .Result_True {
        msg := bop.mdbx_liberr2str(err)
        if msg == nil {
            if err == .Result_True {
                panic("Result_True", location)
            }
        }
        panic(string(bop.mdbx_liberr2str(err)), location)
    }
}

main :: proc() {
    using bop

    err: MDBX_Error
    fmt.println("page size: 512  min db size:", mdbx_limits_dbsize_min(512))

//    err := mdbx_env_delete(cstring("my.db"), .Just_Delete)
//    maybe_panic(err)

    env: ^MDBX_Env
    err = mdbx_env_create(&env)
    maybe_panic(err)
    defer mdbx_env_close_ex(env, false)

    err = mdbx_env_set_geometry(
        env,
        1024*4,
        1024*4,
        1024*1024,
        1024*4,
        1024*32,
        512,
    )
    maybe_panic(err)

    maybe_panic(mdbx_env_set_option(env, MDBX_Option.Max_DB, 2))
    maybe_panic(mdbx_env_set_option(env, MDBX_Option.Max_Readers, 64))

    err = mdbx_env_open(
        env,
        "my.db",
        .No_Sub_Dir | .No_Mem_Init | .Sync_Durable | .Lifo_Reclaim | .Write_Map,
        655
    )
    maybe_panic(err)

    tx : ^MDBX_Txn = nil
    err = mdbx_txn_begin_ex(env, nil, .Read_Write, &tx, nil)
    maybe_panic(err)

    dbi: MDBX_DBI
    err = mdbx_dbi_open(tx, "state", .Integer_Key | .Create, &dbi)
    maybe_panic(err)

    id := u32(1)
    key := MDBX_Val{&id, 4}
    value := MDBX_Val{}
    err = mdbx_get(tx, dbi, &key, &value)
    if err == .Not_Found {
        fmt.println("Not Found")
    }
    if err == nil {

    }

    start := time.tick_now()
    for i in 0..<1000000 {
        mdbx_get(tx, dbi, &key, &value)
    }
    diff := time.tick_diff(start, time.tick_now())
    fmt.printfln("%.2f ns", f64(diff)/f64(1000000))

   // val := "this is a value"
   // value.base = rawptr(raw_data(val))
   // value.len = uint(len(val))
   // err = mdbx_put(tx, dbi, &key, &value, .Upsert)
   // maybe_panic(err)

    latency: MDBX_Commit_Latency
    err = mdbx_txn_commit_ex(tx, &latency)
    maybe_panic(err)

    err = mdbx_txn_begin_ex(env, nil, .Read_Only, &tx, nil)
    maybe_panic(err)

    cursor := mdbx_cursor_create(nil)
    defer mdbx_cursor_close(cursor)
    err = mdbx_cursor_bind(tx, cursor, dbi)
    maybe_panic(err)

    for x in 0..<5 {
        start = time.tick_now()
        for i in 0..<1000000 {
            mdbx_cursor_get(cursor, &key, &value, .Last if i % 2 == 0 else .First)
        }
        diff = time.tick_diff(start, time.tick_now())
        fmt.printfln("%.2f ns", f64(diff)/f64(1000000))
    }

    err = mdbx_reader_list(env, proc "c" (
        ctx: rawptr,
        num: c.int,
        slot: c.int,
        pid: PID,
        thread: TID,
        txnid: u64,
        lag: u64,
        bytes_used: c.size_t,
        bytes_retained: c.size_t,
    ) -> c.int {
        context = runtime.default_context()
        fmt.printfln(
            "num:%d  slot:%d  pid:%d  tid:%d  txnid:%d  lag:%d  bytes_used:%d  bytes_retained:%d",
            num, slot, pid, thread, txnid, lag, bytes_used, bytes_retained
        )
        return 0
    }, nil)
    maybe_panic(err)

    mdbx_txn_abort(tx)
}
