use std::{process::exit, str::FromStr};

use clap::{Parser, Subcommand};
use halo2_proofs::arithmetic::Field;
use num_bigint::BigUint;
use rand::rngs::OsRng;
use serde_json::json;
use url::Url;

use darkfi::{
    cli_desc,
    crypto::{
        address::Address,
        burn_proof::create_burn_proof,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::create_mint_proof,
        proof::ProvingKey,
        token_id,
        types::{DrkCoinBlind, DrkSerial, DrkValueBlind},
        OwnCoin,
    },
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::{cli::progress_bar, encode_base10, serial::deserialize},
    zk::circuit::{BurnContract, MintContract},
    Result,
};

mod cli_util;
use cli_util::{parse_token_pair, parse_value_pair};

#[derive(Parser)]
#[clap(name = "darkotc", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Initialize an atomic swap
    Init {
        #[clap(short, long)]
        /// Pair of token IDs to swap: e.g. token_to_send:token_to_recv
        token_pair: String,

        #[clap(short, long)]
        /// Pair of values to swap: e.g. value_to_send:value_to_recv
        value_pair: String,
    },
}

struct Rpc {
    pub rpc_client: RpcClient,
}

impl Rpc {
    async fn balance_of(&self, token_id: &str) -> Result<u64> {
        let req = JsonRequest::new("wallet.get_balances", json!([]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_object() {
            eprintln!("Error: Invalid balance data received from darkfid RPC endpoint.");
            exit(1);
        }

        for i in rep.as_object().unwrap().keys() {
            if i == &token_id {
                if let Some(balance) = rep[i].as_u64() {
                    return Ok(balance)
                }

                eprintln!("Error: Invalid balance data received from darkfid RPC endpoint.");
                exit(1);
            }
        }

        Ok(0)
    }

    async fn wallet_address(&self) -> Result<Address> {
        let req = JsonRequest::new("wallet.get_addrs", json!([0_i64]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() || !rep.as_array().unwrap()[0].is_string() {
            eprintln!("Error: Invalid wallet address received from darkfid RPC endpoint.");
            exit(1);
        }

        Address::from_str(rep[0].as_str().unwrap())
    }

    async fn get_coins_valtok(&self, value: u64, token_id: &str) -> Result<Vec<OwnCoin>> {
        let req = JsonRequest::new("wallet.get_coins_valtok", json!([value, token_id, true]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() {
            eprintln!("Error: Invalid coin data received from darkfid RPC endpoint.");
            exit(1);
        }

        let mut ret = vec![];
        let rep = rep.as_array().unwrap();

        for i in rep {
            if !i.is_string() {
                eprintln!("Error: Invalid base58 data for OwnCoin");
                exit(1);
            }

            let data = match bs58::decode(i.as_str().unwrap()).into_vec() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed decoding base58 for OwnCoin: {}", e);
                    exit(1);
                }
            };

            let oc = match deserialize(&data) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed deserializing OwnCoin: {}", e);
                    exit(1);
                }
            };

            ret.push(oc);
        }

        Ok(ret)
    }

    async fn get_merkle_path(&self, leaf_pos: usize) -> Result<Vec<MerkleNode>> {
        let req = JsonRequest::new("wallet.get_merkle_path", json!([leaf_pos as u64]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() {
            eprintln!("Error: Invalid merkle path data received from darkfid RPC endpoint.");
            exit(1);
        }

        let mut ret = vec![];
        let rep = rep.as_array().unwrap();

        for i in rep {
            if !i.is_string() {
                eprintln!("Error: Invalid base58 data for MerkleNode");
                exit(1);
            }

            let n = i.as_str().unwrap();
            let n = match bs58::decode(n).into_vec() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed decoding base58 for MerkleNode: {}", e);
                    exit(1);
                }
            };

            if n.len() != 32 {
                eprintln!("Error: MerkleNode byte length is not 32");
                exit(1);
            }

            let n = MerkleNode::from_bytes(&n.try_into().unwrap());
            if n.is_some().unwrap_u8() == 0 {
                eprintln!("Error: Noncanonical bytes of MerkleNode");
                exit(1);
            }

            ret.push(n.unwrap());
        }

        Ok(ret)
    }
}

async fn init_swap(
    endpoint: Url,
    token_pair: (String, String),
    value_pair: (BigUint, BigUint),
) -> Result<()> {
    let rpc_client = RpcClient::new(endpoint).await?;
    let rpc = Rpc { rpc_client };

    // TODO: Rethink the use of BigUint throughout the codebase. Can we just use u64?
    // TODO: Think about decimals as well, there has to be some metadata to keep track.
    let tp = (token_id::parse_b58(&token_pair.0)?, token_id::parse_b58(&token_pair.1)?);
    let vp: (u64, u64) =
        (value_pair.0.clone().try_into().unwrap(), value_pair.1.clone().try_into().unwrap());

    // Connect to darkfid and see if there's available funds.
    let balance = rpc.balance_of(&token_pair.0).await?;
    if balance < vp.0 {
        eprintln!(
            "Error: There is not enough balance for token \"{}\" in your wallet.",
            token_pair.0
        );
        eprintln!(
            "Available balance is {} ({})",
            encode_base10(BigUint::from(balance), 8),
            balance
        );
        exit(1);
    }

    // If not enough funds in a single coin, mint a single new coin
    // with the funds. We do this to minimize the size of the swap
    // transaction, i.e. 2 inputs and 2 outputs.
    // TODO: Implement ^
    // TODO: Maybe this should be done by the user beforehand?

    // Find a coin to spend
    let coins = rpc.get_coins_valtok(vp.0, &token_pair.0).await?;
    if coins.is_empty() {
        eprintln!("Error: Did not manage to find a coin with enough value to spend");
        exit(1);
    }

    eprintln!("Initializing swap data for:");
    eprintln!("Send: {} {} tokens", encode_base10(value_pair.0, 8), token_pair.0);
    eprintln!("Recv: {} {} tokens", encode_base10(value_pair.1, 8), token_pair.1);

    // Fetch our default address
    let our_address = rpc.wallet_address().await?;
    let our_publickey = match PublicKey::try_from(our_address) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error converting our address into PublicKey: {}", e);
            exit(1);
        }
    };

    // Build proving keys
    let pb = progress_bar("Building proving key for the mint contract...");
    let mint_pk = ProvingKey::build(8, &MintContract::default());
    pb.finish();

    let pb = progress_bar("Building proving key for the burn contract...");
    let burn_pk = ProvingKey::build(11, &BurnContract::default());
    pb.finish();

    // The coin we want to receive.
    let recv_value_blind = DrkValueBlind::random(&mut OsRng);
    let recv_token_blind = DrkValueBlind::random(&mut OsRng);
    let recv_coin_blind = DrkCoinBlind::random(&mut OsRng);
    let recv_serial = DrkSerial::random(&mut OsRng);

    let pb = progress_bar("Building mint proof for receiving coin");
    let (mint_proof, mint_revealed) = create_mint_proof(
        &mint_pk,
        vp.1,
        tp.1,
        recv_value_blind,
        recv_token_blind,
        recv_serial,
        recv_coin_blind,
        our_publickey,
    )?;
    pb.finish();

    // The coin we are spending.
    // We'll spend the first one we've found.
    let coin = coins[0];

    let pb = progress_bar("Building burn proof for spending coin");
    let signature_secret = SecretKey::random(&mut OsRng);
    let merkle_path = match rpc.get_merkle_path(usize::from(coin.leaf_position)).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to get merkle path for our coin from darkfid RPC: {}", e);
            exit(1);
        }
    };

    let (burn_proof, burn_revealed) = create_burn_proof(
        &burn_pk,
        vp.0,
        tp.0,
        coin.note.value_blind,
        coin.note.token_blind,
        coin.note.serial,
        coin.note.coin_blind,
        coin.secret,
        coin.leaf_position,
        merkle_path,
        signature_secret,
    )?;
    pb.finish();

    // Pack proofs together with pedersen commitment openings so
    // counterparty can verify correctness.

    // Print encoded data.

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Subcmd::Init { token_pair, value_pair } => {
            let token_pair = parse_token_pair(&token_pair)?;
            let value_pair = parse_value_pair(&value_pair)?;
            let init_swap_data = init_swap(args.endpoint, token_pair, value_pair).await?;
            Ok(())
        }
    }
}
