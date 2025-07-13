package mdbx_test

import "base:runtime"
import "core:fmt"
import "core:strings"
import "core:time"

import c "core:c/libc"

import "../"
import bop "../../libbop"

maybe_panic :: proc(err: mdbx.Error, location := #caller_location) {
	if err != nil && err != .Result_True {
		msg := mdbx.liberr2str(err)
		if msg == nil {
			if err == .Result_True {
				panic("Result_True", location)
			}
		}
		panic(string(mdbx.liberr2str(err)), location)
	}
}

main :: proc() {
	err: mdbx.Error
	fmt.println("page size: 512  min db size:", mdbx.limits_dbsize_min(512))

	//    err := mdbx.env_delete(cstring("my.db"), .Just_Delete)
	//    maybe_panic(err)

	env: ^mdbx.Env
	err = mdbx.env_create(&env)
	maybe_panic(err)
	defer mdbx.env_close_ex(env, false)

	err = mdbx.env_set_geometry(env, 1024 * 4, 1024 * 4, 1024 * 1024, 1024 * 4, 1024 * 32, 512)
	maybe_panic(err)

	maybe_panic(mdbx.env_set_option(env, mdbx.Option.Max_DB, 2))
	maybe_panic(mdbx.env_set_option(env, mdbx.Option.Max_Readers, 64))

	err = mdbx.env_open(
		env,
		"my.db",
		.No_Sub_Dir | .No_Mem_Init | .Sync_Durable | .Lifo_Reclaim | .Write_Map,
		u16(755),
	)
	maybe_panic(err)

	tx: ^mdbx.Txn = nil
	err = mdbx.txn_begin_ex(env, nil, .Read_Write, &tx, nil)
	maybe_panic(err)

	dbi: mdbx.DBI
	err = mdbx.dbi_open(tx, "state", .Integer_Key | .Create, &dbi)
	maybe_panic(err)

	id := u32(1)
	key := mdbx.val_u32(&id)
	value := mdbx.Val{}
	err = mdbx.get(tx, dbi, &key, &value)
	if err == .Not_Found {
		fmt.println("Not Found")
	}
	if err == nil {

	}

	start := time.tick_now()
	for i in 0 ..< 1000000 {
		mdbx.get(tx, dbi, &key, &value)
	}
	diff := time.tick_diff(start, time.tick_now())
	fmt.printfln("%.2f ns", f64(diff) / f64(1000000))

	// val := "this is a value"
	// value.base = rawptr(raw_data(val))
	// value.len = uint(len(val))
	// err = mdbx.put(tx, dbi, &key, &value, .Upsert)
	// maybe_panic(err)

	latency: mdbx.Commit_Latency
	err = mdbx.txn_commit_ex(tx, &latency)
	maybe_panic(err)

	err = mdbx.txn_begin_ex(env, nil, .Read_Only, &tx, nil)
	maybe_panic(err)

	cursor := mdbx.cursor_create(nil)
	defer mdbx.cursor_close(cursor)
	err = mdbx.cursor_bind(tx, cursor, dbi)
	maybe_panic(err)

	for x in 0 ..< 5 {
		start = time.tick_now()
		for i in 0 ..< 1000000 {
			mdbx.cursor_get(cursor, &key, &value, .Last if i % 2 == 0 else .First)
		}
		diff = time.tick_diff(start, time.tick_now())
		fmt.printfln("%.2f ns", f64(diff) / f64(1000000))
	}

	err = mdbx.reader_list(
		env,
		proc "c" (
			ctx: rawptr,
			num: c.int,
			slot: c.int,
			pid: bop.PID,
			thread: bop.TID,
			txnid: u64,
			lag: u64,
			bytes_used: c.size_t,
			bytes_retained: c.size_t,
		) -> c.int {
			context = runtime.default_context()
			fmt.printfln(
				"num:%d  slot:%d  pid:%d  tid:%d  txnid:%d  lag:%d  bytes_used:%d  bytes_retained:%d",
				num,
				slot,
				pid,
				thread,
				txnid,
				lag,
				bytes_used,
				bytes_retained,
			)
			return 0
		},
		nil,
	)
	maybe_panic(err)

	mdbx.txn_abort(tx)
}
