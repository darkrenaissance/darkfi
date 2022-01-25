use halo2::{
    arithmetic::{CurveAffine, Field},
    dev::MockProver,
};
use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use pasta_curves::{group::Curve, pallas};
use rand::rngs::OsRng;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::MintRevealedValues,
        spend_proof::SpendRevealedValues,
    },
    zk::vm2::{WitnessVar, ZkCircuit},
    zkas::decoder::ZkBinary,
    Result,
};

fn mint_proof() -> Result<()> {
    let bincode = include_bytes!("../proofs/mint.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    let value = 42;
    let token_id = pallas::Base::from(22);
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let token_blind = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);
    let public_key = PublicKey::random(&mut OsRng);

    let revealed = MintRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        public_key,
    );

    let pk_coords = public_key.0.to_affine().coordinates().unwrap();
    let witnesses = vec![
        WitnessVar::Base(*pk_coords.x()),
        WitnessVar::Base(*pk_coords.y()),
        WitnessVar::Base(pallas::Base::from(value)),
        WitnessVar::Base(token_id),
        WitnessVar::Base(serial),
        WitnessVar::Base(coin_blind),
        WitnessVar::Scalar(value_blind),
        WitnessVar::Scalar(token_blind),
    ];

    let circuit = ZkCircuit::new(witnesses, revealed.make_outputs().to_vec(), zkbin);
    let prover = MockProver::run(11, &circuit, vec![revealed.make_outputs().to_vec()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    Ok(())
}

fn burn_proof() -> Result<()> {
    let bincode = include_bytes!("../proofs/burn.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    let value = 42;
    let token_id = pallas::Base::from(22);
    let value_blind = pallas::Scalar::random(&mut OsRng);
    let token_blind = pallas::Scalar::random(&mut OsRng);
    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);
    let secret = SecretKey::random(&mut OsRng);
    let sig_secret = SecretKey::random(&mut OsRng);

    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);

    let random_coin_1 = pallas::Base::random(&mut OsRng);
    tree.append(&MerkleNode(random_coin_1));
    tree.witness();
    let random_coin_2 = pallas::Base::random(&mut OsRng);
    tree.append(&MerkleNode(random_coin_2));

    let coin = {
        let coords = PublicKey::from_secret(secret).0.to_affine().coordinates().unwrap();
        let messages =
            [*coords.x(), *coords.y(), pallas::Base::from(value), token_id, serial, coin_blind];

        poseidon::Hash::init(P128Pow5T3, ConstantLength::<6>).hash(messages)
    };

    tree.append(&MerkleNode(coin));
    tree.witness();

    let random_coin_3 = pallas::Base::random(&mut OsRng);
    tree.append(&MerkleNode(random_coin_3));
    tree.witness();

    let (leaf_position, merkle_path) = tree.authentication_path(&MerkleNode(coin)).unwrap();

    let revealed = SpendRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        secret,
        leaf_position,
        merkle_path.clone(),
        sig_secret,
    );

    // Why are these types not matched in halo2 gadgets?
    let leaf_pos: u64 = leaf_position.into();
    let leaf_pos = leaf_pos as u32;

    let witnesses = vec![
        WitnessVar::Base(secret.0),
        WitnessVar::Base(serial),
        WitnessVar::Base(pallas::Base::from(value)),
        WitnessVar::Base(token_id),
        WitnessVar::Base(coin_blind),
        WitnessVar::Scalar(value_blind),
        WitnessVar::Scalar(token_blind),
        WitnessVar::Uint32(leaf_pos),
        WitnessVar::MerklePath(merkle_path),
        WitnessVar::Base(sig_secret.0),
    ];

    let circuit = ZkCircuit::new(witnesses, revealed.make_outputs().to_vec(), zkbin);
    let prover = MockProver::run(11, &circuit, vec![revealed.make_outputs().to_vec()])?;
    assert_eq!(prover.verify(), Ok(()));

    Ok(())
}

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    mint_proof()?;
    burn_proof()?;

    Ok(())
}
