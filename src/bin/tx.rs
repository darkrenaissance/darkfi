use std::io;
use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Group;
use rand::rngs::OsRng;

use sapvi::crypto::{
    create_mint_proof, load_params, save_params, setup_mint_prover, verify_mint_proof,
    MintRevealedValues,
    note::Note
};
use sapvi::serial::{Decodable, Encodable, VarInt};
use sapvi::error::{Error, Result};
use sapvi::tx;

fn txbuilding() {
    {
        let params = setup_mint_prover();
        save_params("mint.params", &params);
    }
    let (mint_params, mint_pvk) = load_params("mint.params").expect("params should load");

    let public = jubjub::SubgroupPoint::random(&mut OsRng);

    let builder = tx::TransactionBuilder {
        clear_inputs: vec![tx::TransactionBuilderClearInputInfo { value: 110 }],
        outputs: vec![tx::TransactionBuilderOutputInfo { value: 110, public }],
    };

    let mut tx_data = vec![];
    {
        let tx = builder.build(&mint_params);
        tx.encode(&mut tx_data).expect("encode tx");
    }
    {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();
        assert!(tx.verify(&mint_pvk));
    }
}

fn main() {
    txbuilding();
    /*let note = Note {
        serial: jubjub::Fr::random(&mut OsRng),
        value: 110,
        coin_blind: jubjub::Fr::random(&mut OsRng),
        valcom_blind: jubjub::Fr::random(&mut OsRng),
    };

    let secret = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let encrypted_note = note.encrypt(&public).unwrap();
    let note2 = encrypted_note.decrypt(&secret).unwrap();
    assert_eq!(note.value, note2.value);*/
}

