use crate::{
    consensus::{BlockInfo},
    util::time::Timestamp,
    blockchain::{Blockchain},
    Result,
};

use pasta_curves::{
    pallas,
};

use group::ff::PrimeField;

pub struct Stakeholder
{
    pub blockchain: Blockchain // stakeholder view of the blockchain
}

impl Stakeholder
{
    pub fn new() -> Result<Self>
    {
        //TODO initialize the blockchain
        let path = "/tmp";
        let db = sled::open(path).unwrap();
        let ts = Timestamp::current_time();
        let genesis_hash = blake3::hash(b"data");
        let bc = Blockchain::new(&db, ts, genesis_hash).unwrap();
        Ok(Self{blockchain: bc})
    }

    pub fn add_block(&self, block: BlockInfo)
    {
        let blocks = [block];
        self.blockchain.add(&blocks);
    }

    pub fn get_eta(&self) -> pallas::Base
    {
        let last_proof_slot : u64 = 0;
        let (sl, proof_tx_hash) = self.blockchain.last().unwrap();
        let mut bytes : [u8;32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }
}
