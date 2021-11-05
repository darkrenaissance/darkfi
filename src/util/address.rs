use group::GroupEncoding;
use sha2::Digest;

#[derive(Clone, Debug)]
pub struct Address {
    pub hash: [u8; 32],
}

impl Address {
    pub fn new(raw: jubjub::SubgroupPoint) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(raw.to_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        Address { hash }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // ripemd160 hash
        let mut hasher = ripemd160::Ripemd160::new();
        hasher.update(self.hash);
        let mut hash = hasher.finalize().to_vec();

        let mut payload: Vec<u8> = vec![0x00_u8];

        // add public key hash
        payload.append(&mut hash);

        // hash the payload + version
        let mut hasher = sha2::Sha256::new();
        hasher.update(payload.clone());
        let payload_hash = hasher.finalize().to_vec();

        payload.append(&mut payload_hash[0..4].to_vec());

        // base56 encoding
        let address: String = bs58::encode(payload).into_string();

        write!(f, "{}", address)
    }
}
