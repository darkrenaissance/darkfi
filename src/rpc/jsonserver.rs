use crate::{Error, Result};

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
    io: Arc<jsonrpc_core::IoHandler>,
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
                let io2 = io.clone();
                executor.spawn(async move {
                    if let Err(err) = async_h1::accept(stream, move |req| {
                        let rpc = rpc.clone();
                        rpc.serve(req, io2.clone())
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
        task.await;
    }
}

pub async fn start(
    executor: Arc<Executor<'_>>,
    rpc_url: std::net::SocketAddr,
    io: Arc<jsonrpc_core::IoHandler>,
) -> Result<()> {
    let rpc = RpcInterface::new()?;
    let http = listen(
        executor.clone(),
        rpc.clone(),
        Async::<TcpListener>::bind(rpc_url)?,
        None,
        io,
    );


    executor.spawn(http).detach();

    *rpc.started.lock().await = true;

    Ok(())
}

#[allow(dead_code)]
pub struct RpcInterface {
    pub started: Mutex<bool>,
    stop_send: async_channel::Sender<()>,
    stop_recv: async_channel::Receiver<()>,
}

impl RpcInterface {
    pub fn new() -> Result<Arc<Self>> {
        let (stop_send, stop_recv) = async_channel::unbounded::<()>();
        Ok(Arc::new(Self {
            started: Mutex::new(false),
            stop_send,
            stop_recv,
        }))
    }

    pub async fn serve(
        self: Arc<Self>,
        mut req: Request,
        io: Arc<jsonrpc_core::IoHandler>,
    ) -> http_types::Result<Response> {
        info!("RPC serving {}", req.url());

        let request = req.body_string().await?;

        debug!(target: "rpc", "JsonRpcInterface::serve() [PROCESSING INPUT]");
        let response = io
            .handle_request_sync(&request)
            .ok_or(Error::BadOperationType)?;
        debug!(target: "rpc", "JsonRpcInterface::serve() [PROCESSED]");

        let mut res = Response::new(StatusCode::Ok);
        res.insert_header("Content-Type", "text/plain");
        res.set_body(response);
        
        Ok(res)
    }

    //pub async fn handle_input(self: Arc<Self>) -> Result<jsonrpc_core::IoHandler> {
    //    debug!(target: "rpc", "JsonRpcInterface::handle_input() [START]");
    //    let io = jsonrpc_core::IoHandler::new();
    //    // this is where adapter is needed
    //    //let io = self.adapter.clone().handle_input(io.clone())?;
    //    debug!(target: "rpc", "JsonRpcInterface::handle_input() [END]");
    //    Ok(io)
    //}

    pub async fn wait_for_quit(self: Arc<Self>) -> Result<()> {
        Ok(self.stop_recv.recv().await?)
    }
}
