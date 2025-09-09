use bop_rs::mdbx::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Env::new()?;
    env.open("./.mdbx-scan", EnvFlags::DEFAULTS, 0o600)?;
    // Seed a few keys
    let wtx = Txn::begin(&env, None, TxnFlags::READWRITE)?;
    let dbi = Dbi::open(&wtx, None, DbFlags::DEFAULTS)?;
    for i in 0..5u32 {
        let key = format!("k{:02}", i);
        let val = format!("v{:02}", i);
        put(&wtx, dbi, key.as_bytes(), val.as_bytes(), PutFlags::UPSERT)?;
    }
    wtx.commit()?;

    let rtx = Txn::begin(&env, None, TxnFlags::RDONLY)?;
    let mut cur = Cursor::open(&rtx, dbi)?;
    // Scan from first, stop when key == "k03"
    let rc = cur.scan(bop_sys::MDBX_cursor_op_MDBX_FIRST, bop_sys::MDBX_cursor_op_MDBX_NEXT, |k, _v| {
        let stop = k == b"k03";
        println!("visit {}", String::from_utf8_lossy(k));
        Ok(stop)
    })?;
    println!("scan rc={} (MDBX_RESULT_TRUE indicates predicate matched)", rc);
    Ok(())
}

