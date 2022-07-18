use crate::{
    consensus::{BlockInfo},
    util::time::Timestamp,
    blockchain::{Blockchain,Epoch},
    Result,
};


use darkfi::crypto::proof::VerifyingKey;
use darkfi::crypto::proof::ProvingKey;


use pasta_curves::{
    pallas,
};

use group::ff::PrimeField;

pub struct Stakeholder
{
    pub blockchain: Blockchain, // stakeholder view of the blockchain
    pub clock : Clock,
    pub coins : Vec<LeadCoin>, // owned stakes
    pub epoch : &Epoch, // current epoch
    pub epoch_consensus : EpochConsensus, // configuration for the epoch
    pub pk : ProvingKey,
    pub vk : VerifyingKey,
}

impl Stakeholder
{
    /// initialize new stakeholder with sled in /tmp
    pub fn new(consensus: EpochConsensus) -> Result<Self>
    {
        //TODO initialize the blockchain
        let path = "/tmp";
        let db = sled::open(path).unwrap();
        let ts = Timestamp::current_time();
        let genesis_hash = blake3::hash(b"");
        let bc = Blockchain::new(&db, ts, genesis_hash).unwrap();
        //TODO replace with const
        let eta = pallas::base::one();
        //let epoch = Epoch::new(consensus, eta);
        let lead_pk = ProvingKey::build(k, &LeadContract::default());
        let lead_vk = VerifyingKey::build(k, &LeadContract::default());
        Ok(Self{blockchain: bc, epoch_consensus: consensus, pk:lead_pk, vk:lead_vk})
    }

    /// add new blockinfo to the blockchain
    pub fn add_block(&self, block: BlockInfo)
    {
        let blocks = [block];
        self.blockchain.add(&blocks);
    }

    /// extract leader selection lottery randomness \eta
    /// it's the hash of the previous lead proof
    /// converted to pallas base
    pub fn get_eta(&self) -> pallas::Base
    {
        let last_proof_slot : u64 = 0;
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes : [u8;32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    /// on the onset of the epoch, layout the new the competing coins
    /// assuming static stake during the epoch, enforced by the commitment to competing coins
    /// in the epoch's gen2esis data.
    pub fn new_epoch(&self)
    {
        let eta = self.get_eta();
        let epoch = Epoch::new(self.consensus, self.get_eta());
        let coins : Vec<LeadCoin> = epoch.create_coins();
        self.epoch = epoch;
        //TODO initialize blocks in the epoch, and add coin commitment in genesis
    }

    /// at the begining of the slot
    /// stakeholder need to play the lottery for the slot.
    /// FIXME if the stakeholder is not winning, staker can try different coins before,
    /// commiting it's coins, to maximize success, thus,
    /// the lottery proof need to be conditioned on the slot itself, and previous proof.
    /// this will encourage each potential leader to play with honesty.
    pub fn new_slot(&self) -> Result<bool, Proof>
    {
        let sl : u64 = self.clock.slot();
        let is_leader : bool = self.epoch.is_leader(sl);
        // if is leader create proof
        let proof = self.epoch.get_proof(sl, self.pk);
        //TODO initialize blocks in the epoch, and add proof
        Ok(is_leader, proof)
    }
}
