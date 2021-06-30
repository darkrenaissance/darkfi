use crate::rpc::adapter::RpcAdapter;
use crate::service::ClientProgramOptions;
use crate::{net, Error, Result};
use async_executor::Executor;
use async_native_tls::TlsAcceptor;
use async_std::sync::Mutex;
use http_types::{Request, Response, StatusCode};
use log::*;
use smol::Async;
use std::net::TcpListener;
use std::sync::Arc;

/// Listens for incoming connections and serves them.
pub async fn listen(
    executor: Arc<Executor<'_>>,
    rpc: Arc<RpcInterface>,
    listener: Async<TcpListener>,
    tls: Option<TlsAcceptor>,
) -> Result<()> {
    // Format the full host address.
    let host = match &tls {
        None => format!("http://{}", listener.get_ref().local_addr()?),
        Some(_) => format!("https://{}", listener.get_ref().local_addr()?),
    };
    println!("Listening on {}", host);

    loop {
        // Accept the next connection.
        debug!(target: "rpc", "waiting for stream accept [START]");
        let (stream, _) = listener.accept().await?;

        debug!(target: "rpc", "stream accepted [END]");
        // Spawn a background task serving this connection.
        let task = match &tls {
            None => {
                let stream = async_dup::Arc::new(stream);
                let rpc = rpc.clone();
                executor.spawn(async move {
                    if let Err(err) = async_h1::accept(stream, move |req| {
                        let rpc = rpc.clone();
                        rpc.serve(req)
                    })
                    .await
                    {
                        println!("Connection error: {:#?}", err);
                    }
                })
            }
            Some(tls) => {
                // In case of HTTPS, establish a secure TLS connection first.
                match tls.accept(stream).await {
                    Ok(stream) => {
                        let _stream = async_dup::Arc::new(async_dup::Mutex::new(stream));
                        executor.spawn(async move {
                            /*if let Err(err) = async_h1::accept(stream, serve).await {
                                println!("Connection error: {:#?}", err);
                            }*/
                            unimplemented!();
                        })
                    }
                    Err(err) => {
                        println!("Failed to establish secure TLS connection: {:#?}", err);
                        continue;
                    }
                }
            }
        };

        // Detach the task to let it run in the background.
        task.detach();
    }
}

pub async fn start(
    executor: Arc<Executor<'_>>,
    options: Arc<ClientProgramOptions>,
    adapter: RpcAdapter,
) -> Result<()> {
    debug!(target: "JSONSERVER", "START FUNCTION CALLED");
    let rpc = RpcInterface::new(adapter)?;
    debug!(target: "JSONSERVER", "Listening...");
    let http = listen(
        executor.clone(),
        rpc.clone(),
        Async::<TcpListener>::bind(([127, 0, 0, 1], options.rpc_port))?,
        None,
    );

    debug!(target: "JSONSERVER", "Spawning http task...");
    let http_task = executor.spawn(http);

    debug!(target: "JSONSERVER", "Locking...");
    *rpc.started.lock().await = true;

    debug!(target: "JSONSERVER", "Waiting for quit...");
    rpc.wait_for_quit().await?;

    debug!(target: "JSONSERVER", "Cancel http task...");
    http_task.cancel().await;

    Ok(())
}
// json RPC server goes here
#[allow(dead_code)]
pub struct RpcInterface {
    pub started: Mutex<bool>,
    stop_send: async_channel::Sender<()>,
    stop_recv: async_channel::Receiver<()>,
    adapter: RpcAdapter,
}

impl RpcInterface {
    pub fn new(adapter: RpcAdapter) -> Result<Arc<Self>> {
        let (stop_send, stop_recv) = async_channel::unbounded::<()>();
        Ok(Arc::new(Self {
            //p2p,
            started: Mutex::new(false),
            stop_send,
            stop_recv,
            adapter,
        }))
    }

    pub async fn serve(self: Arc<Self>, mut req: Request) -> http_types::Result<Response> {
        info!("RPC serving {}", req.url());

        let request = req.body_string().await?;

        let io = self.handle_input().await?;

        let response = io
            .handle_request_sync(&request)
            .ok_or(Error::BadOperationType)?;

        let mut res = Response::new(StatusCode::Ok);
        res.insert_header("Content-Type", "text/plain");
        res.set_body(response);
        Ok(res)
    }

    pub async fn handle_input(self: Arc<Self>) -> Result<jsonrpc_core::IoHandler> {
        debug!(target: "rpc", "JsonRpcInterface::handle_input() [START]");
        let mut io = jsonrpc_core::IoHandler::new();

        io.add_sync_method("say_hello", |_| {
            Ok(jsonrpc_core::Value::String("Hello World!".into()))
        });

        io.add_method("test_path", move |_| async move {
            //RpcAdapter::get_path().await;
            Ok(jsonrpc_core::Value::String("TEST PATH!".into()))
        });

        let self1 = self.clone();
        io.add_method("get_cash_key", move |_| {
            let self2 = self1.clone();
            async move {
                self2.adapter.get_cash_key().await.expect("Failed to get key");
                Ok(jsonrpc_core::Value::String("Getting cashier key...".into()))
            }
        });

        let self1 = self.clone();
        io.add_method("get_info", move |_| {
        let self2 = self1.clone();
            async move {
                self2.adapter.get_info().await;
                Ok(jsonrpc_core::Value::Null)
            }
        });

        let self1 = self.clone();
        io.add_method("stop", move |_| {
        let self2 = self1.clone();
            async move {
                self2.adapter.stop().await;
                Ok(jsonrpc_core::Value::Null)
            }
        });
        let self1 = self.clone();
        io.add_method("create_wallet", move |_| {
            let self2 = self1.clone();
            async move {
            println!("New wallet method called...");
            //RpcAdapter::new("wallet.db").expect("Failed to create wallet");
            println!("Wallet created at path {:?}", self2.adapter.wallet.path);
            Ok(jsonrpc_core::Value::String(
                "Created wallet".into(),))
            }
        });
        let self1 = self.clone();
        io.add_method("key_gen", move |_| {
            let self2 = self1.clone();
            async move {
                println!("Key generation method called...");
                self2.adapter.key_gen().await.expect("Failed to generate key");
                Ok(jsonrpc_core::Value::String(
                    "Attempted key generation".into(),
                ))
            }
        });
        let self1 = self.clone();
        io.add_method("cash_key_gen", move |_| {
            let self2 = self1.clone();
            async move {
                println!("Key generation method called...");
                self2.adapter.cash_key_gen().await.expect("Failed to generate key");
                Ok(jsonrpc_core::Value::String(
                    "Attempted key generation".into(),
                ))
            }
        });
        let self1 = self.clone();
        io.add_method("create_cashier_wallet", move |_| {
            let self2 = self1.clone();
            async move {
            println!("New wallet method called...");
            //RpcAdapter::new("cashier.db").expect("Failed to create wallet");
            println!("Wallet created at path {:?}", self2.adapter.wallet.path);
            Ok(jsonrpc_core::Value::String(
                "Created cashier wallet".into(),
            ))
            }
        });
        debug!(target: "rpc", "JsonRpcInterface::handle_input() [END]");
        Ok(io)
    }

    pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
        Ok(self.stop_recv.recv().await?)
    }
}
