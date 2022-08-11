use halo2_proofs::{arithmetic::Field, dev::MockProver};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

use futures::executor::block_on;

use darkfi::crypto::proof::VerifyingKey;
use darkfi::crypto::proof::ProvingKey;
use url::Url;

use darkfi::{
    blockchain::{
        Blockchain,
        EpochConsensus,
        epoch::{Epoch,EpochItem},
    },
    stakeholder::stakeholder::{Stakeholder},
    util::time::{Timestamp},
    crypto::{
        constants::MERKLE_DEPTH_ORCHARD,
        leadcoin::LeadCoin,
        lead_proof,
        merkle_node::MerkleNode,
    },
    tx::Transaction,
    consensus::{TransactionLeadProof, Metadata, StreamletMetadata, BlockInfo},
    net::{P2p,Settings, SettingsPtr,},
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
    let settings = Settings{
        inbound: Some(Url::parse("tls://127.0.0.1:12002").unwrap()),
        outbound_connections: 4,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        external_addr: Some(Url::parse("tls://127.0.0.1:12002").unwrap()),
        peers: [Url::parse("tls://127.0.0.1:12003").unwrap()].to_vec(),
        seeds: [Url::parse("tls://irc0.dark.fi:11001").unwrap(),
                Url::parse("tls://irc1.dark.fi:11001").unwrap()
        ].to_vec(),
    };
    let consensus = EpochConsensus::new(Some(22), Some(3), Some(22), Some(0));

    let stakeholder : Stakeholder = block_on(Stakeholder::new(consensus, settings, Some(k))).unwrap();

    let eta : pallas::Base = stakeholder.get_eta();
    let mut epoch = Epoch {
        len: Some(LEN),
        item: Some(epoch_item),
        eta: eta,
        coins: vec![],
    };
    let coins: Vec<LeadCoin> = epoch.create_coins();
    let coin_idx = 0;
    let coin = coins[coin_idx];
    let contract = coin.create_contract();

    /*
    let lead_pk = stakeholder.get_provkingkey();
    let lead_vk = stakeholder.get_verifyingkey();


    //
    //let proof = lead_proof::create_lead_proof(lead_pk.clone(), coin.clone());

    //TODO (fix) proof panics
    let lead_tx = TransactionLeadProof::new(lead_pk, coin.clone());

    //lead_tx.verify(lead_vk, coin);

    let (st_id, st_hash)  = stakeholder.blockchain.last().unwrap();
    let empty_txs : Vec<Transaction> = vec!();
    let metadata = Metadata::new(Timestamp::current_time(), epoch.eta.to_repr(), lead_tx);
    let sm = StreamletMetadata::new(vec!());
    let bk_info = BlockInfo::new(st_hash, 1, 0, empty_txs, metadata, sm);
    let blks = [bk_info];
    stakeholder.blockchain.add(&blks);
    */
    // calculate public inputs
    let public_inputs = coin.public_inputs();
    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
