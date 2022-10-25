use std::{collections::HashMap, fs::write, time::Instant};

use darkfi::crypto::coin::Coin;
use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode, Nullifier};
use darkfi_serial::{deserialize, serialize};

use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;
use serde_json::json;

fn main() {
    let mut serial_merkle_time = HashMap::new();

    for i in 1..101 {
        let mut tree: BridgeTree<MerkleNode, MERKLE_DEPTH> = BridgeTree::new(100);

        let coins = 1000 * i; 

        for _ in 0..coins {
            let coin = Coin(pallas::Base::random(&mut OsRng));
            tree.append(&MerkleNode::from(coin.0));
        }

        let start = Instant::now();
        for _ in 0..100 {
            serialize(&tree);
        }
        let elapsed = start.elapsed();

        let result = elapsed / 100;
        serial_merkle_time.insert(coins, result);
    }

    let mut deserial_merkle_time = HashMap::new();

    for i in 1..101 {
        let mut tree: BridgeTree<MerkleNode, MERKLE_DEPTH> = BridgeTree::new(100);

        let coins = 1000 * i; 
        for _ in 0..coins {
            let coin = Coin(pallas::Base::random(&mut OsRng));
            tree.append(&MerkleNode::from(coin.0));
        }

        let ser_tree = serialize(&tree);
        let start = Instant::now();
        for _ in 0..100 {
            deserialize::<BridgeTree<MerkleNode, MERKLE_DEPTH>>(&ser_tree).unwrap();
        }
        let elapsed = start.elapsed();

        let result = elapsed / 100;
        deserial_merkle_time.insert(coins, result);
    }

    let mut serial_null_time = HashMap::new();

    for i in 1..101 {
        let mut nullifiers: Vec<Nullifier> = vec![];

        let coins = 1000 * i; 
        for _ in 0..coins {
            let nullifier = Nullifier::from(pallas::Base::random(&mut OsRng));
            nullifiers.push(nullifier);
        }

        let start = Instant::now();
        for _ in 0..100 {
            serialize(&nullifiers);
        }
        let elapsed = start.elapsed();

        let result = elapsed / 100;
        serial_null_time.insert(coins, result);
    }

    let mut deserial_null_time = HashMap::new();

    for i in 1..101 {
        let mut nullifiers: Vec<Nullifier> = vec![];

        let coins = 1000 * i; 
        for _ in 0..coins {
            let nullifier = Nullifier::from(pallas::Base::random(&mut OsRng));
            nullifiers.push(nullifier);
        }

        let ser_null = serialize(&nullifiers);
        let start = Instant::now();
        for _ in 0..100 {
            deserialize::<Vec<Nullifier>>(&ser_null).unwrap();
        }
        let elapsed = start.elapsed();

        let result = elapsed / 100;
        deserial_null_time.insert(coins, result);
    }

    let serial_merkle_time = json!(serial_merkle_time);
    let deserial_merkle_time = json!(deserial_merkle_time);
    let serial_null_time = json!(serial_null_time);
    let deserial_null_time = json!(deserial_null_time);

    write("serial_merkle_time.json", serde_json::to_string_pretty(&serial_merkle_time).unwrap())
        .unwrap();

    write("deserial_merkle_time.json", serde_json::to_string_pretty(&deserial_merkle_time).unwrap())
        .unwrap();

    write("serial_null_time.json", serde_json::to_string_pretty(&serial_null_time).unwrap())
        .unwrap();

    write("deserial_null_time.json", serde_json::to_string_pretty(&deserial_null_time).unwrap())
        .unwrap();
}
