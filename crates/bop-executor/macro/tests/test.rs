extern crate bop_executor as reexported_bop_executor;

use std::future::ready;

#[bop_executor::test]
async fn basic() {
    ready(42).await;
}

#[bop_executor::test]
async fn result() -> Result<(), std::io::Error> {
    if ready(42).await == 42 {
        Ok(())
    } else {
        unreachable!()
    }
}

#[bop_executor::test(crate = reexported_bop_executor)]
async fn crate_path() {
    ready(42).await;
}

#[bop_executor::test(crate = "reexported_bop_executor")]
async fn crate_str() {
    ready(42).await;
}
