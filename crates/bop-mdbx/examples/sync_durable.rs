use bop_mdbx::{
    DbFlags, Dbi, Env as MdbxEnv, EnvFlags, OptionKey, PutFlags, Txn, TxnFlags, del, put,
};
use heed::{Database, Env as HeedEnv, EnvFlags as HeedEnvFlags, EnvOpenOptions};
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

use diatomic_waker::DiatomicWaker;

const VALUE: &[u8] = b"0123456789abcdef";
const PRELOAD: u64 = 1_000;
const ITERATIONS: u64 = 10_000;
const MAP_SIZE: isize = 64 * 1024 * 1024;

// ----- MDBX helpers -----
fn setup_mdbx_env() -> Result<(MdbxEnv, Dbi, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempdir()?;
    let env = MdbxEnv::new()?;
    env.set_option(OptionKey::MaxDbs, 2)?;
    env.set_geometry(
        MAP_SIZE,
        MAP_SIZE,
        MAP_SIZE * 16,
        MAP_SIZE,
        MAP_SIZE / 4,
        4096,
    )?;
    env.open(dir.path(), EnvFlags::SYNC_DURABLE, 0o600)?;
    let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
    let dbi = Dbi::open(&txn, Some("bench"), DbFlags::CREATE | DbFlags::INTEGERKEY)?;
    txn.commit()?;
    Ok((env, dbi, dir))
}

fn run_mdbx_insert(iterations: u64, batch: u64) -> Result<(), Box<dyn Error>> {
    let (env, dbi, dir) = setup_mdbx_env()?;
    let start = Instant::now();
    for i in 0..iterations {
        let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
        for j in 0..batch {
            let key = ((i * batch) + j).to_le_bytes();
            put(&txn, dbi, &key, VALUE, PutFlags::UPSERT)?;
        }
        txn.commit()?;
    }
    let elapsed = start.elapsed();
    let total_ops = iterations * batch;
    println!(
        "mdbx insert (batch={}): {} ops in {:?} ({:.2} ops/s)",
        batch,
        total_ops,
        elapsed,
        total_ops as f64 / elapsed.as_secs_f64()
    );
    env.close()?;
    drop(dir);
    Ok(())
}

fn run_mdbx_update(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, dbi, dir) = setup_mdbx_env()?;
    {
        let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
        for i in 0..PRELOAD {
            let key = i.to_le_bytes();
            put(&txn, dbi, &key, VALUE, PutFlags::UPSERT)?;
        }
        txn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let key = (i % PRELOAD).to_le_bytes();
        let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
        put(&txn, dbi, &key, VALUE, PutFlags::UPSERT)?;
        txn.commit()?;
    }
    let elapsed = start.elapsed();
    println!(
        "mdbx update: {} ops in {:?} ({:.2} ops/s)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
    env.close()?;
    drop(dir);
    Ok(())
}

fn run_mdbx_delete(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, dbi, dir) = setup_mdbx_env()?;
    {
        let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
        for i in 0..PRELOAD {
            let key = i.to_le_bytes();
            put(&txn, dbi, &key, VALUE, PutFlags::UPSERT)?;
        }
        txn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let key = (i % PRELOAD).to_le_bytes();
        let txn = Txn::begin(&env, None, TxnFlags::READWRITE)?;
        del(&txn, dbi, &key, None)?;
        put(&txn, dbi, &key, VALUE, PutFlags::UPSERT)?;
        txn.commit()?;
    }
    let elapsed = start.elapsed();
    println!(
        "mdbx delete: {} ops in {:?} ({:.2} ops/s)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
    env.close()?;
    drop(dir);
    Ok(())
}

// ----- Heed helpers -----
type HeedDb = Database<
    heed::types::U64<heed::byteorder::BigEndian>,
    heed::types::U64<heed::byteorder::BigEndian>,
>;

fn setup_heed_env() -> Result<(HeedEnv, HeedDb, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempdir()?;
    let mut options = EnvOpenOptions::new();
    options.map_size(MAP_SIZE as usize).max_dbs(2);
    // unsafe {
    //     // options.flags(HeedEnvFlags::);
    // }
    let env = unsafe { options.open(dir.path())? };
    let mut wtxn = env.write_txn()?;
    let db = env.create_database::<heed::types::U64<heed::byteorder::BigEndian>, heed::types::U64<heed::byteorder::BigEndian>>(&mut wtxn, Some("bench"))?;
    wtxn.commit()?;
    Ok((env, db, dir))
}

fn run_heed_insert(iterations: u64, batch: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    let start = Instant::now();
    for i in 0..iterations {
        let mut wtxn = env.write_txn()?;
        for j in 0..batch {
            db.put(&mut wtxn, &i, &j)?;
        }
        wtxn.commit()?;
    }
    let elapsed = start.elapsed();
    let total_ops = iterations * batch;
    println!(
        "heed insert (batch={}): {} ops in {:?} ({:.2} ops/s)",
        batch,
        total_ops,
        elapsed,
        total_ops as f64 / elapsed.as_secs_f64()
    );
    env.force_sync()?;
    drop(env);
    drop(dir);
    Ok(())
}

fn run_heed_update(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    {
        let mut wtxn = env.write_txn()?;
        for i in 0..PRELOAD {
            db.put(&mut wtxn, &i, &i)?;
        }
        wtxn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let mut wtxn = env.write_txn()?;
        db.put(&mut wtxn, &i, &i)?;
        wtxn.commit()?;
    }
    let elapsed = start.elapsed();
    println!(
        "heed update: {} ops in {:?} ({:.2} ops/s)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
    env.force_sync()?;
    drop(env);
    drop(dir);
    Ok(())
}

fn run_heed_delete(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    {
        let mut wtxn = env.write_txn()?;
        for i in 0..PRELOAD {
            db.put(&mut wtxn, &i, &i)?;
        }
        wtxn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let mut wtxn = env.write_txn()?;
        db.delete(&mut wtxn, &i)?;
        db.put(&mut wtxn, &i, &i)?;
        wtxn.commit()?;
    }
    let elapsed = start.elapsed();
    println!(
        "heed delete: {} ops in {:?} ({:.2} ops/s)",
        iterations,
        elapsed,
        iterations as f64 / elapsed.as_secs_f64()
    );
    env.force_sync()?;
    drop(env);
    drop(dir);
    Ok(())
}

fn run_diatomic_waker(producers: usize, ops_per_thread: usize) {
    let queue = Arc::new(DiatomicWaker::new());
    let expected = producers * ops_per_thread;
    let counter = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(producers + 1));

    let mut handles = Vec::with_capacity(producers);
    for _ in 0..producers {
        let queue = queue.clone();
        let barrier = barrier.clone();
        let counter = counter.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            for _ in 0..ops_per_thread {
                queue.notify();
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    barrier.wait();
    let start = Instant::now();
    loop {
        if counter.load(Ordering::Relaxed) < expected {
            std::thread::yield_now();
            // std::thread::yield_now();
            // std::thread::yield_now();
            // std::thread::yield_now();
            // std::thread::yield_now();
        } else {
            break;
        }
    }
    let elapsed = start.elapsed();

    for handle in handles {
        handle.join().expect("producer thread panicked");
    }

    report(producers, counter.load(Ordering::Relaxed), elapsed);
}

fn report(producers: usize, total_ops: usize, elapsed: Duration) {
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 {
        total_ops as f64 / elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "{producers},{total_ops},{elapsed_ms:.3},{ops_per_sec:.0}",
        producers = producers,
        total_ops = total_ops,
        elapsed_ms = elapsed_ms,
        ops_per_sec = ops_per_sec,
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    run_diatomic_waker(4, 10_000_000);

    println!("mdbx sync durable benchmark (iterations={})", ITERATIONS);
    run_mdbx_insert(ITERATIONS, 1)?;
    run_mdbx_insert(ITERATIONS, 100)?;
    run_mdbx_update(ITERATIONS)?;
    run_mdbx_delete(ITERATIONS)?;

    println!("\nheed sync durable benchmark (iterations={})", ITERATIONS);
    run_heed_insert(ITERATIONS, 1)?;
    run_heed_insert(ITERATIONS, 100)?;
    run_heed_update(ITERATIONS)?;
    run_heed_delete(ITERATIONS)?;

    Ok(())
}
