use bop_rs::mdbx::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Env::new()?;
    env.set_option(OptionKey::MaxDbs, 8)?;
    env.open("./.mdbx-dupfixed", EnvFlags::DEFAULTS, 0o600)?;

    // Create DUPFIXED table
    let mut wtx = Txn::begin(&env, None, TxnFlags::READWRITE)?;
    let dbi = Dbi::open(&wtx, Some("dupfixed"), DbFlags::DUPSORT | DbFlags::DUPFIXED)?;

    // Write multiple fixed-size values under one key
    let key = b"k1";
    let elems: &[u32] = &[10, 20, 30, 40];
    let payload = unsafe { std::slice::from_raw_parts(elems.as_ptr() as *const u8, elems.len() * std::mem::size_of::<u32>()) };
    let written = put_multiple(&wtx, dbi, key, std::mem::size_of::<u32>(), payload, PutFlags::UPSERT)?;
    println!("wrote {} elements", written);
    wtx.commit()?;

    // Read back with GET_MULTIPLE iteration
    let rtx = Txn::begin(&env, None, TxnFlags::RDONLY)?;
    let mut cur = Cursor::open(&rtx, dbi)?;
    // Seek to key
    cur.set_range(key);
    if let Some((chunk, cnt)) = cur.get_multiple(std::mem::size_of::<u32>())? {
        let vals: &[u32] = unsafe { std::slice::from_raw_parts(chunk.as_ptr() as *const u32, cnt) };
        println!("first chunk: {:?}", vals);
    }
    Ok(())
}

