#[macro_use]
extern crate clap;
use async_executor::Executor;
use async_std::sync::Mutex;
use easy_parallel::Parallel;
use log::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use sapvi::{ClientProtocol, Result, SeedProtocol, ServerProtocol};

use std::net::TcpListener;

use async_native_tls::{Identity, TlsAcceptor};
use http_types::{Request, Response, StatusCode, Body, Method};
use smol::{future, Async};

/// Serves a request and returns a response.
async fn serve(mut req: Request) -> http_types::Result<Response> {
    println!("Serving {}", req.url());

    let request = req.body_string().await?;

    let mut io = jsonrpc_core::IoHandler::new();
    io.add_sync_method("say_hello", |_| {
        Ok(jsonrpc_core::Value::String("Hello World!".into()))
    });
    io.add_sync_method("quit", |_| {
        Ok(jsonrpc_core::Value::Null)
    });

    //let request = r#"{"jsonrpc": "2.0", "method": "say_hello", "params": [42, 23], "id": 1}"#;
    //let response = r#"{"jsonrpc":"2.0","result":"Hello World!","id":1}"#;

    //assert_eq!(io.handle_request_sync(request), Some(response.to_string()));

    let response = io.handle_request_sync(&request).ok_or(sapvi::Error::BadOperationType)?;

    let mut res = Response::new(StatusCode::Ok);
    res.insert_header("Content-Type", "text/plain");
    res.set_body(response);
    Ok(res)
}

/// Listens for incoming connections and serves them.
async fn listen(executor: Arc<Executor<'_>>, rpc: Arc<RpcInterface>, listener: Async<TcpListener>, tls: Option<TlsAcceptor>) -> Result<()> {
    // Format the full host address.
    let host = match &tls {
        None => format!("http://{}", listener.get_ref().local_addr()?),
        Some(_) => format!("https://{}", listener.get_ref().local_addr()?),
    };
    println!("Listening on {}", host);

    loop {
        // Accept the next connection.
        let (stream, _) = listener.accept().await?;

        // Spawn a background task serving this connection.
        let task = match &tls {
            None => {
                let stream = async_dup::Arc::new(stream);
                let rpc = rpc.clone();
                executor.spawn(async move {
                    if let Err(err) = async_h1::accept(stream, move |mut req| {
                        let rpc = rpc.clone();
                        rpc.serve(req)
                    })
                    .await {
                        println!("Connection error: {:#?}", err);
                    }
                })
            }
            Some(tls) => {
                // In case of HTTPS, establish a secure TLS connection first.
                match tls.accept(stream).await {
                    Ok(stream) => {
                        let stream = async_dup::Arc::new(async_dup::Mutex::new(stream));
                        executor.spawn(async move {
                            if let Err(err) = async_h1::accept(stream, serve).await {
                                println!("Connection error: {:#?}", err);
                            }
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

struct RpcInterface {
    quit_send: async_channel::Sender<()>,
    quit_recv: async_channel::Receiver<()>
}

impl RpcInterface {
    fn new() -> Arc<Self> {
        let (quit_send, quit_recv) = async_channel::unbounded::<()>();

        Arc::new(Self {
            quit_send,
            quit_recv
        })
    }

    async fn serve(self: Arc<Self>, mut req: Request) -> http_types::Result<Response> {
        println!("Serving {}", req.url());

        let request = req.body_string().await?;

        let mut io = jsonrpc_core::IoHandler::new();
        io.add_sync_method("say_hello", |_| {
            Ok(jsonrpc_core::Value::String("Hello World!".into()))
        });
        let quit_send = self.quit_send.clone();
        io.add_method("quit", move |_| {
            let quit_send = quit_send.clone();
            async move {
                quit_send.send(()).await;
                Ok(jsonrpc_core::Value::Null)
            }
        });

        //let request = r#"{"jsonrpc": "2.0", "method": "say_hello", "params": [42, 23], "id": 1}"#;
        //let response = r#"{"jsonrpc":"2.0","result":"Hello World!","id":1}"#;

        //assert_eq!(io.handle_request_sync(request), Some(response.to_string()));

        let response = io.handle_request_sync(&request).ok_or(sapvi::Error::BadOperationType)?;

        let mut res = Response::new(StatusCode::Ok);
        res.insert_header("Content-Type", "text/plain");
        res.set_body(response);
        Ok(res)
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let connections = Arc::new(Mutex::new(HashMap::new()));

    let stored_addrs = Arc::new(Mutex::new(Vec::new()));

    let executor2 = executor.clone();
    let stored_addrs2 = stored_addrs.clone();

    let mut server_task = None;
    if let Some(accept_addr) = options.accept_addr {
        let accept_addr = accept_addr.clone();

        let protocol = ServerProtocol::new(connections.clone(), accept_addr, stored_addrs2);
        server_task = Some(executor.spawn(async move {
            protocol.start(executor2).await?;
            Ok::<(), sapvi::Error>(())
        }));
    }

    let mut seed_protocols = Vec::with_capacity(options.seed_addrs.len());

    // Normally we query this from a server
    let accept_addr = options.accept_addr.clone();

    for seed_addr in options.seed_addrs.iter() {
        let protocol = SeedProtocol::new(seed_addr.clone(), accept_addr, stored_addrs.clone());
        protocol.clone().start(executor.clone()).await;
        seed_protocols.push(protocol);
    }

    debug!("Waiting for seed node queries to finish...");

    for seed_protocol in seed_protocols {
        seed_protocol.await_finish().await;
    }

    debug!("Seed nodes queried.");

    let mut client_slots = vec![];
    for i in 0..options.connection_slots {
        debug!("Starting connection slot {}", i);

        let client = ClientProtocol::new(
            connections.clone(),
            accept_addr.clone(),
            stored_addrs.clone(),
        );
        client.clone().start(executor.clone()).await;
        client_slots.push(client);
    }

    for remote_addr in options.manual_connects {
        debug!("Starting connection (manual) to {}", remote_addr);

        let client = ClientProtocol::new(
            connections.clone(),
            accept_addr.clone(),
            stored_addrs.clone(),
        );
        client
            .clone()
            .start_manual(remote_addr, executor.clone())
            .await;
        client_slots.push(client);
    }

    let rpc = RpcInterface::new();
    let http = listen(executor.clone(), rpc.clone(), Async::<TcpListener>::bind(([127, 0, 0, 1], 8000))?, None);

    let http_task = executor.spawn(http);

    rpc.quit_recv.recv().await?;

    //server_task.cancel().await;
    Ok(())
}

struct ProgramOptions {
    accept_addr: Option<SocketAddr>,
    seed_addrs: Vec<SocketAddr>,
    manual_connects: Vec<SocketAddr>,
    connection_slots: u32,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept address")
            (@arg SEED_NODES: -s --seeds ... "Seed nodes")
            (@arg CONNECTS: -c --connect ... "Manual connections")
            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
        )
        .get_matches();

        let accept_addr = if let Some(accept_addr) = app.value_of("ACCEPT") {
            Some(accept_addr.parse()?)
        } else {
            None
        };

        let mut seed_addrs: Vec<SocketAddr> = vec![];
        if let Some(seeds) = app.values_of("SEED_NODES") {
            for seed in seeds {
                seed_addrs.push(seed.parse()?);
            }
        }

        let mut manual_connects: Vec<SocketAddr> = vec![];
        if let Some(connections) = app.values_of("CONNECTS") {
            for connect in connections {
                manual_connects.push(connect.parse()?);
            }
        }

        let connection_slots = if let Some(connection_slots) = app.value_of("CONNECT_SLOTS") {
            connection_slots.parse()?
        } else {
            0
        };

        Ok(ProgramOptions {
            accept_addr,
            seed_addrs,
            manual_connects,
            connection_slots,
        })
    }
}

fn main() -> Result<()> {
    use simplelog::*;
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
    )
    .unwrap()])
    .unwrap();

    let options = ProgramOptions::load()?;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, options).await?;
                drop(signal);
                Ok::<(), sapvi::Error>(())
            })
        });

    result
}
