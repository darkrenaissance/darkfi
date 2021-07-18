use drk::cli::{DrkCli, DrkConfig};
use drk::util::join_config_path;
use drk::Result;
use log::*;

use rand::Rng;
use async_std::sync::Arc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Payload = HashMap<String, String>;

struct Drk {
    url: String,
    payload: Payload,
}

impl Drk {
    pub fn new(url: String) -> Self {
        let mut payload = HashMap::new();
        payload.insert(String::from("jsonrpc"), String::from("2.0"));
        let id = Self::random_id();
        payload.insert(String::from("id"), id.to_string());
        Self { payload, url }
    }

    pub fn random_id() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
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

fn main() -> Result<()> {
    use simplelog::*;

    let options = Arc::new(DrkCli::load()?);

    let path = join_config_path(&PathBuf::from("drk.toml")).unwrap();

    let config: DrkConfig = if Path::new(&path).exists() {
        DrkConfig::load(path)?
    } else {
        DrkConfig::load_default(path)?
    };

    let config_ptr = Arc::new(&config);

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

    futures::executor::block_on(start(config_ptr, options))?;

    Ok(())
}
