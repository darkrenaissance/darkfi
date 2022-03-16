use async_executor::Executor;
use async_trait::async_trait;
use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::expand_path,
    Result,
};
use easy_parallel::Parallel;
use log::debug;
use serde_json::{json, Value};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    let rpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7777);
    let server_config = RpcServerConfig {
        socket_addr: rpc_addr,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        // TODO: cleanup
        identity_path: expand_path("../..")?,
        identity_pass: "test".to_string(),
    };

    let rpc_interface = Arc::new(JsonRpcInterface {});

    listen_and_serve(server_config, rpc_interface, executor).await?;
    Ok(())
}

struct JsonRpcInterface {}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("say_hello") => return self.say_hello(req.id, req.params).await,
            Some(_) | None => return JsonResult::Err(jsonerr(MethodNotFound, None, req.id)),
        }
    }
}

impl JsonRpcInterface {
    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    //let args = CliDao::parse();

    //let matches = CliDao::command().get_matches();

    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    //let rpc_addr = "tcp:://127.0.0.1:7777";
    //let client = Arc::new(Client::new(rpc_addr.to_string()));

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let ex = Arc::new(Executor::new());
    //let ex2 = ex.clone();
    let ex3 = ex.clone();
    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                start(ex3.clone()).await?;
                //client.run_client(client.clone(), ex2.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
