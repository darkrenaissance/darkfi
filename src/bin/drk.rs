use drk::cli::{DrkCli, DrkConfig};
use drk::util;
use drk::Result;
use log::*;
use std::fs::OpenOptions;
use std::io::Read;
use std::str;
use toml;

use async_std::sync::Arc;
use std::collections::HashMap;
use std::{fs, path::PathBuf};

type Payload = HashMap<String, String>;

struct Drk {
    url: String,
    payload: Payload,
}

impl Drk {
    pub fn new(url: String) -> Self {
        let mut payload = HashMap::new();
        payload.insert(String::from("jsonrpc"), String::from("2.0"));
        payload.insert(String::from("id"), String::from("0"));
        Self { payload, url }
    }

    pub async fn say_hello(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("say_hello"));
        self.request().await
    }

    pub async fn create_cashier_wallet(&mut self) -> Result<()> {
        self.payload.insert(
            String::from("method"),
            String::from("create_cashier_wallet"),
        );
        self.request().await
    }

    pub async fn create_wallet(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("create_wallet"));
        self.request().await
    }

    pub async fn key_gen(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("key_gen"));
        self.request().await
    }

    pub async fn get_info(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("get_info"));
        self.request().await
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("stop"));
        self.request().await
    }

    async fn request(&self) -> Result<()> {
        let mut res = surf::post(&self.url)
            .body(http_types::Body::from_json(&self.payload)?)
            .await?;

        if res.status() == 200 {
            let response = res.take_body();
            let response = response.into_string().await?;
            info!("Response Result: {:?}", response);
        }
        Ok(())
    }
}

async fn start(config: Arc<&DrkConfig>, options: Arc<DrkCli>) -> Result<()> {
    let url = config.rpc_url.clone();

    let mut client = Drk::new(url);

    if options.cashier {
        client.create_cashier_wallet().await?;
    }

    if options.wallet {
        client.create_wallet().await?;
    }

    if options.key {
        client.key_gen().await?;
    }

    if options.info {
        client.get_info().await?;
    }

    if options.hello {
        client.say_hello().await?;
    }

    if options.stop {
        client.stop().await?;
    }

    Ok(())
}

fn set_default() -> Result<DrkConfig> {
    let config_file = DrkConfig {
        rpc_url: String::from("127.0.0.1:8000"),
        log_path: String::from("/tmp/drk_cli.log"),
    };
    Ok(config_file)
}
fn main() -> Result<()> {
    use simplelog::*;

    let options = Arc::new(DrkCli::load()?);

    let config_path = PathBuf::from("drk.toml");
    let path = util::join_config_path(&config_path).unwrap();

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
    let config: DrkConfig = toml::from_str(str_buff)?;
    let config_pointer = Arc::new(&config);

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Info
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

    futures::executor::block_on(start(config_pointer, options))?;

    Ok(())
}
