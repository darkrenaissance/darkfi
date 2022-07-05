use halo2_proofs::{arithmetic::Field, dev::MockProver, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

use darkfi::{
    blockchain::epoch::{Epoch,EpochItem},
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
    let epoch_item = EpochItem{
        value: 1,  //static stake value
    };

    //TODO to read eta you need an access to the blockchain proof transaction.
    //need an emulation of the stakeholder as a node
    //TODO who should have view of the blockchain if not the stakeholder?
    // or should the blockchain be a node in itself?
    // but that doesn't make sense, since each stakeholder might end up with different view of it.
    let genesis_data = flake3::Hash(b"");
    let oc = Blockchain::new(sled_db, Timestamp::curent_time(), genesis_data);
    let stakeholder = Stakeholder
    {
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

    let contract = LeadContract {
        path: Value::known(coin.path.unwrap()),
        coin_pk_x: Value::known(coin.pk_x.unwrap()),
        coin_pk_y: Value::known(coin.pk_y.unwrap()),
        root_sk: Value::known(coin.root_sk.unwrap()),
        sf_root_sk: Value::known(mod_r_p(coin.root_sk.unwrap())),
        path_sk: Value::known(coin.path_sk.unwrap()),
        coin_timestamp: Value::known(coin.tau.unwrap()), //
        coin_nonce: Value::known(coin.nonce.unwrap()),
        coin1_blind: Value::known(coin.c1_blind.unwrap()),
        value: Value::known(coin.value.unwrap()),
        coin2_blind: Value::known(coin.c2_blind.unwrap()),
        cm_pos: Value::known(coin.idx),
        //sn_c1: Value::known(coin.sn.unwrap()),
        slot: Value::known(coin.sl.unwrap()),
        mau_rho: Value::known(mod_r_p(coin.rho_mu)),
        mau_y: Value::known(mod_r_p(coin.y_mu)),
        root_cm: Value::known(coin.root_cm.unwrap()),
    };

    //let proof = create_lead_proof(lead_pk.clone(), coin.clone()).unwrap();
    //verify_lead_proof(&lead_vk, &proof, coin);

    // calculate public inputs
    let public_inputs = coin.public_inputs();

    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    //
    assert_eq!(prover.verify(), Ok(()));
    //
}
