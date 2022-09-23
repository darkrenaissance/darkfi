use darkfi::util::serial::{
    serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt,
};

#[derive(SerialEncodable)]
enum Test {
    Type1(u32),
    Type2,
    Type3,
}

fn main() {
    println!("Hello, world!");
}
