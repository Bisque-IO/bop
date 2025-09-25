use bop_mdbx::{
    DbFlags, Dbi, Env as MdbxEnv, EnvFlags, OptionKey, PutFlags, Txn, TxnFlags, del, put,
};
use heed::{Database, Env as HeedEnv, EnvFlags as HeedEnvFlags, EnvOpenOptions, types::ByteSlice};
use std::error::Error;
use std::time::Instant;
use tempfile::tempdir;

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
type HeedDb = Database<ByteSlice, ByteSlice>;

fn setup_heed_env() -> Result<(HeedEnv, HeedDb, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempdir()?;
    let mut options = EnvOpenOptions::new();
    options.map_size(MAP_SIZE as usize).max_dbs(2);
    unsafe {
        options.flags(HeedEnvFlags::SYNC_DURABLE);
    }
    let env = unsafe { options.open(dir.path())? };
    let mut wtxn = env.write_txn()?;
    let db = env.create_database::<ByteSlice, ByteSlice>(&mut wtxn, Some("bench"))?;
    wtxn.commit()?;
    Ok((env, db, dir))
}

fn run_heed_insert(iterations: u64, batch: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    let start = Instant::now();
    for i in 0..iterations {
        let mut wtxn = env.write_txn()?;
        for j in 0..batch {
            let key = ((i * batch) + j).to_le_bytes();
            db.put(&mut wtxn, &key, VALUE)?;
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
    drop(db);
    drop(env);
    drop(dir);
    Ok(())
}

fn run_heed_update(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    {
        let mut wtxn = env.write_txn()?;
        for i in 0..PRELOAD {
            let key = i.to_le_bytes();
            db.put(&mut wtxn, &key, VALUE)?;
        }
        wtxn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let key = (i % PRELOAD).to_le_bytes();
        let mut wtxn = env.write_txn()?;
        db.put(&mut wtxn, &key, VALUE)?;
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
    drop(db);
    drop(env);
    drop(dir);
    Ok(())
}

fn run_heed_delete(iterations: u64) -> Result<(), Box<dyn Error>> {
    let (env, db, dir) = setup_heed_env()?;
    {
        let mut wtxn = env.write_txn()?;
        for i in 0..PRELOAD {
            let key = i.to_le_bytes();
            db.put(&mut wtxn, &key, VALUE)?;
        }
        wtxn.commit()?;
    }
    let start = Instant::now();
    for i in 0..iterations {
        let key = (i % PRELOAD).to_le_bytes();
        let mut wtxn = env.write_txn()?;
        db.delete(&mut wtxn, &key)?;
        db.put(&mut wtxn, &key, VALUE)?;
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
    drop(db);
    drop(env);
    drop(dir);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
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
