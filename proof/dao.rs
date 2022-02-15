use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use halo2_proofs::dev::MockProver;
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{
        ff::{Field, PrimeField},
        Curve, Group,
    },
    pallas,
};
use rand::rngs::OsRng;
use simplelog::{ColorChoice::Auto, Config, LevelFilter, TermLogger, TerminalMode::Mixed};

use darkfi::{
    crypto::{
        keypair::Keypair,
        merkle_node::MerkleNode,
        schnorr::SchnorrSecret,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    zk::vm::{Witness, ZkCircuit},
    zkas::decoder::ZkBinary,
    Result,
};

fn main() -> Result<()> {
    let loglevel = match option_env!("RUST_LOG") {
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        Some(_) | None => LevelFilter::Info,
    };
    TermLogger::init(loglevel, Config::default(), Mixed, Auto)?;

    let bincode = include_bytes!("dao.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // =============
    // Initial state
    // =============
    let authority = Keypair::random(&mut OsRng);

    let spend_contract = pallas::Base::random(&mut OsRng);
    let cur_balance = pallas::Base::from(666);
    let old_serial = pallas::Base::random(&mut OsRng);
    let old_bulla_blind = pallas::Base::random(&mut OsRng);

    let message = [spend_contract, cur_balance, old_serial, old_bulla_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let our_dao = hasher.hash(message);

    // Merkle tree of DAOs
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    let dao0 = pallas::Base::random(&mut OsRng);
    let dao2 = pallas::Base::random(&mut OsRng);

    tree.append(&MerkleNode(dao0));
    tree.witness();

    tree.append(&MerkleNode(our_dao));
    tree.witness();

    tree.append(&MerkleNode(dao2));
    tree.witness();

    // ========
    // Proposal
    // ========
    let amount_to_send = pallas::Base::from(42);
    let proposal_destination = pallas::Point::random(&mut OsRng);
    let proposal_coords = proposal_destination.to_affine().coordinates().unwrap();
    let proposal_blind = pallas::Base::random(&mut OsRng);

    let message = [amount_to_send, *proposal_coords.x(), *proposal_coords.y(), proposal_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let proposal = hasher.hash(message);

    // Sign the proposal by the authority
    let _signature = authority.secret.sign(&proposal.to_repr());

    // ==============
    // Voting process
    // ==============
    // The voting process happens now, and when finished, the votes are revealed.
    // Votes are weighted by balance.

    let vote0 = pallas::Base::from(44);
    let vote0_blind = pallas::Scalar::random(&mut OsRng);

    let vote1 = pallas::Base::from(13);
    let vote1_blind = pallas::Scalar::random(&mut OsRng);

    let vote2 = -pallas::Base::from(49); // This is a NO vote
    let vote2_blind = pallas::Scalar::random(&mut OsRng);

    let votes = vote0 + vote1 + vote2;
    let vote_blinds = vote0_blind + vote1_blind + vote2_blind;

    if votes < pallas::Base::from(1) {
        // The voting process result is negative, so we don't do anything.
        return Ok(())
    }

    // ==================
    // Proof construction
    // ==================
    let (leaf_pos, merkle_path) = tree.authentication_path(&MerkleNode(our_dao)).unwrap();
    let leaf_pos: u64 = leaf_pos.into();
    let leaf_pos = leaf_pos as u32;

    let new_serial = pallas::Base::random(&mut OsRng);
    let new_bulla_blind = pallas::Base::random(&mut OsRng);

    let new_balance = cur_balance - amount_to_send;

    let message = [spend_contract, new_balance, new_serial, new_bulla_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let new_bulla = hasher.hash(message);

    let merkle_root = tree.root();

    let value_blind = pallas::Scalar::random(&mut OsRng);
    let value_commit = pedersen_commitment_scalar(mod_r_p(amount_to_send), value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        spend_contract,
        old_serial,
        merkle_root.0,
        proposal,
        *value_coords.x(),
        *value_coords.y(),
        new_bulla,
    ];

    let prover_witnesses = vec![
        Witness::Base(Some(spend_contract)),
        Witness::Base(Some(cur_balance)),
        Witness::Base(Some(old_serial)),
        Witness::Base(Some(old_bulla_blind)),
        Witness::Uint32(Some(leaf_pos)),
        Witness::MerklePath(Some(merkle_path.try_into().unwrap())),
        Witness::Base(Some(amount_to_send)),
        Witness::Base(Some(*proposal_coords.x())),
        Witness::Base(Some(*proposal_coords.y())),
        Witness::Base(Some(proposal_blind)),
        Witness::Scalar(Some(value_blind)),
        Witness::Base(Some(new_serial)),
        Witness::Base(Some(new_bulla_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let prover = MockProver::<pallas::Base>::run(11, &circuit, vec![public_inputs])?;
    assert_eq!(prover.verify(), Ok(()));

    Ok(())
}
