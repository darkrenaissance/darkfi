use log::{error, info};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    crypto::{
        lead_proof,
        proof::{ProvingKey, VerifyingKey},
    },
    zk::circuit::LeadContract,
};

mod coins;
mod utils;

/// The porpuse of this script is to simulate a staker actions through an epoch.
/// Main focus is the crypsinous lottery mechanism and the leader proof creation and validation.
/// Other flows that happen through a slot, like broadcasting blocks or syncing are out of scope.
fn main() {
    // Initiate logger
    TermLogger::init(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .unwrap();
    
    // Generating leader proof keys    
    let k: u32 = 13; // Proof rows number
    info!("Generating proof keys with k: {}", k);
    let proving_key = ProvingKey::build(k, &LeadContract::default());
    let verifying_key = VerifyingKey::build(k, &LeadContract::default());
    
    // Simulating an epoch with 10 slots
    let epoch = 0;
    let slot = 0;
    info!("Epoch {} started!", epoch);
    
    // Generating epoch coins
    // TODO: Retrieve own coins
    let owned = vec![];
    // TODO: Retrieve previous lead proof
    let eta = utils::get_eta(blake3::hash(b"Erebus"));    
    let epoch_coins = coins::create_epoch_coins(eta, &owned, epoch, slot);
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
