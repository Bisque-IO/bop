use bop_rs::mdbx::{Env, EnvFlags, OptionKey, TxnFlags, Dbi, DbFlags, put, get};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create environment
    let env = Env::new()?;
    // Configure options before open
    env.set_option(OptionKey::MaxDbs, 8)?;

    // Open a database environment (adjust path as needed)
    // NOTE: This is a simple example; ensure the directory exists and is writable
    env.open("./.mdbx-data", EnvFlags::DEFAULTS, 0o600)?;

    // Begin a write txn
    let mut wtx = bop_rs::mdbx::Txn::begin(&env, None, TxnFlags::READWRITE)?;
    // Open unnamed DB
    let dbi = Dbi::open(&wtx, None, DbFlags::DEFAULTS)?;

    // Put a key/value
    put(&wtx, dbi, b"hello", b"world", bop_rs::mdbx::PutFlags::empty())?;
    wtx.commit()?;

    // Read it back
    let rtx = bop_rs::mdbx::Txn::begin(&env, None, TxnFlags::RDONLY)?;
    let v = get(&rtx, dbi, b"hello")?.expect("missing");
    println!("hello -> {}", String::from_utf8_lossy(v));
    // Cursor example
    let mut cur = bop_rs::mdbx::Cursor::open(&rtx, dbi)?;
    if let Some((k, v)) = cur.first()? { println!("first: {} -> {}", String::from_utf8_lossy(k), String::from_utf8_lossy(v)); }
    drop(cur);

    // Example using the new CursorOp enum
    let mut cur = bop_rs::mdbx::Cursor::open(&rtx, dbi)?;
    use bop_rs::mdbx::CursorOp;

    // Using the enum for cursor operations
    if let Some((k, v)) = cur.get_with_op(CursorOp::First)? {
        println!("CursorOp::First: {} -> {}", String::from_utf8_lossy(k), String::from_utf8_lossy(v));
    }

    if let Some((k, v)) = cur.get_with_op(CursorOp::Next)? {
        println!("CursorOp::Next: {} -> {}", String::from_utf8_lossy(k), String::from_utf8_lossy(v));
    }

    if let Some((k, v)) = cur.get_with_op(CursorOp::Last)? {
        println!("CursorOp::Last: {} -> {}", String::from_utf8_lossy(k), String::from_utf8_lossy(v));
    }

    drop(cur);
    rtx.abort()?;

    Ok(())
}

// Minimal convenience to allow read-only txn abort in example
trait AbortExt { fn abort(self) -> Result<(), Box<dyn std::error::Error>>; }
impl<'e> AbortExt for bop_rs::mdbx::Txn<'e> {
    fn abort(self) -> Result<(), Box<dyn std::error::Error>> { Ok(()) }
}

