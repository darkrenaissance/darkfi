pub struct Nullifier {
    pub repr: [u8; 32],
}

impl Nullifier {
    pub fn new(repr: [u8; 32]) -> Self {
        Self { repr }
    }
}
