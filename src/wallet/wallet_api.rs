use crate::Result;

pub trait WalletApi {
    fn init_db(&self) -> Result<()>; 
    fn get_public_keys(&self) -> Result<Vec<jubjub::SubgroupPoint>>;
    fn key_gen(&self) -> Result<(Vec<u8>, Vec<u8>)>;
    fn put_keypair(&self, key_public: Vec<u8>, key_private: Vec<u8>) -> Result<()>;
    fn get_private_keys(&self) -> Result<Vec<jubjub::Fr>>;
}
