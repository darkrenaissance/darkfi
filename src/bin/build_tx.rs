use drk::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{CashierdConfig, Config, DarkfidConfig},
    client::State,
    crypto::{
        load_params, merkle::CommitmentTree, save_params, setup_mint_prover, setup_spend_prover,
    },
    serial::deserialize,
    state::state_transition,
    tx,
    util::{expand_path, join_config_path},
    wallet::WalletDb,
    Result,
};

use std::path::PathBuf;

use async_std;
use async_std::sync::{Arc, Mutex};
use ff::Field;
use rand::rngs::OsRng;

#[async_std::main]
async fn main() -> Result<()> {
    let config: DarkfidConfig =
        Config::<DarkfidConfig>::load(join_config_path(&PathBuf::from("darkfid.toml"))?)?;

    let config_cashier: CashierdConfig =
        Config::<CashierdConfig>::load(join_config_path(&PathBuf::from("cashierd.toml"))?)?;

    let mut public_keys = Vec::new();

    for cashier in config.clone().cashiers {
        let cashier_public: jubjub::SubgroupPoint =
            deserialize(&bs58::decode(cashier.cashier_public_key).into_vec()?)?;
        public_keys.push(cashier_public);
    }

    let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let params_paths = (
        expand_path(&config.mint_params_path.clone())?,
        expand_path(&config.spend_params_path.clone())?,
    );

    let mint_params_path = &params_paths.0.to_str().unwrap_or("mint.params");
    let spend_params_path = &params_paths.1.to_str().unwrap_or("spend.params");

    let cashier_params_paths = (
        expand_path(&config_cashier.mint_params_path.clone())?,
        expand_path(&config_cashier.spend_params_path.clone())?,
    );

    let cashier_mint_params_path = &cashier_params_paths
        .0
        .to_str()
        .unwrap_or("cash_mint.params");
    let cashier_spend_params_path = &cashier_params_paths
        .1
        .to_str()
        .unwrap_or("cash_spend.params");

    if !params_paths.0.exists() {
        let params = setup_mint_prover();
        save_params(mint_params_path, &params)?;
    }
    if !params_paths.1.exists() {
        let params = setup_spend_prover();
        save_params(spend_params_path, &params)?;
    }

    if !cashier_params_paths.0.exists() {
        let params = setup_mint_prover();
        save_params(cashier_mint_params_path, &params)?;
    }
    if !cashier_params_paths.1.exists() {
        let params = setup_spend_prover();
        save_params(cashier_spend_params_path, &params)?;
    }

    let cashier_wallet = WalletDb::new(
        expand_path(&config_cashier.client_wallet_path.clone())?.as_path(),
        config.wallet_password.clone(),
    )?;

    let wallet = WalletDb::new(
        expand_path(&config.wallet_path.clone())?.as_path(),
        config.wallet_password.clone(),
    )?;

    wallet.init_db().await?;

    if wallet.get_keypairs()?.is_empty() {
        wallet.key_gen()?;
    }

    cashier_wallet.init_db().await?;

    if cashier_wallet.get_keypairs()?.is_empty() {
        cashier_wallet.key_gen()?;
    }

    let user_main_keypair = wallet.get_keypairs()?[0].clone();

    let cashier_main_keypair = cashier_wallet.get_keypairs()?[0].clone();

    wallet.put_cashier_pub(&cashier_main_keypair.public)?;

    // Load trusted setup parameters
    let (_, mint_pvk) = load_params(mint_params_path)?;
    let (_, spend_pvk) = load_params(spend_params_path)?;

    // Load trusted setup parameters
    let (cashier_mint_params, _) = load_params(cashier_mint_params_path)?;
    let (cashier_spend_params, _) = load_params(cashier_spend_params_path)?;

    //
    //
    //
    // user's state
    //
    //
    //
    let state = Arc::new(Mutex::new(State {
        tree: CommitmentTree::empty(),
        merkle_roots,
        nullifiers,
        mint_pvk,
        spend_pvk,
        wallet,
        public_keys,
    }));

    //
    //
    //
    // cashier buid tx
    //
    //
    //
    let mut clear_inputs: Vec<tx::TransactionBuilderClearInputInfo> = vec![];
    let inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
    let mut outputs: Vec<tx::TransactionBuilderOutputInfo> = vec![];

    let token_id = jubjub::Fr::random(&mut OsRng);
    let value = 10;

    let signature_secret = cashier_main_keypair.private;
    let input = tx::TransactionBuilderClearInputInfo {
        value,
        token_id,
        signature_secret,
    };

    clear_inputs.push(input);

    outputs.push(tx::TransactionBuilderOutputInfo {
        value,
        token_id,
        public: user_main_keypair.public,
    });

    let builder = tx::TransactionBuilder {
        clear_inputs,
        inputs,
        outputs,
    };

    let tx = builder.build(&cashier_mint_params, &cashier_spend_params);

    //
    //
    //
    // user get the tx
    //
    //
    //
    let state = state.lock().await;
    let update = state_transition(&state, tx);

    if let Err(e) = update {
        println!("state transition error: {}", e.to_string());
    }

    Ok(())
}
