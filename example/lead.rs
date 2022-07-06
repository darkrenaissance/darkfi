use halo2_proofs::{arithmetic::Field, dev::MockProver};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

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
        merkle_node::MerkleNode,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    zk::circuit::lead_contract::LeadContract,
};


fn main() {
    let k: u32 = 13;

    //let lead_pk = ProvingKey::build(k, &LeadContract::default());
    //let lead_vk = VerifyingKey::build(k, &LeadContract::default());
    //
    const LEN: usize = 10;
    let epoch_item = EpochItem {
        value: 0,  //static stake value
    };

    //TODO to read eta you need an access to the blockchain proof transaction.
    //need an emulation of the stakeholder as a node
    //TODO who should have view of the blockchain if not the stakeholder?
    // or should the blockchain be a node in itself?
    // but that doesn't make sense, since each stakeholder might end up with different view of it.
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
    //let proof = create_lead_proof(lead_pk.clone(), coin.clone()).unwrap();
    //verify_lead_proof(&lead_vk, &proof, coin);

    // calculate public inputs
    let public_inputs = coin.public_inputs();

    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    //
    assert_eq!(prover.verify(), Ok(()));
    //
}
