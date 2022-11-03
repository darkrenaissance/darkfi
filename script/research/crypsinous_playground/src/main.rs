/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use async_std::sync::Arc;
use log::{error, info};
use pasta_curves::pallas;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

use darkfi::{
    async_daemonize, cli_desc,
    consensus::coins,
    crypto::{
        lead_proof,
        proof::{ProvingKey, VerifyingKey},
    },
    node::Client,
    wallet::walletdb::init_wallet,
    zk::circuit::LeadContract,
    Result,
};

const CONFIG_FILE: &str = "crypsinous_playground_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../crypsinous_playground_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "crypsinous_playground", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.config/darkfi/crypsinous_playground/wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(short, default_value = "1")]
    /// How many epochs to simulate
    epochs: u64,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

// The porpuse of this script is to simulate a staker actions through an epoch.
// Main focus is the crypsinous lottery mechanism and the leader proof creation and validation.
// Other flows that happen through a slot, like broadcasting blocks or syncing are out of scope.
async_daemonize!(realmain);
async fn realmain(args: Args, _ex: Arc<smol::Executor<'_>>) -> Result<()>  {

    // Epochs sanity check
    let epochs = args.epochs;
    if epochs < 1 {
        error!("Epochs must be a positive number.");
        return Ok(());
    }
    info!("Simulation epochs: {}", epochs);

    // Initialize wallet that holds coins for staking
    let wallet = init_wallet(&args.wallet_path, &args.wallet_pass).await?;

    // Initialize client
    let client = Arc::new(Client::new(wallet.clone()).await?);

    // Retrieving nodes wallet coins
    let mut owned = client.get_own_coins().await?;
    // If node holds no coins in its wallet, we generate some new staking coins
    if owned.is_empty() {
        info!("Node wallet is empty, generating new staking coins...");
        owned = coins::generate_staking_coins(&wallet).await?;
    }
    // If we want to test what will happen if node holds 0 coins, uncomment the below line
    // owned = vec![];
    info!("Node coins: {:?}", owned);

    // Generating leader proof keys
    let k: u32 = 11; // Proof rows number
    info!("Generating proof keys with k: {}", k);
    let proving_key = ProvingKey::build(k, &LeadContract::default());
    let verifying_key = VerifyingKey::build(k, &LeadContract::default());

    // Simulating epochs with 10 slots
    for epoch in 0..epochs {
        info!("Epoch {} started!", epoch);
        // Generating epoch coins
        // TODO: Retrieve previous lead proof
        let eta = pallas::Base::one();
        let epoch_coins = coins::create_epoch_coins(eta, &owned, epoch, 0);
        info!("Generated epoch_coins: {}", epoch_coins.len());
        for slot in 0..10 {
            // Checking if slot leader
            info!("Slot {} started!", slot);
            let (won, idx) = coins::is_leader(slot, &epoch_coins);
            info!("Lottery outcome: {}", won);
            if !won {
                continue
            }
            // TODO: Generate rewards transaction
            info!("Winning coin index: {}", idx);
            // Generating leader proof
            let coin = epoch_coins[slot as usize][idx];
            let proof = lead_proof::create_lead_proof(&proving_key, coin);
            if proof.is_err() {
                error!("Error during leader proof creation: {}", proof.err().unwrap());
                continue
            }
            //Verifying generated proof against winning coin public inputs
            info!("Leader proof generated successfully, veryfing...");
            match lead_proof::verify_lead_proof(&verifying_key, &proof.unwrap(), &coin.public_inputs()) {
                Ok(_) => info!("Proof veryfied succsessfully!"),
                Err(e) => error!("Error during leader proof verification: {}", e),
            }
        }
    }

    Ok(())
}
