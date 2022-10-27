use futures::executor::block_on;
use halo2_proofs::dev::MockProver;
use pasta_curves::pallas;
use url::Url;

use darkfi::{
    consensus::ouroboros::{Epoch, EpochConsensus, Stakeholder},
    crypto::{
        lead_proof,
        leadcoin::{LeadCoin, LEAD_PUBLIC_INPUT_LEN},
    },
    net::Settings,
};

fn main() {
    env_logger::init();

    let k: u32 = 13;
    //

    let _value = 33223; //static stake value

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
    let mut epoch = Epoch::new(consensus, eta);
    let sigma = pallas::Base::from(10);
    let coins: Vec<Vec<LeadCoin>> = epoch.create_coins(sigma.clone(), sigma, vec![]);
    let coin = coins[0][0];
    let contract = coin.create_contract();

    let public_inputs: [pallas::Base; LEAD_PUBLIC_INPUT_LEN] = coin.public_inputs_as_array();

    let lead_pk = stakeholder.get_leadprovkingkey();
    let lead_vk = stakeholder.get_leadverifyingkey();

    let proof = lead_proof::create_lead_proof(&lead_pk.clone(), coin.clone()).unwrap();
    lead_proof::verify_lead_proof(&lead_vk, &proof, &public_inputs);

    let prover = MockProver::run(k, &contract, vec![public_inputs.to_vec()]).unwrap();
    prover.assert_satisfied();
    assert_eq!(prover.verify(), Ok(()));
}
