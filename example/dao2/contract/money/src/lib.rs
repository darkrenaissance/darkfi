#[repr(u8)]
pub enum MoneyFunction {
    Bar = 0x01,
}

pub fn bar() {
    println!("bar");
}
