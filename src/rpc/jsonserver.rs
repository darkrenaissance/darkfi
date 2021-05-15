use crate::{net, serial, Error, Result};
use async_executor::Executor;
use async_native_tls::TlsAcceptor;
use async_std::sync::Mutex;
use easy_parallel::Parallel;
use ff::Field;
use http_types::{Request, Response, StatusCode};
use log::*;
use rand::rngs::OsRng;
use rusqlite::Connection;
use serde_json::json;
use smol::Async;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::Arc;

// json RPC server goes here
pub struct RpcInterface {
    p2p: Arc<net::P2p>,
    pub started: Mutex<bool>,
    stop_send: async_channel::Sender<()>,
    stop_recv: async_channel::Receiver<()>,
}

impl RpcInterface {
    pub fn new(p2p: Arc<net::P2p>) -> Arc<Self> {
        let (stop_send, stop_recv) = async_channel::unbounded::<()>();

        Arc::new(Self {
            p2p,
            started: Mutex::new(false),
            stop_send,
            stop_recv,
        })
    }

    async fn db_connect() -> Connection {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        let connector = Connection::open(&path);
        connector.expect("Failed to connect to database.")
    }

    async fn generate_key() -> (Vec<u8>, Vec<u8>) {
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = serial::serialize(&public);
        let privkey = serial::serialize(&secret);
        (privkey, pubkey)
    }

    // TODO: fix this
    async fn store_key(conn: &Connection, pubkey: Vec<u8>, privkey: Vec<u8>) -> Result<()> {
        let mut db_file = File::open("wallet.sql")?;
        let mut contents = String::new();
        db_file.read_to_string(&mut contents)?;
        Ok(conn.execute_batch(&mut contents)?)
    }

    // add new methods to handle wallet commands
    pub async fn serve(self: Arc<Self>, mut req: Request) -> http_types::Result<Response> {
        info!("RPC serving {}", req.url());

        let request = req.body_string().await?;

        let mut io = jsonrpc_core::IoHandler::new();
        io.add_sync_method("say_hello", |_| {
            Ok(jsonrpc_core::Value::String("Hello World!".into()))
        });

        let self2 = self.clone();
        io.add_method("get_info", move |_| {
            let self2 = self2.clone();
            async move {
                Ok(json!({
                    "started": *self2.started.lock().await,
                    "connections": self2.p2p.connections_count().await
                }))
            }
        });

        let stop_send = self.stop_send.clone();
        io.add_method("stop", move |_| {
            let stop_send = stop_send.clone();
            async move {
                let _ = stop_send.send(()).await;
                Ok(jsonrpc_core::Value::Null)
            }
        });
        io.add_method("key_gen", move |_| async move {
            RpcInterface::db_connect().await;
            let (pubkey, privkey) = RpcInterface::generate_key().await;
            //println!("{}", pubkey, "{}", privkey);
            Ok(jsonrpc_core::Value::Null)
        });

        let response = io
            .handle_request_sync(&request)
            .ok_or(Error::BadOperationType)?;

        let mut res = Response::new(StatusCode::Ok);
        res.insert_header("Content-Type", "text/plain");
        res.set_body(response);
        Ok(res)
    }

    pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
        Ok(self.stop_recv.recv().await?)
    }
}
