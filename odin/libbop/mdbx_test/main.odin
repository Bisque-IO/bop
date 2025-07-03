package mdbx_test

import "base:runtime"
import "core:fmt"

import c "core:c/libc"

import bop "../"

maybe_panic :: proc(err: bop.Mdbx_Error, location := #caller_location) {
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

    err := mdbx_env_delete(cstring("my.db"), .Just_Delete)
    maybe_panic(err)

    env: ^Mdbx_Env
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

    maybe_panic(mdbx_env_set_option(env, Mdbx_Option.Max_DB, 2))
    maybe_panic(mdbx_env_set_option(env, Mdbx_Option.Max_Readers, 64))

    err = mdbx_env_open(
        env,
        "my.db",
        .No_Sticky_Threads | .No_Sub_Dir | .No_Mem_Init | .Sync_Durable | .Lifo_Reclaim | .Write_Map,
        655
    )
    maybe_panic(err)

    tx : ^Mdbx_Txn = nil
    err = mdbx_txn_begin_ex(env, nil, .Read_Write, &tx, nil)
    maybe_panic(err)

    dbi: Mdbx_DBI
    err = mdbx_dbi_open(tx, "state", .Integer_Key | .Create, &dbi)
    maybe_panic(err)

    latency: Mdbx_Commit_Latency
    err = mdbx_txn_commit_ex(tx, &latency)
    maybe_panic(err)

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
            num, slot, pid, txnid, lag, bytes_used, bytes_retained
        )
        return 0
    }, nil)
    maybe_panic(err)
}