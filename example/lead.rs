use halo2_proofs::{arithmetic::Field, dev::MockProver};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

use futures::executor::block_on;

use darkfi::crypto::proof::{ProvingKey, VerifyingKey};
use url::Url;

use darkfi::{
    blockchain::{
        epoch::{Epoch, EpochItem},
        Blockchain, EpochConsensus,
    },
    consensus::{BlockInfo, StakeholderMetadata, StreamletMetadata, TransactionLeadProof},
    crypto::{
        constants::MERKLE_DEPTH_ORCHARD,
        lead_proof,
        leadcoin::{LeadCoin, LEAD_PUBLIC_INPUT_LEN},
        merkle_node::MerkleNode,
    },
    net::{P2p, Settings, SettingsPtr},
    stakeholder::stakeholder::Stakeholder,
    tx::Transaction,
    util::time::Timestamp,
    zk::circuit::lead_contract::LeadContract,
};

fn main() {
    let k: u32 = 13;
    //

    //
    const LEN: usize = 10;
    let epoch_item = EpochItem {
        value: 332233,  //static stake value
    };
    //
    let settings = Settings {
        inbound: vec![Url::parse("tls://127.0.0.1:12002").unwrap()],
        outbound_connections: 4,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        external_addr: vec![Url::parse("tls://127.0.0.1:12002").unwrap()],
        peers: [Url::parse("tls://127.0.0.1:12003").unwrap()].to_vec(),
        seeds: [
            Url::parse("tls://irc0.dark.fi:11001").unwrap(),
            Url::parse("tls://irc1.dark.fi:11001").unwrap(),
        ]
        .to_vec(),
        ..Default::default()
    };
    let consensus = EpochConsensus::new(Some(22), Some(3), Some(22), Some(0));

    let stakeholder: Stakeholder =
        block_on(Stakeholder::new(consensus, settings, "db", 0, Some(k))).unwrap();

    let eta: pallas::Base = stakeholder.get_eta();
    let mut epoch = Epoch { len: Some(LEN), item: Some(epoch_item), eta, coins: vec![] };
    // sigma is nubmer of slots * reward (assuming reward is 1 for simplicity)
    let sigma = pallas::Base::from(10);
    let coins: Vec<LeadCoin> = epoch.create_coins(sigma);
    let coin_idx = 0;
    let coin = coins[coin_idx];
    let contract = coin.create_contract();

    let public_inputs: [pallas::Base; LEAD_PUBLIC_INPUT_LEN] = coin.public_inputs_as_array();

    let lead_pk = stakeholder.get_provkingkey();
    let lead_vk = stakeholder.get_verifyingkey();

    //let proof = lead_proof::create_lead_proof(&lead_pk.clone(), coin.clone()).unwrap();
    //lead_proof::verify_lead_proof(&lead_vk, &proof, &public_inputs);

    let prover = MockProver::run(k, &contract, vec![public_inputs.to_vec()]).unwrap();
    prover.assert_satisfied();
    //assert_eq!(prover.verify(), Ok(()));
}
