use crate::blockchain::Slab;
use crate::crypto::OwnCoins;
use crate::serial::Encodable;
use crate::state::ProgramState;
use crate::tx;
use crate::Result;

use bls12_381::Bls12;

use std::path::{Path, PathBuf};

pub fn join_config_path(file: &PathBuf) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    let dfi_path = Path::new("darkfi");

    match dirs::config_dir() {
        Some(v) => path.push(v),
        // This should not fail on any modern OS
        None => {}
    }

    path.push(dfi_path);
    path.push(file);

    Ok(path)
}

pub fn prepare_transaction(
    _state: &dyn ProgramState,
    secret: jubjub::Fr,
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    address: jubjub::SubgroupPoint,
    amount: f64,
    own_coins: OwnCoins,
) -> Result<Slab> {
    let witness = &own_coins[0].3;
    let merkle_path = witness.path().unwrap();

    // Construct a new tx spending the coin
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![],
        inputs: vec![tx::TransactionBuilderInputInfo {
            merkle_path,
            secret: secret.clone(),
            note: own_coins[0].1.clone(),
        }],
        // We can add more outputs to this list.
        // The only constraint is that sum(value in) == sum(value out)
        outputs: vec![tx::TransactionBuilderOutputInfo {
            value: amount as u64,
            asset_id: 1,
            public: address,
        }],
    };
    // Build the tx
    let mut tx_data = vec![];
    {
        let tx = builder.build(&mint_params, &spend_params);
        tx.encode(&mut tx_data).expect("encode tx");
    }

    // build slab from the transaction
    let slab = Slab::new(tx_data);
    return Ok(slab);
}
