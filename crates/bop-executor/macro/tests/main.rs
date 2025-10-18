extern crate bop_executor as reexported_bop_executor;

use std::future::ready;

#[bop_executor::main]
async fn main_basic() {
    ready(42).await;
}

#[test]
fn basic() {
    main_basic();
}

#[bop_executor::main]
async fn main_result() -> Result<(), std::io::Error> {
    ready(42).await;
    Ok(())
}

#[test]
fn result() {
    main_result().unwrap();
}

#[bop_executor::main(crate = reexported_bop_executor)]
async fn main_crate_path() {
    ready(42).await;
}

#[bop_executor::main(crate = "reexported_bop_executor")]
async fn main_crate_str() {
    ready(42).await;
}

#[test]
fn crate_() {
    main_crate_path();
    main_crate_str();
}
