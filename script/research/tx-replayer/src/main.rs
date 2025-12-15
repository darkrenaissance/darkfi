use clap::Parser;
use darkfi::{
    blockchain::{Blockchain, BlockchainOverlay, BlockchainOverlayPtr},
    cli_desc,
    util::path::expand_path,
    validator::verification::verify_transaction,
    zk::VerifyingKey,
};
use darkfi_sdk::{crypto::MerkleTree, tx::TransactionHash};
use std::collections::HashMap;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long)]
    database_path: String,
    #[arg(short, long)]
    tx_hash: String,
}

fn main() {
    smol::block_on(async {
        let args = Args::parse();
        replay_tx(args).await;
    });
}

async fn replay_tx(args: Args) {
    let db_path = expand_path(&args.database_path).unwrap();
    let sled_db = sled_overlay::sled::open(&db_path).unwrap();

    let blockchain = Blockchain::new(&sled_db).unwrap();
    let txh: TransactionHash = args.tx_hash.parse().unwrap();
    let tx = blockchain.transactions.get(&[txh], true).unwrap().first().unwrap().clone().unwrap();

    let (overlay, new_height) = rollback_database(&blockchain, txh).await;

    let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();
    for call in &tx.calls {
        vks.insert(call.data.contract_id.to_bytes(), HashMap::new());
    }

    let result =
        verify_transaction(&overlay, new_height, 2, &tx, &mut MerkleTree::new(1), &mut vks, true)
            .await
            .unwrap();

    println!("Verify Transaction Result: {:?}", result);
}

/// Resets the blockchain in memory to a height before the transaction.
async fn rollback_database(
    blockchain: &Blockchain,
    txh: TransactionHash,
) -> (BlockchainOverlayPtr, u32) {
    let (tx_height, _) =
        blockchain.transactions.get_location(&[txh], true).unwrap().first().unwrap().unwrap();

    let new_height = tx_height - 1;
    println!("Rolling back database to Height: {new_height}");

    let (last, _) = blockchain.last().unwrap();
    let heights: Vec<u32> = (new_height + 1..=last).rev().collect();
    let inverse_diffs = blockchain.blocks.get_state_inverse_diff(&heights, true).unwrap();

    let overlay = BlockchainOverlay::new(blockchain).unwrap();

    let overlay_lock = overlay.lock().unwrap();
    let mut lock = overlay_lock.overlay.lock().unwrap();
    for inverse_diff in inverse_diffs {
        let inverse_diff = inverse_diff.unwrap();
        lock.add_diff(&inverse_diff).unwrap();
    }
    drop(lock);
    drop(overlay_lock);

    (overlay, new_height)
}
