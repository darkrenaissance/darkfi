use crate::serial;
use crate::serial;
use crate::Result;
use crate::Result;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::Connection;
use rusqlite::Connection;
use smol::Async;
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn db_connect() -> Connection {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        let connector = Connection::open(&path);
        connector.expect("Failed to connect to database.")
    }

    pub async fn key_gen() -> (Vec<u8>, Vec<u8>) {
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (privkey, pubkey)
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        debug!(target: "adapter", "new_wallet() [FOUND PATH]");
        println!("Found path: {:?}", &path);
        debug!(target: "adapter", "new_wallet() [TRY DB CONNECT]");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let contents = include_str!("../../res/schema.sql");
        Ok(connect.execute_batch(&contents)?)
    }

    //pub async fn decrypt(conn: &Connection, password: )
    // TODO: getting an error when i call this function- does not implement send
    pub async fn save_key(conn: &Connection, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        // loads the walle
        let mut db_file = File::open("wallet.sql")?;
        let mut contents = String::new();
        db_file.read_to_string(&mut contents)?;
        Ok(conn.execute_batch(&mut contents)?)
    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}

//let stop_send = self.stop_send.clone();
//io.add_method("stop", move |_| {
//    let stop_send = stop_send.clone();
//    async move {
//        RpcAdapter::stop().await;
//        let _ = stop_send.send(()).await;
//        Ok(jsonrpc_core::Value::Null)
//    }
//});

//let stop_send = self.stop_send.clone();
//pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
//    Ok(self.stop_recv.recv().await?)
//}
//let self2 = self2.clone();
//async move {
//    Ok(json!({
//        "started": *self2.started.lock().await,
//        "connections": self2.p2p.connections_count().await
//    }))

//pub async fn serve(self: Arc<Self>, mut req: Request) ->
// http_types::Result<Response> {    info!("RPC serving {}", req.url());

//    let request = req.body_string().await?;

//    let mut res = Response::new(StatusCode::Ok);
//    res.insert_header("Content-Type", "text/plain");
//    res.set_body(response);
//    Ok(res)
//}

//pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
//    Ok(self.stop_recv.recv().await?)
//}
