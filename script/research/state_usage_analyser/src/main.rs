use async_std::sync::{Arc, Mutex};
use incrementalmerkletree::bridgetree::BridgeTree;
use lazy_init::Lazy;
use pasta_curves::{group::ff::Field, pallas::Base};
use rand::rngs::OsRng;
use serde::ser::{Serialize, SerializeStruct, Serializer};

use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::{
        constants::MERKLE_DEPTH,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
    },
    node::{
        state::{state_transition, ProgramState},
        Client, MemoryState, State,
    },
    util::{cli::progress_bar, expand_path},
    wallet::walletdb::init_wallet,
};

// Auxiliary struct to Serialize BridgeTree
struct BridgeTreeWrapper {
    tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
}

impl Serialize for BridgeTreeWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BridgeTreeWrapper", 1)?;
        state.serialize_field("tree", &self.tree)?;
        state.end()
    }
}

pub fn bridge_tree_usage(prefix: &str, tree: BridgeTree<MerkleNode, MERKLE_DEPTH>) {
    let wrapper = BridgeTreeWrapper { tree };
    let encoded: Vec<u8> =
        bincode::serde::encode_to_vec(&wrapper, bincode::config::legacy()).unwrap();
    let size = ::std::mem::size_of_val(&*encoded);
    println!("  {} size: {:?} Bytes", prefix, size);
}

#[async_std::main]
async fn main() -> darkfi::Result<()> {
    // Config
    let folder = "~/.config/darkfi/merkle_testing";
    let genesis_ts = *TESTNET_GENESIS_TIMESTAMP;
    let genesis_data = *TESTNET_GENESIS_HASH_BYTES;
    let pass = "changeme";

    // Initialize
    let pb = progress_bar("Initializing wallet...");
    let path = folder.to_owned() + "/wallet.db";
    let wallet = init_wallet(&path, &pass).await?;
    let client = Arc::new(Client::new(wallet.clone()).await?);
    let address = wallet.get_default_address().await?;
    let pubkey = PublicKey::try_from(address)?;
    pb.finish();

    let pb = progress_bar("Initializing sled database...");
    let path = folder.to_owned() + "/blockchain/testnet";
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(&db_path)?;
    pb.finish();

    let pb = progress_bar("Generating state machine...");
    let blockchain = Blockchain::new(&sled_db, genesis_ts, genesis_data)?;
    let state_machine = State {
        tree: client.get_tree().await?,
        merkle_roots: blockchain.merkle_roots.clone(),
        nullifiers: blockchain.nullifiers.clone(),
        cashier_pubkeys: vec![],
        faucet_pubkeys: vec![pubkey.clone()],
        mint_vk: Lazy::new(),
        burn_vk: Lazy::new(),
    };
    pb.finish();

    let pb = progress_bar("Creating zk proof verification keys...");
    let _ = state_machine.mint_vk();
    let _ = state_machine.burn_vk();
    pb.finish();

    let pb = progress_bar("Generating memory state and secret keys...");
    let canon_state_clone = state_machine.clone();
    let mut state = state_machine;
    let mut mem_state = MemoryState::new(canon_state_clone);
    let state_arc = Arc::new(Mutex::new(state.clone()));

    let secret_keys: Vec<SecretKey> =
        client.get_keypairs().await?.iter().map(|x| x.secret).collect();
    pb.finish();

    // Initial size
    bridge_tree_usage("Original state", state.tree.clone());

    // Create txs token
    let token_id = Base::random(&mut OsRng);
    let amount = 1;

    // Generating and applying transactions
    let txs = 5;
    println!("  Applying {} transactions...", txs);
    for i in 0..txs {
        println!("    tx {}", i);
        let tx =
            match client.build_transaction(pubkey, amount, token_id, true, state_arc.clone()).await
            {
                Ok(v) => v,
                Err(e) => {
                    println!("Failed building transaction: {}", e);
                    return Err(e.into())
                }
            };

        let update = match state_transition(&mem_state, tx.clone()) {
            Ok(v) => v,
            Err(e) => {
                println!("validate_state_transition(): Failed for tx {}: {}", i, e);
                return Err(e.into())
            }
        };

        mem_state.apply(update.clone());
        state.apply(update, secret_keys.clone(), None, client.wallet.clone()).await?;
    }

    // Final size
    bridge_tree_usage("Final state", state.tree);

    Ok(())
}
