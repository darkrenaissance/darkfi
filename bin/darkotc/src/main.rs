use std::{
    io::{stdin, Read},
    process::exit,
};

use clap::{Parser, Subcommand};
use halo2_proofs::{arithmetic::Field, pasta::group::ff::PrimeField};
use rand::rngs::OsRng;
use url::Url;

use darkfi::{
    cli_desc,
    crypto::{
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
    serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    tx::{
        partial::{PartialTransaction, PartialTransactionInput},
        Transaction, TransactionInput, TransactionOutput,
    },
    util::{
        cli::{fg_green, fg_red, progress_bar},
        parse::encode_base10,
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
) -> Result<PartialSwapData> {
    let rpc_client = match RpcClient::new(endpoint).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed connecting to darkfid JSON-RPC endpoint.");
            return Err(e)
        }
    };
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

    // Fetch our default address
    let our_addr = rpc.wallet_address().await?;
    let our_pubk = match PublicKey::try_from(our_addr) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error converting our address into PublicKey: {}", e);
            exit(1);
        }
    };

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
        spend_hook,
        user_data,
        recv_coin_blind,
        our_pubk,
    )?;
    pb.finish();

    // The coin we are spending.
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
        encrypted_note,
    };

    Ok(partial_swap_data)
}

fn inspect_partial(data: &str) -> Result<()> {
    let bytes = match bs58::decode(data).into_vec() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error decoding base58 data from input: {}", e);
            exit(1);
        }
    };

    let sd: PartialSwapData = match deserialize(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error deserializing partial swap data into struct: {}", e);
            exit(1);
        }
    };

    eprintln!("Successfully decoded partial swap data");

    // Build ZK verifying keys
    let pb = progress_bar("Building verifying key for the Mint contract");
    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    pb.finish();

    let pb = progress_bar("Building verifying key for the Burn contract");
    let burn_vk = VerifyingKey::build(11, &BurnContract::default());
    pb.finish();

    let pb = progress_bar("Verifying Burn proof");
    let burn_valid = verify_burn_proof(&burn_vk, &sd.burn_proof, &sd.burn_revealed).is_ok();
    pb.finish();

    let pb = progress_bar("Verifying Mint proof");
    let mint_valid = verify_mint_proof(&mint_vk, &sd.mint_proof, &sd.mint_revealed).is_ok();
    pb.finish();

    eprintln!("  Verifying Pedersen commitments");

    let burn_value_valid = pedersen_commitment_u64(sd.burn_value, sd.burn_value_blind) ==
        sd.burn_revealed.value_commit;

    let burn_token_valid = pedersen_commitment_base(sd.burn_token, sd.burn_token_blind) ==
        sd.burn_revealed.token_commit;

    let mint_value_valid = pedersen_commitment_u64(sd.mint_value, sd.mint_value_blind) ==
        sd.mint_revealed.value_commit;

    let mint_token_valid = pedersen_commitment_base(sd.mint_token, sd.mint_token_blind) ==
        sd.mint_revealed.token_commit;

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

    eprint!("\nThe ZK proofs and commitments inspected are ");
    if !valid {
        println!("{}", fg_red("NOT VALID"));
        exit(1);
    } else {
        eprintln!("{}", fg_green("VALID"));
    }

    Ok(())
}

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

    // We assume our input and our output are in the same index, since this
    // transaction contains 2 inputs and 2 outputs, and one of each is ours,
    // and one of each is the other party's. So we go on and sign the input
    // index of the output index we can decrypt the note for.
    let mut idx_to_sign = 0;
    let mut signature = schnorr::Signature::dummy();

    eprintln!("Looking for an encrypted note we can decrypt...");
    let mut found_secret = false;
    for (i, output) in tx.outputs.iter().enumerate() {
        // TODO: FIXME: Consider not closing the RPC on failure.
        let rpc = Rpc { rpc_client: RpcClient::new(endpoint.clone()).await? };

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
            idx_to_sign = i;
            break
        }

        eprintln!("Failed to find a note to decrypt. Signing failed.");
        exit(1);
    }

    if !found_secret {
        eprintln!("Error: Did not manage to sign transaction. Couldn't find any secret keys.");
        exit(1);
    }

    tx.inputs[idx_to_sign].signature = signature.clone();
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
            eprintln!("Creating half of an atomic swap");
            eprintln!("Send: {} {} tokens.", encode_base10(value_pair.0, 8), token_pair.0);
            eprintln!("Recv: {} {} tokens.", encode_base10(value_pair.1, 8), token_pair.1);
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
