use darkfi::serial::SerialEncodable;

#[derive(Debug, SerialEncodable)]
struct Test {
    one: u64,
    two: u64,
}

fn main() {
    let test = Test { one: 1, two: 2 };
    println!("Test: {:?}", test);
}
