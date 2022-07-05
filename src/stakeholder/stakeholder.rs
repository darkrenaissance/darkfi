use super::blockchain::{Blockchain, Block, BlockInfo};
use super::util::time::Timestamp;

#[derive(Copy,Debug,Default,Clone)]
pub struct Stakeholder
{
    blockchain: Blockchain // stakeholder view of the blockchain
}

impl Stakeholder
{
    fn init() {
        //TODO initialize the blockchain
        let path = "/tmp";
        let db = sled::open(path)?;
        let ts = Timestamp::current_time();
        let genesis_data = "data":
        let genesis_hash = flake3::Hash(genesis_data.as_bytes());
        let eta0 = flake3::Hash("let there be dark!");
        self.blockchain = Blockchain::new(db, ts, genesis_hash, eta0.as_bytes());
    }

    fn add_block(&self, block: BlockInfo)
    {
        let blocks = [block];
        self.blockchain.add(blocks);
    }

    fn get_eta(&self) -> pallas::Base
    {
        let last_proof_slot : u64 = 0;
        let (sl, proof_tx_hash) = self.blockchain.last()?;
        pallas::Base::from_bytes(proof_tx_hash.to_bytees())
    }
}
