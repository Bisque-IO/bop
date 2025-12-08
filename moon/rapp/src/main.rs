mod resp3;

use resp3::{parse, write};

fn main() {
    // Example usage
    let test_input = "+OK\r\n";
    if let Some(value) = parse(test_input) {
        println!("Parsed: {:?}", value);
        println!("Written back: {}", write(&value));
    }
}
