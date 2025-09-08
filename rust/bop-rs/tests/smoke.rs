use bop_rs::greeting;

#[test]
fn greeting_is_stable() {
    assert_eq!(greeting(), "Hello from bop-rs!");
}

