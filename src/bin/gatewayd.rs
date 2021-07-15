use std::net::SocketAddr;
use std::str;
use std::sync::Arc;

use std::fs::OpenOptions;
use std::io::Read;
use std::{fs, path::PathBuf};
use toml;
use drk::blockchain::{rocks::columns, Rocks, RocksColumn};
use drk::cli::{GatewaydCli, GatewaydConfig};
use drk::service::GatewayService;
use drk::util::join_config_path;
use drk::Result;

extern crate clap;
use async_executor::Executor;
use easy_parallel::Parallel;

async fn start(executor: Arc<Executor<'_>>, config: Arc<&GatewaydConfig>) -> Result<()> {
    let accept_addr: SocketAddr = config.accept_url.parse()?;
    let pub_addr: SocketAddr = config.publisher_url.parse()?;
    let database_path = config.database_path.clone();
    let database_path = join_config_path(&PathBuf::from(database_path))?;

    let rocks = Rocks::new(&database_path)?;
    let rocks_slabstore_column = RocksColumn::<columns::Slabs>::new(rocks);

    let gateway = GatewayService::new(accept_addr, pub_addr, rocks_slabstore_column)?;

    gateway.start(executor.clone()).await?;
    Ok(())
}

fn set_default() -> Result<GatewaydConfig> {
    let config_file = GatewaydConfig {
        accept_url: String::from("127.0.0.1:3333"),
        publisher_url: String::from("127.0.0.1:4444"),
        database_path: String::from("gatewayd.db"),
        log_path: String::from("/tmp/gatewayd.log"),
    };
    Ok(config_file)
}

fn main() -> Result<()> {
    use simplelog::*;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let config_path = PathBuf::from("gatewayd.toml");
    let path = join_config_path(&config_path).unwrap();

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;

    let mut buffer: Vec<u8> = vec![];
    file.read_to_end(&mut buffer)?;

    if buffer.is_empty() {
        // set the default setting
        let config_file = set_default()?;
        let config_file = toml::to_string(&config_file)?;
        fs::write(&path, &config_file)?;
    }

    // reload the config
    let toml = fs::read(&path)?;
    let str_buff = str::from_utf8(&toml)?;

    // read from config file
    let config: GatewaydConfig = toml::from_str(str_buff)?;
    let config_pointer = Arc::new(&config);

    let options = GatewaydCli::load()?;

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    let log_path = config.log_path.clone();
    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(log_path).unwrap(),
        ),
    ])
    .unwrap();

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config_pointer).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
