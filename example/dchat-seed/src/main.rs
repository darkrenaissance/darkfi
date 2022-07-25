use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;
//use log::{error, info, warn};
use url::Url;

use darkfi::{
    net,
    net::Settings,
    util::cli::{get_log_config, get_log_level},
    Result,
};

#[async_std::main]
async fn main() -> Result<()> {
    let log_level = get_log_level(1);
    let log_config = get_log_config();

    let env_log_file_path = match std::env::var("DARKFI_LOG") {
        Ok(p) => std::fs::File::create(p).unwrap(),
        Err(_) => std::fs::File::create("/tmp/darkfi.log").unwrap(),
    };

    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(
            log_level,
            log_config.clone(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        ),
        simplelog::WriteLogger::new(log_level, log_config, env_log_file_path),
    ])?;

    let url = Url::parse("tcp://127.0.0.1:55555").unwrap();

    let settings = Settings {
        inbound: Some(url),
        outbound_connections: 0,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        outbound_retry_seconds: 1200,
        external_addr: None,
        peers: Vec::new(),
        seeds: Vec::new(),
        node_id: String::new(),
    };

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let p2p = net::P2p::new(settings).await;

    let seed = DchatSeed::new(p2p);

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                seed.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}

struct DchatSeed {
    p2p: net::P2pPtr,
}

impl DchatSeed {
    fn new(p2p: net::P2pPtr) -> Self {
        Self { p2p }
    }

    async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        self.p2p.clone().start(executor.clone()).await?;

        self.p2p.clone().run(executor.clone()).await?;

        Ok(())
    }
}
