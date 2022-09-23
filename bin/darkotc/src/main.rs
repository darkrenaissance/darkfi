use std::{
    io::{stdin, Read},
    process::exit,
    /*
    <<<<<<< HEAD
        str::FromStr,
    };

    use clap::{Parser, Subcommand};
    use darkfi::crypto::proof::VerifyingKey;
    use halo2_proofs::{arithmetic::Field, pasta::group::ff::PrimeField};
    use rand::rngs::OsRng;
    use serde_json::json;
    use termion::color;
          =======
          */
};

use clap::{Parser, Subcommand};
use halo2_proofs::{arithmetic::Field, pasta::group::ff::PrimeField};
use rand::rngs::OsRng;

use url::Url;

use darkfi::{
    cli_desc,
    crypto::{
        /*
        <<<<<<< HEAD
                address::Address,
                burn_proof::{create_burn_proof, verify_burn_proof},
                keypair::{PublicKey, SecretKey},
                merkle_node::MerkleNode,
                mint_proof::{create_mint_proof, verify_mint_proof},
                proof::ProvingKey,
                token_id,
                types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
                util::{pedersen_commitment_base, pedersen_commitment_u64},
                BurnRevealedValues, MintRevealedValues, OwnCoin, Proof,
            },
            rpc::{client::RpcClient, jsonrpc::JsonRequest},
            util::{
                cli::progress_bar,
                encode_base10,
                serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
                =======
                    */
        burn_proof::{create_burn_proof, verify_burn_proof},
        keypair::{PublicKey, SecretKey},
        mint_proof::{create_mint_proof, verify_mint_proof},
        note::{EncryptedNote, Note},
        proof::{ProvingKey, VerifyingKey},
        schnorr,
        schnorr::SchnorrSecret,
        token_id,
        types::{
            DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData, DrkUserDataBlind,
            DrkValueBlind,
        },
        util::{pedersen_commitment_base, pedersen_commitment_u64},
        BurnRevealedValues, MintRevealedValues, Proof,
    },
    rpc::client::RpcClient,
    tx::{
        partial::{PartialTransaction, PartialTransactionInput},
        Transaction, TransactionInput, TransactionOutput,
    },
    util::{
        cli::{fg_green, fg_red, progress_bar},
        encode_base10,
        serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    },
    zk::circuit::{BurnContract, MintContract},
    Result,
};

mod cli_util;
use cli_util::{parse_token_pair, parse_value_pair};

mod rpc;
use rpc::Rpc;

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
        /*
                <<<<<<< HEAD
                /// Pair of token IDs to swap: e.g. token_to_send:token_to_recv
                token_pair: String,

                #[clap(short, long)]
                /// Pair of values to swap: e.g. value_to_send:value_to_recv
                value_pair: String,
            },

            /// Inspect swap data from stdin or file.
            Inspect,
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
                =======
                */
        /// Pair of token IDs to swap: token_to_send:token_to_recv
        token_pair: String,

        #[clap(short, long)]
        /// Pair of values to swap: value_to_send:value_to_recv
        value_pair: String,
    },

    /// Inspect partial swap data from stdin.
    InspectPartial,

    /// Join two partial swap data files and build a tx
    Join { data0: String, data1: String },

    /// Sign a transaction given from stdin.
    SignTx,
}

#[derive(SerialEncodable, SerialDecodable)]
/// Half of the swap data, includes the coin that is supposed to be received,
/// and the coin that is supposed to be sent.
struct PartialSwapData {
    /// Mint proof of coin to be received
    mint_proof: Proof,
    /// Public values for the mint proof
    mint_revealed: MintRevealedValues,
    /// Value of the coin to be received
    mint_value: u64,
    /// Token ID of the coin to be received
    mint_token: DrkTokenId,
    /// Blinding factor for the minted value pedersen commitment
    mint_value_blind: DrkValueBlind,
    /// Blinding factor for the minted token ID pedersen commitment
    mint_token_blind: DrkValueBlind,
    /// Burn proof of the coin to be sent
    burn_proof: Proof,
    /// Public values for the burn proof
    burn_revealed: BurnRevealedValues,
    /// Value of the coin to be sent
    burn_value: u64,
    /// Token ID of the coin to be sent
    burn_token: DrkTokenId,
    /// Blinding factor for the burned value pedersen commitment
    burn_value_blind: DrkValueBlind,
    /// Blinding factor for the burned token ID pedersen commitment
    burn_token_blind: DrkValueBlind,
    /// Encrypted note
    encrypted_note: EncryptedNote,
}

#[derive(SerialEncodable, SerialDecodable)]
/// Full swap data, containing two instances of `PartialSwapData`, which
/// represent an atomic swap.
struct SwapData {
    swap0: PartialSwapData,
    swap1: PartialSwapData,
}

async fn init_swap(
    endpoint: Url,
    token_pair: (String, String),
    value_pair: (u64, u64),
    /*
    <<<<<<< HEAD
    ) -> Result<()> {
        let rpc_client = RpcClient::new(endpoint).await?;
        let rpc = Rpc { rpc_client };

        // TODO: Think about decimals, there has to be some metadata to keep track.
        let tp = (token_id::parse_b58(&token_pair.0)?, token_id::parse_b58(&token_pair.1)?);
        let vp: (u64, u64) =
            (value_pair.0.clone().try_into().unwrap(), value_pair.1.clone().try_into().unwrap());
        =======
        */
) -> Result<PartialSwapData> {
    let rpc_client = RpcClient::new(endpoint).await?;
    let rpc = Rpc { rpc_client };

    // TODO: Implement metadata for decimals, don't hardcode.
    let tp = (token_id::parse_b58(&token_pair.0)?, token_id::parse_b58(&token_pair.1)?);
    let vp = value_pair;

    // Connect to darkfid and see if there's available funds.
    let balance = rpc.balance_of(&token_pair.0).await?;
    if balance < vp.0 {
        eprintln!(
            "Error: There's not enough balance for token \"{}\" in your wallet.",
            token_pair.0
        );
        eprintln!("Available balance is {} ({})", encode_base10(balance, 8), balance);
        exit(1);
    }

    /*
    <<<<<<< HEAD
        // If not enough funds in a single coin, mint a single new coin
        // with the funds. We do this to minimize the size of the swap
        // transaction, i.e. 2 inputs and 2 outputs.
        // TODO: Implement ^
        // TODO: Maybe this should be done by the user beforehand?

        // Find a coin to spend
        let coins = rpc.get_coins_valtok(vp.0, &token_pair.0).await?;
        if coins.is_empty() {
            eprintln!("Error: Did not manage to find a coin with enough value to spend");
        =======
        */
    // If there's not enough funds in a single coin, mint a single new coin
    // with the funds. We do this to minimize the size of the swap transaction.
    // i.e. 2 inputs and 2 outputs.
    // TODO: Implement ^
    // TODO: Maybe this should be done by the user beforehand?

    // Find a coin to spend. We can find multiple, but we'll pick the first one.
    let coins = rpc.get_coins_valtok(vp.0, &token_pair.0).await?;
    if coins.is_empty() {
        eprintln!("Error: Did not manage to find a coin with enough value to spend.");
        exit(1);
    }

    eprintln!("Initializing swap data for:");
    /*
    <<<<<<< HEAD
        eprintln!("Send: {} {} tokens", encode_base10(value_pair.0, 8), token_pair.0);
        eprintln!("Recv: {} {} tokens", encode_base10(value_pair.1, 8), token_pair.1);

        // Fetch our default address
        let our_address = rpc.wallet_address().await?;
        let our_publickey = match PublicKey::try_from(our_address) {
        =======
        */
    eprintln!("Send: {} {} tokens", encode_base10(vp.0, 8), token_pair.0);
    eprintln!("Recv: {} {} tokens", encode_base10(vp.1, 8), token_pair.1);

    // Fetch our default address
    let our_addr = rpc.wallet_address().await?;
    let our_pubk = match PublicKey::try_from(our_addr) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error converting our address into PublicKey: {}", e);
            exit(1);
        }
    };

    /*
    <<<<<<< HEAD
        // Build proving keys
        let pb = progress_bar("Building proving key for the mint contract");
        let mint_pk = ProvingKey::build(8, &MintContract::default());
        pb.finish();

        let pb = progress_bar("Building proving key for the burn contract");
        let burn_pk = ProvingKey::build(11, &BurnContract::default());
        pb.finish();

        // The coin we want to receive.
        =======
        */
    // Build ZK proving keys
    let pb = progress_bar("Building proving key for the Mint contract");
    let mint_pk = ProvingKey::build(11, &MintContract::default());
    pb.finish();

    let pb = progress_bar("Building proving key for the Burn contract");
    let burn_pk = ProvingKey::build(11, &BurnContract::default());
    pb.finish();

    // The coin we want to receive

    let recv_value_blind = DrkValueBlind::random(&mut OsRng);
    let recv_token_blind = DrkValueBlind::random(&mut OsRng);
    let recv_coin_blind = DrkCoinBlind::random(&mut OsRng);
    let recv_serial = DrkSerial::random(&mut OsRng);
    /*
    <<<<<<< HEAD
        let pb = progress_bar("Building mint proof for receiving coin");
        =======
        */
    // Spend hook and user data disabled
    let spend_hook = DrkSpendHook::from(0);
    let user_data = DrkUserData::from(0);

    let pb = progress_bar("Building Mint proof for the receiving coin");

    let (mint_proof, mint_revealed) = create_mint_proof(
        &mint_pk,
        vp.1,
        tp.1,
        recv_value_blind,
        recv_token_blind,
        recv_serial,
        /*
        <<<<<<< HEAD
                recv_coin_blind,
                our_publickey,
                =======
                */
        spend_hook,
        user_data,
        recv_coin_blind,
        our_pubk,
    )?;
    pb.finish();

    // The coin we are spending.
    /*
    <<<<<<< HEAD
        // We'll spend the first one we've found.
        let coin = coins[0];

        let pb = progress_bar("Building burn proof for spending coin");
        =======
        */
    let coin = coins[0].clone();

    let pb = progress_bar("Building Burn proof for the spending coin");

    let signature_secret = SecretKey::random(&mut OsRng);
    let merkle_path = match rpc.get_merkle_path(usize::from(coin.leaf_position)).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to get Merkle path for our coin from darkfid RPC: {}", e);
            exit(1);
        }
    };

    // Spend hook and user data disabled
    let spend_hook = DrkSpendHook::from(0);
    let user_data = DrkUserData::from(0);
    let user_data_blind = DrkUserDataBlind::random(&mut OsRng);

    let (burn_proof, burn_revealed) = create_burn_proof(
        &burn_pk,
        vp.0,
        tp.0,
        coin.note.value_blind,
        coin.note.token_blind,
        coin.note.serial,
        spend_hook,
        user_data,
        user_data_blind,
        coin.note.coin_blind,
        coin.secret,
        coin.leaf_position,
        merkle_path,
        signature_secret,
    )?;
    pb.finish();

    /*
    <<<<<<< HEAD
        // Pack proofs together with pedersen commitment openings so
        // counterparty can verify correctness.
        let swap_data = SwapData {
        =======
        */
    // Create encrypted note
    let note = Note {
        serial: recv_serial,
        value: vp.1,
        token_id: tp.1,
        coin_blind: recv_coin_blind,
        value_blind: recv_value_blind,
        token_blind: recv_token_blind,
        // Here we store our secret key we used for signing
        memo: signature_secret.to_bytes().to_vec(),
    };
    let encrypted_note = note.encrypt(&our_pubk)?;

    // Pack proofs together with pedersen commitment openings so
    // counterparty can verify correctness.
    let partial_swap_data = PartialSwapData {
        mint_proof,
        mint_revealed,
        mint_value: vp.1,
        mint_token: tp.1,
        mint_value_blind: recv_value_blind,
        mint_token_blind: recv_token_blind,
        burn_proof,
        burn_value: vp.0,
        burn_token: tp.0,
        burn_revealed,
        burn_value_blind: coin.note.value_blind,
        burn_token_blind: coin.note.token_blind,
        /*
        <<<<<<< HEAD
            };

            // Print encoded data.
            println!("{}", bs58::encode(serialize(&swap_data)).into_string());

            Ok(())
        }

        fn inspect(data: &str) -> Result<()> {
                =======
                */
        encrypted_note,
    };

    Ok(partial_swap_data)
}

fn inspect_partial(data: &str) -> Result<()> {
    let mut mint_valid = false;
    let mut burn_valid = false;
    let mut mint_value_valid = false;
    let mut mint_token_valid = false;
    let mut burn_value_valid = false;
    let mut burn_token_valid = false;

    let bytes = match bs58::decode(data).into_vec() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error decoding base58 data from input: {}", e);
            exit(1);
        }
    };

    /*
    <<<<<<< HEAD
        let sd: SwapData = match deserialize(&bytes) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: Failed to deserialize swap data into struct: {}", e);
        =======
        */
    let sd: PartialSwapData = match deserialize(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error deserializing partial swap data into struct: {}", e);

            exit(1);
        }
    };

    /*
    <<<<<<< HEAD
        eprintln!("Successfully decoded data into SwapData struct");

        // Build verifying keys
        let pb = progress_bar("Building verifying key for the mint contract");
        let mint_vk = VerifyingKey::build(8, &MintContract::default());
        pb.finish();

        let pb = progress_bar("Building verifying key for the burn contract");
        let burn_vk = VerifyingKey::build(11, &BurnContract::default());
        pb.finish();

        let pb = progress_bar("Verifying burn proof");
        =======
        */
    eprintln!("Successfully decoded partial swap data");

    // Build ZK verifying keys
    let pb = progress_bar("Building verifying key for the Mint contract");
    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    pb.finish();

    let pb = progress_bar("Building verifying key for the Burn contract");
    let burn_vk = VerifyingKey::build(11, &BurnContract::default());
    pb.finish();

    let pb = progress_bar("Verifying Burn proof");

    if verify_burn_proof(&burn_vk, &sd.burn_proof, &sd.burn_revealed).is_ok() {
        burn_valid = true;
    }
    pb.finish();

    /*
    <<<<<<< HEAD
        let pb = progress_bar("Verifying mint proof");
        =======
        */
    let pb = progress_bar("Verifying Mint proof");

    if verify_mint_proof(&mint_vk, &sd.mint_proof, &sd.mint_revealed).is_ok() {
        mint_valid = true;
    }
    pb.finish();

    /*
    <<<<<<< HEAD
        eprintln!("  Verifying pedersen commitments");
        =======
        */
    eprintln!("  Verifying Pedersen commitments");

    if pedersen_commitment_u64(sd.burn_value, sd.burn_value_blind) == sd.burn_revealed.value_commit
    {
        burn_value_valid = true;
    }

    if pedersen_commitment_base(sd.burn_token, sd.burn_token_blind) == sd.burn_revealed.token_commit
    {
        burn_token_valid = true;
    }

    if pedersen_commitment_u64(sd.mint_value, sd.mint_value_blind) == sd.mint_revealed.value_commit
    {
        mint_value_valid = true;
    }

    if pedersen_commitment_base(sd.mint_token, sd.mint_token_blind) == sd.mint_revealed.token_commit
    {
        mint_token_valid = true;
    }

    let mut valid = true;
    eprintln!("Summary:");

    eprint!("  Burn proof: ");
    if burn_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprint!("  Burn proof value commitment: ");
    if burn_value_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprint!("  Burn proof token commitment: ");
    if burn_token_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprint!("  Mint proof: ");
    if mint_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprint!("  Mint proof value commitment: ");
    if mint_value_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprint!("  Mint proof token commitment: ");
    if mint_token_valid {
        eprintln!("{}", fg_green("VALID"));
    } else {
        eprintln!("{}", fg_red("INVALID"));
        valid = false;
    }

    eprintln!("========================================");

    eprintln!(
        "Mint: {} {}",
        encode_base10(sd.mint_value, 8),
        bs58::encode(sd.mint_token.to_repr()).into_string()
    );
    eprintln!(
        "Burn: {} {}",
        encode_base10(sd.burn_value, 8),
        bs58::encode(sd.burn_token.to_repr()).into_string()
    );

    /*
    <<<<<<< HEAD
        if !valid {
            eprintln!(
                "\nThe ZK proofs and commitments inspected are {}NOT VALID{}",
                color::Fg(color::Red),
                color::Fg(color::Reset)
            );
            exit(1);
        } else {
            eprintln!(
                "\nThe ZK proofs and commitments inspected are {}VALID{}",
                color::Fg(color::Green),
                color::Fg(color::Reset)
            );
            =======
                */
    eprint!("\nThe ZK proofs and commitments inspected are ");
    if !valid {
        println!("{}", fg_red("NOT VALID"));
        exit(1);
    } else {
        eprintln!("{}", fg_green("VALID"));
    }

    Ok(())
}

/*
<<<<<<< HEAD
#[derive(SerialEncodable, SerialDecodable)]
struct SwapData {
    mint_proof: Proof,
    mint_revealed: MintRevealedValues,
    mint_value: u64,
    mint_token: DrkTokenId,
    mint_value_blind: DrkValueBlind,
    mint_token_blind: DrkValueBlind,
    burn_proof: Proof,
    burn_revealed: BurnRevealedValues,
    burn_value: u64,
    burn_token: DrkTokenId,
    burn_value_blind: DrkValueBlind,
    burn_token_blind: DrkValueBlind,
=======
*/
async fn join(endpoint: Url, d0: PartialSwapData, d1: PartialSwapData) -> Result<Transaction> {
    eprintln!("Joining data into a transaction");

    let input0 = PartialTransactionInput { burn_proof: d0.burn_proof, revealed: d0.burn_revealed };
    let input1 = PartialTransactionInput { burn_proof: d1.burn_proof, revealed: d1.burn_revealed };
    let inputs = vec![input0, input1];

    let output0 = TransactionOutput {
        mint_proof: d0.mint_proof,
        revealed: d0.mint_revealed,
        enc_note: d0.encrypted_note.clone(),
    };
    let output1 = TransactionOutput {
        mint_proof: d1.mint_proof,
        revealed: d1.mint_revealed,
        enc_note: d1.encrypted_note.clone(),
    };
    let outputs = vec![output0, output1];

    let partial_tx = PartialTransaction { clear_inputs: vec![], inputs, outputs };
    let mut unsigned_tx_data = vec![];
    partial_tx.encode(&mut unsigned_tx_data)?;

    let mut inputs = vec![];
    let mut signed: bool;

    eprint!("Trying to decrypt the note of the first half... ");
    let rpc_client = RpcClient::new(endpoint.clone()).await?;
    let rpc = Rpc { rpc_client };
    let note = match rpc.decrypt_note(&d0.encrypted_note).await {
        Ok(v) => v,
        Err(_) => None,
    };

    if let Some(note) = note {
        eprintln!("{}", fg_green("Success"));
        let signature = try_sign_tx(&note, &unsigned_tx_data[..])?;
        let input = TransactionInput::from_partial(partial_tx.inputs[0].clone(), signature);
        inputs.push(input);
        signed = true;
    } else {
        eprintln!("{}", fg_red("Failure"));
        let signature = schnorr::Signature::dummy();
        let input = TransactionInput::from_partial(partial_tx.inputs[0].clone(), signature);
        inputs.push(input);
        signed = false;
    }

    // If we have signed, we shouldn't have to look in the other one, but we might
    // be sending to ourself for some reason.
    eprint!("Trying to decrypt the note of the second half... ");
    let rpc_client = RpcClient::new(endpoint).await?;
    let rpc = Rpc { rpc_client };
    let note = match rpc.decrypt_note(&d1.encrypted_note).await {
        Ok(v) => v,
        Err(_) => None,
    };

    if let Some(note) = note {
        eprintln!("{}", fg_green("Success"));
        let signature = try_sign_tx(&note, &unsigned_tx_data[..])?;
        let input = TransactionInput::from_partial(partial_tx.inputs[1].clone(), signature);
        inputs.push(input);
        signed = true;
    } else {
        eprintln!("{}", fg_red("Failure"));
        let signature = schnorr::Signature::dummy();
        let input = TransactionInput::from_partial(partial_tx.inputs[1].clone(), signature);
        inputs.push(input);
        if !signed {
            eprintln!("Error: Failed to sign transaction!");
            exit(1);
        }
    }

    if !signed {
        eprintln!("Error: Failed to sign transaction!");
        exit(1);
    }

    let tx = Transaction { clear_inputs: vec![], inputs, outputs: partial_tx.outputs };
    Ok(tx)
}

async fn sign_tx(endpoint: Url, data: &str) -> Result<Transaction> {
    eprintln!("Trying to sign transaction");
    let mut tx: Transaction = deserialize(&bs58::decode(data).into_vec()?)?;

    let mut input_idxs = vec![];
    let mut signature = schnorr::Signature::dummy();

    // Find dummy signatures to fill. We assume we're using the same
    // signature everywhere.
    eprintln!("Looking for dummy signatures...");
    for (i, input) in tx.inputs.iter().enumerate() {
        if input.signature == schnorr::Signature::dummy() {
            eprintln!("Found dummy signature in input {}", i);
            input_idxs.push(i);
        }
    }

    if input_idxs.is_empty() {
        eprintln!("Error: Did not find any dummy signatures in the transaction.");
        exit(1);
    }

    // Find a note to decrypt that holds our secret key.
    let mut found_secret = false;
    for (i, output) in tx.outputs.iter().enumerate() {
        // TODO: FIXME: Consider not closing the RPC on failure.
        let rpc_client = RpcClient::new(endpoint.clone()).await?;
        let rpc = Rpc { rpc_client };

        let note = match rpc.decrypt_note(&output.enc_note).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(note) = note {
            eprintln!("Successfully decrypted note in output {}", i);
            eprintln!("Creating signature...");
            let mut unsigned_tx_data = vec![];
            let _ = tx.encode_without_signature(&mut unsigned_tx_data)?;

            signature = try_sign_tx(&note, &unsigned_tx_data[..])?;
            found_secret = true;
            break
        }

        eprintln!("Failed to find a note to decrypt. Signing failed.");
        exit(1);
    }

    if !found_secret {
        eprintln!("Error: Did not manage to sign transaction. Couldn't find any secret keys.");
        exit(1);
    }

    for i in input_idxs {
        tx.inputs[i].signature = signature.clone();
    }

    Ok(tx)
}

fn try_sign_tx(note: &Note, tx_data: &[u8]) -> Result<schnorr::Signature> {
    if note.memo.len() != 32 {
        eprintln!("Error: The note memo is not 32 bytes");
        exit(1);
    }

    let secret = match SecretKey::from_bytes(note.memo.clone().try_into().unwrap()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Did not manage to cast bytes into SecretKey: {}", e);
            exit(1);
        }
    };

    eprintln!("Signing transaction...");
    let signature = secret.sign(tx_data);
    Ok(signature)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Subcmd::Init { token_pair, value_pair } => {
            let token_pair = parse_token_pair(&token_pair)?;
            let value_pair = parse_value_pair(&value_pair)?;
            /*
                <<<<<<< HEAD

                init_swap(args.endpoint, token_pair, value_pair).await
            }
            Subcmd::Inspect => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                inspect(&buf.trim())
                =======
                */
            let swap_data = init_swap(args.endpoint, token_pair, value_pair).await?;

            println!("{}", bs58::encode(serialize(&swap_data)).into_string());
            Ok(())
        }
        Subcmd::InspectPartial => {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;

            inspect_partial(buf.trim())
        }
        Subcmd::Join { data0, data1 } => {
            let d0 = std::fs::read_to_string(data0)?;
            let d1 = std::fs::read_to_string(data1)?;

            let d0 = deserialize(&bs58::decode(&d0.trim()).into_vec()?)?;
            let d1 = deserialize(&bs58::decode(&d1.trim()).into_vec()?)?;

            let tx = join(args.endpoint, d0, d1).await?;

            println!("{}", bs58::encode(&serialize(&tx)).into_string());
            eprintln!("Successfully signed transaction");
            Ok(())
        }
        Subcmd::SignTx => {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;

            let tx = sign_tx(args.endpoint, buf.trim()).await?;

            println!("{}", bs58::encode(&serialize(&tx)).into_string());
            eprintln!("Successfully signed transaction");
            Ok(())
        }
    }
}
