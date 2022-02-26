use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
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
    crypto::{keypair::Keypair, merkle_node::MerkleNode, schnorr::SchnorrSecret},
    Result,
};

fn main() -> Result<()> {
    let loglevel = match option_env!("RUST_LOG") {
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        Some(_) | None => LevelFilter::Info,
    };
    TermLogger::init(loglevel, Config::default(), Mixed, Auto)?;

    // =============
    // Initial state
    // =============

    // We have a Merkle tree of some DAO treasuries
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    for _ in 0..3 {
        let some_dao = pallas::Base::random(&mut OsRng);
        tree.append(&MerkleNode(some_dao));
    }

    // And we have our DAO
    let dao_authority = Keypair::random(&mut OsRng);

    let spend_contract = pallas::Base::random(&mut OsRng);
    let current_balance = pallas::Base::from(666);
    let current_serial = pallas::Base::random(&mut OsRng);
    let current_dao_blind = pallas::Base::random(&mut OsRng);

    let message = [spend_contract, current_balance, current_serial, current_dao_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let our_dao = hasher.hash(message);

    tree.append(&MerkleNode(our_dao));
    tree.witness();

    // ========
    // Proposal
    // ========

    // We make a proposal to send funds to some public key with the following
    // parameters:
    let amount_to_send = pallas::Base::from(42);
    let destination = pallas::Point::random(&mut OsRng);
    let destination_coords = destination.to_affine().coordinates().unwrap();
    let proposal_blind = pallas::Base::random(&mut OsRng);

    // This proposal is then hashed, and the hash is signed by the DAO authority:
    let message =
        [amount_to_send, *destination_coords.x(), *destination_coords.y(), proposal_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let proposal = hasher.hash(message);

    let _signature = dao_authority.secret.sign(&proposal.to_repr());

    // ======
    // Voting
    // ======

    // Once the proposal is live, voting can become active.
    // Users can now vote on the proposal and their votes are weighted by the
    // balance they commit to their vote.
    // After the voting process is done, the votes and the blinds are revealed.

    let vote0 = pallas::Base::from(44);
    let _vote0_blind = pallas::Scalar::random(&mut OsRng);

    let vote1 = pallas::Base::from(14);
    let _vote1_blind = pallas::Scalar::random(&mut OsRng);

    let vote2 = -pallas::Base::from(39); // This is a NO vote
    let _vote2_blind = pallas::Scalar::random(&mut OsRng);

    // Now let's consider the voting is done, and the votes are revealed.
    let votes = vote0 + vote1 + vote2;

    if votes < pallas::Base::from(1) {
        // In case the sum of the votes is negative, it means that the proposal
        // has not passed, therefore we don't do anything.
        return Ok(())
    }

    // ==================
    // Proposal execution
    // ==================

    // The remaining funds in the DAO become the next treasury, so we append
    // it to the DAO Merkle tree:
    let new_balance = current_balance - amount_to_send;
    let new_serial = pallas::Base::random(&mut OsRng);
    let new_dao_blind = pallas::Base::random(&mut OsRng);

    let message = [spend_contract, new_balance, new_serial, new_dao_blind];
    let hasher = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init();
    let our_new_dao = hasher.hash(message);

    tree.append(&MerkleNode(our_new_dao));
    tree.witness();

    Ok(())
}
