// $ cargo run --features sol --bin solana-poc
// $ solana transfer 10 $pubkey
use futures::{SinkExt, StreamExt};
use rand::rngs::OsRng;
use serde::Serialize;
use serde_json::{json, Value};
use solana_client::{blockhash_query::BlockhashQuery, rpc_client::RpcClient};
use solana_sdk::{
    native_token::lamports_to_sol, pubkey::Pubkey, signature::Signer, signer::keypair::Keypair,
    system_instruction, transaction::Transaction,
};
use std::sync::{Arc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use drk::rpc::{jsonrpc, jsonrpc::JsonResult};

//const RPC_SERVER: &'static str = "https://api.mainnet-beta.solana.com";
//const WSS_SERVER: &'static str = "wss://api.mainnet-beta.solana.com";
//const RPC_SERVER: &'static str = "https://api.devnet.solana.com";
//const WSS_SERVER: &'static str = "wss://api.devnet.solana.com";
const RPC_SERVER: &'static str = "http://localhost:8899";
const WSS_SERVER: &'static str = "ws://localhost:8900";

// https://docs.solana.com/developing/clients/jsonrpc-api#accountsubscribe
#[derive(Serialize)]
struct SubscribeParams {
    encoding: Value,
    commitment: Value,
}

// Example function to show how to transfer `amount` lamports
fn transfer_lamports(from: &Keypair, to: &Pubkey, amount: u64) {
    let rpc = RpcClient::new(RPC_SERVER.to_string());
    let instruction = system_instruction::transfer(&from.pubkey(), to, amount);

    let mut tx = Transaction::new_with_payer(&[instruction], Some(&from.pubkey()));
    let bhq = BlockhashQuery::default();
    match bhq.get_blockhash_and_fee_calculator(&rpc, rpc.commitment()) {
        Err(_) => panic!("Couldn't connect to RPC"),
        Ok(v) => tx.sign(&[from], v.0),
    }

    let _signature = rpc.send_and_confirm_transaction(&tx);
}

#[tokio::main]
async fn main() -> Result<(), &'static str> {
    let keypair = Keypair::generate(&mut OsRng);
    println!("Pubkey: {:?}", keypair.pubkey());

    let rpc = RpcClient::new(RPC_SERVER.to_string());
    let balance = rpc.get_balance(&keypair.pubkey()).unwrap();
    let account_bal = Arc::new(Mutex::new(balance));

    // Parameters for subscription to events related to `pubkey`.
    let sub_params = SubscribeParams {
        encoding: json!("jsonParsed"),
        // XXX: Use "finalized" for 100% certainty.
        commitment: json!("confirmed"),
    };

    let sub_msg = jsonrpc::request(
        json!("accountSubscribe"),
        json!([json!(keypair.pubkey().to_string()), json!(sub_params)]),
    );

    // WebSocket handshake/connect
    let (ws_stream, _) = connect_async(WSS_SERVER)
        .await
        .expect("Failed to connect to WebSocket server");

    let (mut write, read) = ws_stream.split();

    // Send the subscription request
    write
        .send(Message::Text(serde_json::to_string(&sub_msg).unwrap()))
        .await
        .unwrap();
    println!("Subscribed to events for {:?}", keypair.pubkey());

    // Subscription ID so we can map our notifications to our pubkey
    // when we do multiple subscriptions and also do `accountUnsubscribe`.
    let sub_id = Arc::new(Mutex::new(0));

    let read_future = read.for_each(|message| async {
        let data = message.unwrap().into_text().unwrap();
        let v: JsonResult = serde_json::from_str(&data).unwrap();
        match v {
            JsonResult::Resp(r) => {
                println!(
                    "Successfully subscribed with ID: {:?}",
                    r.result.as_i64().unwrap()
                );
                *sub_id.lock().unwrap() = r.result.as_i64().unwrap();
            }

            JsonResult::Err(e) => {
                println!("Error on subscription: {:?}", e.error.message.to_string());
            }

            JsonResult::Notif(n) => {
                println!("Got WebSocket notification: {:?}", n);
                println!(
                    "Old balance: {:?} SOL",
                    lamports_to_sol(*account_bal.lock().unwrap())
                );
                let new_bal = n.params["result"]["value"]["lamports"].as_u64().unwrap();
                *account_bal.lock().unwrap() = new_bal;
                println!("New balance: {:?} SOL", lamports_to_sol(new_bal));
            }
        }
    });

    read_future.await;

    Ok(())
}
