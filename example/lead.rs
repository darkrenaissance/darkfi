use halo2_proofs::{arithmetic::Field, dev::MockProver};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};


use darkfi::crypto::proof::VerifyingKey;
use darkfi::crypto::proof::ProvingKey;

use darkfi::{
    blockchain::{
        Blockchain,
        epoch::{Epoch,EpochItem},
    },
    stakeholder::stakeholder::{Stakeholder},
    util::time::{Timestamp},
    crypto::{
        constants::MERKLE_DEPTH_ORCHARD,
        leadcoin::LeadCoin,
        lead_proof,
        merkle_node::MerkleNode,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    tx::Transaction,
    consensus::{TransactionLeadProof, Metadata, StreamletMetadata, BlockInfo},
    zk::circuit::lead_contract::LeadContract,
};

fn main() {
    let k: u32 = 13;
    //
    let lead_pk = ProvingKey::build(k, &LeadContract::default());
    let lead_vk = VerifyingKey::build(k, &LeadContract::default());
    //
    const LEN: usize = 10;
    let epoch_item = EpochItem {
        value: 0,  //static stake value
    };
    //
    let genesis_data = blake3::hash(b"");
    let db = sled::open("/tmp/darkfi.db").unwrap();
    let oc = Blockchain::new(&db, Timestamp::current_time(), genesis_data).unwrap();
    let stakeholder = Stakeholder {
        blockchain: oc,
    };
    let eta : pallas::Base = stakeholder.get_eta();
    let epoch = Epoch {
        len: Some(LEN),
        item: Some(epoch_item),
        eta: eta,
    };
    let coins: Vec<LeadCoin> = epoch.create_coins();
    let coin_idx = 0;
    let coin = coins[coin_idx];
    let contract = coin.create_contract();

    //
    //let proof = lead_proof::create_lead_proof(lead_pk.clone(), coin.clone());

    //TODO (fix) proof panics
    let lead_tx = TransactionLeadProof::new(lead_pk, coin.clone());
    /*
    //lead_tx.verify(lead_vk, coin);
    let (st_id, st_hash)  = stakeholder.blockchain.last().unwrap();
    let empty_txs : Vec<Transaction> = vec!();
    let metadata = Metadata::new(Timestamp::current_time(), epoch.eta.to_repr(), lead_tx);
    let sm = StreamletMetadata::new(vec!());
    let bk_info = BlockInfo::new(st_hash, 1, 0, empty_txs, metadata, sm);
    let blks = [bk_info];
    stakeholder.blockchain.add(&blks);

    // calculate public inputs
    let public_inputs = coin.public_inputs();
    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
    */
}
